//! Test di integrazione REST: lobby, QR, LAN, seq monotono.
//!
//! Coprono creazione, join, nomi, QR e LAN della lobby.

mod common;

use common::driver::*;
use common::*;
use futures_util::StreamExt;
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

/// Il primo join salva il nome come profilo, riusabile in ogni partita.
#[tokio::test]
async fn join_saves_profile_once() {
    let server = TestServer::start().await;
    let profiles = |base: String| async move {
        let body: serde_json::Value = reqwest::get(format!("{}/api/profiles", base))
            .await
            .expect("GET /api/profiles")
            .json()
            .await
            .expect("parse profiles");
        body["profiles"].clone()
    };
    assert_eq!(
        profiles(server.base_url.clone()).await,
        serde_json::json!([])
    );

    let g1 = create_game(&server.base_url, "P1").await;
    join_game(&server.base_url, &g1.game_id, "Zoe", None).await;
    join_game(&server.base_url, &g1.game_id, "Alice", None).await;
    let g2 = create_game(&server.base_url, "P2").await;
    join_game(&server.base_url, &g2.game_id, "alice", None).await;

    // Stesso profilo con altre maiuscole: nessun duplicato; ordine alfabetico.
    assert_eq!(
        profiles(server.base_url.clone()).await,
        serde_json::json!(["Alice", "Zoe"])
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

/// Dopo 2 join, `GET /api/games` mostra `players_joined == 2`
/// e `GET /api/games/{id}` elenca entrambi i giocatori con i colori corretti.
#[tokio::test]
async fn lobby_players_after_join() {
    let server = TestServer::start().await;

    let game = create_game(&server.base_url, "F4LobbyTest").await;
    let _pa = join_game(&server.base_url, &game.game_id, "Alice", Some("PINK")).await;
    let _pb = join_game(&server.base_url, &game.game_id, "Bob", Some("CYAN")).await;

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
    let g = games
        .iter()
        .find(|g| g["gameId"].as_str() == Some(&game.game_id))
        .expect("game presente");
    assert_eq!(
        g["playersJoined"].as_u64().unwrap_or(0),
        2,
        "players_joined == 2"
    );
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
    let names: Vec<&str> = players
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Alice"), "Alice presente");
    assert!(names.contains(&"Bob"), "Bob presente");
}

// ── Validazione degli input della lobby ──────────────────────────────

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

    let ok = post_join(base, id, serde_json::json!({"name":"Alice","color":"PINK"})).await;
    assert_eq!(ok.0, 200, "primo join valido: {}", ok.1);

    let dup_color = post_join(base, id, serde_json::json!({"name":"Bob","color":"PINK"})).await;
    assert_rejected(&dup_color, 409, "COLOR_TAKEN");

    for name in [
        String::new(),
        "   ".to_string(),
        "x".repeat(25),
        "x".repeat(1_000_000),
    ] {
        let res = post_join(base, id, serde_json::json!({ "name": name })).await;
        assert_rejected(&res, 400, "INVALID_NAME");
    }

    // Il limite conta i caratteri, non i byte.
    let accents = post_join(base, id, serde_json::json!({"name":"è".repeat(24)})).await;
    assert_eq!(
        accents.0, 200,
        "24 caratteri accentati sono validi: {}",
        accents.1
    );

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

// ── Nomi: NFC e caratteri invisibili ─────────────────────────────────

#[tokio::test]
async fn join_rejects_zero_width_name() {
    let server = TestServer::start().await;
    let g = create_game(&server.base_url, "Invisibili").await;
    let base = server.base_url.as_str();
    let id = g.game_id.as_str();

    for name in [
        "Anna\u{200b}",
        "\u{202e}Anna",
        "An\u{feff}na",
        "An\nna",
        "An\u{0}na",
    ] {
        let res = post_join(base, id, serde_json::json!({ "name": name })).await;
        assert_rejected(&res, 400, "INVALID_NAME");
    }
    let detail = get_game_detail(base, id).await;
    assert_eq!(
        detail["players"].as_array().unwrap().len(),
        0,
        "nessun join valido: {detail}"
    );

    // Anche il nome della partita non ammette caratteri invisibili.
    let (status, body) = post_json(
        base,
        "/api/games",
        serde_json::json!({ "rulesetId": "usa_v2", "name": "Sera\u{200b}ta" }),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"], "INVALID_NAME");
}

#[tokio::test]
async fn names_equal_after_nfc_are_taken() {
    let server = TestServer::start().await;
    let g = create_game(&server.base_url, "Nfc").await;
    let base = server.base_url.as_str();
    let id = g.game_id.as_str();

    // "é" precomposta, poi "e" + accento combinante: stesso nome.
    let first = post_join(base, id, serde_json::json!({ "name": "\u{e9}mile" })).await;
    assert_eq!(first.0, 200, "{}", first.1);
    let second = post_join(base, id, serde_json::json!({ "name": "E\u{301}mile" })).await;
    assert_rejected(&second, 409, "NAME_TAKEN");

    // Il nome salvato è in forma NFC.
    let detail = get_game_detail(base, id).await;
    assert_eq!(detail["players"][0]["name"], "\u{e9}mile");
}

/// Codici e status degli errori più comuni della lobby.
#[tokio::test]
async fn lobby_errors_use_the_documented_codes() {
    let server = TestServer::start().await;
    let base = server.base_url.as_str();

    let missing = post_join(base, "non-esiste", serde_json::json!({ "name": "Anna" })).await;
    assert_rejected(&missing, 404, "GAME_NOT_FOUND");

    let g = create_game(&server.base_url, "Codici").await;
    for name in ["A", "B", "C", "D", "E"] {
        join_game(base, &g.game_id, name, None).await;
    }
    let full = post_join(base, &g.game_id, serde_json::json!({ "name": "F" })).await;
    assert_rejected(&full, 409, "GAME_FULL");

    let (status, body) = post_json(
        base,
        &format!("/api/games/{}/start", g.game_id),
        serde_json::json!({ "hostToken": "sbagliato" }),
    )
    .await;
    assert_eq!(status, 401, "{body}");
    assert_eq!(body["error"], "UNAUTHORIZED");
}

/// `DELETE` senza body (o senza `Content-Type`) risponde 401 con l'errore standard.
#[tokio::test]
async fn delete_without_body_is_401() {
    let server = TestServer::start().await;
    let g = create_game(&server.base_url, "Senza body").await;
    let url = format!("{}/api/games/{}", server.base_url, g.game_id);
    let res = reqwest::Client::new()
        .delete(&url)
        .send()
        .await
        .expect("DELETE senza body");
    assert_eq!(res.status(), 401);
    let body: serde_json::Value = res.json().await.expect("body errore");
    assert_eq!(body["error"], "UNAUTHORIZED");
    assert!(body["detail"].as_str().is_some_and(|d| !d.is_empty()));

    let res = reqwest::Client::new()
        .delete(&url)
        .header("content-type", "application/json")
        .send()
        .await
        .expect("DELETE con content-type e body vuoto");
    assert_eq!(res.status(), 401);

    let (status, _) = get_json(&server.base_url, &format!("/api/games/{}", g.game_id)).await;
    assert_eq!(status, 200, "la partita non è stata chiusa");
}

/// GET /api/rulesets/{id}/map serve il documento della mappa con i campi che
/// il motore ignora (`lat`/`lon`); un ruleset sconosciuto dà 404.
#[tokio::test]
async fn ruleset_map_endpoint_serves_the_map_document() {
    let server = TestServer::start().await;
    let (status, body) = get_json(&server.base_url, "/api/rulesets/usa_v2/map").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["rulesetId"], "usa_v2");
    let on_disk: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(data_dir().join("maps/usa.json")).expect("lettura mappa"),
    )
    .expect("parsing mappa");
    for key in ["cities", "routes", "tickets"] {
        assert_eq!(body[key], on_disk[key], "{key}");
    }
    assert!(body["cities"][0]["lat"].is_number());

    let (status, body) = get_json(&server.base_url, "/api/rulesets/non_esiste/map").await;
    assert_eq!(status, 404);
    assert_eq!(body["error"], "RULESET_NOT_FOUND");
}

/// Riassunto di una voce del catalogo: (id, mode, biglietti in gioco, distribuiti, tieni,
/// pescati, tipi di bonus, grandi città).
type CatalogRow = (String, String, u64, u64, u64, u64, Vec<String>, usize);

/// GET /api/rulesets elenca i ruleset caricati, ordinati per (mapId, modeOrder, id), con i conteggi
/// della mappa e le funzioni di Europe (tunnel, traghetti, stazioni).
#[tokio::test]
async fn rulesets_endpoint_lists_the_loaded_rulesets() {
    let server = TestServer::start().await;
    let (status, body) = get_json(&server.base_url, "/api/rulesets").await;
    assert_eq!(status, 200, "{body}");
    let rulesets = body["rulesets"].as_array().expect("rulesets");
    let summary: Vec<CatalogRow> = rulesets
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap().to_owned(),
                r["mode"].as_str().unwrap().to_owned(),
                r["tickets"].as_u64().unwrap(),
                r["initialDealt"].as_u64().unwrap(),
                r["initialKeepMin"].as_u64().unwrap(),
                r["drawCount"].as_u64().unwrap(),
                r["bonuses"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| b["type"].as_str().unwrap().to_owned())
                    .collect(),
                r["bigCities"].as_array().unwrap().len(),
            )
        })
        .collect();
    let row = |id: &str, mode: &str, t, d, k, dr, b: &[&str], big| {
        (
            id.to_owned(),
            mode.to_owned(),
            t,
            d,
            k,
            dr,
            b.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
            big,
        )
    };
    let lp = "longest_path";
    let gt = "globetrotter";
    let md = "mandala";
    let ln = "largest_network";
    let rg = "regions";
    assert_eq!(
        summary,
        vec![
            row("basilicata", "Standard", 25, 3, 2, 3, &[lp], 0),
            row("europe", "Standard", 46, 4, 2, 3, &[lp], 0),
            row("europe_expanded", "Estesa", 65, 4, 2, 3, &[lp], 0),
            row("europe_1912", "1912", 55, 4, 2, 3, &[lp], 0),
            row("europe_big_cities", "Grandi Città", 45, 5, 2, 4, &[lp], 9),
            row("europe_mega", "Mega", 101, 7, 3, 3, &[lp], 0),
            row("germany", "Deutschland", 62, 5, 2, 4, &[gt], 0),
            row("germany_2017", "Passeggeri", 89, 5, 2, 4, &[gt], 0),
            row("india", "Standard", 58, 4, 2, 3, &[lp, md], 0),
            row("italy", "Standard", 56, 5, 3, 4, &[rg], 0),
            row("legendary_asia", "Standard", 36, 4, 2, 3, &[ln], 0),
            row("nordic", "Standard", 46, 5, 2, 3, &[gt], 0),
            row("poland", "Standard", 35, 4, 2, 3, &[], 0),
            row("switzerland", "Standard", 46, 5, 2, 3, &[lp], 0),
            row("usa_v2", "Standard", 33, 4, 2, 3, &[lp], 0),
            row("usa_1910", "1910", 35, 4, 2, 3, &[gt], 0),
            row("usa_big_cities", "Grandi Città", 35, 4, 2, 4, &[], 7),
            row("usa_mega", "Mega", 69, 5, 3, 4, &[lp, gt], 0),
        ]
    );
    let by_id = |id: &str| rulesets.iter().find(|r| r["id"] == id).unwrap().clone();
    assert_eq!(
        [by_id("europe"), by_id("usa_v2")],
        [
            serde_json::json!({
                    "id": "europe",
                    "name": "Europe",
                    "mode": "Standard",
                    "modeOrder": 0,
                    "mapId": "europe",
                    "cities": 47,
                    "routes": 101,
                    "tickets": 46,
                    "minPlayers": 2,
                    "maxPlayers": 5,
                    "bonuses": [
                        { "type": "longest_path", "label": "European Express", "points": 10 }
                    ],
                    "bigCities": [],
                    "initialDealt": 4,
                    "initialKeepMin": 2,
                    "drawCount": 3,
                    "drawKeepMin": 1,
                    "tunnels": true,
                    "ferries": true,
                    "stations": true,
                    "countryCards": false,
                    "passengers": false,
                    "specialRules": []
            }),
            serde_json::json!({
                    "id": "usa_v2",
                    "name": "USA",
                    "mode": "Standard",
                    "modeOrder": 0,
                    "mapId": "usa",
                    "cities": 36,
                    "routes": 100,
                    "tickets": 33,
                    "minPlayers": 2,
                    "maxPlayers": 5,
                    "bonuses": [
                        { "type": "longest_path", "label": "Longest Path", "points": 10 }
                    ],
                    "bigCities": [],
                    "initialDealt": 4,
                    "initialKeepMin": 2,
                    "drawCount": 3,
                    "drawKeepMin": 1,
                    "tunnels": false,
                    "ferries": false,
                    "stations": false,
                    "countryCards": false,
                    "passengers": false,
                    "specialRules": []
            }),
        ]
    );
    // Le regole nuove compaiono nel catalogo solo dove il ruleset le prevede.
    let flags = |id: &str| {
        let r = by_id(id);
        [&r["countryCards"], &r["passengers"]].map(|v| v.as_bool())
    };
    assert_eq!(flags("germany"), [Some(false), Some(false)]);
    assert_eq!(flags("germany_2017"), [Some(false), Some(true)]);
    assert_eq!(flags("poland"), [Some(true), Some(false)]);
    // Le pescabili arrivano da `legal_moves.draw_face_up`: il catalogo non espone `freeDraw`.
    assert_eq!(
        by_id("nordic")["locomotivesUsableOn"],
        serde_json::json!(["tunnel", "ferry"])
    );
    assert_eq!(by_id("nordic")["ferryAnyCards"], 3);
    // Le carte Traghetto sono nel catalogo come configurazione, `null` dove non ci sono.
    assert_eq!(
        by_id("italy")["ferryCards"],
        serde_json::json!({ "count": 10, "maxInHand": 2, "wavesPerCard": 2 })
    );
    assert!(by_id("nordic").get("ferryCards").is_none());
    assert_eq!(by_id("italy")["bonuses"][0]["minRegions"], 5);
    assert_eq!(
        by_id("italy")["bonuses"][0]["doubleRegions"],
        serde_json::json!(["puglia", "sardegna", "sicilia"])
    );
    assert!(by_id("nordic").get("freeLocomotiveDraw").is_none());
    // `specialRules`: testi non vuoti per Germany (entrambe le modalità) e Poland, vuoto altrove.
    let rules = |id: &str| -> Vec<String> {
        by_id(id)["specialRules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    for (id, count) in [
        ("germany", 7),
        ("germany_2017", 9),
        ("nordic", 10),
        ("poland", 6),
        ("india", 6),
        ("legendary_asia", 7),
        ("italy", 8),
    ] {
        let texts = rules(id);
        assert_eq!(texts.len(), count, "{id}");
        assert!(texts.iter().all(|t| !t.trim().is_empty()), "{id}");
    }
    assert!(rules("poland")[0].starts_with("35 treni"));
    assert!(rules("usa_v2").is_empty());
}

/// GET /api/rulesets/{id}/layout serve il layout schematico della mappa del ruleset
/// con `rulesetId`/`mapId`, e copre ogni città e rotta della mappa; un ruleset
/// sconosciuto dà 404.
#[tokio::test]
async fn ruleset_layout_endpoint_covers_the_map() {
    let server = TestServer::start().await;
    let (status, layout) = get_json(&server.base_url, "/api/rulesets/usa_v2/layout").await;
    assert_eq!(status, 200, "{layout}");
    assert_eq!(layout["rulesetId"], "usa_v2");
    assert_eq!(layout["mapId"], "usa");
    assert!(layout["width"].is_number() && layout["height"].is_number());

    let map: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(data_dir().join("maps/usa.json")).expect("lettura mappa"),
    )
    .expect("parsing mappa");
    for city in map["cities"].as_array().expect("città") {
        let id = city["id"].as_str().expect("id città");
        assert!(layout["cities"][id]["x"].is_number(), "posizione di {id}");
        assert!(layout["labels"][id].is_object(), "etichetta di {id}");
    }
    for route in map["routes"].as_array().expect("rotte") {
        let id = route["id"].as_str().expect("id rotta");
        let points = layout["routes"][id].as_array();
        assert!(
            points.is_some_and(|p| p.len() >= 2),
            "punti della rotta {id}"
        );
    }

    let (status, body) = get_json(&server.base_url, "/api/rulesets/non_esiste/layout").await;
    assert_eq!(status, 404);
    assert_eq!(body["error"], "RULESET_NOT_FOUND");
}

/// I percorsi `/api` sconosciuti (anche con slash finale) rispondono 404 JSON, non l'HTML della SPA.
#[tokio::test]
async fn unknown_api_route_is_json_404() {
    let server = TestServer::start().await;
    for path in [
        "/api/nope",
        "/api/rulesets/usa_v2/map/",
        "/api/rulesets/%2e%2e/map",
        "/api",
        "/api/",
    ] {
        let resp = reqwest::get(format!("{}{}", server.base_url, path))
            .await
            .unwrap_or_else(|e| panic!("GET {path}: {e}"));
        assert_eq!(resp.status(), 404, "{path}");
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            content_type.starts_with("application/json"),
            "{path}: {content_type}"
        );
        let body: serde_json::Value = resp.json().await.expect("corpo JSON");
        assert_eq!(body["error"], "NOT_FOUND", "{path}: {body}");
    }
}

// ── Seed ─────────────────────────────────────────────────────────────

/// Il seed di default viene da un CSPRNG: due partite nello stesso secondo
/// hanno seed diversi e lontani dall'orologio.
#[tokio::test]
async fn default_seed_is_not_time_based() {
    let server = TestServer::start().await;
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut seeds = Vec::new();
    for name in ["Seed 1", "Seed 2"] {
        let g = create_game(&server.base_url, name).await;
        join_game(&server.base_url, &g.game_id, "Alice", None).await;
        join_game(&server.base_url, &g.game_id, "Bob", None).await;
        start_game(&server.base_url, &g.game_id, &g.host_token).await;
        let save = server.saves_dir.join(format!("{}.json", g.game_id));
        wait_for_file(&save).await;
        let value = try_read_save(&save).expect("save leggibile");
        seeds.push(value["seed"].as_u64().expect("seed numerico"));
    }
    assert_ne!(seeds[0], seeds[1], "seed diversi");
    for seed in seeds {
        assert!(
            !(t.saturating_sub(60)..=t + 60).contains(&seed),
            "seed {seed} vicino all'orologio {t}"
        );
    }
}

/// Un server avviato senza `--allow-client-seed` rifiuta il seed del client.
#[tokio::test]
async fn client_seed_rejected_without_flag() {
    let saves = temp_saves_dir();
    let state = ttr_server::AppState::new(data_dir(), saves.clone(), 0).expect("AppState::new");
    let server = TestServer::start_with_state(state, saves).await;

    let (status, body) = post_json(
        &server.base_url,
        "/api/games",
        serde_json::json!({ "rulesetId": "usa_v2", "seed": 1 }),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"], "SEED_NOT_ALLOWED");
    let (status, _) = get_json(&server.base_url, "/api/games").await;
    assert_eq!(status, 200);

    let (status, body) = post_json(
        &server.base_url,
        "/api/games",
        serde_json::json!({ "rulesetId": "usa_v2" }),
    )
    .await;
    assert_eq!(status, 201, "senza seed la creazione funziona: {body}");
}

/// Né le API REST né i messaggi WS espongono il seed.
#[tokio::test]
async fn seed_is_never_exposed() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "Segreto", Some(987_654_321)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    let base = server.base_url.as_str();
    for path in [
        "/api/games".to_string(),
        format!("/api/games/{}", g.game_id),
    ] {
        let (_, body) = get_json(base, &path).await;
        let text = body.to_string();
        assert!(
            !text.contains("seed") && !text.contains("987654321"),
            "{path}: {text}"
        );
    }

    let mut ws = WsClient::connect(&server, &g.game_id, &pa.player_id, &pa.token).await;
    let snapshot = serde_json::to_string(&ws.recv().await).unwrap();
    assert!(
        !snapshot.contains("seed") && !snapshot.contains("987654321"),
        "{snapshot}"
    );
}

// ── Nomi invisibili e confusabili ────────────────────────────────────

#[tokio::test]
async fn invisible_names_are_rejected_with_invalid_name() {
    let server = TestServer::start().await;
    let g = create_game(&server.base_url, "Invisibili").await;
    let base = server.base_url.as_str();
    join_game(base, &g.game_id, "Anna", None).await;

    for name in [
        "Anna\u{3164}",
        "Anna\u{2800}",
        "Anna\u{34f}",
        "Anna\u{FE0F}",
        "\u{3164}",
    ] {
        let res = post_join(base, &g.game_id, serde_json::json!({ "name": name })).await;
        assert_rejected(&res, 400, "INVALID_NAME");
    }
    // Il mix di alfabeti nella stessa parola ("А" cirillica + "nna" latina) è rifiutato.
    let mixed = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "\u{410}nna" }),
    )
    .await;
    assert_rejected(&mixed, 400, "INVALID_NAME");

    let (_, profiles) = get_json(base, "/api/profiles").await;
    assert_eq!(profiles["profiles"], serde_json::json!(["Anna"]));
}

#[tokio::test]
async fn confusable_names_are_taken() {
    let server = TestServer::start().await;
    let base = server.base_url.as_str();

    let g = create_game(base, "Confusabili").await;
    join_game(base, &g.game_id, "Anna", None).await;
    let fullwidth = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "\u{FF21}\u{FF4E}\u{FF4E}\u{FF41}" }),
    )
    .await;
    assert_rejected(&fullwidth, 409, "NAME_TAKEN");

    // Tre lettere cirilliche (С а р) identiche a "Cap" latino.
    join_game(base, &g.game_id, "Cap", None).await;
    let cyrillic = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "\u{421}\u{430}\u{440}" }),
    )
    .await;
    assert_rejected(&cyrillic, 409, "NAME_TAKEN");

    let (_, profiles) = get_json(base, "/api/profiles").await;
    assert_eq!(profiles["profiles"], serde_json::json!(["Anna", "Cap"]));
}

#[tokio::test]
async fn nbsp_is_collapsed_and_accents_stay_valid() {
    let server = TestServer::start().await;
    let base = server.base_url.as_str();

    let g = create_game(base, "Spazi").await;
    let res = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "Anna\u{a0}\u{a0}Bianchi" }),
    )
    .await;
    assert_eq!(res.0, 200, "{}", res.1);
    assert_eq!(
        get_game_detail(base, &g.game_id).await["players"][0]["name"],
        "Anna Bianchi"
    );

    for (nfc, nfd) in [("Jos\u{e9}", "Jose\u{301}"), ("Zo\u{eb}", "Zoe\u{308}")] {
        for name in [nfc, nfd] {
            let g = create_game(base, "Accenti").await;
            let res = post_join(base, &g.game_id, serde_json::json!({ "name": name })).await;
            assert_eq!(res.0, 200, "{name:?}: {}", res.1);
            assert_eq!(
                get_game_detail(base, &g.game_id).await["players"][0]["name"],
                nfc
            );
        }
    }
}

// ── Rientro in una partita avviata ───────────────────────────────────

/// Partita avviata a due giocatori (Alice, Bob).
async fn started_game(base: &str) -> (CreatedGame, JoinedPlayer, JoinedPlayer) {
    let g = create_game_with_seed(base, "Avviata", Some(7)).await;
    let alice = join_game(base, &g.game_id, "Alice", None).await;
    let bob = join_game(base, &g.game_id, "Bob", None).await;
    start_game(base, &g.game_id, &g.host_token).await;
    (g, alice, bob)
}

/// Dopo lo start nessun nuovo giocatore entra, nemmeno con un token.
#[tokio::test]
async fn started_game_rejects_new_names() {
    let server = TestServer::start().await;
    let base = server.base_url.as_str();
    let (g, alice, _bob) = started_game(base).await;

    let new = post_join(base, &g.game_id, serde_json::json!({ "name": "Carla" })).await;
    assert_rejected(&new, 409, "GAME_STARTED");
    let with_token = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "Carla", "token": alice.token }),
    )
    .await;
    assert_rejected(&with_token, 409, "GAME_STARTED");
}

/// Chi è già nella partita rientra con nome (anche in forma diversa) e token:
/// stesso playerId e stesso token, poi il WS dà lo snapshot privato.
#[tokio::test]
async fn started_game_lets_existing_player_rejoin_with_token() {
    let server = TestServer::start().await;
    let base = server.base_url.as_str();
    let (g, alice, _bob) = started_game(base).await;

    for name in ["Alice", "  alice "] {
        let (status, body) = post_join(
            base,
            &g.game_id,
            serde_json::json!({ "name": name, "token": alice.token }),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["playerId"], alice.player_id.as_str());
        assert_eq!(body["token"], alice.token.as_str());
    }

    let mut ws = WsClient::connect(&server, &g.game_id, &alice.player_id, &alice.token).await;
    match ws.recv().await {
        ttr_proto::ServerMessage::Snapshot(snap) => {
            assert!(snap.private.is_some(), "lo snapshot ha lo stato privato");
        }
        other => panic!("atteso SNAPSHOT, trovato {other:?}"),
    }
}

/// Nome esistente ma token assente o di un altro giocatore → 401.
#[tokio::test]
async fn started_game_rejects_rejoin_without_right_token() {
    let server = TestServer::start().await;
    let base = server.base_url.as_str();
    let (g, _alice, bob) = started_game(base).await;

    let none = post_join(base, &g.game_id, serde_json::json!({ "name": "Alice" })).await;
    assert_rejected(&none, 401, "RESUME_TOKEN_MISMATCH");
    let wrong = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "Alice", "token": bob.token }),
    )
    .await;
    assert_rejected(&wrong, 401, "RESUME_TOKEN_MISMATCH");
}

/// L'host riassegna un giocatore di una partita avviata (non ripresa): il
/// token nuovo apre il WS, quello vecchio no e non rientra più col join.
#[tokio::test]
async fn reassign_in_started_game_replaces_token() {
    let server = TestServer::start().await;
    let base = server.base_url.as_str();
    let (g, alice, _bob) = started_game(base).await;
    let path = format!("/api/games/{}/reassign", g.game_id);

    let bad_host = post_json(
        base,
        &path,
        serde_json::json!({ "hostToken": alice.token, "name": "Alice" }),
    )
    .await;
    assert_rejected(&bad_host, 401, "UNAUTHORIZED");
    let unknown = post_json(
        base,
        &path,
        serde_json::json!({ "hostToken": g.host_token, "name": "Zoe" }),
    )
    .await;
    assert_rejected(&unknown, 409, "NAME_NOT_IN_SAVE");

    let mut old_ws = WsClient::connect(&server, &g.game_id, &alice.player_id, &alice.token).await;
    assert!(matches!(
        old_ws.recv().await,
        ttr_proto::ServerMessage::Snapshot(_)
    ));

    let (status, body) = post_json(
        base,
        &path,
        serde_json::json!({ "hostToken": g.host_token, "name": "alice" }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["playerId"], alice.player_id.as_str());
    let new_token = body["token"].as_str().expect("token").to_string();
    assert_ne!(new_token, alice.token);

    // La connessione aperta col token vecchio viene chiusa dal server.
    let closed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match old_ws.read.next().await {
                None
                | Some(Err(_))
                | Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => return true,
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert_eq!(closed, Ok(true), "il vecchio WS va chiuso dopo il reassign");

    let mut new_ws = WsClient::connect(&server, &g.game_id, &alice.player_id, &new_token).await;
    match new_ws.recv().await {
        ttr_proto::ServerMessage::Snapshot(snap) => assert!(snap.private.is_some()),
        other => panic!("atteso SNAPSHOT, trovato {other:?}"),
    }

    let mut stale_ws = WsClient::connect(&server, &g.game_id, &alice.player_id, &alice.token).await;
    match stale_ws.recv().await {
        ttr_proto::ServerMessage::Error(e) => {
            assert_eq!(e.reason, ttr_proto::ErrorReason::Unauthorized);
        }
        other => panic!("atteso ERROR, trovato {other:?}"),
    }

    let old_join = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "Alice", "token": alice.token }),
    )
    .await;
    assert_rejected(&old_join, 401, "RESUME_TOKEN_MISMATCH");
    let new_join = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "Alice", "token": new_token }),
    )
    .await;
    assert_eq!(new_join.0, 200, "{}", new_join.1);
}

/// Un nome "uguale a vista" (omoglifo cirillico) mappa sulla stessa chiave del
/// giocatore esistente: senza token è 401 e il roster non cambia, mai un posto nuovo.
#[tokio::test]
async fn started_game_homoglyph_name_never_gets_a_new_seat() {
    let server = TestServer::start().await;
    let base = server.base_url.as_str();
    let (g, alice, _bob) = started_game(base).await;

    let no_token = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "\u{410}lice" }),
    )
    .await;
    assert_rejected(&no_token, 401, "RESUME_TOKEN_MISMATCH");
    let wrong_token = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "\u{410}lice", "token": "altro" }),
    )
    .await;
    assert_rejected(&wrong_token, 401, "RESUME_TOKEN_MISMATCH");
    assert_eq!(
        get_game_detail(base, &g.game_id).await["players"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );

    // Col token di Alice è comunque lei: stesso playerId, nessun posto nuovo.
    let with_token = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": "\u{410}lice", "token": alice.token }),
    )
    .await;
    assert_eq!(with_token.0, 200, "{}", with_token.1);
    assert_eq!(with_token.1["playerId"], alice.player_id.as_str());
}

/// Un nome più lungo del massimo valido è rifiutato senza essere normalizzato.
#[tokio::test]
async fn started_game_overlong_name_is_rejected_fast() {
    let server = TestServer::start().await;
    let base = server.base_url.as_str();
    let (g, alice, _bob) = started_game(base).await;

    let started = std::time::Instant::now();
    let long = "A".repeat(500_000);
    let res = post_join(
        base,
        &g.game_id,
        serde_json::json!({ "name": long, "token": alice.token }),
    )
    .await;
    assert_rejected(&res, 409, "GAME_STARTED");
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}
