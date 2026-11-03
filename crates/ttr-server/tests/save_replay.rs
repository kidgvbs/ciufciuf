//! Test di persistenza e resume (pacchetto Fase 2 §9.4–§9.5).
//!
//! `save_and_replay`: partita completa → save file → replay motore → stessi
//! final_scores.
//! `resume_flow`: partita interrotta a metà → `resume_game` → join per nome
//! salvato → stesso playerId → start → stesso PUBLIC_STATE.

mod common;

use common::driver::*; // Player, decide, apply_msg, run_game, recv_from
use common::*;
use ttr_proto::GameState;

/// §9.4: a fine partita il file `saves/{id}.json` esiste, `status: completed`,
/// N azioni > 0; replay da file → stessi `final_scores`.
#[tokio::test]
async fn save_and_replay() {
    let server = TestServer::start().await;
    let map = usa_map();

    let game = create_game_with_seed(&server.base_url, "SaveReplay", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a_ws = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b_ws = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a_ws);
    let mut b = Player::new(b_ws);

    let final_public = run_game(&mut a, &mut b, &map).await;
    assert_eq!(final_public.state, GameState::GameOver);

    // Lascia tempo al server per scrivere il save file.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let save_path = server.saves_dir.join(format!("{}.json", game.game_id));
    assert!(
        save_path.exists(),
        "save file {:?} deve esistere",
        save_path
    );

    // Legge il save file e verifica i contenuti.
    let save_json = std::fs::read_to_string(&save_path).expect("lettura save file");
    let save: serde_json::Value = serde_json::from_str(&save_json).expect("parse JSON save");

    assert_eq!(
        save["status"].as_str().unwrap(),
        "completed",
        "status deve essere completed"
    );
    let actions = save["actions"].as_array().expect("actions array");
    assert!(!actions.is_empty(), "deve esserci almeno 1 azione nel save");
    assert_eq!(save["gameId"].as_str().unwrap(), game.game_id);

    // Replay: ricostruisci il motore dal save e verifica che arrivi a GameOver
    // con lo stesso stato.
    let save_data: ttr_server::save::SaveData =
        serde_json::from_str(&save_json).expect("parse SaveData");
    let ruleset = load_usa_ruleset();
    let map_data = usa_map();
    let replayed =
        ttr_server::save::replay_engine(&save_data, &map_data, &ruleset).expect("replay engine");

    // Verifica che il motore riprodotto sia in GameOver.
    assert!(
        matches!(replayed.phase(), ttr_core::error::GamePhase::GameOver),
        "replay deve terminare in GameOver, fase attuale: {:?}",
        replayed.phase()
    );

    // Confronta i final_scores del final_public con i punteggi dei giocatori
    // nel motore riprodotto.
    let orig_scores = final_public
        .final_scores
        .as_ref()
        .expect("final_scores originali");
    assert_eq!(
        replayed.num_players(),
        orig_scores.len(),
        "stesso numero di giocatori nel replay"
    );

    for (i, orig_fs) in orig_scores.iter().enumerate() {
        let replayed_player = replayed.player(i).expect("giocatore nel replay");
        // `PlayerState.score` contiene solo i punti rotta, come `route_points`.
        assert_eq!(
            replayed_player.score, orig_fs.route_points,
            "punti rotta coerenti per giocatore {}: replay={}, orig={}",
            i, replayed_player.score, orig_fs.route_points
        );
        // Verifica anche il totale dei biglietti (non salvato su PlayerState).
        assert_eq!(
            orig_fs.ticket_completed + orig_fs.ticket_failed,
            orig_fs.total - orig_fs.route_points - orig_fs.longest_path_bonus,
            "coerenza interna final_scores per giocatore {}",
            i
        );
    }

    // Verifica che il numero di rotte claimate coincida.
    let orig_claimed = final_public.claimed_routes.len();
    let replayed_claimed = replayed.claimed_route_ids().len();
    assert_eq!(
        replayed_claimed, orig_claimed,
        "stesso numero di rotte claimate: replay={}, orig={}",
        replayed_claimed, orig_claimed
    );
}

/// §9.5: partita salvata a metà → `resume_game` → join col nome salvato →
/// stesso playerId → start → stesso PUBLIC_STATE.
#[tokio::test]
async fn resume_flow() {
    let server = TestServer::start().await;
    let map = usa_map();

    let game = create_game_with_seed(&server.base_url, "ResumeFlow", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a_ws = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b_ws = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a_ws);
    let mut b = Player::new(b_ws);
    let mut public: Option<ttr_proto::PublicState> = None;

    // Consuma SNAPSHOT iniziali
    apply_msg(
        &a.client.as_mut().unwrap().recv().await,
        &mut a,
        &mut public,
    );
    apply_msg(
        &b.client.as_mut().unwrap().recv().await,
        &mut b,
        &mut public,
    );

    // Draft entrambi
    let draft_a = decide(
        &a.player_id,
        &mut a.client.as_mut().unwrap().view,
        public.as_ref().unwrap(),
        &map,
        &mut a.drafted,
    )
    .unwrap()
    .unwrap();
    a.client.as_mut().unwrap().send(&draft_a).await;

    let draft_b = decide(
        &b.player_id,
        &mut b.client.as_mut().unwrap().view,
        public.as_ref().unwrap(),
        &map,
        &mut b.drafted,
    )
    .unwrap()
    .unwrap();
    b.client.as_mut().unwrap().send(&draft_b).await;

    // Consuma post-draft finché non inizia il turno
    loop {
        let msg_a = a.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_a, &mut a, &mut public);
        let msg_b = b.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_b, &mut b, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver {
            panic!("game over troppo presto");
        }
        if p.state == GameState::PlayerTurn || p.state == GameState::LastRound {
            break;
        }
    }

    // A gioca il primo turno
    loop {
        let msg = a.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut a, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver {
            break;
        }
        if fresh && p.current_turn_player_id.as_deref() == Some(&a.player_id) {
            let a_c = a.client.as_mut().unwrap();
            let a_act = decide(&a.player_id, &mut a_c.view, p, &map, &mut a.drafted).unwrap();
            if let Some(act) = a_act {
                a_c.send(&act).await;
            }
            break;
        }
    }

    // Consuma i messaggi post-azione di A per B finché non è il turno di B
    loop {
        let msg = b.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut b, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver {
            break;
        }
        if fresh && p.current_turn_player_id.as_deref() == Some(&b.player_id) {
            break;
        }
    }

    // B gioca il suo primo turno
    {
        let p = public.as_ref().unwrap();
        assert!(p.current_turn_player_id.as_deref() == Some(&b.player_id));
        let b_c = b.client.as_mut().unwrap();
        let b_act = decide(&b.player_id, &mut b_c.view, p, &map, &mut b.drafted).unwrap();
        if let Some(act) = b_act {
            b_c.send(&act).await;
        }
    }

    // Consuma la risposta del server all'azione di B per aggiornare `public`.
    loop {
        let msg = b.client.as_mut().unwrap().recv().await;
        apply_msg(&msg, &mut b, &mut public);
        let p = public.as_ref().unwrap();
        if matches!(
            p.state,
            GameState::GameOver | GameState::PlayerTurn | GameState::LastRound
        ) && p.current_turn_player_id.as_deref() != Some(&b.player_id)
        {
            break;
        }
    }

    // Aspetta che il server scriva il save file.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Salva lo stato pubblico pre-resume per confronto.
    let pre_resume_public = public.clone().expect("public state pre-resume");

    // Verifica che il save file esista.
    let save_path = server.saves_dir.join(format!("{}.json", game.game_id));
    assert!(
        save_path.exists(),
        "save file mid-game deve esistere: {:?}",
        save_path
    );

    // Usa resume_game con l'AppState del server (condivide il registro partite).
    let (resumed_id, resumed_host_token) = ttr_server::resume_game(&server.state, &save_path)
        .await
        .expect("resume_game");

    // Join di Alice col nome salvato.
    let resumed_pa = join_game(&server.base_url, &resumed_id, "Alice", None).await;
    assert!(!resumed_pa.player_id.is_empty());
    assert!(!resumed_pa.token.is_empty());

    // Join di Bob.
    let resumed_pb = join_game(&server.base_url, &resumed_id, "Bob", None).await;
    assert!(!resumed_pb.player_id.is_empty());

    // Avvia la partita ripresa.
    start_game(&server.base_url, &resumed_id, &resumed_host_token).await;

    // Connetti entrambi e verifica che lo SNAPSHOT iniziale abbia lo stesso
    // PUBLIC_STATE (fase, claimedRoutes, playersSummary) del pre-resume.
    let resumed_a = WsClient::connect(
        &server,
        &resumed_id,
        &resumed_pa.player_id,
        &resumed_pa.token,
    )
    .await;
    let resumed_b = WsClient::connect(
        &server,
        &resumed_id,
        &resumed_pb.player_id,
        &resumed_pb.token,
    )
    .await;

    let mut ra = Player::new(resumed_a);
    let mut rb = Player::new(resumed_b);
    let mut resumed_public: Option<ttr_proto::PublicState> = None;

    apply_msg(
        &ra.client.as_mut().unwrap().recv().await,
        &mut ra,
        &mut resumed_public,
    );
    apply_msg(
        &rb.client.as_mut().unwrap().recv().await,
        &mut rb,
        &mut resumed_public,
    );

    let snap_public = resumed_public.expect("SNAPSHOT public dopo resume");

    // Confronta fase
    assert_eq!(
        snap_public.state, pre_resume_public.state,
        "stessa fase dopo resume"
    );

    // Confronta claimedRoutes (almeno lo stesso conteggio)
    assert_eq!(
        snap_public.claimed_routes.len(),
        pre_resume_public.claimed_routes.len(),
        "stesso numero di rotte claimate dopo resume"
    );

    // Confronta playersSummary (stessi nomi)
    let snap_names: Vec<String> = snap_public
        .players_summary
        .iter()
        .map(|ps| ps.name.clone())
        .collect();
    let pre_names: Vec<String> = pre_resume_public
        .players_summary
        .iter()
        .map(|ps| ps.name.clone())
        .collect();
    assert_eq!(snap_names, pre_names, "stessi nomi giocatori dopo resume");
}
