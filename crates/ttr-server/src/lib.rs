//! CiufCiuf! — Server Axum: WebSocket, lobby, QR, persistenza, statici.
//!
//! Fornisce `build_router(state, client_dist)` per costruire il router Axum testabile,
//! `resume_game(state, path)` per il resume da save e `shutdown_sessions` per
//! fermare le partite scrivendo su disco quanto è in coda.

use axum::Router;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

mod bridge;
mod error;
mod lobby;
mod names;
pub mod profiles;
pub mod save;
mod session;
mod state;
mod writer;
mod ws;

pub use session::SessionLimits;
pub use state::AppState;

/// Avvia l'attore di una partita. Quando termina (partita conclusa da
/// tempo, lobby o partita abbandonata, shutdown) la partita sparisce
/// dall'elenco, ma solo se l'entry è ancora la sua: un resume con lo stesso
/// game_id registra un attore nuovo.
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
        if games
            .get(&game_id)
            .is_some_and(|h| h.tx.same_channel(&own_tx))
        {
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
        .route(
            "/api/games/{id}",
            get(lobby::get_game).delete(lobby::delete_game),
        )
        .route("/api/games/{id}/join", post(lobby::join_game))
        .route("/api/games/{id}/reassign", post(lobby::reassign_player))
        .route("/api/games/{id}/start", post(lobby::start_game))
        .route("/api/rulesets/{id}/map", get(lobby::ruleset_map))
        .route("/api/standings", get(lobby::standings))
        .route("/api/profiles", get(lobby::list_profiles))
        .route("/ws/{game_id}", get(ws::ws_handler));

    // Statici del client con fallback SPA.
    if client_dist.exists() {
        use tower_http::services::{ServeDir, ServeFile};
        let serve_dir =
            ServeDir::new(&client_dist).fallback(ServeFile::new(client_dist.join("index.html")));

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

/// Ferma tutte le partite e attende, fino a `timeout`, che le scritture in
/// coda siano su disco. Imposta `shutting_down` (creazioni, join e WebSocket
/// nuovi ricevono 503) e ripete `Shutdown` finché l'elenco non è vuoto, così
/// anche le partite create durante il drain si fermano. I save delle partite
/// in corso restano dove sono.
pub async fn shutdown_sessions(state: &AppState, timeout: Duration) {
    state.shutting_down.store(true, Ordering::Release);
    let drained = async {
        loop {
            let handles: Vec<session::GameHandle> =
                state.games.read().await.values().cloned().collect();
            if handles.is_empty() {
                break;
            }
            for handle in &handles {
                let _ = handle.tx.try_send(session::GameCommand::Shutdown);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    if tokio::time::timeout(timeout, drained).await.is_err() {
        tracing::warn!("chiusura: alcune partite non hanno terminato le scritture in tempo");
    }
}

/// Ripristina una partita da un file di salvataggio.
///
/// Restituisce `(game_id, host_token)` se il resume ha successo.
/// La partita viene registrata in lobby col roster salvato; i giocatori
/// si riagganciano per nome e token originale via `POST /api/games/{id}/join`,
/// oppure ricevono un token nuovo dall'host via `POST /api/games/{id}/reassign`.
/// Un save di una partita già conclusa è rifiutato.
pub async fn resume_game(state: &AppState, save_path: &Path) -> Result<(String, String), String> {
    use tokio::sync::mpsc;
    use uuid::Uuid;

    let save_data = save::read_save(save_path)?;

    // `game_id` finisce in un percorso su disco: solo la forma canonica è ammessa.
    let canonical_id = Uuid::parse_str(&save_data.game_id).map(|id| id.to_string());
    if canonical_id.as_deref() != Ok(save_data.game_id.as_str()) {
        return Err(format!(
            "gameId non valido nel save: {:?}",
            save_data.game_id
        ));
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
    if matches!(engine.phase(), ttr_core::error::GamePhase::GameOver) {
        return Err("partita già conclusa".to_string());
    }

    let game_id = save_data.game_id.clone();
    let host_token = Uuid::new_v4().to_string();
    let max_players = ruleset.players.max;

    let save_path = state.saves_dir.join(format!("{}.json", game_id));
    save::backup_before_resume(&save_path)?;
    let (tx, rx) = mpsc::channel(256);
    let finished = Arc::new(AtomicBool::new(false));

    let players = save_data
        .players
        .iter()
        .map(|p| {
            Ok(session::SavedPlayer {
                id: p.id.clone(),
                name: p.name.clone(),
                color: save::parse_color(&p.color)?,
                token_hash: p.token_hash.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let session = session::GameSession::new_resumed(
        session::SessionConfig {
            game_id: game_id.clone(),
            game_name: save_data.game_name.clone(),
            ruleset_id: save_data.ruleset_id.clone(),
            map_id: ruleset.map.clone(),
            max_players,
            host_token: host_token.clone(),
            save_path: Some(save_path),
            results_lock: state.results_lock.clone(),
            limits: state.limits,
            finished: finished.clone(),
        },
        rx,
        session::ResumedGame {
            engine,
            map_data: map,
            players,
            created_at: save_data.created_at.clone(),
        },
    );

    let handle = session::GameHandle {
        tx: tx.clone(),
        game_id: game_id.clone(),
        game_name: save_data.game_name,
        ruleset_id: save_data.ruleset_id.clone(),
        max_players,
        players_joined: 0,
        started: false,
        finished,
    };

    {
        let mut games = state.games.write().await;
        games.insert(game_id.clone(), handle.clone());
    }

    spawn_session(state, game_id.clone(), &handle.tx, session);

    Ok((game_id, host_token))
}
