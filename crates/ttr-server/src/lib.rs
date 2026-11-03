//! CiufCiuf! — Server Axum: WebSocket, lobby, QR, persistenza.
//!
//! Fornisce `build_router(state)` per costruire il router Axum testabile,
//! e `resume_game(state, path)` per il resume da save (D18 §8).

use axum::Router;
use std::path::Path;

mod bridge;
mod lobby;
pub mod save;
mod session;
mod state;
mod ws;

pub use state::AppState;

/// Costruisce il router Axum con tutti gli endpoint.
pub fn build_router(state: AppState) -> Router {
    use axum::routing::{get, post};

    Router::new()
        .route("/api/lan", get(lobby::lan_info))
        .route("/qr", get(lobby::qr_code))
        .route(
            "/api/games",
            post(lobby::create_game).get(lobby::list_games),
        )
        .route("/api/games/{id}", get(lobby::get_game))
        .route("/api/games/{id}/join", post(lobby::join_game))
        .route("/api/games/{id}/start", post(lobby::start_game))
        .route("/ws/{game_id}", get(ws::ws_handler))
        .with_state(state)
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

    tokio::spawn(async move {
        session.run().await;
    });

    Ok((game_id, host_token))
}
