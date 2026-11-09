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
use futures_util::StreamExt;
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

/// Test F1: spettatore (connessione senza playerId) riceve solo pubblico.
#[tokio::test]
async fn spectator_public_only() {
    let server = TestServer::start().await;
    let map = usa_map();

    let game = create_game_with_seed(&server.base_url, "Spectator", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a_ws = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b_ws = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;
    let mut spec = WsClient::connect_spectator(&server, &game.game_id).await;

    let mut a = Player::new(a_ws);
    let mut b = Player::new(b_ws);
    let mut public: Option<ttr_proto::PublicState> = None;

    // Consuma SNAPSHOT iniziali per A, B e spettatore.
    let snap_spec = spec.recv().await;
    assert!(
        matches!(snap_spec, ServerMessage::Snapshot(_)),
        "spettatore riceve SNAPSHOT iniziale"
    );
    if let ServerMessage::Snapshot(ref s) = snap_spec {
        assert!(s.private.is_none(), "spettatore: private None nello SNAPSHOT");
    }

    apply_msg(&a.client.as_mut().unwrap().recv().await, &mut a, &mut public);
    apply_msg(&b.client.as_mut().unwrap().recv().await, &mut b, &mut public);

    // Draft entrambi i giocatori (spettatore non partecipa).
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

    // Consuma post-draft: lo spettatore deve ricevere PUBLIC_STATE ma mai PRIVATE_STATE.
    // Due fasi: prima A e B fino a PlayerTurn, poi drena lo spettatore.
    let mut spec_got_public = false;
    let mut spec_got_private = false;
    loop {
        tokio::select! {
            msg = a.client.as_mut().unwrap().recv() => {
                apply_msg(&msg, &mut a, &mut public);
            }
            msg = b.client.as_mut().unwrap().recv() => {
                apply_msg(&msg, &mut b, &mut public);
            }
        }
        if public.as_ref().map(|p| p.state == GameState::PlayerTurn).unwrap_or(false) {
            break;
        }
    }
    // Drena i messaggi broadcast accumulati nello stream dello spettatore
    // (il PublicState post-draft è stato inviato anche agli spectator).
    while let Ok(msg) =
        tokio::time::timeout(std::time::Duration::from_millis(200), spec.recv()).await
    {
        match msg {
            ServerMessage::PublicState(_) => spec_got_public = true,
            ServerMessage::PrivateState(_) => spec_got_private = true,
            _ => {}
        }
    }
    assert!(spec_got_public, "spettatore riceve PUBLIC_STATE");
    assert!(!spec_got_private, "spettatore NON riceve PRIVATE_STATE");

    // Lo spettatore prova a inviare un'azione → deve ricevere ERROR.
    spec.send(&ttr_proto::InboundAction::DrawTrainCard(
        ttr_proto::DrawCardPayload {
            player_id: "chiunque".into(),
            source: ttr_proto::DrawSource::Deck,
            client_seq: None,
        },
    ))
    .await;
    let msg = spec.recv().await;
    assert!(
        matches!(msg, ServerMessage::Error(ref e) if e.reason == ttr_proto::ErrorReason::Unauthorized),
        "spettatore: azione inbound → ERROR Unauthorized"
    );
}

/// Test N1 (Fase 3b): lo spettatore può chiedere REQUEST_SNAPSHOT.
///
/// Il client TV/spettatore non ha `player_id`, quindi il suo
/// `{"actionType":"REQUEST_SNAPSHOT","client_seq":null}` non è
/// deserializzabile come `InboundAction` (payload proto con player_id
/// obbligatorio): il server lo gestisce con una sonda su `actionType`.
/// Anche un REQUEST_SNAPSHOT ben formato con player_id fasullo deve
/// produrre uno snapshot solo-pubblico (l'identità è della connessione).
#[tokio::test]
async fn spectator_request_snapshot() {
    let server = TestServer::start().await;

    let game = create_game_with_seed(&server.base_url, "SpecSnap", Some(42)).await;
    let _pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;

    let mut spec = WsClient::connect_spectator(&server, &game.game_id).await;

    // SNAPSHOT iniziale alla connessione.
    let first = spec.recv().await;
    assert!(
        matches!(first, ServerMessage::Snapshot(_)),
        "spettatore riceve SNAPSHOT iniziale"
    );

    // 1. REQUEST_SNAPSHOT raw senza player_id (come lo manda il client TV).
    spec.send_raw(r#"{"actionType":"REQUEST_SNAPSHOT","client_seq":null}"#)
        .await;
    let msg = spec.recv().await;
    match msg {
        ServerMessage::Snapshot(ref s) => {
            assert!(s.private.is_none(), "spettatore: private None");
            assert!(
                matches!(s.public.state, GameState::WaitingForPlayers),
                "snapshot in lobby, trovato {:?}",
                s.public.state
            );
        }
        other => panic!("atteso SNAPSHOT per spettatore, trovato {:?}", other),
    }

    // 2. REQUEST_SNAPSHOT ben formato con player_id fasullo: l'identità
    // resta quella della connessione (None) → snapshot solo-pubblico.
    spec.send(&ttr_proto::InboundAction::RequestSnapshot(
        ttr_proto::RequestSnapshotPayload {
            player_id: "chiunque".into(),
            client_seq: None,
        },
    ))
    .await;
    let msg = spec.recv().await;
    assert!(
        matches!(msg, ServerMessage::Snapshot(ref s) if s.private.is_none()),
        "REQUEST_SNAPSHOT da spettatore con player_id fasullo → SNAPSHOT pubblico"
    );
}

/// Test F2: token errato → socket chiuso dal server.
#[tokio::test]
async fn wrong_token_closes_socket() {
    let server = TestServer::start().await;

    let game = create_game_with_seed(&server.base_url, "WrongToken", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;

    // Connetti con token sbagliato.
    let url = format!(
        "{}?playerId={}&token=token_sbagliato",
        server.ws_url(&game.game_id),
        pa.player_id,
    );
    let (ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("connect WS (token errato)");
    let (_write, mut read) = ws.split();

    // Il server deve chiudere il socket: la read deve tornare Close o None.
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), read.next())
        .await
        .expect("timeout attesa chiusura");
    match msg {
        Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
            // OK: server ha chiuso
        }
        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
            // Verifica che sia ERROR Unauthorized
            let parsed: ServerMessage =
                serde_json::from_str(&text).expect("parse close msg");
            assert!(
                matches!(parsed, ServerMessage::Error(ref e) if e.reason == ttr_proto::ErrorReason::Unauthorized),
                "token errato → ERROR Unauthorized"
            );
            // Dopo l'ERROR il server deve chiudere: la prossima read è Close, None, o errore di reset.
            let next = tokio::time::timeout(std::time::Duration::from_secs(5), read.next())
                .await
                .ok()
                .flatten();
            match next {
                None
                | Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))
                | Some(Err(tokio_tungstenite::tungstenite::Error::Protocol(
                    tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                ))) => { /* OK */ }
                other => panic!("dopo ERROR il socket deve chiudersi, ricevuto: {:?}", other),
            }
        }
        None => {
            // Socket chiuso senza frame
        }
        other => {
            panic!("atteso Close o ERROR, ricevuto: {:?}", other);
        }
    }
}

/// Test F5: `client_seq: Some(0)` non deve bypassare il dedup.
#[tokio::test]
async fn client_seq_zero_dedup() {
    let server = TestServer::start().await;
    let map = usa_map();

    let game = create_game_with_seed(&server.base_url, "SeqZero", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a_ws = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b_ws = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a_ws);
    let mut b = Player::new(b_ws);
    let mut public: Option<ttr_proto::PublicState> = None;

    // Consuma SNAPSHOT iniziali e draft.
    apply_msg(&a.client.as_mut().unwrap().recv().await, &mut a, &mut public);
    apply_msg(&b.client.as_mut().unwrap().recv().await, &mut b, &mut public);

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

    // Consuma post-draft finché non inizia il turno di A.
    loop {
        let msg_a = a.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_a, &mut a, &mut public);
        let msg_b = b.client.as_mut().unwrap().recv().await;
        apply_msg(&msg_b, &mut b, &mut public);
        let p = public.as_ref().unwrap();
        if p.state == GameState::GameOver {
            panic!("game over troppo presto");
        }
        if (p.state == GameState::PlayerTurn || p.state == GameState::LastRound)
            && p.current_turn_player_id.as_deref() == Some(&a.player_id)
        {
            break;
        }
    }

    // Svuota eventuali PrivateState pendenti per avere un hand pulito.
    while let Ok(msg) = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        a.client.as_mut().unwrap().recv(),
    )
    .await
    {
        apply_msg(&msg, &mut a, &mut public);
    }
    let hand_before = a.client.as_ref().unwrap().view.hand.clone();

    // Azione con client_seq: Some(0) — deve essere accettata la prima volta.
    let draw = InboundAction::DrawTrainCard(ttr_proto::DrawCardPayload {
        player_id: a.player_id.clone(),
        source: ttr_proto::DrawSource::Deck,
        client_seq: Some(0),
    });
    a.client.as_mut().unwrap().send(&draw).await;

    // Ricevi risposta.
    loop {
        let msg = a.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut a, &mut public);
        if fresh {
            break;
        }
    }
    let hand_after_first = a.client.as_ref().unwrap().view.hand.clone();

    // Ritrasmetti con stesso client_seq: Some(0) → deve essere droppata.
    a.client.as_mut().unwrap().send(&draw).await;
    loop {
        let msg = a.client.as_mut().unwrap().recv().await;
        let fresh = apply_msg(&msg, &mut a, &mut public);
        if fresh {
            break;
        }
    }
    let hand_after_second = a.client.as_ref().unwrap().view.hand.clone();

    assert_eq!(
        hand_after_second, hand_after_first,
        "F5: stesso client_seq Some(0) → dedup, mano invariata"
    );
    // Verifica che la prima applicazione abbia effettivamente cambiato la mano.
    assert!(
        hand_after_first != hand_before,
        "prima applicazione con client_seq Some(0) deve funzionare"
    );
}

// ── Helper per i test presence ────────────────────────────────────────

/// Estrae il flag `connected` di un giocatore da `PlayerSummary`.
fn connected(ps: &ttr_proto::PublicState, player_id: &str) -> bool {
    ps.players_summary
        .iter()
        .find(|s| s.player_id == player_id)
        .map(|s| s.connected)
        .unwrap_or(false)
}

/// Test F3e-presence: `connected` in `PlayerSummary` riflette le
/// connessioni WS attive. In lobby (no start) per evitare interferenze
/// del motore; verifica soglie 0→1 e N→0 broadcast.
#[tokio::test]
async fn presence_connected_flag() {
    let server = TestServer::start().await;

    let game = create_game_with_seed(&server.base_url, "Presence", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    // NO start_game: lobby test.

    // ── A connects (0→1) ──────────────────────────────────────────
    let mut a1 = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;

    let snap_a = a1.recv().await;
    assert!(
        matches!(snap_a, ServerMessage::Snapshot(_)),
        "A: SNAPSHOT iniziale"
    );
    if let ServerMessage::Snapshot(ref s) = snap_a {
        assert!(
            connected(&s.public, &pa.player_id),
            "A connected nel proprio SNAPSHOT"
        );
        assert!(
            !connected(&s.public, &pb.player_id),
            "B non ancora connesso"
        );
    }

    // 0→1 scatena broadcast PublicState.
    let ps_a = a1.recv().await;
    assert!(
        matches!(ps_a, ServerMessage::PublicState(_)),
        "A: PublicState broadcast dopo 0→1"
    );

    // ── B connects (0→1) ──────────────────────────────────────────
    let mut b = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let snap_b = b.recv().await;
    assert!(matches!(snap_b, ServerMessage::Snapshot(_)));
    if let ServerMessage::Snapshot(ref s) = snap_b {
        assert!(connected(&s.public, &pa.player_id), "A connected per B");
        assert!(connected(&s.public, &pb.player_id), "B connected per B");
    }

    // B: broadcast PublicState per il proprio 0→1.
    let ps_b = b.recv().await;
    assert!(matches!(ps_b, ServerMessage::PublicState(_)));

    // A riceve il broadcast di B (presence di B ora visibile).
    let ps_a2 = a1.recv().await;
    assert!(
        matches!(ps_a2, ServerMessage::PublicState(_)),
        "A riceve broadcast presenza di B"
    );
    if let ServerMessage::PublicState(ref ps) = ps_a2 {
        assert!(connected(ps, &pb.player_id), "A vede B connected");
    }

    // ── A seconda connessione (già online → NO broadcast) ─────────
    let mut a2 = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let snap_a2 = a2.recv().await;
    assert!(matches!(snap_a2, ServerMessage::Snapshot(_)));
    if let ServerMessage::Snapshot(ref s) = snap_a2 {
        assert!(connected(&s.public, &pa.player_id));
        assert!(connected(&s.public, &pb.player_id));
    }

    // ── Chiudi a2 (non ultima → NO broadcast) ─────────────────────
    a2.disconnect().await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // B chiede SNAPSHOT: A deve risultare ancora connected.
    b.send(&ttr_proto::InboundAction::RequestSnapshot(
        ttr_proto::RequestSnapshotPayload {
            player_id: pb.player_id.clone(),
            client_seq: None,
        },
    ))
    .await;
    let snap_b2 = b.recv().await;
    assert!(
        matches!(snap_b2, ServerMessage::Snapshot(_)),
        "B: SNAPSHOT dopo chiusura a2"
    );
    if let ServerMessage::Snapshot(ref s) = snap_b2 {
        assert!(
            connected(&s.public, &pa.player_id),
            "A ancora connected (a1 rimasta)"
        );
    }

    // ── Chiudi a1 (ultima → broadcast N→0) ────────────────────────
    a1.disconnect().await;

    // B riceve PublicState con A disconnected.
    let ps_b2 = tokio::time::timeout(std::time::Duration::from_secs(5), b.recv())
        .await
        .expect("timeout: B attende PublicState dopo disconnect A");
    assert!(
        matches!(ps_b2, ServerMessage::PublicState(_)),
        "B: PublicState dopo disconnect A"
    );
    if let ServerMessage::PublicState(ref ps) = ps_b2 {
        assert!(
            !connected(ps, &pa.player_id),
            "A disconnected dopo chiusura a1"
        );
        assert!(connected(ps, &pb.player_id), "B ancora connected");
    }
}

/// Test F3e-deck_size: `deck_size` in `PublicState` parte da 0 pre-start,
/// è >0 dopo start, e diminuisce dopo una pescata dal mazzo.
#[tokio::test]
async fn deck_size_reported_and_decreases() {
    let server = TestServer::start().await;
    let map = usa_map();

    let game = create_game_with_seed(&server.base_url, "DeckSize", Some(42)).await;
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;

    // ── Spettatore pre-start: deck_size == 0 ──────────────────────
    let mut spec = WsClient::connect_spectator(&server, &game.game_id).await;
    let snap_spec = spec.recv().await;
    assert!(matches!(snap_spec, ServerMessage::Snapshot(_)));
    if let ServerMessage::Snapshot(ref s) = snap_spec {
        assert_eq!(s.public.state, GameState::WaitingForPlayers);
        assert_eq!(s.public.deck_size, 0, "deck_size 0 pre-start");
    }

    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let a_ws = WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await;
    let b_ws = WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await;

    let mut a = Player::new(a_ws);
    let mut b = Player::new(b_ws);
    let mut public: Option<ttr_proto::PublicState> = None;

    // ── SNAPSHOT A: cattura deck_size iniziale d0 ─────────────────
    apply_msg(
        &a.client.as_mut().unwrap().recv().await,
        &mut a,
        &mut public,
    );
    let d0 = public.as_ref().unwrap().deck_size;
    assert!(d0 > 0, "deck_size > 0 dopo start, trovato {}", d0);
    assert!(d0 <= 105, "deck_size <= 105 dopo start, trovato {}", d0);

    // SNAPSHOT B
    apply_msg(
        &b.client.as_mut().unwrap().recv().await,
        &mut b,
        &mut public,
    );

    // ── Draft entrambi (non tocca il mazzo treno) ─────────────────
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

    // Consuma post-draft fino a PlayerTurn.
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

    assert_eq!(
        public.as_ref().unwrap().deck_size,
        d0,
        "deck_size invariato dopo draft"
    );

    // ── A pesca dal mazzo ─────────────────────────────────────────
    // Consuma fino al PrivateState fresco di A (suo turno).
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

    let draw = InboundAction::DrawTrainCard(ttr_proto::DrawCardPayload {
        player_id: a.player_id.clone(),
        source: ttr_proto::DrawSource::Deck,
        client_seq: None,
    });
    a.client.as_mut().unwrap().send(&draw).await;

    // Consuma messaggi post-pescata finché PublicState mostra deck_size == d0 - 1.
    loop {
        let msg = a.client.as_mut().unwrap().recv().await;
        apply_msg(&msg, &mut a, &mut public);
        let p = public.as_ref().unwrap();
        if p.deck_size == d0 - 1 {
            break;
        }
        if p.state == GameState::GameOver {
            break;
        }
    }

    assert_eq!(
        public.as_ref().unwrap().deck_size,
        d0 - 1,
        "deck_size diminuito di 1 dopo pescata dal mazzo"
    );

    // Lo spettatore riceve i PublicState broadcast; l'ultimo deve avere
    // deck_size == d0 - 1.
    let mut spec_deck = d0;
    while let Ok(msg) =
        tokio::time::timeout(std::time::Duration::from_millis(200), spec.recv()).await
    {
        if let ServerMessage::PublicState(ps) = msg {
            spec_deck = ps.deck_size;
        }
    }
    assert_eq!(
        spec_deck, d0 - 1,
        "spettatore vede deck_size aggiornato dopo pescata"
    );
}
