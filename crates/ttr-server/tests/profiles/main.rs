//! Test di abuso e recupero della lobby: profili, limiti di partite, identità al resume.

#[allow(dead_code)]
#[path = "../common/mod.rs"]
mod common;

use common::*;

fn corrupt_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .expect("lettura dir")
        .map(|e| e.expect("voce dir").path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("profiles.json.corrupt-")
        })
        .collect()
}

/// Un file dei profili troncato non viene mai sovrascritto: va in quarantena.
#[test]
fn p1_corrupt_profiles_file_is_quarantined_by_next_join() {
    let dir = temp_saves_dir();
    for n in ["Anna", "Bob", "Ciro"] {
        ttr_server::profiles::add_profile(&dir, n).unwrap();
    }
    let path = dir.join("profiles.json");
    let original = std::fs::read_to_string(&path).unwrap();
    let truncated = &original[..original.len() - 3];
    std::fs::write(&path, truncated).unwrap();

    assert!(ttr_server::profiles::read_profiles(&dir).is_err());
    ttr_server::profiles::add_profile(&dir, "Dario").unwrap();

    let quarantined = corrupt_files(&dir);
    assert_eq!(quarantined.len(), 1, "{quarantined:?}");
    assert_eq!(std::fs::read_to_string(&quarantined[0]).unwrap(), truncated);
    assert_eq!(
        ttr_server::profiles::read_profiles(&dir).unwrap(),
        ["Dario"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Via REST: l'elenco con file illeggibile è vuoto ma risponde 200; il join
/// successivo mette il file in quarantena.
#[tokio::test]
async fn p1_rest_corrupt_profiles_file_answers_200_and_join_quarantines() {
    let server = TestServer::start().await;
    let g = create_game(&server.base_url, "P1").await;
    join_game(&server.base_url, &g.game_id, "Anna", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;

    let path = server.saves_dir.join("profiles.json");
    std::fs::write(&path, "[\"Anna\", \"Bo").unwrap();
    let (status, body) = get_json(&server.base_url, "/api/profiles").await;
    assert_eq!(status, 200);
    assert_eq!(body["profiles"], serde_json::json!([]));

    join_game(&server.base_url, &g.game_id, "Ciro", None).await;
    let (_, body) = get_json(&server.base_url, "/api/profiles").await;
    assert_eq!(body["profiles"], serde_json::json!(["Ciro"]));
    let quarantined = corrupt_files(&server.saves_dir);
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        std::fs::read_to_string(&quarantined[0]).unwrap(),
        "[\"Anna\", \"Bo"
    );
}

/// Creare partite a raffica si ferma al limite di partite attive.
#[tokio::test]
async fn p2_game_spam_is_limited() {
    let server = TestServer::start().await;
    let limit = ttr_server::SessionLimits::default().max_active_games;

    let mut created = 0;
    let mut rejected = 0;
    for i in 0..limit + 5 {
        let (status, body) = post_json(
            &server.base_url,
            "/api/games",
            serde_json::json!({ "rulesetId": "usa_v2", "name": format!("spam{i}") }),
        )
        .await;
        match status {
            201 => created += 1,
            429 => {
                assert_eq!(body["error"], "TOO_MANY_GAMES");
                rejected += 1;
            }
            other => panic!("status inatteso {other}: {body}"),
        }
    }
    assert_eq!(created, limit);
    assert_eq!(rejected, 5);
    let (_, list) = get_json(&server.base_url, "/api/games").await;
    assert_eq!(list["games"].as_array().unwrap().len(), limit);
}

/// Al resume il solo nome non dà l'identità di un altro giocatore.
#[tokio::test]
async fn p3_resume_join_by_name_only_is_rejected() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "R", Some(3)).await;
    let alice = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_file(&save).await;
    let (rid, _) = ttr_server::resume_game(&server.state, &save).await.unwrap();

    // L'attaccante conosce solo il nome.
    let (status, body) = post_json(
        &server.base_url,
        &format!("/api/games/{rid}/join"),
        serde_json::json!({ "name": "ALICE" }),
    )
    .await;
    assert_eq!(status, 401, "{body}");
    assert_eq!(body["error"], "RESUME_TOKEN_MISMATCH");
    assert!(body.get("playerId").is_none() && body.get("token").is_none());

    // Alice, col suo token, rientra.
    let back = join_game_resume(&server.base_url, &rid, "ALICE", &alice.token).await;
    assert_eq!(back.player_id, alice.player_id);
}
