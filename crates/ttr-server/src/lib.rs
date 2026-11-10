//! CiufCiuf! — Server Axum: WebSocket, lobby, QR, persistenza, statici.
//!
//! Fornisce `build_router(state, client_dist)` per costruire il router Axum testabile,
//! e `resume_game(state, path)` per il resume da save (D18 §8).

use axum::Router;
use std::path::{Path, PathBuf};

mod bridge;
mod lobby;
pub mod profiles;
pub mod save;
mod session;
mod state;
mod ws;

pub use state::AppState;

/// Avvia l'attore di una partita. Quando termina (partita conclusa da
/// tempo), la partita sparisce dall'elenco, ma solo se l'entry è ancora la
/// sua: un resume con lo stesso game_id registra un attore nuovo.
pub(crate) fn spawn_session(
    state: &AppState,
    game_id: String,
    tx: &tokio::sync::mpsc::Sender<session::GameCommand>,
    session: session::GameSession,
) {
    let games = state.games.clone();
    let own_tx = tx.clone();
    tokio::spawn(async move {
        session.run().await;
        let mut games = games.write().await;
        if games.get(&game_id).is_some_and(|h| h.tx.same_channel(&own_tx)) {
            games.remove(&game_id);
        }
    });
}

/// Costruisce il router Axum con tutti gli endpoint.
///
/// Se `client_dist` esiste come directory, vi serve i file statici del client
/// con fallback SPA a `index.html` per le rotte `/` e `/controller`.
pub fn build_router(state: AppState, client_dist: PathBuf) -> Router {
    use axum::routing::{get, post};

    let api_router = Router::new()
        .route("/api/lan", get(lobby::lan_info))
        .route("/qr", get(lobby::qr_code))
        .route(
            "/api/games",
            post(lobby::create_game).get(lobby::list_games),
        )
        .route("/api/games/{id}", get(lobby::get_game))
        .route("/api/games/{id}/join", post(lobby::join_game))
        .route("/api/games/{id}/start", post(lobby::start_game))
        .route("/api/standings", get(lobby::standings))
        .route("/api/profiles", get(lobby::list_profiles))
        .route("/ws/{game_id}", get(ws::ws_handler));

    // Serving statici del client con fallback SPA (Fase 3b)
    if client_dist.exists() {
        use tower_http::services::{ServeDir, ServeFile};
        let serve_dir = ServeDir::new(&client_dist)
            .fallback(ServeFile::new(client_dist.join("index.html")));

        Router::new()
            .merge(api_router)
            .fallback_service(serve_dir)
            .with_state(state)
    } else {
        tracing::warn!(
            "Directory client-dist {:?} non trovata — statici non serviti (usa --client-dist)",
            client_dist
        );
        api_router.with_state(state)
    }
}

/// Ripristina una partita da un file di salvataggio (D18 §8).
///
/// Restituisce `(game_id, host_token)` se il resume ha successo.
/// La partita viene registrata in lobby col roster salvato; i giocatori
/// si riagganciano per nome via `POST /api/games/{id}/join`.
pub async fn resume_game(state: &AppState, save_path: &Path) -> Result<(String, String), String> {
    use tokio::sync::mpsc;
    use uuid::Uuid;

    let save_data = save::read_save(save_path)?;

    // `game_id` finisce in un percorso su disco: solo la forma canonica è ammessa.
    let canonical_id = Uuid::parse_str(&save_data.game_id).map(|id| id.to_string());
    if canonical_id.as_deref() != Ok(save_data.game_id.as_str()) {
        return Err(format!("gameId non valido nel save: {:?}", save_data.game_id));
    }

    let ruleset = state
        .ruleset(&save_data.ruleset_id)
        .ok_or_else(|| format!("ruleset '{}' sconosciuto", save_data.ruleset_id))?
        .clone();

    let map = state
        .map(ruleset.map.as_str())
        .ok_or_else(|| format!("mappa '{}' sconosciuta", ruleset.map))?
        .clone();

    let engine = save::replay_engine(&save_data, &map, &ruleset)?;

    let game_id = save_data.game_id.clone();
    let host_token = Uuid::new_v4().to_string();
    let max_players = ruleset.players.max;

    let save_path = state.saves_dir.join(format!("{}.json", game_id));
    save::backup_before_resume(&save_path)?;
    let (tx, rx) = mpsc::channel(256);

    let saved_player_entries: Vec<(String, String, ttr_proto::PlayerColor)> = save_data
        .players
        .iter()
        .map(|p| {
            (
                p.id.clone(),
                p.name.clone(),
                save::parse_color(&p.color).unwrap_or(ttr_proto::PlayerColor::Red),
            )
        })
        .collect();

    let session = session::GameSession::new_resumed(
        game_id.clone(),
        save_data.game_name.clone(),
        save_data.ruleset_id.clone(),
        max_players,
        host_token.clone(),
        Some(save_path),
        rx,
        engine,
        map,
        saved_player_entries,
    );

    let handle = session::GameHandle {
        tx: tx.clone(),
        game_id: game_id.clone(),
        game_name: save_data.game_name,
        ruleset_id: save_data.ruleset_id.clone(),
        max_players,
        players_joined: 0,
        started: false,
    };

    {
        let mut games = state.games.write().await;
        games.insert(game_id.clone(), handle.clone());
    }

    let tx_for_cleanup = handle.tx.clone();
    spawn_session(state, game_id.clone(), &tx_for_cleanup, session);

    Ok((game_id, host_token))
}
