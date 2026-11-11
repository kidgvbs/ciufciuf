//! Test dell'API dei save: elenco dei save riprendibili e resume dalla TV
//! (`GET /api/saves`, `POST /api/saves/{id}/resume`).

mod common;

use common::driver::*;
use common::*;

/// Partita a due giocatori avviata, con il save su disco, poi chiusa
/// dall'host: il save resta e la partita non è più in lobby.
async fn saved_game(server: &TestServer, name: &str) -> CreatedGame {
    let g = started_game(server, name).await;
    close_game(server, &g).await;
    g
}

/// Partita a due giocatori avviata, con il save già su disco.
async fn started_game(server: &TestServer, name: &str) -> CreatedGame {
    let g = create_game_with_seed(&server.base_url, name, Some(7)).await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    wait_for_file(&save_path(server, &g.game_id)).await;
    g
}

async fn close_game(server: &TestServer, g: &CreatedGame) {
    let status = delete_json(
        &server.base_url,
        &format!("/api/games/{}", g.game_id),
        serde_json::json!({ "hostToken": g.host_token }),
    )
    .await;
    assert_eq!(status, 204);
}

fn save_path(server: &TestServer, game_id: &str) -> std::path::PathBuf {
    server.saves_dir.join(format!("{game_id}.json"))
}

/// POST di resume senza body, come fa la TV.
async fn resume(server: &TestServer, game_id: &str) -> (u16, serde_json::Value) {
    let resp = reqwest::Client::new()
        .post(format!("{}/api/saves/{}/resume", server.base_url, game_id))
        .send()
        .await
        .expect("POST /api/saves/{id}/resume");
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap_or(serde_json::Value::Null))
}

async fn listed_saves(server: &TestServer) -> Vec<serde_json::Value> {
    let (status, body) = get_json(&server.base_url, "/api/saves").await;
    assert_eq!(status, 200, "{body}");
    body["saves"].as_array().expect("saves array").clone()
}

fn listed_ids(saves: &[serde_json::Value]) -> Vec<&str> {
    saves
        .iter()
        .map(|s| s["gameId"].as_str().expect("gameId"))
        .collect()
}

fn assert_error(res: &(u16, serde_json::Value), status: u16, code: &str) {
    assert_eq!(res.0, status, "{}", res.1);
    assert_eq!(res.1["error"], code, "{}", res.1);
}

/// Copia il save di `from` con un gameId nuovo e le modifiche di `edit`.
fn clone_save(
    server: &TestServer,
    from: &str,
    edit: impl FnOnce(&mut serde_json::Value),
) -> String {
    let mut save: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(save_path(server, from)).unwrap()).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    save["gameId"] = id.clone().into();
    edit(&mut save);
    std::fs::write(save_path(server, &id), save.to_string()).unwrap();
    id
}

/// L'elenco mostra i save `in_progress` fermi, dal più recente, e nasconde
/// partite attive, save conclusi, file illeggibili e file che non sono save.
#[tokio::test]
async fn list_shows_only_resumable_saves() {
    let server = TestServer::start().await;
    let stopped = saved_game(&server, "Ferma").await;
    let active = started_game(&server, "Attiva").await;

    let newer = clone_save(&server, &stopped.game_id, |s| {
        s["gameName"] = "Più recente".into();
        s["updatedAt"] = "2999-01-01T00:00:00Z".into();
    });
    clone_save(&server, &stopped.game_id, |s| {
        s["status"] = "completed".into()
    });
    clone_save(&server, &stopped.game_id, |s| {
        s["gameId"] = uuid::Uuid::new_v4().to_string().into();
    });
    // Regole precedenti e ruleset sconosciuto: il resume li rifiuterebbe.
    clone_save(&server, &stopped.game_id, |s| s["rulesVersion"] = 1.into());
    clone_save(&server, &stopped.game_id, |s| {
        s["rulesetId"] = "nessuno".into()
    });
    std::fs::write(
        save_path(&server, &uuid::Uuid::new_v4().to_string()),
        "{non json",
    )
    .unwrap();
    std::fs::write(server.saves_dir.join("appunti.json"), "{}").unwrap();
    std::fs::write(
        server
            .saves_dir
            .join(format!("{}.resume-1.bak", stopped.game_id)),
        std::fs::read(save_path(&server, &stopped.game_id)).unwrap(),
    )
    .unwrap();

    let saves = listed_saves(&server).await;
    assert_eq!(
        listed_ids(&saves),
        [newer.as_str(), stopped.game_id.as_str()],
        "{saves:?}"
    );
    assert!(!listed_ids(&saves).contains(&active.game_id.as_str()));

    let item = &saves[1];
    assert_eq!(item["name"], "Ferma");
    assert_eq!(item["rulesetId"], "usa_v2");
    assert_eq!(item["players"], serde_json::json!(["Alice", "Bob"]));
    assert!(!item["createdAt"].as_str().unwrap().is_empty());
    assert!(!item["updatedAt"].as_str().unwrap().is_empty());
    assert_eq!(saves[0]["name"], "Più recente");
}

/// Il resume dà un hostToken che la TV usa subito per `reassign`; da quel
/// momento il save non è più elencato e un secondo resume è rifiutato.
#[tokio::test]
async fn resume_gives_a_working_host_token_and_only_once() {
    let server = TestServer::start().await;
    let g = saved_game(&server, "Da riprendere").await;

    let (status, body) = resume(&server, &g.game_id).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["gameId"], g.game_id.as_str());
    assert_eq!(body["rulesetId"], "usa_v2");
    assert_eq!(body["maxPlayers"], 5);
    let host_token = body["hostToken"].as_str().expect("hostToken").to_string();
    assert_ne!(host_token, g.host_token);

    let (_, game) = get_json(&server.base_url, &format!("/api/games/{}", g.game_id)).await;
    assert_eq!(game["missingPlayers"], serde_json::json!(["Alice", "Bob"]));

    let (status, reassigned) = post_json(
        &server.base_url,
        &format!("/api/games/{}/reassign", g.game_id),
        serde_json::json!({ "hostToken": host_token, "name": "Alice" }),
    )
    .await;
    assert_eq!(status, 200, "{reassigned}");
    assert!(!reassigned["token"].as_str().unwrap().is_empty());

    assert!(listed_saves(&server).await.is_empty());
    let backups = std::fs::read_dir(&server.saves_dir)
        .unwrap()
        .filter(|e| {
            let name = e.as_ref().unwrap().file_name();
            let name = name.to_string_lossy();
            name.starts_with(&g.game_id) && name.ends_with(".bak")
        })
        .count();
    assert_eq!(backups, 1, "un solo backup, dal resume che ha vinto");

    let again = resume(&server, &g.game_id).await;
    assert_error(&again, 409, "GAME_ALREADY_ACTIVE");
    let again = again.1;
    assert!(again.get("hostToken").is_none());
}

/// Una partita ancora in lobby non può essere ripresa: niente secondo
/// hostToken per una partita viva.
#[tokio::test]
async fn resume_of_a_live_game_is_rejected() {
    let server = TestServer::start().await;
    let g = started_game(&server, "Viva").await;

    let res = resume(&server, &g.game_id).await;
    assert_error(&res, 409, "GAME_ALREADY_ACTIVE");
}

/// Resume concorrenti dello stesso save: ne passa esattamente uno.
#[tokio::test]
async fn concurrent_resumes_yield_a_single_winner() {
    let server = TestServer::start().await;
    let g = saved_game(&server, "Corsa").await;

    let results = futures_util::future::join_all((0..8).map(|_| resume(&server, &g.game_id))).await;

    let created = results.iter().filter(|(s, _)| *s == 201).count();
    let conflicts = results
        .iter()
        .filter(|(s, b)| *s == 409 && b["error"] == "GAME_ALREADY_ACTIVE")
        .count();
    assert_eq!((created, conflicts), (1, 7), "{results:?}");
}

#[tokio::test]
async fn resume_of_unknown_or_malformed_id_is_not_found() {
    let server = TestServer::start().await;
    let g = saved_game(&server, "Esiste").await;

    for id in [
        uuid::Uuid::new_v4().to_string(),
        "non-un-uuid".to_string(),
        // Forma non canonica dello stesso id: mai mappata sul file.
        g.game_id.to_uppercase(),
        "..%2F..%2Fsecret".to_string(),
    ] {
        let (status, body) = resume(&server, &id).await;
        assert_error(&(status, body), 404, "SAVE_NOT_FOUND");
    }
}

/// Un save concluso (qui lasciato su disco perché l'archiviazione fallisce)
/// non è elencato e non si può riprendere.
#[tokio::test]
async fn completed_save_is_neither_listed_nor_resumable() {
    let server = TestServer::start().await;
    let map = usa_map();
    // `results.jsonl` è una directory: l'archiviazione fallisce.
    std::fs::create_dir(server.saves_dir.join(ttr_server::save::RESULTS_FILE)).unwrap();

    let g = create_game_with_seed(&server.base_url, "Conclusa", Some(42)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let mut a = Player::new(WsClient::connect(&server, &g.game_id, &pa.player_id, &pa.token).await);
    let mut b = Player::new(WsClient::connect(&server, &g.game_id, &pb.player_id, &pb.token).await);
    let final_public = run_game(&mut a, &mut b, &map).await;
    assert_eq!(final_public.state, ttr_proto::GameState::GameOver);
    wait_for_save(&save_path(&server, &g.game_id), |s| {
        s["status"] == "completed"
    })
    .await;
    // La sessione conclusa è scaduta: il registro non la contiene più.
    server.state.games.write().await.remove(&g.game_id);

    assert!(listed_saves(&server).await.is_empty());
    let (status, body) = resume(&server, &g.game_id).await;
    assert_error(&(status, body.clone()), 409, "SAVE_NOT_RESUMABLE");
    assert!(body["detail"].as_str().unwrap().contains("conclusa"));
}
