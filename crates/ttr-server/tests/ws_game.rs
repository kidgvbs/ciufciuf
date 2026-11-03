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

    // Consuma i messaggi post-draft per entrambi finché non inizia il turno.
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

    // A gioca il primo turno (dopo il draft il turno è del primo giocatore, A).
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

    // Consuma i messaggi post-azione di A per B: il server manda PublicState
    // (turno di B) + PrivateState(B) dopo l'azione di A.
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

    // Salva stato A e ticket count prima della disconnessione
    let a_tickets = a.client.as_ref().unwrap().view.tickets.clone();
    let a_last_seq = a.last_acted_seq;
    a.disconnect().await;

    // B gioca 1 turno (il PrivateState è già stato consumato sopra,
    // ma dobbiamo agire: public ha già current_turn = B).
    {
        let p = public.as_ref().unwrap();
        assert!(
            p.current_turn_player_id.as_deref() == Some(&b.player_id),
            "dopo azione di A il turno deve essere di B"
        );
        let b_c = b.client.as_mut().unwrap();
        let b_act = decide(&b.player_id, &mut b_c.view, p, &map, &mut b.drafted).unwrap();
        if let Some(act) = b_act {
            b_c.send(&act).await;
        }
    }

    // Dai tempo al server di processare l'azione di B prima del reconnect.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Riconnetti A: il server deve inviare uno SNAPSHOT.
    a.reconnect(&server, &game.game_id).await;
    let snap_msg = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        a.client.as_mut().unwrap().recv(),
    )
    .await
    .expect("timeout in attesa SNAPSHOT");
    assert!(
        matches!(snap_msg, ServerMessage::Snapshot(_)),
        "dopo reconnect: SNAPSHOT"
    );
    let fresh = apply_msg(&snap_msg, &mut a, &mut public);

    let snap = a.client.as_ref().unwrap().last_snapshot.as_ref().unwrap();
    let priv_ = snap.private.as_ref().expect("private per giocatore");
    assert_eq!(
        a.client.as_ref().unwrap().view.tickets.len(),
        a_tickets.len(),
        "stessi biglietti dopo reconnect"
    );
    assert!(
        priv_.seq > a_last_seq,
        "seq snapshot {} > {}",
        priv_.seq,
        a_last_seq
    );

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
            if let Some(act) = a_act {
                a_c.send(&act).await;
            }
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

    // Consuma i messaggi post-draft per entrambi finché non inizia il turno.
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

    // Dopo il draft il turno è del primo giocatore (A, indice 0). Consuma il
    // PrivateState fresco di A e verifica che sia effettivamente il suo turno.
    loop {
        let msg = a.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut a, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver {
            panic!("game over troppo presto");
        }
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
        if fresh {
            break;
        }
    }
    let hand_after = a.client.as_ref().unwrap().view.hand.clone();
    assert_eq!(
        hand_after.len(),
        hand_before.len() + 1,
        "+1 carta dopo pescata"
    );
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
        if fresh {
            break;
        }
    }
    let hand_after_retransmit = a.client.as_ref().unwrap().view.hand.clone();

    assert_eq!(
        hand_after_retransmit, hand_after,
        "idempotenza: ritrasmissione non deve cambiare la mano"
    );
}
