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

/// Come `fast_limits`, ma la lobby non scade durante i join dei test che
/// verificano solo l'abbandono di una partita avviata.
fn abandon_limits(abandoned_ttl: Duration) -> SessionLimits {
    SessionLimits {
        abandoned_ttl,
        ..SessionLimits::default()
    }
}

async fn listed_games(base: &str) -> Vec<serde_json::Value> {
    get_json(base, "/api/games").await.1["games"]
        .as_array()
        .expect("games array")
        .clone()
}

/// Attende che l'elenco delle partite soddisfi `done`.
async fn wait_for_games(base: &str, done: impl Fn(&[serde_json::Value]) -> bool) {
    wait_for_async(WAIT, || async {
        done(&listed_games(base).await).then_some(())
    })
    .await;
}

#[tokio::test]
async fn idle_lobby_is_removed() {
    let server = TestServer::start_with_limits(fast_limits()).await;
    create_game(&server.base_url, "Idle").await;
    assert_eq!(listed_games(&server.base_url).await.len(), 1);

    // La lobby inattiva deve sparire dall'elenco.
    wait_for_games(&server.base_url, |games| games.is_empty()).await;
}

#[tokio::test]
async fn active_lobby_is_not_removed() {
    let limits = SessionLimits {
        lobby_idle_ttl: Duration::from_millis(400),
        ..SessionLimits::default()
    };
    let server = TestServer::start_with_limits(limits).await;
    let g = create_game(&server.base_url, "Busy").await;
    // Un join ogni 150 ms tiene viva la lobby oltre il TTL; lo sleep fa
    // trascorrere il tempo, non sincronizza nulla.
    for name in ["Anna", "Bob", "Ciro", "Dora"] {
        tokio::time::sleep(Duration::from_millis(150)).await;
        join_game(&server.base_url, &g.game_id, name, None).await;
    }
    assert_eq!(listed_games(&server.base_url).await.len(), 1);
}

#[tokio::test]
async fn abandoned_game_keeps_save() {
    let server = TestServer::start_with_limits(abandon_limits(Duration::from_millis(300))).await;
    let g = create_game_with_seed(&server.base_url, "Abandoned", Some(1)).await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    // La partita senza connessioni deve terminare.
    wait_for_games(&server.base_url, |games| games.is_empty()).await;
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
    let server = TestServer::start_with_limits(abandon_limits(Duration::from_millis(300))).await;
    let g = create_game_with_seed(&server.base_url, "Connected", Some(1)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let mut ws = WsClient::connect(&server, &g.game_id, &pa.player_id, &pa.token).await;
    ws.recv().await;

    // Verifica che qualcosa non accada: il tempo deve superare `abandoned_ttl`.
    tokio::time::sleep(Duration::from_millis(900)).await;
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
    // Il posto torna disponibile quando l'attore processa il `Disconnect`.
    wait_for_async(WAIT, || async {
        let mut again = WsClient::connect_spectator(&server, &g.game_id).await;
        matches!(again.recv().await, ttr_proto::ServerMessage::Snapshot(_)).then_some(())
    })
    .await;
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

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_save(&save, |s| s["status"] == "completed").await;

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

/// Uno spettatore che si riconnette di continuo non tiene viva una lobby vuota.
#[tokio::test]
async fn anonymous_spectators_do_not_keep_lobby_alive() {
    let limits = SessionLimits {
        empty_lobby_ttl: Duration::from_millis(300),
        ..SessionLimits::default()
    };
    let server = TestServer::start_with_limits(limits).await;
    let g = create_game(&server.base_url, "Vuota").await;

    let started = tokio::time::Instant::now();
    wait_for_async(Duration::from_secs(5), || async {
        let mut spectator = WsClient::connect_spectator(&server, &g.game_id).await;
        let _ = spectator.recv().await;
        // Un passo di polling ogni ~100 ms, come lo spettatore del test.
        tokio::time::sleep(Duration::from_millis(80)).await;
        listed_games(&server.base_url)
            .await
            .is_empty()
            .then_some(())
    })
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "la lobby vuota doveva scadere entro il TTL, ci ha messo {:?}",
        started.elapsed()
    );
}

/// Una lobby con un giocatore resta governata da `lobby_idle_ttl`, non da `empty_lobby_ttl`.
#[tokio::test]
async fn lobby_with_a_player_ignores_the_empty_ttl() {
    let limits = SessionLimits {
        empty_lobby_ttl: Duration::from_millis(100),
        ..SessionLimits::default()
    };
    let server = TestServer::start_with_limits(limits).await;
    let g = create_game(&server.base_url, "Con giocatore").await;
    join_game(&server.base_url, &g.game_id, "Anna", None).await;

    // Verifica che qualcosa non accada: oltre `empty_lobby_ttl` la lobby resta.
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(listed_games(&server.base_url).await.len(), 1);
}

#[tokio::test]
async fn finished_games_do_not_count_toward_limit() {
    let limits = SessionLimits {
        max_active_games: 1,
        ..SessionLimits::default()
    };
    let server = TestServer::start_with_limits(limits).await;
    let map = usa_map();

    let g = create_game_with_seed(&server.base_url, "Finita", Some(42)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    let (status, _) = post_json(
        &server.base_url,
        "/api/games",
        serde_json::json!({ "rulesetId": "usa_v2" }),
    )
    .await;
    assert_eq!(status, 429, "con una partita in corso il limite vale");

    let mut a = Player::new(WsClient::connect(&server, &g.game_id, &pa.player_id, &pa.token).await);
    let mut b = Player::new(WsClient::connect(&server, &g.game_id, &pb.player_id, &pb.token).await);
    let final_public = run_game(&mut a, &mut b, &map).await;
    assert_eq!(final_public.state, ttr_proto::GameState::GameOver);

    let (status, body) = post_json(
        &server.base_url,
        "/api/games",
        serde_json::json!({ "rulesetId": "usa_v2" }),
    )
    .await;
    assert_eq!(status, 201, "{body}");
}

#[tokio::test]
async fn delete_game_with_host_token() {
    let server = TestServer::start().await;
    let g = create_game(&server.base_url, "Da cancellare").await;
    let path = format!("/api/games/{}", g.game_id);

    let wrong = delete_json(
        &server.base_url,
        &path,
        serde_json::json!({ "hostToken": "sbagliato" }),
    )
    .await;
    assert_eq!(wrong, 401);
    assert_eq!(get_json(&server.base_url, &path).await.0, 200);

    let ok = delete_json(
        &server.base_url,
        &path,
        serde_json::json!({ "hostToken": g.host_token }),
    )
    .await;
    assert_eq!(ok, 204);
    assert_eq!(get_json(&server.base_url, &path).await.0, 404);

    let again = delete_json(
        &server.base_url,
        &path,
        serde_json::json!({ "hostToken": g.host_token }),
    )
    .await;
    assert_eq!(again, 404);
}

#[tokio::test]
async fn fifth_connection_evicts_oldest() {
    let server = TestServer::start().await;
    let g = create_game(&server.base_url, "Connessioni").await;
    let alice = join_game(&server.base_url, &g.game_id, "Alice", None).await;

    let mut clients = Vec::new();
    for _ in 0..4 {
        let mut c = WsClient::connect(&server, &g.game_id, &alice.player_id, &alice.token).await;
        c.recv().await;
        clients.push(c);
    }
    let mut fifth = WsClient::connect(&server, &g.game_id, &alice.player_id, &alice.token).await;
    assert!(matches!(
        fifth.recv().await,
        ttr_proto::ServerMessage::Snapshot(_)
    ));

    let mut oldest = clients.remove(0);
    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match oldest.read.next().await {
                None
                | Some(Err(_))
                | Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => return true,
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert_eq!(closed, Ok(true), "la connessione più vecchia va chiusa");
}

#[tokio::test]
async fn silent_client_is_disconnected() {
    let limits = SessionLimits {
        ws_ping_interval: Duration::from_millis(200),
        ws_idle_timeout: Duration::from_millis(600),
        ..SessionLimits::default()
    };
    let server = TestServer::start_with_limits(limits).await;
    let g = create_game(&server.base_url, "Ping").await;
    let alice = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let bob = join_game(&server.base_url, &g.game_id, "Bob", None).await;

    // Alice non legge mai dal socket, quindi non risponde ai ping.
    let _silent = WsClient::connect(&server, &g.game_id, &alice.player_id, &alice.token).await;
    // Bob legge di continuo: la sua libreria risponde ai ping.
    let mut bob_ws = WsClient::connect(&server, &g.game_id, &bob.player_id, &bob.token).await;

    let alice_offline = tokio::time::timeout(Duration::from_secs(10), async {
        let mut alice_seen_online = false;
        loop {
            let public = match bob_ws.recv().await {
                ttr_proto::ServerMessage::Snapshot(s) => s.public,
                ttr_proto::ServerMessage::PublicState(p) => p,
                _ => continue,
            };
            let alice_connected = public
                .players_summary
                .iter()
                .find(|p| p.player_id == alice.player_id)
                .is_some_and(|p| p.connected);
            if alice_connected {
                alice_seen_online = true;
            } else if alice_seen_online {
                return true;
            }
        }
    })
    .await;
    assert_eq!(alice_offline, Ok(true), "il client muto va scollegato");
}

#[tokio::test]
async fn shutdown_completes_with_game_created_during_drain() {
    let limits = SessionLimits {
        max_active_games: 10_000,
        ..SessionLimits::default()
    };
    let server = TestServer::start_with_limits(limits).await;
    let first = create_game(&server.base_url, "Prima").await;

    let state = server.state.clone();
    let shutdown = tokio::spawn(async move {
        ttr_server::shutdown_sessions(&state, Duration::from_secs(10)).await;
    });
    // Le creazioni concorrenti allo shutdown o riescono (e vengono fermate dal
    // giro successivo di `Shutdown`) o ricevono 503.
    let mut refused = None;
    for _ in 0..10_000 {
        let (status, body) = post_json(
            &server.base_url,
            "/api/games",
            serde_json::json!({ "rulesetId": "usa_v2" }),
        )
        .await;
        match status {
            201 => {}
            503 => {
                assert_eq!(body["error"], "SHUTTING_DOWN");
                refused = Some(());
                break;
            }
            other => panic!("stato inatteso {other}: {body}"),
        }
    }
    refused.expect("il server doveva rifiutare le creazioni durante lo shutdown");

    tokio::time::timeout(Duration::from_secs(10), shutdown)
        .await
        .expect("lo shutdown termina")
        .expect("task di shutdown");
    assert!(server.state.games.read().await.is_empty());

    let (status, body) = post_json(
        &server.base_url,
        &format!("/api/games/{}/join", first.game_id),
        serde_json::json!({ "name": "Tardiva" }),
    )
    .await;
    assert_eq!(status, 503, "{body}");
    let ws = format!("{}?playerId=x&token=y", server.ws_url(&first.game_id));
    assert!(
        tokio_tungstenite::connect_async(&ws).await.is_err(),
        "l'upgrade WebSocket riceve 503 durante lo shutdown"
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
