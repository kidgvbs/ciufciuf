//! Endpoint REST: lobby, LAN info, QR code, creazione/join/reassign/start partite.
//!
//! Tutti gli endpoint REST usano `axum::extract::State<AppState>` e
//! producono JSON. Errori nel formato `{"error":"REASON","detail":"…"}`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use ttr_proto::PlayerColor;

use crate::error::LobbyError;
use crate::names::{NameError, normalize_name};
use crate::session::{GameCommand, GameHandle, GameInfo, GameSession, JoinResponse, SessionConfig};
use crate::state::AppState;

// ── Tipi risposta REST ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
    detail: Option<String>,
}

impl From<LobbyError> for ErrorResponse {
    fn from(e: LobbyError) -> Self {
        if let LobbyError::Internal(cause) = &e {
            tracing::error!(error = %cause, "errore interno della lobby");
        }
        Self {
            error: e.code(),
            detail: Some(e.to_string()),
        }
    }
}

/// Converte un errore di lobby nella risposta HTTP con status e corpo JSON.
fn error_response(e: LobbyError) -> axum::response::Response {
    let status = e.status();
    (status, Json(ErrorResponse::from(e))).into_response()
}

/// Risposta 404 JSON per ogni percorso `/api` senza endpoint.
pub async fn api_not_found() -> axum::response::Response {
    error_response(LobbyError::EndpointNotFound)
}

// ── /api/rulesets ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct RulesetListItem {
    id: String,
    name: String,
    /// Modalità del ruleset (es. "Standard", "Mega") e sua posizione sulla mappa.
    mode: String,
    #[serde(rename = "modeOrder")]
    mode_order: u32,
    #[serde(rename = "mapId")]
    map_id: String,
    cities: usize,
    routes: usize,
    /// Biglietti in gioco con il `ticketSet` del ruleset.
    tickets: usize,
    #[serde(rename = "minPlayers")]
    min_players: u32,
    #[serde(rename = "maxPlayers")]
    max_players: u32,
    /// Bonus di fine partita del ruleset.
    bonuses: Vec<ttr_core::types::Bonus>,
    /// Città «grandi» della modalità Grandi Città (vuoto altrove).
    #[serde(rename = "bigCities")]
    big_cities: Vec<String>,
    /// Biglietti distribuiti a ogni giocatore nel draft iniziale.
    #[serde(rename = "initialDealt")]
    initial_dealt: u32,
    #[serde(rename = "initialKeepMin")]
    initial_keep_min: u32,
    /// Biglietti offerti a ogni pesca di metà partita.
    #[serde(rename = "drawCount")]
    draw_count: u32,
    #[serde(rename = "drawKeepMin")]
    draw_keep_min: u32,
    /// La mappa ha almeno una rotta tunnel.
    tunnels: bool,
    /// La mappa ha almeno una rotta con traghetti.
    ferries: bool,
    /// Il ruleset prevede le stazioni.
    stations: bool,
    /// Il ruleset ha le carte Paese.
    #[serde(rename = "countryCards")]
    country_cards: bool,
    /// Il ruleset ha i passeggeri (Germany 2017).
    passengers: bool,
    /// Regole speciali (testo) mostrate a inizio partita; vuoto se il ruleset non ne ha.
    #[serde(rename = "specialRules")]
    special_rules: Vec<String>,
    /// Tipi di rotta su cui una locomotiva paga da sola uno spazio (`tunnel`, `ferry`);
    /// assente = tutte.
    #[serde(
        rename = "locomotivesUsableOn",
        skip_serializing_if = "Option::is_none"
    )]
    locomotives_usable_on: Option<Vec<String>>,
    /// Sui traghetti questo numero di carte qualsiasi vale una locomotiva (assente = nessuna
    /// sostituzione).
    #[serde(rename = "ferryAnyCards", skip_serializing_if = "Option::is_none")]
    ferry_any_cards: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ListRulesetsResponse {
    rulesets: Vec<RulesetListItem>,
}

/// GET /api/rulesets — i ruleset caricati con i dati per sceglierli in lobby,
/// ordinati per mappa, ordine della modalità e id.
pub async fn list_rulesets(State(state): State<AppState>) -> impl IntoResponse {
    let mut rulesets: Vec<RulesetListItem> = state
        .rulesets
        .iter()
        .filter_map(|(id, ruleset)| {
            let map = state.map(&ruleset.map)?;
            Some(RulesetListItem {
                id: id.clone(),
                name: ruleset.name.clone(),
                mode: ruleset.mode.clone(),
                mode_order: ruleset.mode_order,
                map_id: ruleset.map.clone(),
                cities: map.cities.len(),
                routes: map.routes.len(),
                tickets: map.tickets_in_play(ruleset).count(),
                min_players: ruleset.players.min,
                max_players: ruleset.players.max,
                bonuses: ruleset.bonuses.clone(),
                big_cities: ruleset.big_cities.clone(),
                initial_dealt: ruleset.initial_tickets.dealt.iter().map(|d| d.count).sum(),
                initial_keep_min: ruleset.initial_tickets.keep_min,
                draw_count: ruleset.ticket_draw.count,
                draw_keep_min: ruleset.ticket_draw.keep_min,
                tunnels: map.routes.iter().any(|r| r.tunnel),
                ferries: map.routes.iter().any(|r| r.ferries > 0),
                stations: ruleset.stations.is_some(),
                country_cards: !ruleset.country_cards.is_empty(),
                passengers: ruleset.passengers.is_some(),
                special_rules: ruleset.special_rules.clone(),
                locomotives_usable_on: ruleset
                    .locomotives
                    .as_ref()
                    .and_then(|l| l.usable_on.clone()),
                ferry_any_cards: ruleset.ferries.as_ref().map(|f| f.any_cards_per_locomotive),
            })
        })
        .collect();
    rulesets
        .sort_by(|a, b| (&a.map_id, a.mode_order, &a.id).cmp(&(&b.map_id, b.mode_order, &b.id)));
    Json(ListRulesetsResponse { rulesets })
}

// ── /api/rulesets/{id}/map ─────────────────────────────────────────

/// GET /api/rulesets/{id}/map — il documento JSON della mappa del ruleset
/// (`cities`, `routes`, `tickets`, con i campi extra come `lat`/`lon`) più
/// `rulesetId`.
pub async fn ruleset_map(
    State(state): State<AppState>,
    Path(ruleset_id): Path<String>,
) -> axum::response::Response {
    let document = state
        .ruleset(&ruleset_id)
        .and_then(|r| state.map_documents.get(&r.map));
    let Some(serde_json::Value::Object(fields)) = document else {
        return error_response(LobbyError::RulesetNotFound);
    };
    let mut body = fields.clone();
    body.insert("rulesetId".into(), ruleset_id.into());
    Json(body).into_response()
}

// ── /api/rulesets/{id}/layout ──────────────────────────────────────

/// GET /api/rulesets/{id}/layout — il layout schematico della mappa del ruleset
/// (`data/layouts/{mapId}.json`) più `rulesetId` e `mapId`; 404 `LAYOUT_NOT_FOUND`
/// se la mappa non ha un file di layout.
pub async fn ruleset_layout(
    State(state): State<AppState>,
    Path(ruleset_id): Path<String>,
) -> axum::response::Response {
    let Some(ruleset) = state.ruleset(&ruleset_id) else {
        return error_response(LobbyError::RulesetNotFound);
    };
    let Some(serde_json::Value::Object(fields)) = state.layout_documents.get(&ruleset.map) else {
        return error_response(LobbyError::LayoutNotFound);
    };
    let mut body = fields.clone();
    body.insert("rulesetId".into(), ruleset_id.into());
    body.insert("mapId".into(), ruleset.map.clone().into());
    Json(body).into_response()
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
    /// Seed del RNG opzionale per partite deterministiche; accettato solo se il
    /// server è avviato con `--allow-client-seed`, altrimenti 400. `None` = seed
    /// casuale da CSPRNG.
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

pub(crate) const MAX_GAME_NAME_CHARS: usize = 40;

const DEFAULT_GAME_NAME: &str = "Partita";

/// Valida il nome della partita: vuoto = nome di default; troppo lungo è
/// `BAD_REQUEST`, caratteri non ammessi sono `INVALID_NAME`.
fn game_name(raw: Option<&str>) -> Result<String, LobbyError> {
    let Some(raw) = raw.map(str::trim).filter(|name| !name.is_empty()) else {
        return Ok(DEFAULT_GAME_NAME.to_string());
    };
    normalize_name(raw, MAX_GAME_NAME_CHARS).map_err(|e| match e {
        NameError::TooLong { .. } => LobbyError::BadRequest(format!(
            "Il nome della partita può avere al massimo {MAX_GAME_NAME_CHARS} caratteri."
        )),
        other => LobbyError::InvalidName(other),
    })
}

/// POST /api/games — crea una nuova partita in lobby.
pub async fn create_game(
    State(state): State<AppState>,
    Json(body): Json<CreateGameRequest>,
) -> axum::response::Response {
    let ruleset = match state.ruleset(&body.ruleset_id) {
        Some(r) => r.clone(),
        None => {
            return error_response(LobbyError::BadRequest(format!(
                "ruleset '{}' sconosciuto",
                body.ruleset_id
            )));
        }
    };

    if body.seed.is_some() && !state.allow_client_seed {
        return error_response(LobbyError::SeedNotAllowed);
    }

    let game_name = match game_name(body.name.as_deref()) {
        Ok(name) => name,
        Err(e) => return error_response(e),
    };

    let game_id = Uuid::new_v4().to_string();
    let host_token = Uuid::new_v4().to_string();
    let max_players = ruleset.players.max;

    let save_path = state.saves_dir.join(format!("{}.json", game_id));
    let (tx, rx) = mpsc::channel(256);
    let finished = Arc::new(AtomicBool::new(false));

    let session = GameSession::new_lobby(
        SessionConfig {
            game_id: game_id.clone(),
            game_name: game_name.clone(),
            ruleset_id: body.ruleset_id.clone(),
            map_id: ruleset.map.clone(),
            max_players,
            host_token: host_token.clone(),
            save_path: Some(save_path),
            results_lock: state.results_lock.clone(),
            limits: state.limits,
            finished: finished.clone(),
        },
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
        finished,
    };

    // I controlli e l'inserimento stanno nella stessa sezione critica: lo
    // shutdown imposta il flag prima di leggere l'elenco, quindi una partita
    // inserita qui è sempre vista dal suo giro di `Shutdown`.
    {
        let mut games = state.games.write().await;
        if state.shutting_down.load(Ordering::Acquire) {
            return error_response(LobbyError::ShuttingDown);
        }
        let active = games
            .values()
            .filter(|h| !h.finished.load(Ordering::Acquire))
            .count();
        if active >= state.limits.max_active_games {
            return error_response(LobbyError::TooManyGames);
        }
        games.insert(game_id.clone(), handle.clone());
    }

    // Avvia l'attore (si rimuove dall'elenco quando termina).
    crate::spawn_session(&state, game_id.clone(), &handle.tx, session);

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
    /// Partita conclusa (`GAME_OVER`), ancora nell'elenco finché l'attore non termina.
    finished: bool,
}

#[derive(Debug, Serialize)]
struct ListGamesResponse {
    games: Vec<GameListItem>,
}

#[derive(Debug, Serialize)]
struct StandingsResponse {
    /// Partite concluse archiviate.
    games: usize,
    standings: Vec<crate::save::StandingRow>,
}

/// GET /api/standings — classifica generale dalle partite concluse.
pub async fn standings(State(state): State<AppState>) -> impl IntoResponse {
    let dir = state.saves_dir.clone();
    let results = tokio::task::spawn_blocking(move || crate::save::read_results(&dir))
        .await
        .unwrap_or_default();
    Json(StandingsResponse {
        games: results.len(),
        standings: crate::save::standings(&results),
    })
}

/// GET /api/games — elenca le partite attive, interrogando l'attore.
pub async fn list_games(State(state): State<AppState>) -> impl IntoResponse {
    let handles: Vec<GameHandle> = {
        let games = state.games.read().await;
        games.values().cloned().collect()
    };

    let infos = futures_util::future::join_all(handles.iter().map(query_game_info)).await;
    let list = handles
        .iter()
        .zip(infos)
        .map(|(h, info)| GameListItem {
            game_id: h.game_id.clone(),
            name: h.game_name.clone(),
            ruleset_id: h.ruleset_id.clone(),
            players_joined: info
                .as_ref()
                .map(|i| i.players_joined)
                .unwrap_or(h.players_joined),
            max_players: h.max_players,
            started: info.as_ref().map(|i| i.started).unwrap_or(h.started),
            finished: h.finished.load(Ordering::Acquire),
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
    /// Nomi del save non ancora rientrati; vuoto fuori dal resume.
    #[serde(rename = "missingPlayers")]
    missing_players: Vec<String>,
}

/// GET /api/games/{id} — dettagli partita in lobby, via attore.
pub async fn get_game(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> axum::response::Response {
    let handle = {
        let games = state.games.read().await;
        match games.get(&game_id) {
            Some(h) => h.clone(),
            None => return error_response(LobbyError::GameNotFound),
        }
    };

    let info = query_game_info(&handle).await;

    let response = GetGameResponse {
        game_id: handle.game_id.clone(),
        name: handle.game_name.clone(),
        ruleset_id: handle.ruleset_id.clone(),
        players: info
            .as_ref()
            .map(|i| {
                i.players
                    .iter()
                    .map(|(id, name, color)| LobbyPlayer {
                        player_id: id.clone(),
                        name: name.clone(),
                        color: *color,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        started: info.as_ref().map(|i| i.started).unwrap_or(handle.started),
        max_players: handle.max_players,
        missing_players: info.map(|i| i.missing_players).unwrap_or_default(),
    };

    Json(response).into_response()
}

/// Interroga l'attore con `GetInfo`; timeout breve → fallback ai dati cache.
async fn query_game_info(handle: &GameHandle) -> Option<GameInfo> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    if handle
        .tx
        .send(GameCommand::GetInfo { reply: reply_tx })
        .await
        .is_err()
    {
        return None; // attore morto
    }
    tokio::time::timeout(std::time::Duration::from_millis(500), reply_rx)
        .await
        .ok()
        .and_then(|r| r.ok())
}

/// Cerca la partita nel registro.
async fn find_game(state: &AppState, game_id: &str) -> Result<GameHandle, LobbyError> {
    state
        .games
        .read()
        .await
        .get(game_id)
        .cloned()
        .ok_or(LobbyError::GameNotFound)
}

/// Invia un comando all'attore e attende la risposta.
async fn ask_actor<T>(
    handle: &GameHandle,
    command: impl FnOnce(tokio::sync::oneshot::Sender<Result<T, LobbyError>>) -> GameCommand,
) -> Result<T, LobbyError> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    handle
        .tx
        .send(command(reply_tx))
        .await
        .map_err(|_| LobbyError::GameActorGone)?;
    reply_rx.await.map_err(|_| LobbyError::GameActorGone)?
}

// ── /api/games/{id}/join ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct JoinRequest {
    pub name: String,
    pub color: Option<PlayerColor>,
    /// Token originale del giocatore, obbligatorio al join di una partita ripresa.
    pub token: Option<String>,
}

#[derive(Debug, Serialize)]
struct JoinResponseBody {
    #[serde(rename = "playerId")]
    player_id: String,
    token: String,
    #[serde(rename = "wsUrl")]
    ws_url: String,
}

/// Salva il nome come profilo (se nuovo). Errori solo nel log: il join è
/// già riuscito e un profilo mancato non deve farlo fallire.
async fn remember_profile(state: &AppState, name: String) {
    let dir = state.saves_dir.clone();
    let lock = state.profiles_lock.clone();
    let res = tokio::task::spawn_blocking(move || {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        crate::profiles::add_profile(&dir, &name)
    })
    .await;
    if let Ok(Err(e)) = res {
        tracing::warn!(error = %e, "salvataggio profilo fallito");
    }
}

#[derive(Debug, Serialize)]
struct ProfilesResponse {
    profiles: Vec<String>,
}

/// GET /api/profiles — nomi dei profili salvati. Un file illeggibile dà
/// un elenco vuoto e un avviso nel log.
pub async fn list_profiles(State(state): State<AppState>) -> impl IntoResponse {
    let dir = state.saves_dir.clone();
    let profiles = tokio::task::spawn_blocking(move || crate::profiles::read_profiles(&dir))
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "lettura profili interrotta");
            Ok(Vec::new())
        })
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "profili illeggibili: elenco vuoto");
            Vec::new()
        });
    Json(ProfilesResponse { profiles })
}

/// POST /api/games/{id}/join — join di un giocatore in lobby.
pub async fn join_game(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(body): Json<JoinRequest>,
) -> axum::response::Response {
    if state.shutting_down.load(Ordering::Acquire) {
        return error_response(LobbyError::ShuttingDown);
    }
    let handle = match find_game(&state, &game_id).await {
        Ok(h) => h,
        Err(e) => return error_response(e),
    };

    let player_id = Uuid::new_v4().to_string();
    let token = Uuid::new_v4().to_string();

    // Il colore (eventualmente automatico) viene risolto dall'attore, che
    // possiede il roster e conosce i colori già usati.
    let result = ask_actor(&handle, |reply| GameCommand::Join {
        player_id,
        name: body.name,
        color: body.color,
        token,
        resume_token: body.token,
        reply,
    })
    .await;

    match result {
        Ok(resp) => {
            // Il nome resta salvato come profilo per le partite future.
            remember_profile(&state, resp.name.clone()).await;
            Json(JoinResponseBody {
                player_id: resp.player_id,
                token: resp.token,
                ws_url: format!("/ws/{}", game_id),
            })
            .into_response()
        }
        Err(e) => error_response(e),
    }
}

// ── /api/games/{id}/reassign ───────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ReassignRequest {
    #[serde(rename = "hostToken")]
    pub host_token: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
struct ReassignResponseBody {
    #[serde(rename = "playerId")]
    player_id: String,
    token: String,
}

/// POST /api/games/{id}/reassign — token nuovo per un giocatore, su richiesta
/// dell'host: nei nomi del save di una partita ripresa, e in una partita già
/// avviata (anche non ripresa) per i giocatori del roster.
pub async fn reassign_player(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Json(body): Json<ReassignRequest>,
) -> axum::response::Response {
    let handle = match find_game(&state, &game_id).await {
        Ok(h) => h,
        Err(e) => return error_response(e),
    };

    let result: Result<JoinResponse, LobbyError> =
        ask_actor(&handle, |reply| GameCommand::Reassign {
            host_token: body.host_token,
            name: body.name,
            new_token: Uuid::new_v4().to_string(),
            reply,
        })
        .await;

    match result {
        Ok(resp) => Json(ReassignResponseBody {
            player_id: resp.player_id,
            token: resp.token,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

// ── /api/games/{id} (DELETE) ───────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    #[serde(rename = "hostToken")]
    pub host_token: String,
}

/// DELETE /api/games/{id} — l'host chiude la partita: l'attore termina e la
/// partita sparisce dall'elenco. Un save su disco resta dov'è.
pub async fn delete_game(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    // Alcuni proxy tolgono il body alle DELETE: senza token vale 401,
    // con o senza `Content-Type`.
    if body.is_empty() {
        return error_response(LobbyError::Unauthorized);
    }
    let body: DeleteRequest = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return error_response(LobbyError::BadRequest(format!("body non valido: {e}"))),
    };
    let handle = match find_game(&state, &game_id).await {
        Ok(h) => h,
        Err(e) => return error_response(e),
    };

    let result = ask_actor(&handle, |reply| GameCommand::Delete {
        host_token: body.host_token,
        reply,
    })
    .await;

    match result {
        Ok(()) => {
            let mut games = state.games.write().await;
            if games
                .get(&game_id)
                .is_some_and(|h| h.tx.same_channel(&handle.tx))
            {
                games.remove(&game_id);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => error_response(e),
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
) -> axum::response::Response {
    let handle = match find_game(&state, &game_id).await {
        Ok(h) => h,
        Err(e) => return error_response(e),
    };
    let Some(ruleset) = state.ruleset(&handle.ruleset_id).cloned() else {
        return error_response(LobbyError::BadRequest("ruleset sconosciuto".into()));
    };
    let Some(map) = state.map(ruleset.map.as_str()).cloned() else {
        return error_response(LobbyError::BadRequest(format!(
            "mappa '{}' sconosciuta",
            ruleset.map
        )));
    };

    let result = ask_actor(&handle, |reply| GameCommand::Start {
        host_token: body.host_token,
        ruleset: Box::new(ruleset),
        map,
        reply,
    })
    .await;

    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e),
    }
}

// ── /api/saves ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ListSavesResponse {
    saves: Vec<crate::save::SaveSummary>,
}

/// GET /api/saves — i save `in_progress` che si possono riprendere, cioè
/// quelli la cui partita non è già in lobby; dal più recente.
pub async fn list_saves(State(state): State<AppState>) -> axum::response::Response {
    let list_state = state.clone();
    let mut saves =
        match tokio::task::spawn_blocking(move || crate::save::list_in_progress_saves(&list_state))
            .await
        {
            Ok(saves) => saves,
            Err(e) => {
                return error_response(LobbyError::Internal(format!("elenco dei save: {e}")));
            }
        };
    {
        let games = state.games.read().await;
        saves.retain(|s| !games.contains_key(&s.game_id));
    }
    Json(ListSavesResponse { saves }).into_response()
}

#[derive(Debug, Serialize)]
struct ResumeSaveResponse {
    #[serde(rename = "gameId")]
    game_id: String,
    #[serde(rename = "hostToken")]
    host_token: String,
    #[serde(rename = "rulesetId")]
    ruleset_id: String,
    #[serde(rename = "maxPlayers")]
    max_players: u32,
}

/// POST /api/saves/{id}/resume — riporta in lobby una partita salvata e dà il
/// `hostToken` a chi la riprende. Il body è ignorato.
pub async fn resume_save(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> axum::response::Response {
    match crate::resume_saved_game(&state, &game_id).await {
        Ok(resumed) => {
            let response = ResumeSaveResponse {
                game_id: resumed.game_id,
                host_token: resumed.host_token,
                ruleset_id: resumed.ruleset_id,
                max_players: resumed.max_players,
            };
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => error_response(e.into()),
    }
}

/// DELETE /api/saves/{id} — elimina un save non attivo rinominandolo in
/// `saves/{id}.deleted-{unix}.bak`; risponde 204.
pub async fn delete_save(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> axum::response::Response {
    match crate::delete_saved_game(&state, &game_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e.into()),
    }
}

// ── Conversione InboundAction → ttr_core::Action ───────────────────

/// Converte un'azione inbound del protocollo in un'azione core,
/// dato l'indice del giocatore già risolto dall'attore. `None` per
/// `RequestSnapshot`, che non ha un'azione core corrispondente.
pub(crate) fn inbound_to_core_action(
    inbound: &ttr_proto::InboundAction,
    player_index: usize,
) -> Option<ttr_core::Action> {
    let action = match inbound {
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
            // A parità di città vale l'ultima scelta; il motore rifiuta colori assenti.
            passengers: p
                .passengers
                .iter()
                .flatten()
                .map(|pick| (pick.city_id.clone(), pick.color.clone()))
                .collect(),
        },
        ttr_proto::InboundAction::BuildStation(p) => ttr_core::Action::BuildStation {
            player_index,
            city_id: p.city_id.clone(),
            cards_discarded: p.cards_discarded.clone(),
        },
        ttr_proto::InboundAction::TunnelDecision(p) => ttr_core::Action::TunnelDecision {
            player_index,
            pay: p.pay,
            cards_discarded: p.cards_discarded.clone(),
        },
        ttr_proto::InboundAction::RequestSnapshot(_) => return None,
    };
    Some(action)
}
