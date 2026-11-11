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

pub use error::ResumeError;
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
    use axum::routing::{any, get, post};

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
        .route("/api/rulesets/{id}/layout", get(lobby::ruleset_layout))
        .route("/api/standings", get(lobby::standings))
        .route("/api/profiles", get(lobby::list_profiles))
        .route("/api/saves", get(lobby::list_saves))
        .route("/api/saves/{id}/resume", post(lobby::resume_save))
        .route("/ws/{game_id}", get(ws::ws_handler))
        // I percorsi `/api` senza endpoint rispondono 404 JSON invece di cadere nel fallback SPA.
        .route("/api", any(lobby::api_not_found))
        .route("/api/", any(lobby::api_not_found))
        .route("/api/{*rest}", any(lobby::api_not_found));

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

/// Partita ripristinata: dati per la risposta REST e per il log dell'operatore.
pub(crate) struct ResumeOutcome {
    pub game_id: String,
    pub host_token: String,
    pub ruleset_id: String,
    pub max_players: u32,
}

/// Ripristina una partita da un file di salvataggio.
///
/// Restituisce `(game_id, host_token)` se il resume ha successo.
/// La partita viene registrata in lobby col roster salvato; i giocatori
/// si riagganciano per nome e token originale via `POST /api/games/{id}/join`,
/// oppure ricevono un token nuovo dall'host via `POST /api/games/{id}/reassign`.
/// Un save di una partita già conclusa o già attiva è rifiutato.
pub async fn resume_game(
    state: &AppState,
    save_path: &Path,
) -> Result<(String, String), ResumeError> {
    let outcome = resume_from_file(state, save_path, None).await?;
    Ok((outcome.game_id, outcome.host_token))
}

/// Resume di `{saves_dir}/{game_id}.json` per `POST /api/saves/{id}/resume`.
pub(crate) async fn resume_saved_game(
    state: &AppState,
    game_id: &str,
) -> Result<ResumeOutcome, ResumeError> {
    if !save::is_canonical_uuid(game_id) {
        return Err(ResumeError::NotFound);
    }
    // Risposta rapida senza replay; il controllo che vale è quello sotto lock.
    if state.games.read().await.contains_key(game_id) {
        return Err(ResumeError::AlreadyActive);
    }
    let path = state.saves_dir.join(format!("{game_id}.json"));
    resume_from_file(state, &path, Some(game_id)).await
}

/// Replay e registrazione in lobby: lettura e replay avvengono fuori dal
/// runtime e prima del lock; controlli, backup e inserimento stanno in una sola
/// sezione critica, quindi di due resume concorrenti dello stesso save vince uno.
async fn resume_from_file(
    state: &AppState,
    save_path: &Path,
    expected_id: Option<&str>,
) -> Result<ResumeOutcome, ResumeError> {
    let prepare_state = state.clone();
    let path = save_path.to_path_buf();
    let expected = expected_id.map(str::to_owned);
    let prepared = tokio::task::spawn_blocking(move || {
        prepare_resume(&prepare_state, &path, expected.as_deref())
    })
    .await
    .map_err(|e| ResumeError::Io(format!("replay interrotto: {e}")))??;
    register_resumed(state, prepared).await
}

/// Save letto, validato e riprodotto, pronto per essere registrato.
struct PreparedResume {
    save: save::SaveData,
    ruleset: ttr_core::Ruleset,
    map: ttr_core::MapData,
    engine: ttr_core::Engine,
    players: Vec<session::SavedPlayer>,
}

fn prepare_resume(
    state: &AppState,
    save_path: &Path,
    expected_id: Option<&str>,
) -> Result<PreparedResume, ResumeError> {
    use ResumeError::NotResumable;

    if !save_path.is_file() {
        return Err(ResumeError::NotFound);
    }
    let save = save::read_save(save_path).map_err(ResumeError::Unreadable)?;

    // `game_id` finisce in un percorso su disco: solo la forma canonica è ammessa.
    if !save::is_canonical_uuid(&save.game_id) {
        return Err(NotResumable(format!(
            "gameId non valido nel save: {:?}",
            save.game_id
        )));
    }
    if expected_id.is_some_and(|id| id != save.game_id) {
        return Err(NotResumable(
            "il gameId nel save non corrisponde al nome del file".to_string(),
        ));
    }

    let ruleset = state
        .ruleset(&save.ruleset_id)
        .ok_or_else(|| NotResumable(format!("ruleset '{}' sconosciuto", save.ruleset_id)))?
        .clone();
    let map = state
        .map(ruleset.map.as_str())
        .ok_or_else(|| NotResumable(format!("mappa '{}' sconosciuta", ruleset.map)))?
        .clone();

    let engine = save::replay_engine(&save, &map, &ruleset).map_err(NotResumable)?;
    if matches!(engine.phase(), ttr_core::error::GamePhase::GameOver) {
        return Err(NotResumable("partita già conclusa".to_string()));
    }

    let players = save
        .players
        .iter()
        .map(|p| {
            Ok(session::SavedPlayer {
                id: p.id.clone(),
                name: p.name.clone(),
                color: save::parse_color(&p.color).map_err(NotResumable)?,
                token_hash: p.token_hash.clone(),
            })
        })
        .collect::<Result<Vec<_>, ResumeError>>()?;

    Ok(PreparedResume {
        save,
        ruleset,
        map,
        engine,
        players,
    })
}

async fn register_resumed(
    state: &AppState,
    prepared: PreparedResume,
) -> Result<ResumeOutcome, ResumeError> {
    use tokio::sync::mpsc;
    use uuid::Uuid;

    let PreparedResume {
        save: save_data,
        ruleset,
        map,
        engine,
        players,
    } = prepared;

    let game_id = save_data.game_id.clone();
    let host_token = Uuid::new_v4().to_string();
    let max_players = ruleset.players.max;

    let save_path = state.saves_dir.join(format!("{}.json", game_id));
    let (tx, rx) = mpsc::channel(256);
    let finished = Arc::new(AtomicBool::new(false));

    let session = session::GameSession::new_resumed(
        session::SessionConfig {
            game_id: game_id.clone(),
            game_name: save_data.game_name.clone(),
            ruleset_id: save_data.ruleset_id.clone(),
            map_id: ruleset.map.clone(),
            max_players,
            host_token: host_token.clone(),
            save_path: Some(save_path.clone()),
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

    // Come in `create_game`: i controlli e l'inserimento stanno nella stessa
    // sezione critica, così lo shutdown vede la partita inserita. Il backup è
    // fatto qui, dopo i controlli, quindi solo dal resume che vince; costa una
    // copia di file con il lock tenuto.
    {
        let mut games = state.games.write().await;
        if state.shutting_down.load(Ordering::Acquire) {
            return Err(ResumeError::ShuttingDown);
        }
        if games.contains_key(&game_id) {
            return Err(ResumeError::AlreadyActive);
        }
        let active = games
            .values()
            .filter(|h| !h.finished.load(Ordering::Acquire))
            .count();
        if active >= state.limits.max_active_games {
            return Err(ResumeError::TooManyGames);
        }
        tokio::task::spawn_blocking(move || save::backup_before_resume(&save_path))
            .await
            .map_err(|e| ResumeError::Io(format!("backup interrotto: {e}")))?
            .map_err(ResumeError::Io)?;
        games.insert(game_id.clone(), handle.clone());
    }

    spawn_session(state, game_id.clone(), &handle.tx, session);

    Ok(ResumeOutcome {
        game_id,
        host_token,
        ruleset_id: handle.ruleset_id,
        max_players,
    })
}
