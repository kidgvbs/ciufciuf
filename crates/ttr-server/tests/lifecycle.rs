//! Test del ciclo di vita delle partite: TTL, limiti, shutdown, dati incoerenti.

mod common;

use std::time::Duration;

use common::driver::*;
use common::*;
use futures_util::StreamExt;
use ttr_server::SessionLimits;

/// Limiti con durate brevi: i test non aspettano minuti.
fn fast_limits() -> SessionLimits {
    SessionLimits {
        lobby_idle_ttl: Duration::from_millis(100),
        abandoned_ttl: Duration::from_millis(150),
        ..SessionLimits::default()
    }
}

async fn listed_games(base: &str) -> Vec<serde_json::Value> {
    get_json(base, "/api/games").await.1["games"]
        .as_array()
        .expect("games array")
        .clone()
}

/// Attende che l'elenco delle partite soddisfi `done`, fino a 5 secondi.
async fn wait_for_games(base: &str, done: impl Fn(&[serde_json::Value]) -> bool) -> bool {
    for _ in 0..100 {
        if done(&listed_games(base).await) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn idle_lobby_is_removed() {
    let server = TestServer::start_with_limits(fast_limits()).await;
    create_game(&server.base_url, "Idle").await;
    assert_eq!(listed_games(&server.base_url).await.len(), 1);

    assert!(
        wait_for_games(&server.base_url, |games| games.is_empty()).await,
        "la lobby inattiva deve sparire dall'elenco"
    );
}

#[tokio::test]
async fn active_lobby_is_not_removed() {
    let limits = SessionLimits {
        lobby_idle_ttl: Duration::from_millis(400),
        ..SessionLimits::default()
    };
    let server = TestServer::start_with_limits(limits).await;
    let g = create_game(&server.base_url, "Busy").await;
    // Un join ogni 150 ms tiene viva la lobby oltre il TTL.
    for name in ["Anna", "Bob", "Ciro", "Dora"] {
        tokio::time::sleep(Duration::from_millis(150)).await;
        join_game(&server.base_url, &g.game_id, name, None).await;
    }
    assert_eq!(listed_games(&server.base_url).await.len(), 1);
}

#[tokio::test]
async fn abandoned_game_keeps_save() {
    let server = TestServer::start_with_limits(fast_limits()).await;
    let g = create_game_with_seed(&server.base_url, "Abandoned", Some(1)).await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    assert!(
        wait_for_games(&server.base_url, |games| games.is_empty()).await,
        "la partita senza connessioni deve terminare"
    );
    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    assert!(
        save.exists(),
        "il save di una partita abbandonata resta su disco"
    );
    let bytes = std::fs::read(&save).expect("lettura save");
    serde_json::from_slice::<serde_json::Value>(&bytes).expect("save completo e valido");
}

#[tokio::test]
async fn connected_game_is_not_abandoned() {
    let server = TestServer::start_with_limits(fast_limits()).await;
    let g = create_game_with_seed(&server.base_url, "Connected", Some(1)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let mut ws = WsClient::connect(&server, &g.game_id, &pa.player_id, &pa.token).await;
    ws.recv().await;

    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(
        listed_games(&server.base_url).await.len(),
        1,
        "una connessione tiene viva la partita"
    );
}

#[tokio::test]
async fn too_many_games_returns_429() {
    let limits = SessionLimits {
        max_active_games: 3,
        ..SessionLimits::default()
    };
    let server = TestServer::start_with_limits(limits).await;
    for i in 0..3 {
        create_game(&server.base_url, &format!("G{i}")).await;
    }
    let (status, body) = post_json(
        &server.base_url,
        "/api/games",
        serde_json::json!({ "rulesetId": "usa_v2", "name": "Di troppo" }),
    )
    .await;
    assert_eq!(status, 429, "{body}");
    assert_eq!(body["error"], "TOO_MANY_GAMES");
    assert!(body["detail"].as_str().is_some_and(|d| !d.is_empty()));
    assert_eq!(listed_games(&server.base_url).await.len(), 3);
}

#[tokio::test]
async fn oversized_ws_message_closes_connection() {
    let server = TestServer::start().await;
    let g = create_game(&server.base_url, "Big").await;
    let mut spectator = WsClient::connect_spectator(&server, &g.game_id).await;
    spectator.recv().await;

    spectator.send_raw(&"x".repeat(100 * 1024)).await;

    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match spectator.read.next().await {
                None
                | Some(Err(_))
                | Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                    return true;
                }
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert_eq!(closed, Ok(true), "il server deve chiudere la connessione");
}

#[tokio::test]
async fn too_many_spectators_are_rejected() {
    let limits = SessionLimits {
        max_spectators: 2,
        ..SessionLimits::default()
    };
    let server = TestServer::start_with_limits(limits).await;
    let g = create_game(&server.base_url, "Spectators").await;

    let mut accepted = Vec::new();
    for _ in 0..2 {
        let mut s = WsClient::connect_spectator(&server, &g.game_id).await;
        assert!(matches!(
            s.recv().await,
            ttr_proto::ServerMessage::Snapshot(_)
        ));
        accepted.push(s);
    }
    let mut third = WsClient::connect_spectator(&server, &g.game_id).await;
    match third.recv().await {
        ttr_proto::ServerMessage::Error(e) => {
            assert_eq!(e.reason, ttr_proto::ErrorReason::GameFull);
            assert_eq!(e.detail.as_deref(), Some("TOO_MANY_SPECTATORS"));
        }
        other => panic!("atteso un errore, ricevuto {:?}", other),
    }

    // Un posto liberato torna disponibile.
    accepted.pop().expect("spettatore").disconnect().await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut again = WsClient::connect_spectator(&server, &g.game_id).await;
    assert!(matches!(
        again.recv().await,
        ttr_proto::ServerMessage::Snapshot(_)
    ));
}

/// Se l'archiviazione fallisce il save finale (`completed`) resta su disco, e
/// il resume di un save concluso è rifiutato.
#[tokio::test]
async fn resume_of_completed_save_is_rejected() {
    let server = TestServer::start().await;
    let map = usa_map();
    // `results.jsonl` è una directory: l'archiviazione fallisce.
    std::fs::create_dir(server.saves_dir.join(ttr_server::save::RESULTS_FILE)).unwrap();

    let g = create_game_with_seed(&server.base_url, "Completed", Some(42)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let mut a = Player::new(WsClient::connect(&server, &g.game_id, &pa.player_id, &pa.token).await);
    let mut b = Player::new(WsClient::connect(&server, &g.game_id, &pb.player_id, &pb.token).await);
    let final_public = run_game(&mut a, &mut b, &map).await;
    assert_eq!(final_public.state, ttr_proto::GameState::GameOver);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&save).expect("save finale")).unwrap();
    assert_eq!(saved["status"], "completed");

    let err = ttr_server::resume_game(&server.state, &save)
        .await
        .expect_err("il resume di una partita conclusa va rifiutato");
    assert!(err.contains("già conclusa"), "{err}");
}

/// Alla chiusura le scritture in coda sono su disco e le partite spariscono dal registro.
#[tokio::test]
async fn shutdown_sessions_flushes_pending_save() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "Shutdown", Some(1)).await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    ttr_server::shutdown_sessions(&server.state, Duration::from_secs(5)).await;

    assert!(server.state.games.read().await.is_empty());
    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    assert!(
        save.exists(),
        "il save iniziale è su disco dopo la chiusura"
    );
}

// ── Dati incoerenti ──────────────────────────────────────────────────

fn data_dir_with_ruleset(edit: impl FnOnce(&mut serde_json::Value)) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("maps")).unwrap();
    std::fs::create_dir_all(dir.path().join("rulesets")).unwrap();
    std::fs::copy(
        data_dir().join("maps/usa.json"),
        dir.path().join("maps/usa.json"),
    )
    .unwrap();
    let mut ruleset: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(data_dir().join("rulesets/usa_v2.json")).unwrap(),
    )
    .unwrap();
    edit(&mut ruleset);
    std::fs::write(dir.path().join("rulesets/usa_v2.json"), ruleset.to_string()).unwrap();
    dir
}

fn state_result(data: &std::path::Path) -> Result<(), String> {
    let saves = tempfile::tempdir().expect("tempdir");
    ttr_server::AppState::new(data.to_path_buf(), saves.path().to_path_buf(), 0).map(|_| ())
}

#[test]
fn server_starts_with_the_shipped_data() {
    let dir = data_dir_with_ruleset(|_| {});
    state_result(dir.path()).expect("i dati in data/ sono coerenti");
}

#[test]
fn server_refuses_to_start_with_bad_ruleset() {
    let unsupported_final_round = data_dir_with_ruleset(|r| {
        r["finalRoundIncludesTrigger"] = serde_json::json!(false);
    });
    let err = state_result(unsupported_final_round.path()).expect_err("ruleset non supportato");
    assert!(err.contains("usa_v2"), "{err}");

    let missing_draw_deck = data_dir_with_ruleset(|r| {
        r["ticketDraw"]["deck"] = serde_json::json!("inesistente");
    });
    assert!(
        state_result(missing_draw_deck.path()).is_err(),
        "deck di pesca assente"
    );

    let missing_dealt_deck = data_dir_with_ruleset(|r| {
        r["initialTickets"]["dealt"][0]["deck"] = serde_json::json!("inesistente");
    });
    assert!(
        state_result(missing_dealt_deck.path()).is_err(),
        "deck iniziale assente"
    );
}
