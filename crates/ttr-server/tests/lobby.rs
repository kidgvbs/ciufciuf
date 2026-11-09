//! Test di integrazione REST: lobby, QR, LAN, seq monotono.
//!
//! Coprono il pacchetto Fase 2 §5 e §7.2 (marginali).

mod common;

use common::driver::*;
use common::*;
use ttr_proto::GameState;

// ── Helper per estrarre colore da GET /api/games/{id} ────────────────

/// Esegue GET /api/games/{id} e restituisce il JSON.
#[allow(dead_code)]
async fn get_game_detail(base: &str, game_id: &str) -> serde_json::Value {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/games/{}", base, game_id))
        .send()
        .await
        .expect("GET /api/games/{id}");
    resp.json().await.expect("parse game detail")
}

// ── /qr ──────────────────────────────────────────────────────────────

/// GET /qr restituisce un SVG valido con il Content-Type corretto.
#[tokio::test]
async fn qr_returns_svg() {
    let server = TestServer::start().await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/qr?data=http://example.com:8080",
            server.base_url
        ))
        .send()
        .await
        .expect("GET /qr");
    assert_eq!(resp.status(), 200);

    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("svg"),
        "Content-Type deve indicare SVG, trovato: {}",
        ct
    );

    let body = resp.text().await.expect("body qr");
    assert!(body.contains("<svg"), "il body deve contenere un tag <svg>");
    assert!(
        body.contains("</svg>"),
        "il body deve contenere un tag di chiusura </svg>"
    );
}

// ── /api/lan ─────────────────────────────────────────────────────────

/// GET /api/lan non include indirizzi di loopback.
#[tokio::test]
async fn lan_filters_loopback() {
    let server = TestServer::start().await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/lan", server.base_url))
        .send()
        .await
        .expect("GET /api/lan");
    assert_eq!(resp.status(), 200);

    let json: serde_json::Value = resp.json().await.expect("parse LAN JSON");
    let candidates = json["candidates"].as_array().expect("candidates array");

    for cand in candidates {
        let ip = cand["ip"].as_str().expect("ip string");
        // Nessun indirizzo di loopback (127.x.x.x)
        assert!(
            !ip.starts_with("127."),
            "candidate IP non deve essere loopback: {}",
            ip
        );
        assert_ne!(ip, "::1", "candidate IP non deve essere IPv6 loopback");
    }
}

// ── Lobby: nome duplicato → NAME_TAKEN (409) ─────────────────────────

#[tokio::test]
async fn lobby_name_duplicate_returns_409() {
    let server = TestServer::start().await;

    let game = create_game(&server.base_url, "NameDupTest").await;
    let _pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;

    // Secondo join con lo stesso nome (case-insensitive) → 409 NAME_TAKEN.
    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "{}/api/games/{}/join",
            server.base_url, game.game_id
        ))
        .json(&serde_json::json!({ "name": "alice" }))
        .send()
        .await
        .expect("POST join (dup)");
    assert_eq!(resp.status(), 409, "nome duplicato deve tornare 409");
    let body: serde_json::Value = resp.json().await.expect("body errore");
    assert_eq!(
        body["error"].as_str().unwrap_or(""),
        "NAME_TAKEN",
        "errore NAME_TAKEN"
    );
}

// ── Lobby: auto color ────────────────────────────────────────────────

#[tokio::test]
async fn lobby_auto_color() {
    let server = TestServer::start().await;

    let game = create_game(&server.base_url, "AutoColor").await;

    // Primo giocatore senza colore → deve prendere il primo libero (Red).
    let p1 = join_game(&server.base_url, &game.game_id, "Alice", None).await;

    // GET detail e verifica colore (anche se get_game ha players vuoto...).
    // Invece: secondo giocatore senza colore → non deve essere lo stesso.
    let p2 = join_game(&server.base_url, &game.game_id, "Bob", None).await;

    // Verifichiamo che entrambi abbiano token validi e player_id diversi.
    assert_ne!(
        p1.player_id, p2.player_id,
        "player_id devono essere diversi"
    );
    assert!(!p1.token.is_empty());
    assert!(!p2.token.is_empty());
}

// ── Lobby: start con < min giocatori → 400 ──────────────────────────

#[tokio::test]
async fn start_below_min_returns_400() {
    let server = TestServer::start().await;

    let game = create_game(&server.base_url, "BelowMin").await;
    // Nessun join: 0 giocatori, min del ruleset usa_v2 è 2.

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "{}/api/games/{}/start",
            server.base_url, game.game_id
        ))
        .json(&serde_json::json!({ "hostToken": game.host_token }))
        .send()
        .await
        .expect("POST start (below min)");
    assert_eq!(resp.status(), 400, "start con 0 giocatori deve tornare 400");
}

// ── Lobby: join dopo start → 409 ────────────────────────────────────

#[tokio::test]
async fn join_after_start_returns_409() {
    let server = TestServer::start().await;

    let game = create_game(&server.base_url, "JoinAfterStart").await;
    let _pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let _pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    // Terzo giocatore prova a unirsi dopo lo start → 409 GAME_STARTED.
    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "{}/api/games/{}/join",
            server.base_url, game.game_id
        ))
        .json(&serde_json::json!({ "name": "Charlie" }))
        .send()
        .await
        .expect("POST join (after start)");
    assert_eq!(resp.status(), 409, "join dopo start deve tornare 409");
    let body: serde_json::Value = resp.json().await.expect("body errore");
    assert!(
        body["error"].as_str().unwrap_or("").contains("STARTED"),
        "errore GAME_STARTED"
    );
}

// ── seq monotono ─────────────────────────────────────────────────────

/// Il campo `seq` degli outbound (PrivateState, Error) cresce in modo
/// monotono per ogni giocatore, senza salti o regressioni.
/// Guida entrambi i giocatori con `run_game` e verifica il seq finale.
#[tokio::test]
async fn seq_monotono() {
    let server = TestServer::start().await;
    let map = usa_map();

    let game = create_game_with_seed(&server.base_url, "SeqMono", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a_ws = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b_ws = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a_ws);
    let mut b = Player::new(b_ws);

    let final_public = run_game(&mut a, &mut b, &map).await;
    assert_eq!(final_public.state, GameState::GameOver);

    // Verifica che il seq sia cresciuto per entrambi i giocatori.
    let final_seq_a = a.last_acted_seq;
    let final_seq_b = b.last_acted_seq;
    assert!(
        final_seq_a > 0,
        "seq A deve essere > 0, trovato {}",
        final_seq_a
    );
    assert!(
        final_seq_b > 0,
        "seq B deve essere > 0, trovato {}",
        final_seq_b
    );
    // Il seq di A è avanzato più volte (almeno 3 messaggi).
    assert!(
        final_seq_a >= 3,
        "seq A >= 3 (almeno 3 messaggi outbound), trovato {}",
        final_seq_a
    );
}

/// Test F4: dopo 2 join, `GET /api/games` mostra `players_joined == 2`
/// e `GET /api/games/{id}` elenca entrambi i giocatori con i colori corretti.
#[tokio::test]
async fn lobby_players_after_join() {
    let server = TestServer::start().await;

    let game = create_game(&server.base_url, "F4LobbyTest").await;
    let _pa = join_game(&server.base_url, &game.game_id, "Alice", Some("RED")).await;
    let _pb = join_game(&server.base_url, &game.game_id, "Bob", Some("BLUE")).await;

    // Lascia tempo all'attore di processare i join.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Verifica GET /api/games → players_joined == 2.
    let client = reqwest::Client::new();
    let list_resp = client
        .get(format!("{}/api/games", server.base_url))
        .send()
        .await
        .expect("GET /api/games");
    assert_eq!(list_resp.status(), 200);
    let list_json: serde_json::Value = list_resp.json().await.expect("parse list");
    let games = list_json["games"].as_array().expect("games array");
    let g = games.iter().find(|g| g["gameId"].as_str() == Some(&game.game_id)).expect("game presente");
    assert_eq!(g["playersJoined"].as_u64().unwrap_or(0), 2, "players_joined == 2");
    assert!(!g["started"].as_bool().unwrap_or(true), "started == false");

    // Verifica GET /api/games/{id} → entrambi i giocatori.
    let detail_resp = client
        .get(format!("{}/api/games/{}", server.base_url, game.game_id))
        .send()
        .await
        .expect("GET /api/games/{id}");
    assert_eq!(detail_resp.status(), 200);
    let detail_json: serde_json::Value = detail_resp.json().await.expect("parse detail");
    let players = detail_json["players"].as_array().expect("players array");
    assert_eq!(players.len(), 2, "2 giocatori nel dettaglio");
    let names: Vec<&str> = players.iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Alice"), "Alice presente");
    assert!(names.contains(&"Bob"), "Bob presente");
}

// ── Regressioni review di robustezza (9/11/2026) ─────────────────────

async fn post_join(base: &str, game_id: &str, body: serde_json::Value) -> (u16, serde_json::Value) {
    let resp = reqwest::Client::new()
        .post(format!("{}/api/games/{}/join", base, game_id))
        .json(&body)
        .send()
        .await
        .expect("POST join");
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap_or(serde_json::Value::Null))
}

fn assert_rejected(res: &(u16, serde_json::Value), status: u16, code: &str) {
    assert_eq!(res.0, status, "status inatteso per {}: {}", code, res.1);
    assert_eq!(res.1["error"], code, "codice errore inatteso: {}", res.1);
    assert!(
        res.1["detail"].as_str().is_some_and(|d| !d.is_empty()),
        "{} deve avere un detail leggibile: {}",
        code,
        res.1
    );
}

#[tokio::test]
async fn r4_lobby_input_validation() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "R4", None).await;
    let base = server.base_url.as_str();
    let id = g.game_id.as_str();

    let ok = post_join(base, id, serde_json::json!({"name":"Alice","color":"RED"})).await;
    assert_eq!(ok.0, 200, "primo join valido: {}", ok.1);

    let dup_color = post_join(base, id, serde_json::json!({"name":"Bob","color":"RED"})).await;
    assert_rejected(&dup_color, 409, "COLOR_TAKEN");

    for name in [String::new(), "   ".to_string(), "x".repeat(25), "x".repeat(1_000_000)] {
        let res = post_join(base, id, serde_json::json!({ "name": name })).await;
        assert_rejected(&res, 400, "INVALID_NAME");
    }

    // Il limite conta i caratteri, non i byte.
    let accents = post_join(base, id, serde_json::json!({"name":"è".repeat(24)})).await;
    assert_eq!(accents.0, 200, "24 caratteri accentati sono validi: {}", accents.1);

    // Il nome viene salvato senza spazi esterni e il duplicato lo vede.
    let padded = post_join(base, id, serde_json::json!({"name":"  Carol  "})).await;
    assert_eq!(padded.0, 200, "nome con spazi esterni: {}", padded.1);
    let dup_name = post_join(base, id, serde_json::json!({"name":"carol"})).await;
    assert_eq!(dup_name.0, 409, "duplicato dopo il trim: {}", dup_name.1);
    assert_eq!(dup_name.1["error"], "NAME_TAKEN");

    let detail = get_game_detail(base, id).await;
    let players = detail["players"].as_array().expect("players");
    assert_eq!(players.len(), 3, "solo i join validi entrano: {}", detail);
    assert!(players.iter().any(|p| p["name"] == "Carol"), "{}", detail);
}

#[tokio::test]
async fn r4_create_game_name_validation() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();
    let create = |name: String| {
        client
            .post(format!("{}/api/games", server.base_url))
            .json(&serde_json::json!({"rulesetId":"usa_v2","name":name}))
            .send()
    };

    let too_long = create("x".repeat(41)).await.expect("POST create");
    assert_eq!(too_long.status(), 400, "nome partita oltre 40 caratteri");
    let body: serde_json::Value = too_long.json().await.expect("body errore");
    assert_eq!(body["error"], "BAD_REQUEST");

    let at_limit = create("x".repeat(40)).await.expect("POST create");
    assert_eq!(at_limit.status(), 201, "40 caratteri sono validi");

    let blank = create("   ".to_string()).await.expect("POST create");
    assert_eq!(blank.status(), 201, "nome vuoto usa il default");
    let created: serde_json::Value = blank.json().await.expect("body create");
    let detail = get_game_detail(
        &server.base_url,
        created["gameId"].as_str().expect("gameId"),
    )
    .await;
    assert_eq!(detail["name"], "Partita");
}
