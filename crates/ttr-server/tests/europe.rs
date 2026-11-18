//! Test WS e save/replay sulla mappa Europe: stazioni, tunnel, ripresa di un save
//! con un tunnel pendente.

mod common;

use std::collections::BTreeSet;

use common::driver::*; // Player, apply_msg, recv_from
use common::*;
use ttr_core::types::Route;
use ttr_proto::{
    BuildStationPayload, CardColor, ClaimRoutePayload, GameState, InboundAction, ServerMessage,
    TunnelDecisionPayload,
};

/// Pagamento valido per `route` con la mano data (traghetti compresi), se esiste.
fn route_payment(hand: &[CardColor], route: &Route) -> Option<Vec<CardColor>> {
    let target = route_color_card(&route.color);
    cards_for(hand, target, route.len as usize, route.ferries as usize)
}

/// `len` carte del colore `target` (o di un colore qualunque se `None`), con almeno
/// `ferries` locomotive e le locomotive a riempimento.
fn cards_for(
    hand: &[CardColor],
    target: Option<CardColor>,
    len: usize,
    ferries: usize,
) -> Option<Vec<CardColor>> {
    let count = |c: CardColor| hand.iter().filter(|h| **h == c).count();
    let locos = count(CardColor::Locomotive);
    let colors: Vec<CardColor> = target.map_or_else(|| CARD_COLOR_ORDER.to_vec(), |c| vec![c]);
    for color in colors {
        let colored = count(color).min(len - ferries);
        if locos >= len - colored {
            let mut payment = vec![color; colored];
            payment.resize(len, CardColor::Locomotive);
            return Some(payment);
        }
    }
    (locos >= len).then(|| vec![CardColor::Locomotive; len])
}

/// `extra` carte del colore del tunnel o locomotive (solo locomotive con `LOCOMOTIVE`).
fn tunnel_cards(hand: &[CardColor], color: CardColor, extra: usize) -> Option<Vec<CardColor>> {
    let mut payment: Vec<CardColor> = hand.iter().copied().filter(|c| *c == color).collect();
    payment.truncate(extra);
    let locos = hand.iter().filter(|c| **c == CardColor::Locomotive);
    payment.extend(locos.take(extra - payment.len()));
    (payment.len() == extra).then_some(payment)
}

/// Rotta da reclamare. Il bot tiene sempre qualche carta di scorta per un eventuale costo
/// extra dei tunnel: prima i tunnel dopo i quali restano tre carte utili, poi i traghetti,
/// poi le altre.
fn pick_route<'a>(
    map: &'a ttr_core::MapData,
    legal: &[String],
    hand: &[CardColor],
) -> Option<(&'a Route, Vec<CardColor>)> {
    let mut candidates: Vec<(&Route, Vec<CardColor>, bool)> = legal
        .iter()
        .filter_map(|id| map.routes.iter().find(|r| &r.id == id))
        .filter(|r| hand.len() >= r.len as usize + 3)
        .filter_map(|r| route_payment(hand, r).map(|p| (r, p)))
        .map(|(r, payment)| {
            let color = payment
                .iter()
                .copied()
                .find(|c| *c != CardColor::Locomotive)
                .unwrap_or(CardColor::Locomotive);
            let mut rest = hand.to_vec();
            for card in &payment {
                let pos = rest.iter().position(|c| c == card).unwrap();
                rest.remove(pos);
            }
            let useful = rest
                .iter()
                .filter(|c| **c == color || **c == CardColor::Locomotive)
                .count();
            let spare = r.tunnel && useful >= 3;
            (r, payment, spare)
        })
        .collect();
    candidates.sort_by_key(|(r, _, spare)| (!spare, r.ferries == 0));
    candidates.into_iter().next().map(|(r, p, _)| (r, p))
}

#[derive(Default)]
struct Stats {
    stations_built: BTreeSet<String>,
    paid: usize,
    declined: usize,
    ferries: usize,
    tunnels: usize,
}

/// Prossima mossa del client "scemo" Europe, decisa solo da `legalMoves` e dallo stato pubblico.
fn decide_europe(
    me: &str,
    view: &PlayerView,
    public: &ttr_proto::PublicState,
    map: &ttr_core::MapData,
    drafted: &mut bool,
    stats: &mut Stats,
) -> Option<InboundAction> {
    let player_id = me.to_string();
    let my_turn = public.current_turn_player_id.as_deref() == Some(me);
    let moves = &view.legal_moves;
    match public.state {
        GameState::InitialDraft => {
            if *drafted {
                return None;
            }
            let keep: Vec<String> = view
                .offered_tickets
                .iter()
                .take(2)
                .map(|t| t.id.clone())
                .collect();
            assert_eq!(keep.len(), 2, "draft Europe: biglietti offerti a {me}");
            // 1 lungo + 3 regolari offerti.
            assert_eq!(view.offered_tickets.len(), 4);
            *drafted = true;
            Some(InboundAction::ChooseTickets(
                ttr_proto::ChooseTicketsPayload {
                    player_id,
                    keep,
                    client_seq: None,
                },
            ))
        }
        GameState::TunnelResolution if my_turn => {
            let tunnel = moves.tunnel.as_ref()?;
            let reveal = public
                .tunnel_reveal
                .as_ref()
                .expect("tunnel_reveal in TUNNEL_RESOLUTION");
            assert_eq!(reveal.route_id, tunnel.route_id);
            assert_eq!(reveal.extra_cost, tunnel.extra_cost);
            assert_eq!(reveal.color, tunnel.color);
            assert!(reveal.cards.len() <= 3);
            assert!(tunnel.extra_cost >= 1);
            // Paga quando può, altrimenti rinuncia (l'unica scelta senza `can_pay`).
            let cards = tunnel
                .can_pay
                .then(|| tunnel_cards(&view.hand, tunnel.color, tunnel.extra_cost as usize))
                .flatten();
            let pay = cards.is_some();
            if pay {
                stats.paid += 1;
            } else {
                stats.declined += 1;
            }
            Some(InboundAction::TunnelDecision(TunnelDecisionPayload {
                player_id,
                pay,
                cards_discarded: cards.unwrap_or_default(),
                client_seq: None,
            }))
        }
        GameState::PlayerTurn | GameState::LastRound if my_turn => {
            // Una stazione a testa, appena la mano la copre.
            if !stats.stations_built.contains(me)
                && let Some(city) = moves.station_cities.first()
                && let Some(cards) = cards_for(&view.hand, None, moves.station_cost as usize, 0)
            {
                assert!(moves.station_cost >= 1);
                stats.stations_built.insert(me.to_string());
                return Some(InboundAction::BuildStation(BuildStationPayload {
                    player_id,
                    city_id: city.clone(),
                    cards_discarded: cards,
                    client_seq: None,
                }));
            }
            if let Some((route, cards)) = pick_route(map, &moves.routes, &view.hand) {
                stats.ferries += usize::from(route.ferries > 0);
                stats.tunnels += usize::from(route.tunnel);
                return Some(InboundAction::ClaimRoute(ClaimRoutePayload {
                    player_id,
                    route_id: route.id.clone(),
                    cards_discarded: cards,
                    passengers: None,
                    client_seq: None,
                }));
            }
            if moves.can_draw_deck {
                return Some(InboundAction::DrawTrainCard(ttr_proto::DrawCardPayload {
                    player_id,
                    source: ttr_proto::DrawSource::Deck,
                    client_seq: None,
                }));
            }
            moves.can_draw_tickets.then_some(InboundAction::DrawTickets(
                ttr_proto::DrawTicketsPayload {
                    player_id,
                    client_seq: None,
                },
            ))
        }
        GameState::SecondCardDraw if my_turn => {
            if moves.can_draw_deck {
                return Some(InboundAction::DrawTrainCard(ttr_proto::DrawCardPayload {
                    player_id,
                    source: ttr_proto::DrawSource::Deck,
                    client_seq: None,
                }));
            }
            moves.draw_face_up.first().map(|&index| {
                InboundAction::DrawTrainCard(ttr_proto::DrawCardPayload {
                    player_id,
                    source: ttr_proto::DrawSource::FaceUp { index },
                    client_seq: None,
                })
            })
        }
        GameState::TicketChoice if my_turn => {
            let first = view.offered_tickets.first()?;
            Some(InboundAction::ChooseTickets(
                ttr_proto::ChooseTicketsPayload {
                    player_id,
                    keep: vec![first.id.clone()],
                    client_seq: None,
                },
            ))
        }
        _ => None,
    }
}

/// Due client Europe fino a `GAME_OVER` (stessa coordinazione di `driver::run_game`).
async fn run_europe_game(
    a: &mut Player,
    b: &mut Player,
    map: &ttr_core::MapData,
    stats: &mut Stats,
) -> ttr_proto::PublicState {
    let mut public_a: Option<ttr_proto::PublicState> = None;
    let mut public_b: Option<ttr_proto::PublicState> = None;
    let (mut pending_a, mut pending_b) = (false, false);
    let mut actions = 0usize;
    loop {
        tokio::select! {
            msg = recv_from(a) => {
                pending_a |= apply_msg(&msg, a, &mut public_a);
                let Some(p) = public_a.clone() else { continue; };
                if p.state == GameState::GameOver {
                    return p;
                }
                if pending_a {
                    let client = a.client.as_mut().unwrap();
                    if let Some(act) = decide_europe(&a.player_id, &client.view, &p, map, &mut a.drafted, stats) {
                        client.send(&act).await;
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
                    let client = b.client.as_mut().unwrap();
                    if let Some(act) = decide_europe(&b.player_id, &client.view, &p, map, &mut b.drafted, stats) {
                        client.send(&act).await;
                        actions += 1;
                        pending_b = false;
                    }
                }
            }
        }
        assert!(actions < 5000, "partita non terminata entro 5000 azioni");
    }
}

/// Partita Europe completa via WS: stazioni, tunnel pagati e rifiutati, traghetti,
/// punteggio finale con `stationPoints` e `borrowedRoutes`.
#[tokio::test]
async fn europe_full_game_over_ws_with_stations_and_tunnels() {
    let server = TestServer::start().await;
    let map = europe_map();
    let game = create_game_for_ruleset(&server.base_url, "europe", "Europe", Some(5)).await;
    assert_eq!(game.ruleset_id, "europe");
    let pa = join_game(&server.base_url, &game.game_id, "Alice", None).await;
    let pb = join_game(&server.base_url, &game.game_id, "Bob", None).await;
    start_game(&server.base_url, &game.game_id, &game.host_token).await;

    let mut a =
        Player::new(WsClient::connect(&server, &game.game_id, &pa.player_id, &pa.token).await);
    let mut b =
        Player::new(WsClient::connect(&server, &game.game_id, &pb.player_id, &pb.token).await);
    let mut stats = Stats::default();
    let public = run_europe_game(&mut a, &mut b, &map, &mut stats).await;

    for p in [&a, &b] {
        let error = &p.client.as_ref().unwrap().view.last_error;
        assert!(
            error.is_none(),
            "il server ha rifiutato un'azione: {error:?}"
        );
    }
    assert_eq!(public.state, GameState::GameOver);
    assert!(public.tunnel_reveal.is_none());
    eprintln!(
        "stazioni {}, tunnel {}, pagati {}, rinunce {}, traghetti {}",
        stats.stations_built.len(),
        stats.tunnels,
        stats.paid,
        stats.declined,
        stats.ferries
    );
    assert_eq!(stats.stations_built.len(), 2, "una stazione a testa");
    assert!(
        stats.paid + stats.declined >= 1,
        "nessun tunnel è arrivato alla decisione"
    );
    assert_eq!(public.stations.len(), 2);
    for summary in &public.players_summary {
        let built = public
            .stations
            .iter()
            .filter(|s| s.player_id == summary.player_id)
            .count();
        assert_eq!(built, 1);
        assert_eq!(summary.stations_remaining, 2);
        assert!(summary.trains_remaining <= 45);
    }
    let cities: Vec<&String> = public.stations.iter().map(|s| &s.city_id).collect();
    let mut sorted = cities.clone();
    sorted.sort();
    assert_eq!(cities, sorted, "stazioni in ordine di città");

    let scores = public.final_scores.as_ref().expect("final_scores");
    assert_eq!(scores.len(), 2);
    for s in scores {
        assert_eq!(s.station_points, 8, "due stazioni non costruite × 4");
        assert_eq!(
            s.total,
            s.route_points
                + s.ticket_completed
                + s.ticket_failed
                + s.longest_path_bonus
                + s.station_points
        );
        assert!(s.borrowed_routes.len() <= 1, "una sola stazione");
        for borrowed in &s.borrowed_routes {
            assert!(
                public
                    .stations
                    .iter()
                    .any(|st| st.city_id == borrowed.city_id)
            );
            assert!(
                public
                    .claimed_routes
                    .iter()
                    .any(|c| c.route_id == borrowed.route_id
                        && c.claimed_by_player_id != s.player_id)
            );
        }
    }
}

/// Un save Europe preso mentre un tunnel è pendente si riprende nello stesso stato e il
/// giocatore di turno può decidere.
#[tokio::test]
async fn europe_save_with_pending_tunnel_resumes_and_accepts_the_decision() {
    use ttr_core::{Action, DrawSource, error::GamePhase};

    for pay in [true, false] {
        let server = TestServer::start().await;
        let map = europe_map();
        let ruleset = load_europe_ruleset();
        let g = create_game_for_ruleset(&server.base_url, "europe", "EuropeSave", Some(11)).await;
        let pa = join_game(&server.base_url, &g.game_id, "Alice", Some("PINK")).await;
        let pb = join_game(&server.base_url, &g.game_id, "Bob", Some("CYAN")).await;
        start_game(&server.base_url, &g.game_id, &g.host_token).await;

        let save = server.saves_dir.join(format!("{}.json", g.game_id));
        wait_for_file(&save).await;
        let mut raw = serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(&save).expect("lettura save"),
        )
        .expect("parse save");
        let parsed: ttr_server::save::SaveData =
            serde_json::from_value(raw.clone()).expect("parse SaveData");
        let mut engine =
            ttr_server::save::replay_engine(&parsed, &map, &ruleset).expect("replay engine");

        for player_index in 0..2 {
            let keep = engine.initial_tickets_for(player_index).unwrap()[..2].to_vec();
            engine
                .apply(Action::ChooseTickets { player_index, keep })
                .unwrap();
        }
        // Gioca offline pescando carte; a ogni turno prova (su una copia del motore) i
        // tunnel reclamabili e si ferma al primo che resta in attesa della decisione
        // (per la variante "paga", al primo che il giocatore può anche pagare).
        for _ in 0..3000 {
            if engine.phase() == GamePhase::TunnelResolution {
                break;
            }
            let pi = engine.current_player();
            let hand = engine.player_hand(pi).unwrap().to_vec();
            let legal = engine.legal_moves(pi).routes;
            let attempt = legal
                .iter()
                .filter_map(|id| map.routes.iter().find(|r| &r.id == id))
                .filter(|r| r.tunnel && engine.phase() == GamePhase::PlayerTurn)
                .filter_map(|r| {
                    let cards_discarded = route_payment(&hand, r)?;
                    Some(Action::ClaimRoute {
                        player_index: pi,
                        route_id: r.id.clone(),
                        cards_discarded,
                        passengers: Default::default(),
                    })
                })
                .find(|claim| {
                    let mut copy = engine.clone();
                    copy.apply(claim.clone()).unwrap();
                    copy.phase() == GamePhase::TunnelResolution
                        && (!pay
                            || copy
                                .legal_moves(copy.current_player())
                                .tunnel
                                .unwrap()
                                .can_pay)
                });
            let action = attempt.unwrap_or(Action::DrawTrainCard {
                player_index: pi,
                source: DrawSource::Deck,
            });
            engine.apply(action).unwrap();
        }
        assert_eq!(
            engine.phase(),
            GamePhase::TunnelResolution,
            "nessun tunnel pendente"
        );
        let pending = engine.pending_tunnel().expect("tunnel pendente").clone();
        let decider = engine.current_player();

        raw["actions"] = engine
            .action_log()
            .iter()
            .map(|rec| serde_json::json!({ "action": rec.action }))
            .collect();
        std::fs::write(&save, raw.to_string()).expect("scrittura save");

        // Il replay del save riproduce esattamente lo stato, tunnel pendente compreso.
        let saved: ttr_server::save::SaveData =
            serde_json::from_value(raw).expect("parse SaveData");
        let replayed = ttr_server::save::replay_engine(&saved, &map, &ruleset).expect("replay");
        assert_eq!(replayed.serialize_state(), engine.serialize_state());
        assert_eq!(replayed.pending_tunnel(), Some(&pending));

        let (rid, rtok) = resume_after_restart(&server, &save).await.unwrap();
        let ra = join_game_resume(&server.base_url, &rid, "Alice", &pa.token).await;
        let rb = join_game_resume(&server.base_url, &rid, "Bob", &pb.token).await;
        start_game(&server.base_url, &rid, &rtok).await;
        let ids = [ra, rb];
        let mut clients = Vec::new();
        for joined in &ids {
            let mut client =
                WsClient::connect(&server, &rid, &joined.player_id, &joined.token).await;
            loop {
                if let ServerMessage::Snapshot(_) = client.recv().await {
                    break;
                }
            }
            clients.push(client);
        }

        // Il giocatore di turno è quello del motore (ordine dei giocatori nel save).
        let decider_id = engine.player(decider).unwrap().id.clone();
        let (me, other): (Vec<_>, Vec<_>) =
            clients.into_iter().partition(|c| c.player_id == decider_id);
        let (mut me, other) = (
            me.into_iter().next().unwrap(),
            other.into_iter().next().unwrap(),
        );
        let public = me.view.public.clone().expect("public state");
        assert_eq!(public.state, GameState::TunnelResolution);
        let reveal = public.tunnel_reveal.as_ref().expect("tunnel_reveal");
        assert_eq!(reveal.route_id, pending.route_id);
        assert_eq!(reveal.cards, pending.revealed);
        assert_eq!(reveal.color, pending.color);
        assert_eq!(reveal.extra_cost, pending.extra);
        let tunnel = me
            .view
            .legal_moves
            .tunnel
            .clone()
            .expect("legal_moves.tunnel");
        assert_eq!(tunnel.extra_cost, pending.extra);
        assert!(me.view.legal_moves.routes.is_empty());
        assert!(other.view.legal_moves.tunnel.is_none());

        let can_pay = tunnel.can_pay;
        let cards = tunnel_cards(&me.view.hand, tunnel.color, tunnel.extra_cost as usize);
        let pay = pay && can_pay;
        me.send(&InboundAction::TunnelDecision(TunnelDecisionPayload {
            player_id: me.player_id.clone(),
            pay,
            cards_discarded: if pay { cards.unwrap() } else { Vec::new() },
            client_seq: None,
        }))
        .await;
        let after = loop {
            if let ServerMessage::PublicState(ps) = me.recv().await
                && ps.state != GameState::TunnelResolution
            {
                break ps;
            }
        };
        assert!(after.tunnel_reveal.is_none());
        assert!(me.view.last_error.is_none(), "{:?}", me.view.last_error);
        let claimed = after
            .claimed_routes
            .iter()
            .any(|c| c.route_id == pending.route_id);
        assert_eq!(
            claimed, pay,
            "la rotta è reclamata solo se il tunnel è pagato"
        );
    }
}
