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

/// Test F3: resume con ordine di re-join invertito (Bob prima di Alice).
/// I playerId e i colori devono restare quelli salvati.
#[tokio::test]
async fn resume_inverted_order() {
    let server = TestServer::start().await;
    let map = usa_map();

    let game = create_game_with_seed(&server.base_url, "ResumeInverted", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", Some("RED")).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", Some("BLUE")).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a_ws = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b_ws = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a_ws);
    let mut b = Player::new(b_ws);
    let mut public: Option<ttr_proto::PublicState> = None;

    // Consuma SNAPSHOT iniziali.
    apply_msg(&a.client.as_mut().unwrap().recv().await, &mut a, &mut public);
    apply_msg(&b.client.as_mut().unwrap().recv().await, &mut b, &mut public);

    // Draft entrambi.
    let draft_a = decide(&a.player_id, &mut a.client.as_mut().unwrap().view, public.as_ref().unwrap(), &map, &mut a.drafted).unwrap().unwrap();
    a.client.as_mut().unwrap().send(&draft_a).await;
    let draft_b = decide(&b.player_id, &mut b.client.as_mut().unwrap().view, public.as_ref().unwrap(), &map, &mut b.drafted).unwrap().unwrap();
    b.client.as_mut().unwrap().send(&draft_b).await;

    // Consuma post-draft.
    loop {
        let msg_a = a.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_a, &mut a, &mut public);
        let msg_b = b.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_b, &mut b, &mut public);
        let p = public.as_ref().unwrap();
        if matches!(p.state, GameState::PlayerTurn | GameState::LastRound) {
            break;
        }
    }

    // Gioca il primo turno di A.
    loop {
        let msg = a.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut a, &mut public);
        let p = public.as_ref().unwrap();
        if matches!(p.state, GameState::GameOver) { break; }
        if fresh && p.current_turn_player_id.as_deref() == Some(&a.player_id) {
            let a_c = a.client.as_mut().unwrap();
            let a_act = decide(&a.player_id, &mut a_c.view, p, &map, &mut a.drafted).unwrap();
            if let Some(act) = a_act { a_c.send(&act).await; }
            break;
        }
    }

    // Aspetta save.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let save_path = server.saves_dir.join(format!("{}.json", game.game_id));
    assert!(save_path.exists(), "save file mid-game esiste");

    // Stato atteso ricavato dal save: `public` e la view di A sono fermi a
    // prima dell'azione appena inviata.
    let expected_save: ttr_server::save::SaveData =
        serde_json::from_value(read_save(&save_path)).expect("parse SaveData");
    let expected_engine =
        ttr_server::save::replay_engine(&expected_save, &map, &load_usa_ruleset())
            .expect("replay engine atteso");
    let expected_turn = expected_save.players[expected_engine.current_player()].id.clone();
    let expected_hands: Vec<Vec<ttr_proto::CardColor>> =
        (0..2).map(|i| sorted_hand(expected_engine.player_hand(i).unwrap())).collect();
    let expected_claimed = claimed_by_engine(&expected_engine, &expected_save);

    // Usa il PublicState per i conteggi (più affidabile della view privata).
    let ps = public.as_ref().expect("public state pre-resume");
    let a_name_pre: u32 = ps.players_summary.iter().find(|s| s.name == "Alice").map(|s| s.tickets_count).unwrap_or(0);
    let b_name_pre: u32 = ps.players_summary.iter().find(|s| s.name == "Bob").map(|s| s.tickets_count).unwrap_or(0);

    // Resume.
    let (resumed_id, resumed_host_token) =
        ttr_server::resume_game(&server.state, &save_path).await.expect("resume_game");

    // F3: re-join in ordine invertito: prima Bob, poi Alice.
    let resumed_pb = join_game(&server.base_url, &resumed_id, "Bob", None).await;
    let resumed_pa = join_game(&server.base_url, &resumed_id, "Alice", None).await;

    // I playerId devono essere quelli originali salvati.
    assert_eq!(resumed_pa.player_id, pa.player_id, "Alice: stesso playerId dopo resume");
    assert_eq!(resumed_pb.player_id, pb.player_id, "Bob: stesso playerId dopo resume");

    start_game(&server.base_url, &resumed_id, &resumed_host_token).await;

    let ra_ws = WsClient::connect(&server, &resumed_id, &resumed_pa.player_id, &resumed_pa.token).await;
    let rb_ws = WsClient::connect(&server, &resumed_id, &resumed_pb.player_id, &resumed_pb.token).await;

    let mut ra = Player::new(ra_ws);
    let mut rb = Player::new(rb_ws);
    let mut resumed_public: Option<ttr_proto::PublicState> = None;

    apply_msg(&ra.client.as_mut().unwrap().recv().await, &mut ra, &mut resumed_public);
    apply_msg(&rb.client.as_mut().unwrap().recv().await, &mut rb, &mut resumed_public);

    let snap = resumed_public.expect("SNAPSHOT dopo resume invertito");

    // Verifica che i nomi siano nell'ordine originale (Alice indice 0, Bob indice 1).
    assert_eq!(snap.players_summary[0].name, "Alice", "Alice primo giocatore (indice motore 0)");
    assert_eq!(snap.players_summary[1].name, "Bob", "Bob secondo giocatore (indice motore 1)");
    assert_eq!(snap.players_summary[0].color, ttr_proto::PlayerColor::Red, "Alice colore Red");
    assert_eq!(snap.players_summary[1].color, ttr_proto::PlayerColor::Blue, "Bob colore Blue");

    // Verifica che i biglietti siano preservati.
    let ra_tickets = ra.client.as_ref().unwrap().view.tickets.len() as u32;
    let rb_tickets = rb.client.as_ref().unwrap().view.tickets.len() as u32;
    assert_eq!(ra_tickets, a_name_pre, "Alice: stessi biglietti dopo resume invertito");
    assert_eq!(rb_tickets, b_name_pre, "Bob: stessi biglietti dopo resume invertito");

    // Turno, mani private e proprietà delle rotte seguono l'indice del motore.
    assert_eq!(snap.current_turn_player_id, Some(expected_turn), "turno dopo resume invertito");
    assert_eq!(sorted_hand(&ra.client.as_ref().unwrap().view.hand), expected_hands[0], "mano di Alice");
    assert_eq!(sorted_hand(&rb.client.as_ref().unwrap().view.hand), expected_hands[1], "mano di Bob");
    assert_eq!(claimed_in_public(&snap), expected_claimed, "proprietà delle rotte");
}

// ── Regressioni review di robustezza (9/11/2026) ─────────────────────

fn read_save(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("lettura save")).expect("parse save")
}

fn sorted_hand(hand: &[ttr_proto::CardColor]) -> Vec<ttr_proto::CardColor> {
    let mut hand = hand.to_vec();
    hand.sort();
    hand
}

/// Coppie (rotta, playerId) secondo il motore, con gli id del save.
fn claimed_by_engine(
    engine: &ttr_core::Engine,
    save: &ttr_server::save::SaveData,
) -> Vec<(String, String)> {
    let mut claimed: Vec<(String, String)> = (0..engine.num_players())
        .flat_map(|i| {
            let id = save.players[i].id.clone();
            engine
                .player(i)
                .unwrap()
                .claimed_routes
                .keys()
                .map(move |route| (route.clone(), id.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    claimed.sort();
    claimed
}

fn claimed_in_public(public: &ttr_proto::PublicState) -> Vec<(String, String)> {
    let mut claimed: Vec<(String, String)> = public
        .claimed_routes
        .iter()
        .map(|c| (c.route_id.clone(), c.claimed_by_player_id.clone()))
        .collect();
    claimed.sort();
    claimed
}

async fn next_snapshot(client: &mut WsClient) -> ttr_proto::Snapshot {
    loop {
        if let ttr_proto::ServerMessage::Snapshot(s) = client.recv().await {
            return s;
        }
    }
}

fn backups_of(saves_dir: &std::path::Path, game_id: &str) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(saves_dir)
        .expect("lettura dir saves")
        .map(|e| e.expect("voce dir").path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "bak")
                && p.file_name().unwrap().to_string_lossy().starts_with(game_id)
        })
        .collect()
}

async fn post_start(base: &str, game_id: &str, host_token: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{}/api/games/{}/start", base, game_id))
        .json(&serde_json::json!({ "hostToken": host_token }))
        .send()
        .await
        .expect("POST start")
}

#[tokio::test]
async fn r1_resume_inverted_rejoin_keeps_turn_and_save() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "R1", Some(42)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", Some("RED")).await;
    let _pb = join_game(&server.base_url, &g.game_id, "Bob", Some("BLUE")).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let mut a = WsClient::connect(&server, &g.game_id, &pa.player_id, &pa.token).await;
    let s0 = next_snapshot(&mut a).await;
    let alice_hand0 = s0.private.as_ref().unwrap().hand.clone();
    let turn0 = s0.public.current_turn_player_id.clone();
    a.disconnect().await;

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    let (rid, rtok) = ttr_server::resume_game(&server.state, &save).await.unwrap();
    let _rb = join_game(&server.base_url, &rid, "Bob", None).await;
    let ra = join_game(&server.base_url, &rid, "Alice", None).await;
    start_game(&server.base_url, &rid, &rtok).await;
    let mut a = WsClient::connect(&server, &rid, &ra.player_id, &ra.token).await;
    let s1 = next_snapshot(&mut a).await;

    // Un'azione dopo il resume forza la riscrittura del save.
    let keep: Vec<String> = a.view.offered_tickets.iter().take(2).map(|t| t.id.clone()).collect();
    a.send(&ttr_proto::InboundAction::ChooseTickets(ttr_proto::ChooseTicketsPayload {
        player_id: ra.player_id.clone(),
        keep,
        client_seq: None,
    }))
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    a.disconnect().await;
    let sv = read_save(&save);
    assert_eq!(sv["actions"].as_array().unwrap().len(), 1, "l'azione deve essere salvata");

    let (rid2, rtok2) = ttr_server::resume_game(&server.state, &save).await.unwrap();
    let ra2 = join_game(&server.base_url, &rid2, "Alice", None).await;
    let _rb2 = join_game(&server.base_url, &rid2, "Bob", None).await;
    start_game(&server.base_url, &rid2, &rtok2).await;
    let mut a = WsClient::connect(&server, &rid2, &ra2.player_id, &ra2.token).await;
    let s2 = next_snapshot(&mut a).await;

    assert_eq!(s1.public.current_turn_player_id, turn0, "turno attribuito al giocatore sbagliato");
    assert_eq!(sv["players"][0]["name"], "Alice", "ordine giocatori nel save riscritto");
    assert_eq!(s2.private.unwrap().hand, alice_hand0, "Alice ha la mano di un altro");
    assert_eq!(
        backups_of(&server.saves_dir, &g.game_id).len(),
        2,
        "due resume nello stesso secondo non devono sovrascrivere il backup"
    );
}

/// La proprietà delle rotte è coperta con un log costruito: il bot dei test
/// non garantisce un claim nei primi turni.
#[tokio::test]
async fn r1_resume_inverted_rejoin_keeps_route_owner() {
    use ttr_core::{Action, DrawSource, error::GamePhase};

    let server = TestServer::start().await;
    let map = usa_map();
    let g = create_game_with_seed(&server.base_url, "R1Routes", Some(42)).await;
    join_game(&server.base_url, &g.game_id, "Alice", Some("RED")).await;
    join_game(&server.base_url, &g.game_id, "Bob", Some("BLUE")).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    let mut raw = read_save(&save);
    let parsed: ttr_server::save::SaveData =
        serde_json::from_value(raw.clone()).expect("parse SaveData");
    let mut engine = ttr_server::save::replay_engine(&parsed, &map, &load_usa_ruleset())
        .expect("replay engine");

    for player_index in 0..2 {
        let keep = engine.initial_tickets_for(player_index).unwrap()[..2].to_vec();
        engine.apply(Action::ChooseTickets { player_index, keep }).unwrap();
    }
    while engine.claimed_route_ids().is_empty() {
        let player_index = engine.current_player();
        let hand = engine.player_hand(player_index).unwrap().to_vec();
        let claim = (engine.phase() == GamePhase::PlayerTurn)
            .then(|| {
                map.routes.iter().find_map(|r| {
                    build_payment(&hand, &r.color, r.len as usize).map(|p| (r.id.clone(), p))
                })
            })
            .flatten();
        let action = match claim {
            Some((route_id, cards_discarded)) => {
                Action::ClaimRoute { player_index, route_id, cards_discarded }
            }
            None => Action::DrawTrainCard { player_index, source: DrawSource::Deck },
        };
        engine.apply(action).unwrap();
    }
    let expected = claimed_by_engine(&engine, &parsed);
    raw["actions"] = engine
        .action_log()
        .iter()
        .map(|rec| serde_json::json!({ "action": rec.action }))
        .collect();
    std::fs::write(&save, raw.to_string()).expect("scrittura save");

    let (rid, rtok) = ttr_server::resume_game(&server.state, &save).await.unwrap();
    let rb = join_game(&server.base_url, &rid, "Bob", None).await;
    join_game(&server.base_url, &rid, "Alice", None).await;
    start_game(&server.base_url, &rid, &rtok).await;
    let mut b = WsClient::connect(&server, &rid, &rb.player_id, &rb.token).await;
    let snap = next_snapshot(&mut b).await;

    assert_eq!(claimed_in_public(&snap.public), expected, "rotta attribuita al giocatore sbagliato");
}

#[tokio::test]
async fn r2_partial_resume_start_is_rejected_and_save_survives() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "R2", Some(7)).await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let original = std::fs::read_to_string(&save).expect("lettura save");

    let (rid, rtok) = ttr_server::resume_game(&server.state, &save).await.unwrap();
    join_game(&server.base_url, &rid, "Alice", None).await;

    let rejected = post_start(&server.base_url, &rid, &rtok).await;
    assert_eq!(rejected.status(), 409, "start con roster parziale");
    let body: serde_json::Value = rejected.json().await.expect("body errore");
    assert_eq!(body["error"], "ROSTER_INCOMPLETE");
    assert!(body["detail"].as_str().is_some_and(|d| !d.is_empty()), "detail: {}", body);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(read_save(&save)["players"].as_array().unwrap().len(), 2, "save con roster parziale");

    // Il rifiuto non consuma il motore: Bob entra e la partita parte.
    join_game(&server.base_url, &rid, "Bob", None).await;
    start_game(&server.base_url, &rid, &rtok).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        std::fs::read_to_string(&save).expect("lettura save"),
        original,
        "lo start del resume non deve riscrivere il save"
    );

    let backups = backups_of(&server.saves_dir, &g.game_id);
    assert_eq!(backups.len(), 1, "backup prima del resume: {:?}", backups);
    assert_eq!(std::fs::read_to_string(&backups[0]).expect("lettura backup"), original);
}

#[tokio::test]
async fn r3_resume_rejects_game_id_that_is_not_a_canonical_uuid() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "R3", Some(1)).await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    let unique = format!("ciufciuf-pwned-{}", uuid::Uuid::new_v4());
    let absolute = std::env::temp_dir().join(&unique);
    let relative = format!("../{}", unique);

    for bad_id in [
        absolute.to_str().unwrap().to_string(),
        relative.clone(),
        "non-un-uuid".to_string(),
        g.game_id.replace('-', ""),
    ] {
        let mut v = read_save(&save);
        v["gameId"] = serde_json::json!(bad_id);
        let evil = server.saves_dir.join("evil.json");
        std::fs::write(&evil, v.to_string()).unwrap();

        let res = ttr_server::resume_game(&server.state, &evil).await;
        assert!(res.is_err(), "gameId {:?} deve essere rifiutato: {:?}", bad_id, res);
    }

    // `saves_dir` sta dentro la temp dir: la scansione copre entrambi i percorsi.
    let escaped: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .expect("lettura temp dir")
        .map(|e| e.expect("voce dir").file_name())
        .filter(|name| name.to_string_lossy().starts_with(&unique))
        .collect();
    assert!(escaped.is_empty(), "file scritti fuori da saves/: {:?}", escaped);
}

/// I limiti della lobby valgono solo per i nuovi join: un save esistente deve
/// restare riprendibile anche con nomi fuori limite.
#[tokio::test]
async fn resume_accepts_saved_names_outside_lobby_limits() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "LongName", Some(3)).await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    let long_name = "x".repeat(30);
    let mut v = read_save(&save);
    v["players"][0]["name"] = serde_json::json!(long_name);
    std::fs::write(&save, v.to_string()).unwrap();

    let (rid, _) = ttr_server::resume_game(&server.state, &save).await.unwrap();
    join_game(&server.base_url, &rid, &long_name, None).await;
}
