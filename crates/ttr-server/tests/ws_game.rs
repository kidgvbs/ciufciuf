//! Test di integrazione WebSocket reali (pacchetto Fase 2 §9).
//!
//! Server axum su porta effimera, client tokio-tungstenite "scemi" che
//! decidono solo dai messaggi WS (`legalMoves`, `offeredTickets`). Un unico
//! driver coordina entrambi i giocatori via `tokio::select!`; ogni giocatore
//! agisce al massimo una volta per `seq` di `PRIVATE_STATE` fresco (evita di
//! agire due volte sullo stesso stato stantio).
//!
//! Il driver "scemo" e `Player` sono condivisi in `common::driver`.

mod common;

use common::driver::*; // Player, decide, apply_msg, run_game, recv_from
use common::*;
use ttr_proto::{GameState, InboundAction, ServerMessage};

/// Test §9.1: partita completa fino a `GAME_OVER` con `finalScores` coerenti.
#[tokio::test]
async fn full_game_over_ws() {
    let server = TestServer::start().await;
    let map = usa_map();

    let game = create_game_with_seed(&server.base_url, "FullGame", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a);
    let mut b = Player::new(b);

    let final_public = run_game(&mut a, &mut b, &map).await;

    assert_eq!(final_public.state, GameState::GameOver);
    let scores = final_public
        .final_scores
        .as_ref()
        .expect("final_scores Some a fine partita");
    assert_eq!(scores.len(), 2, "due giocatori: due punteggi finali");

    // Totali coerenti: ogni total == route+ticket_completed+ticket_failed+bonus.
    for s in scores {
        assert_eq!(
            s.total,
            s.route_points + s.ticket_completed + s.ticket_failed + s.longest_path_bonus,
            "totale coerente per {}",
            s.player_id
        );
    }
    // Almeno un giocatore ha punteggio > 0 (il vincitore).
    assert!(
        scores.iter().any(|s| s.total > 0),
        "il vincitore deve avere un totale > 0"
    );
}

/// Test §9.2: disconnect mid-game e reconnect con snapshot coerente.
#[tokio::test]
async fn reconnect_mid_game() {
    let server = TestServer::start().await;
    let map = usa_map();

    let game = create_game_with_seed(&server.base_url, "ReconnectGame", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a_ws = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b_ws = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a_ws);
    let mut b = Player::new(b_ws);
    let mut public: Option<ttr_proto::PublicState> = None;

    // Consuma SNAPSHOT iniziali
    apply_msg(&a.client.as_mut().unwrap().recv().await, &mut a, &mut public);
    apply_msg(&b.client.as_mut().unwrap().recv().await, &mut b, &mut public);

    // Draft entrambi
    let draft_a = decide(&a.player_id, &mut a.client.as_mut().unwrap().view,
        public.as_ref().unwrap(), &map, &mut a.drafted).unwrap().unwrap();
    a.client.as_mut().unwrap().send(&draft_a).await;

    let draft_b = decide(&b.player_id, &mut b.client.as_mut().unwrap().view,
        public.as_ref().unwrap(), &map, &mut b.drafted).unwrap().unwrap();
    b.client.as_mut().unwrap().send(&draft_b).await;

    // Consuma i messaggi post-draft per entrambi finché non inizia il turno.
    loop {
        let msg_a = a.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_a, &mut a, &mut public);
        let msg_b = b.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_b, &mut b, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver { panic!("game over troppo presto"); }
        if p.state == GameState::PlayerTurn || p.state == GameState::LastRound { break; }
    }

    // A gioca il primo turno (dopo il draft il turno è del primo giocatore, A).
    loop {
        let msg = a.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut a, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver { break; }
        if fresh && p.current_turn_player_id.as_deref() == Some(&a.player_id) {
            let a_c = a.client.as_mut().unwrap();
            let a_act = decide(&a.player_id, &mut a_c.view, p, &map, &mut a.drafted).unwrap();
            if let Some(act) = a_act { a_c.send(&act).await; }
            break;
        }
    }

    // Consuma i messaggi post-azione di A per B: il server manda PublicState
    // (turno di B) + PrivateState(B) dopo l'azione di A.
    loop {
        let msg = b.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut b, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver { break; }
        if fresh && p.current_turn_player_id.as_deref() == Some(&b.player_id) {
            break;
        }
    }

    // Salva stato A e ticket count prima della disconnessione
    let a_tickets = a.client.as_ref().unwrap().view.tickets.clone();
    let a_last_seq = a.last_acted_seq;
    a.disconnect().await;

    // B gioca 1 turno (il PrivateState è già stato consumato sopra,
    // ma dobbiamo agire: public ha già current_turn = B).
    {
        let p = public.as_ref().unwrap();
        assert!(p.current_turn_player_id.as_deref() == Some(&b.player_id),
            "dopo azione di A il turno deve essere di B");
        let b_c = b.client.as_mut().unwrap();
        let b_act = decide(&b.player_id, &mut b_c.view, p, &map, &mut b.drafted).unwrap();
        if let Some(act) = b_act { b_c.send(&act).await; }
    }

    // Dai tempo al server di processare l'azione di B prima del reconnect.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Riconnetti A: il server deve inviare uno SNAPSHOT.
    a.reconnect(&server, &game.game_id).await;
    let snap_msg = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        a.client.as_mut().unwrap().recv()
    ).await.expect("timeout in attesa SNAPSHOT");
    assert!(matches!(snap_msg, ServerMessage::Snapshot(_)), "dopo reconnect: SNAPSHOT");
    let fresh = apply_msg(&snap_msg, &mut a, &mut public);

    let snap = a.client.as_ref().unwrap().last_snapshot.as_ref().unwrap();
    let priv_ = snap.private.as_ref().expect("private per giocatore");
    assert_eq!(a.client.as_ref().unwrap().view.tickets.len(), a_tickets.len(),
        "stessi biglietti dopo reconnect");
    assert!(priv_.seq > a_last_seq, "seq snapshot {} > {}", priv_.seq, a_last_seq);

    // Il SNAPSHOT è "fresco" (seq avanza): se dopo il reconnect il turno è di A,
    // A deve agire subito, altrimenti run_game resterebbe bloccato senza
    // un PrivateState fresco per innescare la decisione.
    if fresh {
        let p = public.as_ref().unwrap();
        if p.state != GameState::GameOver
            && p.current_turn_player_id.as_deref() == Some(&a.player_id)
        {
            let a_c = a.client.as_mut().unwrap();
            let a_act = decide(&a.player_id, &mut a_c.view, p, &map, &mut a.drafted).unwrap();
            if let Some(act) = a_act { a_c.send(&act).await; }
        }
    }

    // Finisci
    let fp = run_game(&mut a, &mut b, &map).await;
    assert_eq!(fp.state, GameState::GameOver);
    assert!(fp.final_scores.as_ref().unwrap().len() == 2);
}

/// Test §9.3: idempotenza inbound — stesso clientSeq ritrasmesso.
#[tokio::test]
async fn idempotency() {
    let server = TestServer::start().await;
    let map = usa_map();

    let game = create_game_with_seed(&server.base_url, "IdempotencyGame", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a_ws = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b_ws = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a_ws);
    let mut b = Player::new(b_ws);
    let mut public: Option<ttr_proto::PublicState> = None;

    // Consuma SNAPSHOT iniziali (stato post-start con offeredTickets)
    apply_msg(&a.client.as_mut().unwrap().recv().await, &mut a, &mut public);
    apply_msg(&b.client.as_mut().unwrap().recv().await, &mut b, &mut public);

    // Draft entrambi
    let draft_a = decide(&a.player_id, &mut a.client.as_mut().unwrap().view,
        public.as_ref().unwrap(), &map, &mut a.drafted).unwrap().unwrap();
    a.client.as_mut().unwrap().send(&draft_a).await;

    let draft_b = decide(&b.player_id, &mut b.client.as_mut().unwrap().view,
        public.as_ref().unwrap(), &map, &mut b.drafted).unwrap().unwrap();
    b.client.as_mut().unwrap().send(&draft_b).await;

    // Consuma i messaggi post-draft per entrambi finché non inizia il turno.
    loop {
        let msg_a = a.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_a, &mut a, &mut public);
        let msg_b = b.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_b, &mut b, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver { panic!("game over troppo presto"); }
        if p.state == GameState::PlayerTurn || p.state == GameState::LastRound { break; }
    }

    // Dopo il draft il turno è del primo giocatore (A, indice 0). Consuma il
    // PrivateState fresco di A e verifica che sia effettivamente il suo turno.
    loop {
        let msg = a.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut a, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver { panic!("game over troppo presto"); }
        if fresh && p.current_turn_player_id.as_deref() == Some(&a.player_id) {
            break;
        }
    }

    // La pescata è valida solo in un `PLAYER_TURN` pulito: verifichiamo la
    // fase (non solo il currentTurn) e che non sia già una seconda pescata o
    // una scelta biglietti (viste stantie avrebbero fatto scartare l'azione).
    let p_now = public.as_ref().unwrap();
    assert_eq!(
        p_now.state,
        GameState::PlayerTurn,
        "la pescata richiede una fase PlayerTurn pulita"
    );
    assert_eq!(
        p_now.current_turn_player_id.as_deref(),
        Some(a.player_id.as_str()),
        "il turno deve essere di A"
    );

    let hand_before = a.client.as_ref().unwrap().view.hand.clone();
    assert!(
        a.client.as_ref().unwrap().view.legal_moves.can_draw_deck,
        "A deve poter pescare dal mazzo all'inizio del turno"
    );

    // Azione con client_seq esplicito
    let draw = InboundAction::DrawTrainCard(ttr_proto::DrawCardPayload {
        player_id: a.player_id.clone(),
        source: ttr_proto::DrawSource::Deck,
        client_seq: Some(1),
    });
    // Azzera eventuali errori pregressi: nel tratto hand_before→hand_after
    // qualunque Error è un fallimento (la pescata deve essere accettata).
    a.client.as_mut().unwrap().view.last_error = None;
    a.client.as_mut().unwrap().send(&draw).await;

    // Ricevi PrivateState dopo l'azione (ignora il PublicState di passaggio turno).
    loop {
        let msg = a.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut a, &mut public);
        if fresh { break; }
    }
    let hand_after = a.client.as_ref().unwrap().view.hand.clone();
    assert_eq!(hand_after.len(), hand_before.len() + 1, "+1 carta dopo pescata");
    assert!(
        a.client.as_ref().unwrap().view.last_error.is_none(),
        "nessun errore dopo pescata valida"
    );

    // Ritrasmetti la stessa azione con lo stesso client_seq
    a.client.as_mut().unwrap().send(&draw).await;

    // Ricevi re-push del PrivateState (azione droppata per idempotenza)
    loop {
        let msg = a.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut a, &mut public);
        if fresh { break; }
    }
    let hand_after_retransmit = a.client.as_ref().unwrap().view.hand.clone();

    assert_eq!(hand_after_retransmit, hand_after,
        "idempotenza: ritrasmissione non deve cambiare la mano");
}

/// Test §9.4: salvataggio su disco e replay deterministico da save file.
#[tokio::test]
async fn save_and_replay() {
    let server = TestServer::start().await;
    let map = usa_map();

    // create_game senza seed → time-based (evita dead_code warning).
    let game = create_game(&server.base_url, "SaveReplay").await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a);
    let mut b = Player::new(b);

    let final_public = run_game(&mut a, &mut b, &map).await;
    assert_eq!(final_public.state, GameState::GameOver);

    // Il save file deve esistere.
    let save_path = server.saves_dir.join(format!("{}.json", game.game_id));
    assert!(save_path.exists(), "save file deve esistere dopo GameOver");

    let save_json = std::fs::read_to_string(&save_path).expect("lettura save");
    let save: serde_json::Value =
        serde_json::from_str(&save_json).expect("parsing save JSON");

    assert_eq!(save["status"].as_str().unwrap(), "completed");
    let actions_arr = save["actions"].as_array().expect("actions array");
    assert!(!actions_arr.is_empty(), "almeno un'azione salvata");

    let seed = save["seed"].as_u64().expect("seed u64");
    assert!(seed > 0, "seed time-based > 0");

    // Estrai dati giocatori.
    let players_arr = save["players"].as_array().unwrap();
    let player_ids: Vec<String> = players_arr
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    let player_names: Vec<String> = players_arr
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    let player_colors: Vec<ttr_proto::PlayerColor> = players_arr
        .iter()
        .map(|p| match p["color"].as_str().unwrap() {
            "RED" => ttr_proto::PlayerColor::Red,
            "BLUE" => ttr_proto::PlayerColor::Blue,
            "GREEN" => ttr_proto::PlayerColor::Green,
            "YELLOW" => ttr_proto::PlayerColor::Yellow,
            "BLACK" => ttr_proto::PlayerColor::Black,
            c => panic!("colore sconosciuto nel save: {}", c),
        })
        .collect();

    let actions: Vec<ttr_core::Action> = actions_arr
        .iter()
        .map(|a| {
            serde_json::from_value(a["action"].clone())
                .expect("deserializza azione dal save")
        })
        .collect();

    let ruleset = load_usa_ruleset();

    // Primo replay.
    let mut e1 = ttr_core::Engine::new(
        seed,
        ruleset.clone(),
        map.clone(),
        player_ids.clone(),
        player_names.clone(),
        player_colors.clone(),
        0,
    );
    for act in &actions {
        e1.apply(act.clone())
            .unwrap_or_else(|e| panic!("replay azione fallita: {}", e));
    }
    assert_eq!(
        e1.phase(),
        ttr_core::GamePhase::GameOver,
        "replay deve terminare in GameOver"
    );

    // Secondo replay: stato identico (determinismo).
    let mut e2 = ttr_core::Engine::new(
        seed,
        ruleset,
        map.clone(),
        player_ids,
        player_names,
        player_colors,
        0,
    );
    for act in &actions {
        e2.apply(act.clone())
            .unwrap_or_else(|e| panic!("replay2 azione fallita: {}", e));
    }
    assert_eq!(
        e2.serialize_state(),
        e1.serialize_state(),
        "due replay con stesso seed devono produrre lo stesso stato"
    );
}

/// Test §9.5: resume da save file — ripristina stato, giocatori si riagganciano
/// per nome e ritrovano playerId, fase e claimed routes coerenti.
#[tokio::test]
async fn resume_flow() {
    // ── Fase 1: partita parziale ──────────────────────────────────
    let server1 = TestServer::start().await;
    let map = usa_map();

    let game = create_game(&server1.base_url, "ResumeGame").await;
    let pa = join_game(&server1.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server1.base_url, &game.game_id, "Bob", None).await;
    start_game(&server1.base_url, &game.game_id, &game.host_token).await;

    let a_ws1 = WsClient::connect(&server1, &game.game_id, &pa.player_id, &pa.token).await;
    let b_ws1 = WsClient::connect(&server1, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a_ws1);
    let mut b = Player::new(b_ws1);
    let mut public: Option<ttr_proto::PublicState> = None;

    // SNAPSHOT iniziali
    apply_msg(&a.client.as_mut().unwrap().recv().await, &mut a, &mut public);
    apply_msg(&b.client.as_mut().unwrap().recv().await, &mut b, &mut public);

    // Draft
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

    // Consuma post-draft fino a PlayerTurn
    loop {
        let msg_a = a.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_a, &mut a, &mut public);
        let msg_b = b.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_b, &mut b, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver {
            panic!("game over troppo presto");
        }
        if p.state == GameState::PlayerTurn {
            break;
        }
    }

    // A gioca il primo turno (draw deck)
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

    // Ricevi PrivateState di B (azione di A processata, save scritto).
    loop {
        let msg = b.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut b, &mut public);
        if fresh {
            break;
        }
    }

    let saved_public = public.clone().expect("public state catturato");
    let save_path = server1.saves_dir.join(format!("{}.json", game.game_id));
    assert!(save_path.exists(), "save scritto dopo la prima azione");

    // Copia il save in una posizione persistente (server1 lo cancella al Drop).
    let persistent_save = temp_saves_dir().join(format!("{}.json", game.game_id));
    std::fs::copy(&save_path, &persistent_save).expect("copia save persistente");

    // Chiudi server1.
    drop(server1);
    drop(a);
    drop(b);

    // ── Fase 2: resume ────────────────────────────────────────────
    let saves2 = temp_saves_dir();
    let state2 = ttr_server::AppState::new(data_dir(), saves2.clone(), 0)
        .expect("AppState::new");
    let (resumed_id, new_host_token) = ttr_server::resume_game(&state2, &persistent_save)
        .await
        .expect("resume_game");
    assert_eq!(resumed_id, game.game_id, "stesso gameId dopo resume");

    let server2 = TestServer::start_with_state(state2, saves2).await;

    // I giocatori si riagganciano per nome: stesso playerId.
    let pa2 = join_game(&server2.base_url, &game.game_id, "Alice", None).await;
    let pb2 = join_game(&server2.base_url, &game.game_id, "Bob", None).await;

    assert_eq!(pa2.player_id, pa.player_id, "Alice riottiene lo stesso playerId");
    assert_eq!(pb2.player_id, pb.player_id, "Bob riottiene lo stesso playerId");

    // Avvia la partita ripresa.
    start_game(&server2.base_url, &game.game_id, &new_host_token).await;

    // Connetti WS e ottieni SNAPSHOT.
    let a2 = WsClient::connect(&server2, &game.game_id, &pa2.player_id, &pa2.token).await;
    let b2 = WsClient::connect(&server2, &game.game_id, &pb2.player_id, &pb2.token).await;

    let mut a2 = Player::new(a2);
    let mut b2 = Player::new(b2);
    let mut public2: Option<ttr_proto::PublicState> = None;

    apply_msg(&a2.client.as_mut().unwrap().recv().await, &mut a2, &mut public2);
    apply_msg(&b2.client.as_mut().unwrap().recv().await, &mut b2, &mut public2);

    let resumed = public2.expect("public state dopo resume");

    // La fase NON deve essere InitialDraft.
    assert_ne!(
        resumed.state,
        GameState::InitialDraft,
        "dopo resume non si torna al draft iniziale"
    );
    assert_eq!(
        resumed.claimed_routes.len(),
        saved_public.claimed_routes.len(),
        "stesse rotte reclamate dopo resume"
    );
    assert_eq!(
        resumed.players_summary.len(),
        saved_public.players_summary.len(),
        "stesso numero giocatori dopo resume"
    );
    for (r, s) in resumed
        .players_summary
        .iter()
        .zip(saved_public.players_summary.iter())
    {
        assert_eq!(r.name, s.name, "nome giocatore preservato");
    }

    // Pulizia.
    drop(server2);
    let _ = std::fs::remove_dir_all(
        persistent_save.parent().expect("parent dir"),
    );
}