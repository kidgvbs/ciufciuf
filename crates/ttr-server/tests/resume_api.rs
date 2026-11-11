//! Test dell'API dei save: elenco dei save riprendibili, resume ed eliminazione
//! dalla TV (`GET /api/saves`, `POST /api/saves/{id}/resume`, `DELETE /api/saves/{id}`).

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

/// DELETE senza body, come fa la TV.
async fn delete_save(server: &TestServer, game_id: &str) -> (u16, serde_json::Value) {
    let resp = reqwest::Client::new()
        .delete(format!("{}/api/saves/{}", server.base_url, game_id))
        .send()
        .await
        .expect("DELETE /api/saves/{id}");
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap_or(serde_json::Value::Null))
}

/// Backup `{id}.deleted-*.bak` lasciati dall'eliminazione.
fn deleted_backups(server: &TestServer, game_id: &str) -> Vec<std::path::PathBuf> {
    let prefix = format!("{game_id}.deleted-");
    std::fs::read_dir(&server.saves_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".bak"))
        })
        .collect()
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

/// L'eliminazione risponde 204 senza body, rinomina il save in un backup
/// recuperabile a mano e lo toglie dall'elenco; un secondo delete è 404.
#[tokio::test]
async fn delete_renames_the_save_and_hides_it() {
    let server = TestServer::start().await;
    let g = saved_game(&server, "Da eliminare").await;
    let other = saved_game(&server, "Resta").await;
    let original = std::fs::read_to_string(save_path(&server, &g.game_id)).unwrap();

    let (status, body) = delete_save(&server, &g.game_id).await;
    assert_eq!((status, &body), (204, &serde_json::Value::Null));

    assert!(!save_path(&server, &g.game_id).exists());
    let backups = deleted_backups(&server, &g.game_id);
    assert_eq!(backups.len(), 1, "{backups:?}");
    assert_eq!(std::fs::read_to_string(&backups[0]).unwrap(), original);
    assert_eq!(
        listed_ids(&listed_saves(&server).await),
        [other.game_id.as_str()]
    );

    let again = delete_save(&server, &g.game_id).await;
    assert_error(&again, 404, "SAVE_NOT_FOUND");
}

/// Il save di una partita viva non si tocca: l'host la chiude prima.
#[tokio::test]
async fn delete_of_a_live_game_is_rejected_and_leaves_the_file() {
    let server = TestServer::start().await;
    let g = started_game(&server, "Viva").await;

    let res = delete_save(&server, &g.game_id).await;
    assert_error(&res, 409, "GAME_ALREADY_ACTIVE");

    assert!(save_path(&server, &g.game_id).exists());
    assert!(deleted_backups(&server, &g.game_id).is_empty());
}

#[tokio::test]
async fn delete_of_unknown_or_malformed_id_is_not_found() {
    let server = TestServer::start().await;
    let g = saved_game(&server, "Esiste").await;

    for id in [
        uuid::Uuid::new_v4().to_string(),
        "non-un-uuid".to_string(),
        g.game_id.to_uppercase(),
        "..%2F..%2Fsecret".to_string(),
    ] {
        let res = delete_save(&server, &id).await;
        assert_error(&res, 404, "SAVE_NOT_FOUND");
    }
    assert!(save_path(&server, &g.game_id).exists());
}

/// Dopo l'eliminazione il save non si può più riprendere.
#[tokio::test]
async fn resume_after_delete_is_not_found() {
    let server = TestServer::start().await;
    let g = saved_game(&server, "Eliminata").await;

    assert_eq!(delete_save(&server, &g.game_id).await.0, 204);

    let res = resume(&server, &g.game_id).await;
    assert_error(&res, 404, "SAVE_NOT_FOUND");
    assert!(!save_path(&server, &g.game_id).exists());
}

/// Delete e resume concorrenti dello stesso save: non riescono entrambi e non
/// resta mai una partita registrata senza il suo file (né un file resuscitato).
#[tokio::test]
async fn concurrent_delete_and_resume_never_both_win() {
    let server = TestServer::start().await;
    let g = saved_game(&server, "Corsa").await;

    for _ in 0..8 {
        let id = clone_save(&server, &g.game_id, |_| {});
        let (deleted, resumed) = tokio::join!(delete_save(&server, &id), resume(&server, &id));

        match (deleted.0, resumed.0) {
            (204, 404) => {
                assert!(!server.state.games.read().await.contains_key(&id));
                assert!(!save_path(&server, &id).exists());
                assert_eq!(deleted_backups(&server, &id).len(), 1);
            }
            (409, 201) => {
                assert!(server.state.games.read().await.contains_key(&id));
                assert!(save_path(&server, &id).exists());
                assert!(deleted_backups(&server, &id).is_empty());
            }
            other => panic!("esito inatteso {other:?}: {deleted:?} {resumed:?}"),
        }
    }
}

/// Un symlink `{uuid}.json` non è elencato né riprendibile, anche se punta a un save valido.
#[cfg(unix)]
#[tokio::test]
async fn list_saves_skips_symlinks() {
    let server = TestServer::start().await;
    let g = saved_game(&server, "Reale").await;
    let linked = clone_save(&server, &g.game_id, |_| {});

    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("save.json");
    std::fs::copy(save_path(&server, &linked), &target).unwrap();
    std::fs::remove_file(save_path(&server, &linked)).unwrap();
    std::os::unix::fs::symlink(&target, save_path(&server, &linked)).unwrap();

    assert_eq!(
        listed_ids(&listed_saves(&server).await),
        [g.game_id.as_str()]
    );
    let res = resume(&server, &linked).await;
    assert_error(&res, 404, "SAVE_NOT_FOUND");
}

/// Un save oltre 16 MiB non è elencato e il resume lo rifiuta senza leggerlo.
#[tokio::test]
async fn list_saves_skips_oversized_files() {
    use std::io::Write;

    let server = TestServer::start().await;
    let g = saved_game(&server, "Normale").await;
    let big = clone_save(&server, &g.game_id, |_| {});
    // Spazi finali: il JSON resta valido, quindi solo la dimensione lo esclude.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(save_path(&server, &big))
        .unwrap();
    file.write_all(&vec![b' '; 16 * 1024 * 1024 + 1]).unwrap();
    drop(file);

    assert_eq!(
        listed_ids(&listed_saves(&server).await),
        [g.game_id.as_str()]
    );
    let res = resume(&server, &big).await;
    assert_error(&res, 409, "SAVE_NOT_RESUMABLE");
    assert!(save_path(&server, &big).exists());
}

/// Nome partita e nomi dei giocatori di un save manomesso sono troncati nell'elenco.
#[tokio::test]
async fn list_saves_truncates_long_names() {
    let server = TestServer::start().await;
    let g = saved_game(&server, "Normale").await;
    clone_save(&server, &g.game_id, |s| {
        s["gameName"] = "é".repeat(100).into();
        s["players"][0]["name"] = "ü".repeat(60).into();
    });

    let saves = listed_saves(&server).await;
    let long = saves
        .iter()
        .find(|s| s["gameId"] != g.game_id.as_str())
        .expect("save clonato elencato");
    assert_eq!(long["name"], "é".repeat(40));
    assert_eq!(long["players"][0], "ü".repeat(24));
    assert_eq!(long["players"][1], "Bob");
}
