//! Driver "scemo" per i test WS: coordina due giocatori via `tokio::select!`,
//! decidendo solo dai messaggi WS (`legalMoves`, `offeredTickets`).
//!
//! Condiviso tra `ws_game` e `save_replay`.

use super::*;
use ttr_proto::{GameState, InboundAction, ServerMessage};

/// Un giocatore guidato dal driver: client WS + contatore seq per capire
/// quando uno stato privato è fresco e richiede una decisione.
/// `client = None` quando il giocatore è disconnesso (test reconnect).
pub struct Player {
    pub player_id: String,
    #[allow(dead_code)]
    pub token: String,
    pub client: Option<WsClient>,
    /// Ultimo `seq` di PrivateState per cui abbiamo già deciso/inviato.
    pub last_acted_seq: u64,
    /// Se il draft iniziale di questo giocatore è già stato inviato.
    pub drafted: bool,
}

impl Player {
    pub fn new(client: WsClient) -> Self {
        let pid = client.player_id.clone();
        let tok = client.token.clone();
        Player {
            player_id: pid,
            token: tok,
            client: Some(client),
            last_acted_seq: 0,
            drafted: false,
        }
    }

    /// Disconnette il giocatore consumando il client.
    #[allow(dead_code)]
    pub async fn disconnect(&mut self) {
        if let Some(c) = self.client.take() {
            c.disconnect().await;
        }
    }

    /// Riconnette il giocatore (stesso playerId+token salvati).
    #[allow(dead_code)]
    pub async fn reconnect(&mut self, server: &TestServer, game_id: &str) {
        assert!(self.client.is_none(), "giocatore già connesso");
        let new = WsClient::connect(server, game_id, &self.player_id, &self.token).await;
        self.client = Some(new);
    }
}

/// Quote per una pesca con scelta del mazzo (un biglietto alla volta, un mazzo dopo
/// l'altro); `None` se la pesca non è con le quote.
pub fn ticket_quotas(view: &PlayerView) -> Option<Vec<ttr_proto::TicketDeckCount>> {
    let draw = view.legal_moves.ticket_draw.as_ref()?;
    let mut quotas: Vec<ttr_proto::TicketDeckCount> = draw
        .available
        .iter()
        .map(|a| ttr_proto::TicketDeckCount {
            deck: a.deck.clone(),
            count: 0,
        })
        .collect();
    let mut left = draw.total;
    while left > 0 {
        let mut progressed = false;
        for (quota, available) in quotas.iter_mut().zip(&draw.available) {
            if left > 0 && quota.count < available.count {
                quota.count += 1;
                left -= 1;
                progressed = true;
            }
        }
        assert!(progressed, "quote impossibili da distribuire");
    }
    quotas.retain(|q| q.count > 0);
    Some(quotas)
}

/// Primo colore (alfabetico) dei passeggeri su ciascun estremo della rotta; `None` senza
/// passeggeri sulla mappa.
pub fn passenger_picks(
    public: &ttr_proto::PublicState,
    map: &MapData,
    route_id: &str,
) -> Option<Vec<ttr_proto::PassengerPick>> {
    let route = map.routes.iter().find(|r| r.id == route_id)?;
    let picks: Vec<ttr_proto::PassengerPick> = public
        .passengers
        .iter()
        .filter(|city| city.city_id == route.from || city.city_id == route.to)
        .filter_map(|city| {
            city.passengers.first().map(|p| ttr_proto::PassengerPick {
                city_id: city.city_id.clone(),
                color: p.color.clone(),
            })
        })
        .collect();
    (!picks.is_empty()).then_some(picks)
}

/// Decide la prossima mossa per `me` dato lo stato pubblico e la vista privata.
/// Ritorna `Some` quando c'è un'azione da inviare, `None` altrimenti.
pub fn decide(
    me: &str,
    view: &mut PlayerView,
    public: &ttr_proto::PublicState,
    map: &MapData,
    drafted: &mut bool,
) -> Result<Option<InboundAction>, String> {
    let player_id = me.to_string();

    match public.state {
        GameState::GameOver => Ok(None),
        GameState::InitialDraft => {
            if *drafted {
                return Ok(None);
            }
            // Con `chooseFrom` si pesca prima con le quote, poi si sceglie.
            if view.offered_tickets.is_empty() && view.legal_moves.ticket_draw.is_some() {
                return Ok(Some(InboundAction::DrawTickets(
                    ttr_proto::DrawTicketsPayload {
                        player_id,
                        counts: ticket_quotas(view),
                        client_seq: None,
                    },
                )));
            }
            // keep = primi 2 biglietti offerti (l'ordine è deterministico).
            let keep: Vec<String> = view
                .offered_tickets
                .iter()
                .take(2)
                .map(|t| t.id.clone())
                .collect();
            if keep.len() < 2 {
                return Err(format!(
                    "draft: solo {} biglietti offerti per {}",
                    keep.len(),
                    me
                ));
            }
            *drafted = true;
            Ok(Some(InboundAction::ChooseTickets(
                ttr_proto::ChooseTicketsPayload {
                    player_id,
                    keep,
                    client_seq: None,
                },
            )))
        }
        GameState::PlayerTurn | GameState::LastRound => {
            if public.current_turn_player_id.as_deref() != Some(me) {
                return Ok(None);
            }
            // 1) Claim della prima rotta legale (pagamento deterministico).
            if let Some(rid) = view.legal_moves.routes.first()
                && let Some((len, color)) = route_len_color(map, rid)
                && let Some(payment) = build_payment(&view.hand, &color, len)
            {
                return Ok(Some(InboundAction::ClaimRoute(
                    ttr_proto::ClaimRoutePayload {
                        player_id,
                        route_id: rid.clone(),
                        cards_discarded: payment,
                        passengers: passenger_picks(public, map, rid),
                        client_seq: None,
                    },
                )));
            }
            // 2) DRAW_TRAIN_CARD dal mazzo.
            if view.legal_moves.can_draw_deck {
                return Ok(Some(InboundAction::DrawTrainCard(
                    ttr_proto::DrawCardPayload {
                        player_id,
                        source: ttr_proto::DrawSource::Deck,
                        client_seq: None,
                    },
                )));
            }
            // 3) DRAW_TICKETS.
            if view.legal_moves.can_draw_tickets {
                return Ok(Some(InboundAction::DrawTickets(
                    ttr_proto::DrawTicketsPayload {
                        player_id,
                        counts: ticket_quotas(view),
                        client_seq: None,
                    },
                )));
            }
            Ok(None)
        }
        GameState::SecondCardDraw => {
            if public.current_turn_player_id.as_deref() != Some(me) {
                return Ok(None);
            }
            if view.legal_moves.can_draw_deck {
                return Ok(Some(InboundAction::DrawTrainCard(
                    ttr_proto::DrawCardPayload {
                        player_id,
                        source: ttr_proto::DrawSource::Deck,
                        client_seq: None,
                    },
                )));
            }
            // Seconda pescata da una delle scoperte (mai una locomotiva).
            if let Some(idx) = view.legal_moves.draw_face_up.first().copied() {
                return Ok(Some(InboundAction::DrawTrainCard(
                    ttr_proto::DrawCardPayload {
                        player_id,
                        source: ttr_proto::DrawSource::FaceUp { index: idx },
                        client_seq: None,
                    },
                )));
            }
            Err(format!("SecondCardDraw senza pescata legale per {}", me))
        }
        GameState::TicketChoice => {
            if public.current_turn_player_id.as_deref() != Some(me) {
                return Ok(None);
            }
            // keep = primo biglietto offerto (keepMin mid-game è 1).
            if let Some(first) = view.offered_tickets.first().cloned() {
                return Ok(Some(InboundAction::ChooseTickets(
                    ttr_proto::ChooseTicketsPayload {
                        player_id,
                        keep: vec![first.id],
                        client_seq: None,
                    },
                )));
            }
            Err(format!("TicketChoice senza biglietti offerti per {}", me))
        }
        _ => Ok(None),
    }
}

/// Applica un messaggio WS alla vista del giocatore e aggiorna `public`.
/// Ritorna `true` se il messaggio era uno stato privato fresco.
pub fn apply_msg(
    msg: &ServerMessage,
    player: &mut Player,
    public: &mut Option<ttr_proto::PublicState>,
) -> bool {
    match msg {
        ServerMessage::PrivateState(ps) => {
            if let Some(ref mut client) = player.client {
                client.view.apply_private(ps);
            }
            let fresh = ps.seq > player.last_acted_seq;
            if fresh {
                player.last_acted_seq = ps.seq;
            }
            fresh
        }
        ServerMessage::Snapshot(snap) => {
            if let Some(ps) = &snap.private {
                if let Some(ref mut client) = player.client {
                    client.view.apply_private(ps);
                }
                let fresh = ps.seq > player.last_acted_seq;
                if fresh {
                    player.last_acted_seq = ps.seq;
                }
                *public = Some(snap.public.clone());
                fresh
            } else {
                *public = Some(snap.public.clone());
                false
            }
        }
        ServerMessage::PublicState(ps) => {
            *public = Some(ps.clone());
            false
        }
        ServerMessage::Error(ep) => {
            if let Some(ref mut client) = player.client {
                client.view.last_error = Some(ep.clone());
            }
            false
        }
    }
}

/// Helper async per `select!`: riceve dal client se connesso.
pub async fn recv_from(p: &mut Player) -> ServerMessage {
    p.client.as_mut().expect("player connesso").recv().await
}

/// Coordina due client scemi fino a `GAME_OVER`. Ritorna l'ultimo PublicState.
/// Entrambi i giocatori devono essere connessi.
#[allow(dead_code)]
pub async fn run_game(a: &mut Player, b: &mut Player, map: &MapData) -> ttr_proto::PublicState {
    // Stato pubblico per giocatore: i due stream non sono ordinati tra loro, quindi uno
    // stato condiviso potrebbe tornare indietro e far decidere su dati vecchi.
    let (mut public_a, mut public_b): (
        Option<ttr_proto::PublicState>,
        Option<ttr_proto::PublicState>,
    ) = (None, None);
    let mut actions = 0usize;
    let max_actions = 5000;
    // Una decisione resta in sospeso finché il giocatore non agisce: lo stato pubblico
    // dell'altro stream può arrivare dopo lo stato privato.
    let (mut pending_a, mut pending_b) = (false, false);

    loop {
        tokio::select! {
            msg = recv_from(a) => {
                pending_a |= apply_msg(&msg, a, &mut public_a);
                let Some(p) = public_a.clone() else { continue; };
                if p.state == GameState::GameOver {
                    return p;
                }
                if pending_a {
                    let act = decide(&a.player_id, &mut a.client.as_mut().unwrap().view, &p, map, &mut a.drafted)
                        .expect("decisione giocatore A");
                    if let Some(act) = act {
                        a.client.as_mut().unwrap().send(&act).await;
                        actions += 1;
                        pending_a = false;
                    }
                }
            }
            msg = recv_from(b) => {
                pending_b |= apply_msg(&msg, b, &mut public_b);
                let Some(p) = public_b.clone() else { continue; };
                if p.state == GameState::GameOver {
                    return p;
                }
                if pending_b {
                    let act = decide(&b.player_id, &mut b.client.as_mut().unwrap().view, &p, map, &mut b.drafted)
                        .expect("decisione giocatore B");
                    if let Some(act) = act {
                        b.client.as_mut().unwrap().send(&act).await;
                        actions += 1;
                        pending_b = false;
                    }
                }
            }
        }

        assert!(
            actions < max_actions,
            "partita non terminata entro {} azioni",
            max_actions
        );
    }
}
