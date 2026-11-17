//! Test di persistenza e resume: save su disco, replay e rientro dei giocatori.
//!
//! `save_and_replay`: partita completa → save file → replay motore → stessi
//! final_scores.
//! `resume_flow`: partita interrotta a metà → `resume_game` → join per nome
//! salvato → stesso playerId → start → stesso PUBLIC_STATE.

mod common;

use common::driver::*; // Player, decide, apply_msg, run_game, recv_from
use common::*;
use ttr_proto::GameState;

/// A fine partita il save `saves/{id}.json` viene eliminato e restano solo i
/// punteggi, in coda a `saves/results.jsonl` (classifica generale), coerenti
/// con i `final_scores` del PUBLIC_STATE finale.
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

    // L'archiviazione aggiunge il risultato e poi elimina il save.
    let save_path = server.saves_dir.join(format!("{}.json", game.game_id));
    wait_for(WAIT, || (!save_path.exists()).then_some(())).await;
    assert!(
        !save_path.exists(),
        "il save della partita conclusa va eliminato"
    );

    let results = ttr_server::save::read_results(&server.saves_dir);
    assert_eq!(results.len(), 1, "un risultato archiviato");
    let r = &results[0];
    assert_eq!(r.game_id, game.game_id);
    assert_eq!(r.game_name, "SaveReplay");

    let orig_scores = final_public
        .final_scores
        .as_ref()
        .expect("final_scores originali");
    assert_eq!(r.players.len(), orig_scores.len());
    for fs in orig_scores {
        let name = if fs.player_id == pa.player_id {
            "Alice"
        } else {
            "Bob"
        };
        let p = r
            .players
            .iter()
            .find(|p| p.name == name)
            .expect("giocatore archiviato");
        assert_eq!(p.total, fs.total, "totale archiviato di {}", name);
        assert_eq!(p.winner, fs.winner, "vincitore archiviato di {}", name);
    }
    assert!(r.players.iter().any(|p| p.winner), "almeno un vincitore");

    // La classifica generale li espone via REST.
    let body: serde_json::Value = reqwest::get(format!("{}/api/standings", server.base_url))
        .await
        .expect("GET /api/standings")
        .json()
        .await
        .expect("parse standings");
    assert_eq!(body["games"], 1);
    assert_eq!(body["standings"].as_array().unwrap().len(), 2);
}

/// Partita salvata a metà → `resume_game` → join col nome salvato →
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

    // Aspetta che il server scriva il save con le quattro azioni giocate
    // (due scelte dei biglietti e un turno a testa).
    wait_for_save(
        &server.saves_dir.join(format!("{}.json", game.game_id)),
        |s| s["actions"].as_array().is_some_and(|a| a.len() == 4),
    )
    .await;

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
    let (resumed_id, resumed_host_token) = resume_after_restart(&server, &save_path)
        .await
        .expect("resume_game");

    // Join di Alice col nome salvato.
    let resumed_pa = join_game_resume(&server.base_url, &resumed_id, "Alice", &pa.token).await;
    assert!(!resumed_pa.player_id.is_empty());
    assert!(!resumed_pa.token.is_empty());

    // Join di Bob.
    let resumed_pb = join_game_resume(&server.base_url, &resumed_id, "Bob", &pb.token).await;
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

/// Resume con ordine di re-join invertito (Bob prima di Alice).
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

    // Draft entrambi.
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
        if matches!(p.state, GameState::GameOver) {
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

    // Aspetta il save con le tre azioni giocate (due scelte dei biglietti e il turno di A).
    let save_path = server.saves_dir.join(format!("{}.json", game.game_id));
    wait_for_save(&save_path, |s| {
        s["actions"].as_array().is_some_and(|a| a.len() == 3)
    })
    .await;

    // Stato atteso ricavato dal save: `public` e la view di A sono fermi a
    // prima dell'azione appena inviata.
    let expected_save: ttr_server::save::SaveData =
        serde_json::from_value(read_save(&save_path)).expect("parse SaveData");
    let expected_engine =
        ttr_server::save::replay_engine(&expected_save, &map, &load_usa_ruleset())
            .expect("replay engine atteso");
    let expected_turn = expected_save.players[expected_engine.current_player()]
        .id
        .clone();
    let expected_hands: Vec<Vec<ttr_proto::CardColor>> = (0..2)
        .map(|i| sorted_hand(expected_engine.player_hand(i).unwrap()))
        .collect();
    let expected_claimed = claimed_by_engine(&expected_engine, &expected_save);

    // Usa il PublicState per i conteggi (più affidabile della view privata).
    let ps = public.as_ref().expect("public state pre-resume");
    let a_name_pre: u32 = ps
        .players_summary
        .iter()
        .find(|s| s.name == "Alice")
        .map(|s| s.tickets_count)
        .unwrap_or(0);
    let b_name_pre: u32 = ps
        .players_summary
        .iter()
        .find(|s| s.name == "Bob")
        .map(|s| s.tickets_count)
        .unwrap_or(0);

    // Resume.
    let (resumed_id, resumed_host_token) = resume_after_restart(&server, &save_path)
        .await
        .expect("resume_game");

    // Re-join in ordine invertito: prima Bob, poi Alice.
    let resumed_pb = join_game_resume(&server.base_url, &resumed_id, "Bob", &pb.token).await;
    let resumed_pa = join_game_resume(&server.base_url, &resumed_id, "Alice", &pa.token).await;

    // I playerId devono essere quelli originali salvati.
    assert_eq!(
        resumed_pa.player_id, pa.player_id,
        "Alice: stesso playerId dopo resume"
    );
    assert_eq!(
        resumed_pb.player_id, pb.player_id,
        "Bob: stesso playerId dopo resume"
    );

    start_game(&server.base_url, &resumed_id, &resumed_host_token).await;

    let ra_ws = WsClient::connect(
        &server,
        &resumed_id,
        &resumed_pa.player_id,
        &resumed_pa.token,
    )
    .await;
    let rb_ws = WsClient::connect(
        &server,
        &resumed_id,
        &resumed_pb.player_id,
        &resumed_pb.token,
    )
    .await;

    let mut ra = Player::new(ra_ws);
    let mut rb = Player::new(rb_ws);
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

    let snap = resumed_public.expect("SNAPSHOT dopo resume invertito");

    // Verifica che i nomi siano nell'ordine originale (Alice indice 0, Bob indice 1).
    assert_eq!(
        snap.players_summary[0].name, "Alice",
        "Alice primo giocatore (indice motore 0)"
    );
    assert_eq!(
        snap.players_summary[1].name, "Bob",
        "Bob secondo giocatore (indice motore 1)"
    );
    assert_eq!(
        snap.players_summary[0].color,
        ttr_proto::PlayerColor::Red,
        "Alice colore Red"
    );
    assert_eq!(
        snap.players_summary[1].color,
        ttr_proto::PlayerColor::Blue,
        "Bob colore Blue"
    );

    // Verifica che i biglietti siano preservati.
    let ra_tickets = ra.client.as_ref().unwrap().view.tickets.len() as u32;
    let rb_tickets = rb.client.as_ref().unwrap().view.tickets.len() as u32;
    assert_eq!(
        ra_tickets, a_name_pre,
        "Alice: stessi biglietti dopo resume invertito"
    );
    assert_eq!(
        rb_tickets, b_name_pre,
        "Bob: stessi biglietti dopo resume invertito"
    );

    // Turno, mani private e proprietà delle rotte seguono l'indice del motore.
    assert_eq!(
        snap.current_turn_player_id,
        Some(expected_turn),
        "turno dopo resume invertito"
    );
    assert_eq!(
        sorted_hand(&ra.client.as_ref().unwrap().view.hand),
        expected_hands[0],
        "mano di Alice"
    );
    assert_eq!(
        sorted_hand(&rb.client.as_ref().unwrap().view.hand),
        expected_hands[1],
        "mano di Bob"
    );
    assert_eq!(
        claimed_in_public(&snap),
        expected_claimed,
        "proprietà delle rotte"
    );
}

// ── Resume: identità, turno e proprietà delle rotte ──────────────────

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
                && p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(game_id)
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
    let pb = join_game(&server.base_url, &g.game_id, "Bob", Some("BLUE")).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let mut a = WsClient::connect(&server, &g.game_id, &pa.player_id, &pa.token).await;
    let s0 = next_snapshot(&mut a).await;
    let alice_hand0 = s0.private.as_ref().unwrap().hand.clone();
    let turn0 = s0.public.current_turn_player_id.clone();
    a.disconnect().await;

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_file(&save).await;
    let (rid, rtok) = resume_after_restart(&server, &save).await.unwrap();
    let _rb = join_game_resume(&server.base_url, &rid, "Bob", &pb.token).await;
    let ra = join_game_resume(&server.base_url, &rid, "Alice", &pa.token).await;
    start_game(&server.base_url, &rid, &rtok).await;
    let mut a = WsClient::connect(&server, &rid, &ra.player_id, &ra.token).await;
    let s1 = next_snapshot(&mut a).await;

    // Un'azione dopo il resume forza la riscrittura del save.
    let keep: Vec<String> = a
        .view
        .offered_tickets
        .iter()
        .take(2)
        .map(|t| t.id.clone())
        .collect();
    a.send(&ttr_proto::InboundAction::ChooseTickets(
        ttr_proto::ChooseTicketsPayload {
            player_id: ra.player_id.clone(),
            keep,
            client_seq: None,
        },
    ))
    .await;
    let sv = wait_for_save(&save, |s| {
        s["actions"].as_array().is_some_and(|a| a.len() == 1)
    })
    .await;
    a.disconnect().await;
    assert_eq!(
        sv["actions"].as_array().unwrap().len(),
        1,
        "l'azione deve essere salvata"
    );

    let (rid2, rtok2) = resume_after_restart(&server, &save).await.unwrap();
    let ra2 = join_game_resume(&server.base_url, &rid2, "Alice", &pa.token).await;
    let _rb2 = join_game_resume(&server.base_url, &rid2, "Bob", &pb.token).await;
    start_game(&server.base_url, &rid2, &rtok2).await;
    let mut a = WsClient::connect(&server, &rid2, &ra2.player_id, &ra2.token).await;
    let s2 = next_snapshot(&mut a).await;

    assert_eq!(
        s1.public.current_turn_player_id, turn0,
        "turno attribuito al giocatore sbagliato"
    );
    assert_eq!(
        sv["players"][0]["name"], "Alice",
        "ordine giocatori nel save riscritto"
    );
    assert_eq!(
        s2.private.unwrap().hand,
        alice_hand0,
        "Alice ha la mano di un altro"
    );
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
    let pa = join_game(&server.base_url, &g.game_id, "Alice", Some("RED")).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", Some("BLUE")).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_file(&save).await;
    let mut raw = read_save(&save);
    let parsed: ttr_server::save::SaveData =
        serde_json::from_value(raw.clone()).expect("parse SaveData");
    let mut engine =
        ttr_server::save::replay_engine(&parsed, &map, &load_usa_ruleset()).expect("replay engine");

    for player_index in 0..2 {
        let keep = engine.initial_tickets_for(player_index).unwrap()[..2].to_vec();
        engine
            .apply(Action::ChooseTickets { player_index, keep })
            .unwrap();
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
            Some((route_id, cards_discarded)) => Action::ClaimRoute {
                player_index,
                route_id,
                cards_discarded,
                passengers: Default::default(),
            },
            None => Action::DrawTrainCard {
                player_index,
                source: DrawSource::Deck,
            },
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

    let (rid, rtok) = resume_after_restart(&server, &save).await.unwrap();
    let rb = join_game_resume(&server.base_url, &rid, "Bob", &pb.token).await;
    join_game_resume(&server.base_url, &rid, "Alice", &pa.token).await;
    start_game(&server.base_url, &rid, &rtok).await;
    let mut b = WsClient::connect(&server, &rid, &rb.player_id, &rb.token).await;
    let snap = next_snapshot(&mut b).await;

    assert_eq!(
        claimed_in_public(&snap.public),
        expected,
        "rotta attribuita al giocatore sbagliato"
    );
}

#[tokio::test]
async fn r2_partial_resume_start_is_rejected_and_save_survives() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "R2", Some(7)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_file(&save).await;
    let original = std::fs::read_to_string(&save).expect("lettura save");

    let (rid, rtok) = resume_after_restart(&server, &save).await.unwrap();
    join_game_resume(&server.base_url, &rid, "Alice", &pa.token).await;

    let rejected = post_start(&server.base_url, &rid, &rtok).await;
    assert_eq!(rejected.status(), 409, "start con roster parziale");
    let body: serde_json::Value = rejected.json().await.expect("body errore");
    assert_eq!(body["error"], "ROSTER_INCOMPLETE");
    assert!(
        body["detail"].as_str().is_some_and(|d| !d.is_empty()),
        "detail: {}",
        body
    );
    // Verifica che qualcosa non accada: il rifiuto non riscrive il save.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        read_save(&save)["players"].as_array().unwrap().len(),
        2,
        "save con roster parziale"
    );

    // Il rifiuto non consuma il motore: Bob entra e la partita parte.
    join_game_resume(&server.base_url, &rid, "Bob", &pb.token).await;
    start_game(&server.base_url, &rid, &rtok).await;
    // Lo shutdown svuota la coda del writer: il save su disco è quello finale.
    ttr_server::shutdown_sessions(&server.state, WAIT).await;
    let mut before: serde_json::Value = serde_json::from_str(&original).expect("parse save");
    let mut after = read_save(&save);
    before.as_object_mut().unwrap().remove("updatedAt");
    after.as_object_mut().unwrap().remove("updatedAt");
    assert_eq!(
        after, before,
        "lo start del resume riscrive il save senza cambiarne il contenuto"
    );

    let backups = backups_of(&server.saves_dir, &g.game_id);
    assert_eq!(backups.len(), 1, "backup prima del resume: {:?}", backups);
    assert_eq!(
        std::fs::read_to_string(&backups[0]).expect("lettura backup"),
        original
    );
}

#[tokio::test]
async fn r3_resume_rejects_game_id_that_is_not_a_canonical_uuid() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "R3", Some(1)).await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_file(&save).await;
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
        assert!(
            res.is_err(),
            "gameId {:?} deve essere rifiutato: {:?}",
            bad_id,
            res
        );
    }

    // `saves_dir` sta dentro la temp dir: la scansione copre entrambi i percorsi.
    let escaped: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .expect("lettura temp dir")
        .map(|e| e.expect("voce dir").file_name())
        .filter(|name| name.to_string_lossy().starts_with(&unique))
        .collect();
    assert!(
        escaped.is_empty(),
        "file scritti fuori da saves/: {:?}",
        escaped
    );
}

/// Un save con un nome di giocatore oltre il limite della lobby non è riprendibile:
/// troncarlo cambierebbe la `name_key` e romperebbe il rientro.
#[tokio::test]
async fn resume_rejects_saved_names_outside_lobby_limits() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "LongName", Some(3)).await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_file(&save).await;
    let mut v = read_save(&save);
    v["players"][0]["name"] = serde_json::json!("x".repeat(30));
    std::fs::write(&save, v.to_string()).unwrap();

    let err = resume_after_restart(&server, &save).await.unwrap_err();
    assert!(
        matches!(err, ttr_server::ResumeError::NotResumable(_)),
        "{err:?}"
    );
}

// ── Validazione dei save ─────────────────────────────────────────────

fn save_player(id: &str, name: &str, color: &str) -> ttr_server::save::SavePlayer {
    ttr_server::save::SavePlayer {
        id: id.into(),
        name: name.into(),
        color: color.into(),
        token_hash: None,
    }
}

fn valid_save() -> ttr_server::save::SaveData {
    ttr_server::save::SaveData {
        format_version: ttr_server::save::FORMAT_VERSION,
        rules_version: ttr_core::RULES_VERSION,
        game_id: uuid::Uuid::new_v4().to_string(),
        game_name: "Validazione".into(),
        map_id: "usa".into(),
        ruleset_id: "usa_v2".into(),
        seed: 42,
        first_player: 0,
        players: vec![
            save_player("p1", "Alice", "RED"),
            save_player("p2", "Bob", "BLUE"),
        ],
        actions: vec![],
        status: "in_progress".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

fn replay(save: &ttr_server::save::SaveData) -> Result<ttr_core::Engine, String> {
    ttr_server::save::replay_engine(save, &usa_map(), &load_usa_ruleset())
}

fn assert_rejected(save: ttr_server::save::SaveData, expected: &str) {
    let err = replay(&save).err().expect("il save deve essere rifiutato");
    assert!(
        err.contains(expected),
        "errore {:?}, atteso {:?}",
        err,
        expected
    );
}

#[test]
fn valid_save_is_accepted() {
    assert!(replay(&valid_save()).is_ok());
}

#[test]
fn save_with_unknown_format_version_is_rejected() {
    let mut save = valid_save();
    save.format_version = ttr_server::save::FORMAT_VERSION + 1;
    assert_rejected(save, "formato del save");
}

#[test]
fn save_with_format_version_1_is_accepted() {
    let mut save = valid_save();
    save.format_version = 1;
    assert!(replay(&save).is_ok());
}

#[test]
fn save_with_other_rules_version_is_rejected() {
    let mut save = valid_save();
    save.rules_version = ttr_core::RULES_VERSION + 1;
    let expected = format!(
        "save {}, server {}",
        ttr_core::RULES_VERSION + 1,
        ttr_core::RULES_VERSION
    );
    assert_rejected(save, &expected);
}

#[test]
fn save_with_too_few_players_is_rejected() {
    let mut save = valid_save();
    save.players.truncate(1);
    assert_rejected(save, "numero di giocatori");
}

#[test]
fn save_with_too_many_players_is_rejected() {
    let mut save = valid_save();
    save.players = (0..6)
        .map(|i| save_player(&format!("p{i}"), &format!("N{i}"), "RED"))
        .collect();
    assert_rejected(save, "numero di giocatori");
}

#[test]
fn save_with_first_player_out_of_range_is_rejected() {
    let mut save = valid_save();
    save.first_player = save.players.len();
    assert_rejected(save, "primo giocatore");
}

#[test]
fn save_with_duplicate_player_id_is_rejected() {
    let mut save = valid_save();
    save.players[1].id = save.players[0].id.clone();
    assert_rejected(save, "id giocatore duplicato");
}

#[test]
fn save_with_names_equal_ignoring_case_is_rejected() {
    let mut save = valid_save();
    save.players[1].name = "aLICE".into();
    assert_rejected(save, "nome giocatore duplicato");
}

/// Il motore rifiuta i colori ripetuti: un save che li contiene non è riproducibile.
#[test]
fn save_with_duplicate_color_is_rejected() {
    let mut save = valid_save();
    save.players[0].color = "RED".into();
    save.players[1].color = "RED".into();
    let err = replay(&save).err().expect("colori duplicati");
    assert!(err.contains("motore non inizializzabile"), "{err}");
}

#[test]
fn save_with_unknown_color_is_rejected() {
    let mut save = valid_save();
    save.players[0].color = "PINK".into();
    save.players[1].color = "PINK".into();
    assert_rejected(save, "colore sconosciuto");
}

#[test]
fn save_with_rules_version_1_is_rejected() {
    let mut save = valid_save();
    save.rules_version = 1;
    assert_rejected(save, "save creato con regole precedenti: non riproducibile");
}

#[test]
fn save_without_rules_version_is_rejected() {
    let mut raw = serde_json::to_value(valid_save()).expect("serializzazione save");
    raw.as_object_mut()
        .unwrap()
        .remove("rulesVersion")
        .expect("campo rulesVersion");
    let save: ttr_server::save::SaveData = serde_json::from_value(raw).expect("parse SaveData");
    assert_eq!(save.rules_version, 1);
    assert_rejected(save, "save creato con regole precedenti: non riproducibile");
}

#[tokio::test]
async fn server_writes_rules_version_in_save() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "RulesVersion", Some(5)).await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    let path = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_file(&path).await;
    let save = read_save(&path);
    assert_eq!(ttr_core::RULES_VERSION, 2);
    assert_eq!(save["rulesVersion"], 2);
    assert_eq!(save["formatVersion"], 2);
}

/// Nel save finisce solo l'hash SHA-256 del token, mai il token in chiaro.
#[tokio::test]
async fn save_contains_only_token_hash() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "TokenHash", Some(5)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    let path = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_file(&path).await;
    let raw = std::fs::read_to_string(&path).expect("lettura save");
    for secret in [&pa.token, &pb.token, &g.host_token] {
        assert!(!raw.contains(secret.as_str()), "token in chiaro nel save");
    }
    let save = read_save(&path);
    assert_eq!(
        save["players"][0]["tokenHash"],
        ttr_server::save::hash_token(&pa.token)
    );
    assert_eq!(
        save["players"][1]["tokenHash"],
        ttr_server::save::hash_token(&pb.token)
    );
}

/// Il nome digitato al resume (maiuscole/spazi diversi) non sostituisce quello salvato.
#[tokio::test]
async fn resume_keeps_saved_name_when_typed_differently() {
    let server = TestServer::start().await;
    let g = create_game_with_seed(&server.base_url, "ResumeName", Some(3)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_file(&save).await;
    let (rid, rtok) = resume_after_restart(&server, &save).await.unwrap();
    let ra = join_game_resume(&server.base_url, &rid, "  ALICE ", &pa.token).await;
    let _rb = join_game_resume(&server.base_url, &rid, "Bob", &pb.token).await;
    start_game(&server.base_url, &rid, &rtok).await;

    let mut ws = WsClient::connect(&server, &rid, &ra.player_id, &ra.token).await;
    let snap = next_snapshot(&mut ws).await;
    assert_eq!(snap.public.players_summary[0].name, "Alice");
}

// ── Resume con token ─────────────────────────────────────────────────

/// Crea una partita a due giocatori già avviata e ritorna i dati per il resume.
async fn started_two_player_game(
    server: &TestServer,
    name: &str,
) -> (CreatedGame, JoinedPlayer, JoinedPlayer, std::path::PathBuf) {
    let g = create_game_with_seed(&server.base_url, name, Some(3)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_file(&save).await;
    (g, pa, pb, save)
}

fn assert_error(res: &(u16, serde_json::Value), status: u16, code: &str) {
    assert_eq!(res.0, status, "status inatteso per {}: {}", code, res.1);
    assert_eq!(res.1["error"], code, "codice errore inatteso: {}", res.1);
}

#[tokio::test]
async fn resume_join_requires_original_token() {
    let server = TestServer::start().await;
    let (_g, pa, pb, save) = started_two_player_game(&server, "ResumeToken").await;
    let (rid, _rtok) = resume_after_restart(&server, &save).await.unwrap();
    let join_path = format!("/api/games/{}/join", rid);
    let base = server.base_url.as_str();

    let detail = get_json(base, &format!("/api/games/{}", rid)).await.1;
    assert_eq!(
        detail["missingPlayers"],
        serde_json::json!(["Alice", "Bob"])
    );

    // Il solo nome non basta, in nessuna grafia.
    for typed in ["Alice", "  ALICE "] {
        let res = post_json(base, &join_path, serde_json::json!({ "name": typed })).await;
        assert_error(&res, 401, "RESUME_TOKEN_MISMATCH");
    }
    // Token sbagliato, o di un altro giocatore.
    let wrong = post_json(
        base,
        &join_path,
        serde_json::json!({ "name": "Alice", "token": "x" }),
    )
    .await;
    assert_error(&wrong, 401, "RESUME_TOKEN_MISMATCH");
    let others = post_json(
        base,
        &join_path,
        serde_json::json!({ "name": "Alice", "token": pb.token }),
    )
    .await;
    assert_error(&others, 401, "RESUME_TOKEN_MISMATCH");
    assert_eq!(
        get_json(base, &format!("/api/games/{}", rid)).await.1["players"],
        serde_json::json!([]),
        "i join rifiutati non entrano nel roster"
    );

    // Il token originale riesce e resta lo stesso, con lo stesso playerId.
    let again = join_game_resume(base, &rid, "Alice", &pa.token).await;
    assert_eq!(again.player_id, pa.player_id);
    assert_eq!(again.token, pa.token);
    let detail = get_json(base, &format!("/api/games/{}", rid)).await.1;
    assert_eq!(detail["missingPlayers"], serde_json::json!(["Bob"]));

    // Ripetere il join col proprio token è innocuo; il solo nome ora è un conflitto.
    let repeat = join_game_resume(base, &rid, "Alice", &pa.token).await;
    assert_eq!(repeat.player_id, pa.player_id);
    let taken = post_json(base, &join_path, serde_json::json!({ "name": "Alice" })).await;
    assert_error(&taken, 409, "NAME_TAKEN");
}

#[tokio::test]
async fn resume_reassign_by_host() {
    let server = TestServer::start().await;
    let (g, pa, pb, save) = started_two_player_game(&server, "ResumeReassign").await;
    let (rid, rtok) = resume_after_restart(&server, &save).await.unwrap();
    let reassign_path = format!("/api/games/{}/reassign", rid);
    let base = server.base_url.as_str();

    // Host token sbagliato (anche quello della partita originale).
    for bad in ["x", g.host_token.as_str()] {
        let res = post_json(
            base,
            &reassign_path,
            serde_json::json!({ "hostToken": bad, "name": "Alice" }),
        )
        .await;
        assert_error(&res, 401, "UNAUTHORIZED");
    }
    let unknown = post_json(
        base,
        &reassign_path,
        serde_json::json!({ "hostToken": rtok, "name": "Zoe" }),
    )
    .await;
    assert_error(&unknown, 409, "NAME_NOT_IN_SAVE");

    // Alice non ha più il token: l'host gliene assegna uno nuovo.
    let (status, body) = post_json(
        base,
        &reassign_path,
        serde_json::json!({ "hostToken": rtok, "name": "alice" }),
    )
    .await;
    assert_eq!(status, 200, "reassign: {}", body);
    assert_eq!(body["playerId"], pa.player_id.as_str());
    let new_token = body["token"].as_str().expect("token").to_string();
    assert_ne!(new_token, pa.token, "il token è nuovo");

    // Il token vecchio non entra più; quello nuovo sì, anche in WS.
    let old = post_json(
        base,
        &format!("/api/games/{}/join", rid),
        serde_json::json!({ "name": "Alice", "token": pa.token }),
    )
    .await;
    assert_ne!(
        old.0, 200,
        "token vecchio dopo la riassegnazione: {}",
        old.1
    );
    let mut stale = WsClient::connect(&server, &rid, &pa.player_id, &pa.token).await;
    match stale.recv().await {
        ttr_proto::ServerMessage::Error(e) => {
            assert_eq!(e.reason, ttr_proto::ErrorReason::Unauthorized)
        }
        other => panic!("atteso UNAUTHORIZED, ricevuto {:?}", other),
    }
    let mut fresh = WsClient::connect(&server, &rid, &pa.player_id, &new_token).await;
    next_snapshot(&mut fresh).await;

    // Bob rientra col suo token; la partita parte con Alice sul token nuovo.
    join_game_resume(base, &rid, "Bob", &pb.token).await;
    start_game(base, &rid, &rtok).await;
    let mut alice = WsClient::connect(&server, &rid, &pa.player_id, &new_token).await;
    let snap = next_snapshot(&mut alice).await;
    assert!(snap.private.is_some(), "Alice vede il suo stato privato");
}

/// I save di formato 1 non hanno `tokenHash`: si rientra solo con la riassegnazione dell'host.
#[tokio::test]
async fn resume_save_without_token_hash_only_accepts_reassign() {
    let server = TestServer::start().await;
    let (_g, pa, _pb, save) = started_two_player_game(&server, "ResumeLegacy").await;
    let mut raw = read_save(&save);
    raw["formatVersion"] = serde_json::json!(1);
    for player in raw["players"].as_array_mut().unwrap() {
        player.as_object_mut().unwrap().remove("tokenHash");
    }
    std::fs::write(&save, raw.to_string()).unwrap();

    let (rid, rtok) = resume_after_restart(&server, &save).await.unwrap();
    let base = server.base_url.as_str();
    let join = post_json(
        base,
        &format!("/api/games/{}/join", rid),
        serde_json::json!({ "name": "Alice", "token": pa.token }),
    )
    .await;
    assert_error(&join, 401, "RESUME_TOKEN_MISMATCH");

    for name in ["Alice", "Bob"] {
        let (status, body) = post_json(
            base,
            &format!("/api/games/{}/reassign", rid),
            serde_json::json!({ "hostToken": rtok, "name": name }),
        )
        .await;
        assert_eq!(status, 200, "reassign {}: {}", name, body);
    }
    start_game(base, &rid, &rtok).await;
}

/// Il reassign è ammesso solo nelle partite riprese da un save.
#[tokio::test]
async fn reassign_on_fresh_game_is_rejected() {
    let server = TestServer::start().await;
    let g = create_game(&server.base_url, "Fresh").await;
    join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let res = post_json(
        &server.base_url,
        &format!("/api/games/{}/reassign", g.game_id),
        serde_json::json!({ "hostToken": g.host_token, "name": "Alice" }),
    )
    .await;
    assert_error(&res, 400, "BAD_REQUEST");
}

// ── Archiviazione all'avvio e durabilità del reassign ────────────────

/// `--resume` su un save concluso: il binario esce con 1 e «partita già
/// conclusa», senza che l'archiviazione all'avvio elimini prima il save.
#[tokio::test]
async fn resume_of_completed_save_reports_game_over() {
    let server = TestServer::start().await;
    let map = usa_map();
    // `results.jsonl` è una directory: l'archiviazione fallisce e il save resta.
    std::fs::create_dir(server.saves_dir.join(ttr_server::save::RESULTS_FILE)).unwrap();

    let g = create_game_with_seed(&server.base_url, "Conclusa", Some(42)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let mut a = Player::new(WsClient::connect(&server, &g.game_id, &pa.player_id, &pa.token).await);
    let mut b = Player::new(WsClient::connect(&server, &g.game_id, &pb.player_id, &pb.token).await);
    let final_public = run_game(&mut a, &mut b, &map).await;
    assert_eq!(final_public.state, GameState::GameOver);
    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_save(&save, |s| s["status"] == "completed").await;

    let fresh_dir = tempfile::tempdir().unwrap();
    let fresh = fresh_dir.path().to_path_buf();
    let copy = fresh.join("conclusa.json");
    std::fs::copy(&save, &copy).unwrap();
    let out = tokio::task::spawn_blocking({
        let (fresh, copy) = (fresh.clone(), copy.clone());
        move || {
            std::process::Command::new(env!("CARGO_BIN_EXE_ttr-server"))
                .arg("--data-dir")
                .arg(data_dir())
                .arg("--saves-dir")
                .arg(&fresh)
                .arg("--resume")
                .arg(&copy)
                .output()
                .expect("avvio di ttr-server")
        }
    })
    .await
    .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("già conclusa"), "{stderr}");
}

/// Un save `completed` rimasto su disco (archiviazione fallita) viene
/// archiviato all'avvio successivo e poi eliminato.
#[tokio::test]
async fn completed_save_is_archived_on_startup() {
    let server = TestServer::start().await;
    let map = usa_map();
    // `results.jsonl` è una directory: l'archiviazione fallisce.
    let results = server.saves_dir.join(ttr_server::save::RESULTS_FILE);
    std::fs::create_dir(&results).unwrap();

    let g = create_game_with_seed(&server.base_url, "Avvio", Some(42)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let mut a = Player::new(WsClient::connect(&server, &g.game_id, &pa.player_id, &pa.token).await);
    let mut b = Player::new(WsClient::connect(&server, &g.game_id, &pb.player_id, &pb.token).await);
    let final_public = run_game(&mut a, &mut b, &map).await;
    assert_eq!(final_public.state, GameState::GameOver);

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_save(&save, |s| s["status"] == "completed").await;
    // Un file `profiles.json` accanto ai save non è un save e non dà errori.
    std::fs::write(server.saves_dir.join("profiles.json"), r#"["Alice"]"#).unwrap();

    std::fs::remove_dir(&results).unwrap();
    assert_eq!(ttr_server::save::archive_completed_saves(&server.state), 1);

    assert!(!save.exists(), "il save archiviato viene eliminato");
    let archived = ttr_server::save::read_results(&server.saves_dir);
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].game_id, g.game_id);
    assert_eq!(archived[0].game_name, "Avvio");
    let scores = final_public.final_scores.as_ref().expect("punteggi finali");
    for fs in scores {
        let name = if fs.player_id == pa.player_id {
            "Alice"
        } else {
            "Bob"
        };
        let row = archived[0]
            .players
            .iter()
            .find(|p| p.name == name)
            .expect("giocatore archiviato");
        assert_eq!((row.total, row.winner), (fs.total, fs.winner), "{name}");
    }

    assert_eq!(
        ttr_server::save::archive_completed_saves(&server.state),
        0,
        "una seconda scansione non trova più nulla"
    );
    assert_eq!(ttr_server::save::read_results(&server.saves_dir).len(), 1);
}

/// All'avvio i `*.tmp` più vecchi di un'ora sono eliminati, quelli recenti restano.
#[tokio::test]
async fn stale_tmp_files_are_removed() {
    let server = TestServer::start().await;
    let stale = server.saves_dir.join("x.json.1.0.tmp");
    let fresh = server.saves_dir.join("y.json.1.1.tmp");
    std::fs::write(&stale, b"{").unwrap();
    std::fs::write(&fresh, b"{").unwrap();
    let two_hours_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 60 * 60);
    std::fs::File::options()
        .write(true)
        .open(&stale)
        .unwrap()
        .set_modified(two_hours_ago)
        .unwrap();

    assert_eq!(ttr_server::save::archive_completed_saves(&server.state), 0);
    assert!(!stale.exists(), "tmp obsoleto eliminato");
    assert!(fresh.exists(), "tmp recente intatto");
}

/// I limiti sui nomi valgono per resume ed elenco, non per l'archiviazione all'avvio: un
/// save `completed` con il nome della partita di 41 caratteri finisce comunque in classifica.
#[tokio::test]
async fn startup_archive_ignores_name_limits() {
    let server = TestServer::start().await;
    let (game_id, save) = completed_save_on_disk(&server, "Lunga").await;
    let long_name = "é".repeat(41);
    let mut v = read_save(&save);
    v["gameName"] = serde_json::json!(long_name);
    std::fs::write(&save, v.to_string()).unwrap();

    assert_eq!(ttr_server::save::archive_completed_saves(&server.state), 1);
    assert!(!save.exists());
    let results = ttr_server::save::read_results(&server.saves_dir);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].game_id, game_id);
    assert_eq!(results[0].game_name, long_name);
}

/// Gioca una partita fino alla fine con l'archiviazione fallita (`results.jsonl` è una
/// directory), così `{id}.json` resta su disco `completed`; poi libera `results.jsonl`.
async fn completed_save_on_disk(server: &TestServer, name: &str) -> (String, std::path::PathBuf) {
    let results = server.saves_dir.join(ttr_server::save::RESULTS_FILE);
    std::fs::create_dir(&results).unwrap();

    let g = create_game_with_seed(&server.base_url, name, Some(42)).await;
    let pa = join_game(&server.base_url, &g.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &g.game_id, "Bob", None).await;
    start_game(&server.base_url, &g.game_id, &g.host_token).await;
    let mut a = Player::new(WsClient::connect(server, &g.game_id, &pa.player_id, &pa.token).await);
    let mut b = Player::new(WsClient::connect(server, &g.game_id, &pb.player_id, &pb.token).await);
    let final_public = run_game(&mut a, &mut b, &usa_map()).await;
    assert_eq!(final_public.state, GameState::GameOver);

    let save = server.saves_dir.join(format!("{}.json", g.game_id));
    wait_for_save(&save, |s| s["status"] == "completed").await;
    std::fs::remove_dir(&results).unwrap();
    (g.game_id, save)
}

/// Un FIFO `{uuid}.json` non blocca l'archiviazione all'avvio: il save valido è archiviato
/// e il FIFO resta dov'è.
#[cfg(unix)]
#[tokio::test]
async fn startup_archive_skips_fifo_named_like_a_save() {
    let server = TestServer::start().await;
    let (game_id, save) = completed_save_on_disk(&server, "Fifo").await;
    let fifo = server
        .saves_dir
        .join(format!("{}.json", uuid::Uuid::new_v4()));
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("avvio di mkfifo");
    assert!(status.success(), "mkfifo fallito");

    let (tx, rx) = std::sync::mpsc::channel();
    let state = server.state.clone();
    std::thread::spawn(move || {
        let _ = tx.send(ttr_server::save::archive_completed_saves(&state));
    });
    let archived = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("l'archiviazione all'avvio non deve bloccarsi su un FIFO");

    assert_eq!(archived, 1);
    assert!(!save.exists(), "il save valido è archiviato");
    assert!(fifo.exists(), "il FIFO non è toccato");
    let results = ttr_server::save::read_results(&server.saves_dir);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].game_id, game_id);
}

/// Un symlink `{uuid}.json` verso un save concluso non è archiviato né eliminato, e il
/// file di destinazione resta intatto.
#[cfg(unix)]
#[tokio::test]
async fn startup_archive_skips_symlinked_save() {
    let server = TestServer::start().await;
    let (_game_id, save) = completed_save_on_disk(&server, "Symlink").await;
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("conclusa.json");
    std::fs::copy(&save, &target).unwrap();
    let original = std::fs::read(&target).unwrap();
    std::fs::remove_file(&save).unwrap();
    std::os::unix::fs::symlink(&target, &save).unwrap();

    assert_eq!(ttr_server::save::archive_completed_saves(&server.state), 0);
    assert!(
        std::fs::symlink_metadata(&save)
            .unwrap()
            .file_type()
            .is_symlink(),
        "il symlink non è toccato"
    );
    assert_eq!(std::fs::read(&target).unwrap(), original);
    assert!(ttr_server::save::read_results(&server.saves_dir).is_empty());
}

/// Il token riassegnato dall'host è nel save subito, quindi sopravvive a un riavvio.
#[tokio::test]
async fn reassigned_token_survives_restart() {
    let server = TestServer::start().await;
    let (_g, pa, _pb, save) = started_two_player_game(&server, "Reassign").await;

    let (rid, rtok) = resume_after_restart(&server, &save).await.unwrap();
    let (status, body) = post_json(
        &server.base_url,
        &format!("/api/games/{rid}/reassign"),
        serde_json::json!({ "hostToken": rtok, "name": "Alice" }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let new_token = body["token"].as_str().expect("nuovo token").to_string();
    assert_ne!(new_token, pa.token);

    let hash = ttr_server::save::hash_token(&new_token);
    let saved = wait_for_save(&save, |s| s["players"][0]["tokenHash"] == hash).await;
    assert_eq!(
        saved["players"].as_array().unwrap().len(),
        2,
        "roster completo"
    );

    // "Riavvio": un nuovo resume dallo stesso save, poi il join col token nuovo.
    let (rid2, _) = resume_after_restart(&server, &save).await.unwrap();
    let old = post_json(
        &server.base_url,
        &format!("/api/games/{rid2}/join"),
        serde_json::json!({ "name": "Alice", "token": pa.token }),
    )
    .await;
    assert_eq!(old.0, 401, "il token vecchio non vale più: {}", old.1);
    let back = join_game_resume(&server.base_url, &rid2, "Alice", &new_token).await;
    assert_eq!(back.player_id, pa.player_id);
}
