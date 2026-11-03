//! Endpoint REST: lobby, LAN info, QR code, creazione/join/start partite.
//!
//! Tutti gli endpoint REST usano `axum::extract::State<AppState>` e
//! producono JSON. Errori nel formato `{"error":"REASON","detail":"…"}`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use ttr_proto::PlayerColor;

use crate::session::{GameCommand, GameHandle, GameSession};
use crate::state::AppState;

// ── Tipi risposta REST ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    detail: Option<String>,
}

impl ErrorResponse {
    fn new(error: &str, detail: Option<String>) -> Self {
        Self {
            error: error.to_string(),
            detail,
        }
    }

    fn status(&self) -> StatusCode {
        let e = self.error.as_str();
        if e == "GAME_NOT_FOUND" {
            StatusCode::NOT_FOUND
        } else if e == "GAME_FULL" || e == "GAME_STARTED" {
            StatusCode::CONFLICT
        } else if e == "UNAUTHORIZED" {
            StatusCode::UNAUTHORIZED
        } else if e == "NAME_TAKEN" || e == "NAME_NOT_IN_SAVE" {
            StatusCode::CONFLICT
        } else if e == "BAD_REQUEST" || e.starts_with("MIN_PLAYERS") {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }

    fn into_response(self) -> axum::response::Response {
        let status = self.status();
        (status, Json(self)).into_response()
    }
}

// ── /api/lan ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct LanInfo {
    port: u16,
    candidates: Vec<LanCandidate>,
    fallback: String,
}

#[derive(Debug, Serialize)]
struct LanCandidate {
    ip: String,
    url: String,
    preferred: bool,
}

/// GET /api/lan — enumera gli IP locali per la connessione LAN.
pub async fn lan_info(State(state): State<AppState>) -> impl IntoResponse {
    let port = state.port;
    let mut candidates: Vec<LanCandidate> = Vec::new();

    if let Ok(ips) = local_ip_address::list_afinet_netifas() {
        for (_name, ip) in &ips {
            if ip.is_loopback() {
                continue;
            }
            if let std::net::IpAddr::V4(v4) = ip {
                let ip_str = v4.to_string();
                let preferred = is_private_ipv4(*v4);
                candidates.push(LanCandidate {
                    ip: ip_str.clone(),
                    url: format!("http://{}:{}", ip_str, port),
                    preferred,
                });
            }
        }
    }

    if candidates.is_empty() {
        // Fallback: prova local-ip-address
        if let Ok(my_ip) = local_ip_address::local_ip()
            && !my_ip.is_loopback()
        {
            let ip_str = my_ip.to_string();
            candidates.push(LanCandidate {
                ip: ip_str.clone(),
                url: format!("http://{}:{}", ip_str, port),
                preferred: true,
            });
        }
    }

    Json(LanInfo {
        port,
        candidates,
        fallback: format!("http://ttr.local:{}", port),
    })
}

fn is_private_ipv4(ip: std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 10
        || (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31)
        || (octets[0] == 192 && octets[1] == 168)
}

// ── /qr ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct QrParams {
    pub data: Option<String>,
}

/// GET /qr?data=<url> — restituisce un QR code SVG.
pub async fn qr_code(
    Query(params): Query<QrParams>,
) -> Result<(StatusCode, [(String, String); 1], String), StatusCode> {
    let data = params.data.as_deref().unwrap_or("http://ttr.local:8080");
    let code = qrcode::QrCode::new(data.as_bytes()).map_err(|_| StatusCode::BAD_REQUEST)?;
    let svg = code.render::<qrcode::render::svg::Color>().build();
    Ok((
        StatusCode::OK,
        [("Content-Type".into(), "image/svg+xml".into())],
        svg,
    ))
}

// ── /api/games (POST) ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateGameRequest {
    #[serde(rename = "rulesetId")]
    pub ruleset_id: String,
    pub name: Option<String>,
    /// Seed del RNG opzionale: permette partite deterministiche
    /// (test deterministici + replay). `None` = seed time-based.
    pub seed: Option<u64>,
}

#[derive(Debug, Serialize)]
struct CreateGameResponse {
    #[serde(rename = "gameId")]
    game_id: String,
    #[serde(rename = "hostToken")]
    host_token: String,
    #[serde(rename = "rulesetId")]
    ruleset_id: String,
    #[serde(rename = "maxPlayers")]
    max_players: u32,
}

/// POST /api/games — crea una nuova partita in lobby.
pub async fn create_game(
    State(state): State<AppState>,
    Json(body): Json<CreateGameRequest>,
) -> impl IntoResponse {
    let ruleset = match state.ruleset(&body.ruleset_id) {
        Some(r) => r.clone(),
        None => {
            return ErrorResponse::new(
                "BAD_REQUEST",
                Some(format!("ruleset '{}' sconosciuto", body.ruleset_id)),
            )
            .into_response();
        }
    };

    let game_id = Uuid::new_v4().to_string();
    let host_token = Uuid::new_v4().to_string();
    let max_players = ruleset.players.max;
    let game_name = body.name.unwrap_or_else(|| "Partita".into());

    let save_path = state.saves_dir.join(format!("{}.json", game_id));
    let (tx, rx) = mpsc::channel(256);

    let session = GameSession::new_lobby(
        game_id.clone(),
        game_name.clone(),
        body.ruleset_id.clone(),
        max_players,
        host_token.clone(),
        Some(save_path),
        rx,
        body.seed,
    );

    let handle = GameHandle {
        tx: tx.clone(),
        game_id: game_id.clone(),
        game_name,
        ruleset_id: body.ruleset_id.clone(),
        max_players,
        players_joined: 0,
        started: false,
    };

    // Registra la partita nello stato globale
    {
        let mut games = state.games.write().await;
        games.insert(game_id.clone(), handle.clone());
    }

    // Avvia l'attore
    tokio::spawn(async move {
        session.run().await;
        // Cleanup alla chiusura
    });

    let response = CreateGameResponse {
        game_id,
        host_token,
        ruleset_id: body.ruleset_id,
        max_players,
    };

    (StatusCode::CREATED, Json(response)).into_response()
}

// ── /api/games (GET) ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct GameListItem {
    #[serde(rename = "gameId")]
    game_id: String,
    name: String,
    #[serde(rename = "rulesetId")]
    ruleset_id: String,
    #[serde(rename = "playersJoined")]
    players_joined: usize,
    #[serde(rename = "maxPlayers")]
    max_players: u32,
    started: bool,
}

#[derive(Debug, Serialize)]
struct ListGamesResponse {
    games: Vec<GameListItem>,
}

/// GET /api/games — elenca le partite attive.
pub async fn list_games(State(state): State<AppState>) -> impl IntoResponse {
    let games = state.games.read().await;
    let list: Vec<GameListItem> = games
        .values()
        .map(|h| GameListItem {
            game_id: h.game_id.clone(),
            name: h.game_name.clone(),
            ruleset_id: h.ruleset_id.clone(),
            players_joined: h.players_joined,
            max_players: h.max_players,
            started: h.started,
        })
        .collect();
    Json(ListGamesResponse { games: list })
}

// ── /api/games/{id} (GET) ──────────────────────────────────────────

#[derive(Debug, Serialize)]
struct LobbyPlayer {
    #[serde(rename = "playerId")]
    player_id: String,
    name: String,
    color: PlayerColor,
}

#[derive(Debug, Serialize)]
struct GetGameResponse {
    #[serde(rename = "gameId")]
    game_id: String,
    name: String,
    #[serde(rename = "rulesetId")]
    ruleset_id: String,
    players: Vec<LobbyPlayer>,
    started: bool,
    #[serde(rename = "maxPlayers")]
    max_players: u32,
}

/// GET /api/games/{id} — dettagli partita in lobby.
pub async fn get_game(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> axum::response::Response {
    let games = state.games.read().await;
    let handle = match games.get(&game_id) {
        Some(h) => h.clone(),
        None => {
            return ErrorResponse::new("GAME_NOT_FOUND", None).into_response();
        }
    };
    drop(games);

    // Chiedi all'attore lo stato corrente dei giocatori
    // Per semplicità usiamo il players_joined del GameHandle
    // (andrebbe arricchito con i dettagli giocatore via query all'attore)
    let response = GetGameResponse {
        game_id: handle.game_id.clone(),
        name: handle.game_name.clone(),
        ruleset_id: handle.ruleset_id.clone(),
        players: vec![], // TODO: popolare da attore
        started: handle.started,
        max_players: handle.max_players,
    };

    Json(response).into_response()
}

// ── /api/games/{id}/join ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct JoinRequest {
    pub name: String,
    pub color: Option<PlayerColor>,
}

#[derive(Debug, Serialize)]
struct JoinResponseBody {
    #[serde(rename = "playerId")]
    player_id: String,
    token: String,
    #[serde(rename = "wsUrl")]
    ws_url: String,
}

/// POST /api/games/{id}/join — join di un giocatore in lobby.
pub async fn join_game(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(body): Json<JoinRequest>,
) -> axum::response::Response {
    let handle = {
        let games = state.games.read().await;
        match games.get(&game_id) {
            Some(h) => h.clone(),
            None => {
                return ErrorResponse::new("GAME_NOT_FOUND", None).into_response();
            }
        }
    };

    let player_id = Uuid::new_v4().to_string();
    let token = Uuid::new_v4().to_string();

    // Il colore (eventualmente automatico) viene risolto dall'attore, che
    // possiede il roster e conosce i colori già usati (pacchetto §5).
    let color = body.color;

    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    if handle
        .tx
        .send(GameCommand::Join {
            player_id: player_id.clone(),
            name: body.name.clone(),
            color,
            token: token.clone(),
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return ErrorResponse::new(
            "GAME_NOT_FOUND",
            Some("l'attore della partita è terminato".into()),
        )
        .into_response();
    }

    match reply_rx.await {
        Ok(Ok(resp)) => Json(JoinResponseBody {
            player_id: resp.player_id,
            token: resp.token,
            ws_url: format!("/ws/{}", game_id),
        })
        .into_response(),
        Ok(Err(reason)) => ErrorResponse::new(&reason, None).into_response(),
        Err(_) => ErrorResponse::new("GAME_NOT_FOUND", Some("timeout risposta attore".into()))
            .into_response(),
    }
}

// ── /api/games/{id}/start ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StartRequest {
    #[serde(rename = "hostToken")]
    pub host_token: String,
}

/// POST /api/games/{id}/start — avvia la partita.
pub async fn start_game(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(body): Json<StartRequest>,
) -> impl IntoResponse {
    let (handle, ruleset, map) = {
        let games = state.games.read().await;
        let handle = match games.get(&game_id) {
            Some(h) => h.clone(),
            None => {
                return ErrorResponse::new("GAME_NOT_FOUND", None).into_response();
            }
        };
        let ruleset = match state.ruleset(&handle.ruleset_id) {
            Some(r) => r.clone(),
            None => {
                return ErrorResponse::new("BAD_REQUEST", Some("ruleset sconosciuto".into()))
                    .into_response();
            }
        };
        let map = match state.map(ruleset.map.as_str()) {
            Some(m) => m.clone(),
            None => {
                return ErrorResponse::new(
                    "BAD_REQUEST",
                    Some(format!("mappa '{}' sconosciuta", ruleset.map)),
                )
                .into_response();
            }
        };
        (handle, ruleset, map)
    };

    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    if handle
        .tx
        .send(GameCommand::Start {
            host_token: body.host_token,
            ruleset,
            map,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return ErrorResponse::new(
            "GAME_NOT_FOUND",
            Some("l'attore della partita è terminato".into()),
        )
        .into_response();
    }

    match reply_rx.await {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(reason)) => ErrorResponse::new(&reason, None).into_response(),
        Err(_) => ErrorResponse::new("GAME_NOT_FOUND", Some("timeout risposta attore".into()))
            .into_response(),
    }
}

// ── Conversione InboundAction → ttr_core::Action ───────────────────

/// Converte un'azione inbound del protocollo in un'azione core,
/// dato l'indice del giocatore già risolto dall'attore.
pub(crate) fn inbound_to_core_action(
    inbound: &ttr_proto::InboundAction,
    player_index: usize,
) -> ttr_core::Action {
    match inbound {
        ttr_proto::InboundAction::DrawTrainCard(p) => {
            let source = match &p.source {
                ttr_proto::DrawSource::Deck => ttr_core::DrawSource::Deck,
                ttr_proto::DrawSource::FaceUp { index } => {
                    ttr_core::DrawSource::FaceUp { index: *index }
                }
            };
            ttr_core::Action::DrawTrainCard {
                player_index,
                source,
            }
        }
        ttr_proto::InboundAction::DrawTickets(_) => ttr_core::Action::DrawTickets { player_index },
        ttr_proto::InboundAction::ChooseTickets(p) => ttr_core::Action::ChooseTickets {
            player_index,
            keep: p.keep.clone(),
        },
        ttr_proto::InboundAction::ClaimRoute(p) => ttr_core::Action::ClaimRoute {
            player_index,
            route_id: p.route_id.clone(),
            cards_discarded: p.cards_discarded.clone(),
        },
        ttr_proto::InboundAction::RequestSnapshot(_) => {
            // RequestSnapshot non ha un'azione core corrispondente;
            // viene gestito come caso speciale dal chiamante.
            unreachable!("RequestSnapshot non è un'azione core")
        }
    }
}
