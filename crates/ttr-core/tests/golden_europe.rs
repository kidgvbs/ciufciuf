//! Casi del regolamento Europe sulla mappa e sul ruleset reali: traghetti, tunnel,
//! stazioni, punteggio finale con rotte prese in prestito, conservazione delle carte.

use ttr_core::error::GamePhase;
use ttr_core::event::FinalScore;
use ttr_core::types::{MapData, Route, Ruleset};
use ttr_core::{
    Action, DrawSource, Engine, EngineError, EngineInitError, GameEvent, TunnelMoves,
    default_player_colors,
};
use ttr_proto::CardColor::{self, Black, Blue, Green, Locomotive, Orange, Purple, Red, Yellow};

fn europe() -> (Ruleset, MapData) {
    let ruleset = std::fs::read_to_string("../../data/rulesets/europe.json").unwrap();
    let map = std::fs::read_to_string("../../data/maps/europe.json").unwrap();
    (
        serde_json::from_str(&ruleset).unwrap(),
        serde_json::from_str(&map).unwrap(),
    )
}

/// Partita Europe con draft iniziale completato (si tengono i primi due biglietti
/// offerti), al turno del giocatore 0.
fn new_game(players: usize) -> Engine {
    let (ruleset, map) = europe();
    let mut eng = Engine::new(
        7,
        ruleset,
        map,
        (0..players).map(|i| format!("p{i}")).collect(),
        (0..players).map(|i| format!("P{i}")).collect(),
        default_player_colors(players),
        0,
    )
    .unwrap();
    for pi in 0..players {
        let keep: Vec<String> = eng
            .initial_tickets_for(pi)
            .unwrap()
            .iter()
            .take(2)
            .cloned()
            .collect();
        eng.apply(Action::ChooseTickets {
            player_index: pi,
            keep,
        })
        .unwrap();
    }
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
    assert_eq!(eng.current_player(), 0);
    eng
}

fn claim(
    eng: &mut Engine,
    pi: usize,
    route: &str,
    cards: &[CardColor],
) -> Result<Vec<GameEvent>, EngineError> {
    eng.apply(Action::ClaimRoute {
        player_index: pi,
        route_id: route.into(),
        cards_discarded: cards.to_vec(),
        passengers: Default::default(),
    })
}

fn decide(
    eng: &mut Engine,
    pi: usize,
    pay: bool,
    cards: &[CardColor],
) -> Result<Vec<GameEvent>, EngineError> {
    eng.apply(Action::TunnelDecision {
        player_index: pi,
        pay,
        cards_discarded: cards.to_vec(),
    })
}

fn build_station(
    eng: &mut Engine,
    pi: usize,
    city: &str,
    cards: &[CardColor],
) -> Result<Vec<GameEvent>, EngineError> {
    eng.apply(Action::BuildStation {
        player_index: pi,
        city_id: city.into(),
        cards_discarded: cards.to_vec(),
    })
}

/// Mette in cima al mazzo le carte indicate, nell'ordine in cui verranno rivelate.
fn set_top(eng: &mut Engine, reveal_order: &[CardColor]) {
    let mut pile = eng.drain_draw_pile();
    pile.extend(reveal_order.iter().rev());
    eng.set_draw_pile(pile);
}

/// Come `set_top`, ma sposta carte già nel mazzo (conserva il totale).
fn move_to_top(eng: &mut Engine, reveal_order: &[CardColor]) {
    let mut pile = eng.drain_draw_pile();
    let mut top = Vec::new();
    for card in reveal_order {
        let pos = pile
            .iter()
            .position(|c| c == card)
            .unwrap_or_else(|| panic!("{card:?} non c'è nel mazzo"));
        top.push(pile.remove(pos));
    }
    pile.extend(top.into_iter().rev());
    eng.set_draw_pile(pile);
}

/// Sposta dal mazzo alla mano del giocatore le carte indicate (conserva il totale).
fn give(eng: &mut Engine, pi: usize, cards: &[CardColor]) {
    let mut pile = eng.drain_draw_pile();
    let mut hand = eng.player_hand(pi).unwrap().to_vec();
    for card in cards {
        let pos = pile
            .iter()
            .position(|c| c == card)
            .unwrap_or_else(|| panic!("{card:?} non c'è nel mazzo"));
        hand.push(pile.remove(pos));
    }
    eng.set_draw_pile(pile);
    eng.set_player_hand(pi, hand);
}

fn force_turn(eng: &mut Engine, pi: usize) {
    eng.set_phase_for_test(GamePhase::PlayerTurn, pi);
}

/// Stato più RNG: un'azione rifiutata deve lasciarli identici.
fn snapshot(eng: &Engine) -> (String, u64, Vec<Vec<CardColor>>) {
    let hands = (0..eng.num_players())
        .map(|pi| eng.player_hand(pi).unwrap().to_vec())
        .collect();
    (eng.serialize_state(), eng.peek_rng_word(), hands)
}

fn total_cards(eng: &Engine) -> usize {
    let hands: usize = (0..eng.num_players())
        .map(|pi| eng.player_hand(pi).unwrap().len())
        .sum();
    let face_up = eng.face_up_cards().iter().flatten().count();
    let pending = eng
        .pending_tunnel()
        .map_or(0, |p| p.played.len() + p.revealed.len());
    eng.draw_pile_len() + eng.discards_len() + face_up + hands + pending
}

fn has_route(eng: &Engine, route: &str) -> bool {
    eng.claimed_route_ids().iter().any(|r| r.as_str() == route)
}

// ── Traghetti ────────────────────────────────────────────────────────

#[test]
fn ferry_roma_palermo_needs_one_locomotive() {
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Red, Red, Red, Red]);
    assert!(!eng.legal_moves(0).routes.contains(&"palermo_roma_1".into()));
    let before = snapshot(&eng);
    let err = claim(&mut eng, 0, "palermo_roma_1", &[Red; 4]).unwrap_err();
    assert!(matches!(err, EngineError::InsufficientCards { .. }));
    assert_eq!(snapshot(&eng), before);

    eng.set_player_hand(0, vec![Red, Red, Red, Locomotive]);
    assert!(eng.legal_moves(0).routes.contains(&"palermo_roma_1".into()));
    let events = claim(&mut eng, 0, "palermo_roma_1", &[Red, Red, Red, Locomotive]).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::RouteClaimed { points: 7, .. }))
    );
    assert!(has_route(&eng, "palermo_roma_1"));
}

#[test]
fn ferry_check_uses_the_cards_actually_removed() {
    // Quattro Red dichiarate, ma in mano una sola locomotiva al posto della quarta:
    // le carte tolte contengono la locomotiva richiesta.
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Red, Red, Red, Locomotive]);
    claim(&mut eng, 0, "palermo_roma_1", &[Red; 4]).unwrap();

    // Locomotiva dichiarata ma assente: nessuna sostituzione possibile.
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Red, Red, Red, Red]);
    let err = claim(&mut eng, 0, "palermo_roma_1", &[Red, Red, Red, Locomotive]).unwrap_err();
    assert!(matches!(err, EngineError::InsufficientCards { .. }));
}

#[test]
fn double_ferry_needs_two_locomotives() {
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Red, Red, Red, Red, Red, Locomotive]);
    assert!(
        !eng.legal_moves(0)
            .routes
            .contains(&"palermo_smyrna_1".into())
    );
    eng.set_player_hand(0, vec![Red, Red, Red, Red, Locomotive, Locomotive]);
    assert!(
        eng.legal_moves(0)
            .routes
            .contains(&"palermo_smyrna_1".into())
    );
    // Con una sola locomotiva dichiarata e due in mano, la quinta Red dichiarata (assente)
    // è sostituita dalla seconda: le carte tolte hanno due locomotive.
    claim(
        &mut eng,
        0,
        "palermo_smyrna_1",
        &[Red, Red, Red, Red, Red, Locomotive],
    )
    .unwrap();
    assert!(has_route(&eng, "palermo_smyrna_1"));
}

// ── Tunnel ───────────────────────────────────────────────────────────

#[test]
fn tunnel_without_matching_reveal_is_claimed_immediately() {
    // Esempio 4A: nessuna carta rivelata del colore giocato né locomotiva.
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Purple, Purple, Red]);
    set_top(&mut eng, &[Red, Blue, Black]);
    let discards_before = eng.discards_len();
    let events = claim(&mut eng, 0, "marseille_zurich_1", &[Purple, Purple]).unwrap();

    let revealed = events
        .iter()
        .position(|e| matches!(e, GameEvent::TunnelRevealed { .. }))
        .expect("TunnelRevealed");
    let claimed = events
        .iter()
        .position(|e| matches!(e, GameEvent::RouteClaimed { .. }))
        .expect("RouteClaimed");
    assert!(revealed < claimed);
    match &events[revealed] {
        GameEvent::TunnelRevealed {
            cards,
            color,
            extra_cost,
            ..
        } => {
            assert_eq!(cards, &[Red, Blue, Black]);
            assert_eq!(*color, Purple);
            assert_eq!(*extra_cost, 0);
        }
        _ => unreachable!(),
    }
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
    assert_eq!(eng.current_player(), 1);
    assert!(eng.pending_tunnel().is_none());
    assert!(has_route(&eng, "marseille_zurich_1"));
    assert_eq!(eng.discards_len(), discards_before + 5);
    assert_eq!(
        &eng.discard_cards()[discards_before..],
        &[Purple, Purple, Red, Blue, Black]
    );
}

#[test]
fn tunnel_extra_cost_green_and_locomotive_is_paid_in_order() {
    // Esempio 4B: giocate Green, rivelate Locomotive + Green → costo extra 2.
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Green, Green, Green, Locomotive, Red]);
    set_top(&mut eng, &[Locomotive, Green, Red]);
    let discards_before = eng.discards_len();
    let events = claim(&mut eng, 0, "venezia_zurich_1", &[Green, Green]).unwrap();
    assert!(matches!(
        events.last(),
        Some(GameEvent::TunnelRevealed {
            extra_cost: 2,
            color: Green,
            ..
        })
    ));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::RouteClaimed { .. }))
    );
    assert_eq!(eng.phase(), GamePhase::TunnelResolution);
    assert_eq!(eng.current_player(), 0);
    let pending = eng.pending_tunnel().unwrap();
    assert_eq!(pending.route_id, "venezia_zurich_1");
    assert_eq!(pending.played, [Green, Green]);
    assert_eq!(pending.revealed, [Locomotive, Green, Red]);
    assert_eq!(pending.extra, 2);
    assert_eq!(
        eng.discards_len(),
        discards_before,
        "carte ancora sul tavolo"
    );

    let moves = eng.legal_moves(0);
    assert_eq!(
        moves.tunnel,
        Some(TunnelMoves {
            route_id: "venezia_zurich_1".into(),
            extra_cost: 2,
            color: Green,
            can_pay: true,
        })
    );
    assert!(moves.routes.is_empty() && moves.draw_face_up.is_empty() && !moves.can_draw_deck);
    assert!(eng.legal_moves(1).tunnel.is_none());

    // Nessuna azione diversa dalla decisione è ammessa.
    let before = snapshot(&eng);
    assert!(matches!(
        eng.apply(Action::DrawTrainCard {
            player_index: 0,
            source: DrawSource::Deck
        }),
        Err(EngineError::WrongState { .. })
    ));
    assert!(matches!(
        claim(&mut eng, 0, "bruxelles_paris_1", &[Red, Red]),
        Err(EngineError::WrongState { .. })
    ));
    assert!(matches!(
        build_station(&mut eng, 0, "paris", &[Red]),
        Err(EngineError::WrongState { .. })
    ));
    assert!(matches!(
        decide(&mut eng, 1, true, &[Green, Locomotive]),
        Err(EngineError::NotYourTurn { .. })
    ));
    // Pagamenti non validi: stato (e RNG) invariati, si può ancora decidere.
    assert!(matches!(
        decide(&mut eng, 0, true, &[Green]),
        Err(EngineError::InsufficientTunnelCards { needed: 2, .. })
    ));
    assert!(matches!(
        decide(&mut eng, 0, true, &[Red, Green]),
        Err(EngineError::InvalidPayload { .. })
    ));
    assert_eq!(snapshot(&eng), before);

    let events = decide(&mut eng, 0, true, &[Green, Locomotive]).unwrap();
    match events
        .iter()
        .find(|e| matches!(e, GameEvent::RouteClaimed { .. }))
        .unwrap()
    {
        GameEvent::RouteClaimed {
            cards_used, points, ..
        } => {
            assert_eq!(cards_used, &[Green, Green, Green, Locomotive]);
            assert_eq!(*points, 2);
        }
        _ => unreachable!(),
    }
    assert!(has_route(&eng, "venezia_zurich_1"));
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
    assert_eq!(eng.current_player(), 1);
    assert!(eng.pending_tunnel().is_none());
    assert_eq!(eng.player_hand(0).unwrap(), [Red]);
    assert_eq!(eng.player(0).unwrap().trains_remaining, 43);
    // Scarti: giocate, extra, rivelate.
    assert_eq!(
        &eng.discard_cards()[discards_before..],
        &[Green, Green, Green, Locomotive, Locomotive, Green, Red]
    );
}

#[test]
fn tunnel_paid_with_locomotives_only_charges_only_locomotives() {
    // Esempio 4C: giocate solo locomotive, rivelate Locomotive + Green → extra 1,
    // e solo una locomotiva lo copre.
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Locomotive, Locomotive, Locomotive, Green]);
    set_top(&mut eng, &[Locomotive, Green, Red]);
    let events = claim(&mut eng, 0, "venezia_zurich_1", &[Locomotive, Locomotive]).unwrap();
    assert!(matches!(
        events.last(),
        Some(GameEvent::TunnelRevealed {
            extra_cost: 1,
            color: Locomotive,
            ..
        })
    ));
    assert_eq!(eng.phase(), GamePhase::TunnelResolution);

    let before = snapshot(&eng);
    assert!(matches!(
        decide(&mut eng, 0, true, &[Green]),
        Err(EngineError::InvalidPayload { .. })
    ));
    assert_eq!(snapshot(&eng), before);

    decide(&mut eng, 0, true, &[Locomotive]).unwrap();
    assert!(has_route(&eng, "venezia_zurich_1"));
    assert_eq!(eng.player_hand(0).unwrap(), [Green]);
}

#[test]
fn tunnel_extra_cost_beyond_the_hand_leaves_only_the_decline() {
    // D31: costo extra > 0 → sempre TUNNEL_RESOLUTION, anche con la mano insufficiente.
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Green, Green, Red]);
    set_top(&mut eng, &[Locomotive, Green, Green]);
    let trains_before = eng.player(0).unwrap().trains_remaining;
    claim(&mut eng, 0, "venezia_zurich_1", &[Green, Green]).unwrap();
    assert_eq!(eng.phase(), GamePhase::TunnelResolution);
    let tunnel = eng.legal_moves(0).tunnel.unwrap();
    assert_eq!((tunnel.extra_cost, tunnel.can_pay), (3, false));

    let before = snapshot(&eng);
    assert!(matches!(
        decide(&mut eng, 0, true, &[Red, Red, Red]),
        Err(EngineError::InsufficientTunnelCards { .. })
    ));
    assert_eq!(snapshot(&eng), before);

    let discards_before = eng.discards_len();
    let events = decide(&mut eng, 0, false, &[]).unwrap();
    assert!(matches!(events[0], GameEvent::TunnelDeclined { .. }));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::RouteClaimed { .. }))
    );
    assert!(matches!(
        events.last(),
        Some(GameEvent::TurnAdvanced {
            previous_player: 0,
            next_player: 1
        })
    ));
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
    assert_eq!(eng.current_player(), 1);
    assert!(eng.pending_tunnel().is_none());
    assert!(!has_route(&eng, "venezia_zurich_1"));
    assert_eq!(eng.player(0).unwrap().trains_remaining, trains_before);
    // Le giocate tornano in mano, le rivelate vanno agli scarti.
    let mut hand = eng.player_hand(0).unwrap().to_vec();
    hand.sort();
    let mut expected = vec![Green, Green, Red];
    expected.sort();
    assert_eq!(hand, expected);
    assert_eq!(
        &eng.discard_cards()[discards_before..],
        &[Locomotive, Green, Green]
    );
}

#[test]
fn invalid_tunnel_claims_leave_state_and_rng_untouched() {
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Purple, Blue, Purple, Locomotive]);
    set_top(&mut eng, &[Purple, Purple, Purple]);
    let before = snapshot(&eng);

    // Colori misti, carte in numero sbagliato, rotta già inesistente, turno altrui.
    let attempts: [(usize, &str, &[CardColor]); 5] = [
        (0, "marseille_zurich_1", &[Purple, Blue]),
        (0, "marseille_zurich_1", &[Purple]),
        (0, "marseille_zurich_1", &[Purple, Purple, Purple]),
        (1, "marseille_zurich_1", &[Purple, Purple]),
        (0, "venezia_zurich_1", &[Blue, Blue]),
    ];
    for (pi, route, cards) in attempts {
        assert!(
            claim(&mut eng, pi, route, cards).is_err(),
            "{route} {cards:?}"
        );
        assert_eq!(snapshot(&eng), before, "{route} {cards:?}");
    }

    // Treni insufficienti.
    eng.set_player_trains(0, 1);
    let before = snapshot(&eng);
    assert!(claim(&mut eng, 0, "marseille_zurich_1", &[Purple, Purple]).is_err());
    assert_eq!(snapshot(&eng), before);

    // Traghetto senza locomotive: la validazione precede la rivelazione.
    eng.set_player_trains(0, 45);
    eng.set_player_hand(0, vec![Red, Red, Red, Red]);
    let before = snapshot(&eng);
    assert!(claim(&mut eng, 0, "palermo_roma_1", &[Red; 4]).is_err());
    assert_eq!(snapshot(&eng), before);
}

#[test]
fn tunnel_reveal_reshuffles_discards_when_the_deck_runs_short() {
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Purple, Purple]);
    eng.set_draw_pile(vec![Red]);
    eng.set_discards(vec![Purple, Blue, Blue, Black]);
    let events = claim(&mut eng, 0, "marseille_zurich_1", &[Purple, Purple]).unwrap();

    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DeckReshuffled))
    );
    let Some(GameEvent::TunnelRevealed { cards, .. }) = events
        .iter()
        .find(|e| matches!(e, GameEvent::TunnelRevealed { .. }))
    else {
        panic!("TunnelRevealed mancante");
    };
    assert_eq!(cards.len(), 3);
    assert_eq!(cards[0], Red, "prima la carta rimasta nel mazzo");
    let mut rest = cards[1..].to_vec();
    rest.sort();
    let pool = [Black, Blue, Blue, Purple];
    assert!(
        rest.iter().all(|c| pool.contains(c)),
        "le altre due arrivano dagli scarti rimescolati"
    );
}

#[test]
fn tunnel_reveal_never_builds_a_new_deck() {
    // D30: mazzo e scarti vuoti → nessuna carta rivelata, nessun costo extra.
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Purple, Purple]);
    eng.clear_deck_and_discards();
    let events = claim(&mut eng, 0, "marseille_zurich_1", &[Purple, Purple]).unwrap();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::DeckReshuffled))
    );
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::TunnelRevealed { cards, extra_cost: 0, .. } if cards.is_empty()
    )));
    assert!(has_route(&eng, "marseille_zurich_1"));
    assert_eq!(eng.draw_pile_len(), 0);
    assert_eq!(eng.discards_len(), 2);

    // Mazzo con una sola carta e scarti vuoti: si rivela quella sola.
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Purple, Purple]);
    eng.set_draw_pile(vec![Red]);
    eng.set_discards(Vec::new());
    let events = claim(&mut eng, 0, "marseille_zurich_1", &[Purple, Purple]).unwrap();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::DeckReshuffled))
    );
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::TunnelRevealed { cards, .. } if cards == &[Red]
    )));
    assert_eq!(eng.draw_pile_len(), 0);
}

#[test]
fn cards_are_conserved_through_a_tunnel_payment_and_a_decline() {
    let mut eng = new_game(2);
    let total = total_cards(&eng);
    assert_eq!(total, 110);
    give(&mut eng, 0, &[Green, Green, Green, Locomotive]);
    move_to_top(&mut eng, &[Locomotive, Green, Red]);
    claim(&mut eng, 0, "venezia_zurich_1", &[Green, Green]).unwrap();
    assert_eq!(eng.phase(), GamePhase::TunnelResolution);
    assert_eq!(
        total_cards(&eng),
        total,
        "carte giocate e rivelate sul tavolo"
    );

    let mut declined = eng.clone();
    decide(&mut declined, 0, false, &[]).unwrap();
    assert_eq!(total_cards(&declined), total);

    decide(&mut eng, 0, true, &[Green, Locomotive]).unwrap();
    assert_eq!(total_cards(&eng), total);
}

// ── Stazioni ─────────────────────────────────────────────────────────

#[test]
fn stations_cost_one_two_then_three_cards_of_one_colour() {
    let mut eng = new_game(2);
    eng.set_player_hand(
        0,
        vec![
            Red, Blue, Blue, Blue, Blue, Blue, Blue, Locomotive, Locomotive, Locomotive,
        ],
    );
    let moves = eng.legal_moves(0);
    assert_eq!(moves.station_cost, 1);
    assert_eq!(moves.station_cities.len(), 47);
    assert!(moves.station_cities.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(eng.stations_remaining(0), 3);

    let trains = eng.player(0).unwrap().trains_remaining;
    let events = build_station(&mut eng, 0, "paris", &[Red]).unwrap();
    assert!(matches!(&events[0], GameEvent::StationBuilt { city_id, .. } if city_id == "paris"));
    assert_eq!(eng.current_player(), 1);
    assert_eq!(
        eng.player(0).unwrap().trains_remaining,
        trains,
        "nessun treno"
    );
    assert_eq!(eng.stations_remaining(0), 2);

    // Seconda stazione: due carte dello stesso colore (le locomotive fanno da jolly).
    force_turn(&mut eng, 0);
    let moves = eng.legal_moves(0);
    assert_eq!(moves.station_cost, 2);
    assert!(!moves.station_cities.contains(&"paris".to_string()));
    assert!(matches!(
        build_station(&mut eng, 0, "paris", &[Blue, Blue]),
        Err(EngineError::StationCityOccupied { .. })
    ));
    assert!(matches!(
        build_station(&mut eng, 0, "roma", &[Blue]),
        Err(EngineError::InsufficientStationCards { needed: 2 })
    ));
    let before = snapshot(&eng);
    assert!(matches!(
        build_station(&mut eng, 0, "roma", &[Blue, Blue, Blue]),
        Err(EngineError::InsufficientStationCards { needed: 2 })
    ));
    assert!(matches!(
        build_station(&mut eng, 0, "atlantis", &[Blue, Blue]),
        Err(EngineError::CityNotFound { .. })
    ));
    assert_eq!(snapshot(&eng), before);
    build_station(&mut eng, 0, "roma", &[Blue, Locomotive]).unwrap();

    // Terza stazione: tre carte; una dichiarata assente in mano diventa locomotiva.
    force_turn(&mut eng, 0);
    eng.set_player_hand(0, vec![Red, Blue, Locomotive]);
    assert_eq!(eng.legal_moves(0).station_cost, 3);
    assert!(matches!(
        build_station(&mut eng, 0, "madrid", &[Red, Blue, Locomotive]),
        Err(EngineError::InvalidPayload { .. })
    ));
    eng.set_player_hand(0, vec![Red, Locomotive, Locomotive]);
    build_station(&mut eng, 0, "madrid", &[Red, Red, Red]).unwrap();
    assert_eq!(eng.stations_remaining(0), 0);

    // Nessuna quarta stazione.
    force_turn(&mut eng, 0);
    eng.set_player_hand(0, vec![Red; 5]);
    let moves = eng.legal_moves(0);
    assert_eq!(moves.station_cost, 0);
    assert!(moves.station_cities.is_empty());
    assert!(matches!(
        build_station(&mut eng, 0, "berlin", &[Red]),
        Err(EngineError::NoStationsLeft)
    ));
    let placements: Vec<(String, usize)> = eng
        .station_placements()
        .into_iter()
        .map(|(c, p)| (c.clone(), p))
        .collect();
    assert_eq!(
        placements,
        [
            ("madrid".into(), 0),
            ("paris".into(), 0),
            ("roma".into(), 0)
        ]
    );
}

#[test]
fn station_moves_listed_by_legal_moves_are_accepted() {
    let mut eng = new_game(2);
    eng.set_player_hand(0, vec![Red, Blue]);
    let moves = eng.legal_moves(0);
    assert_eq!(moves.station_cost, 1);
    for city in &moves.station_cities {
        let mut copy = eng.clone();
        build_station(&mut copy, 0, city, &[Red]).unwrap();
    }
    // Una mano vuota non copre nemmeno la prima stazione.
    eng.set_player_hand(0, Vec::new());
    assert!(eng.legal_moves(0).station_cities.is_empty());
    // Il giocatore che non è di turno non ha mosse.
    assert!(eng.legal_moves(1).station_cities.is_empty());
}

// ── Punteggio con le stazioni ────────────────────────────────────────

/// Reclama `route` come `pi` con carte del colore dato; per i tunnel le tre carte
/// rivelate non corrispondono, quindi la rotta è presa subito.
fn claim_as(eng: &mut Engine, pi: usize, route: &str, color: CardColor) {
    let len = eng.get_route(route).unwrap().len as usize;
    force_turn(eng, pi);
    eng.set_player_hand(pi, vec![color; len]);
    let other = if color == Black { Red } else { Black };
    set_top(eng, &[other, other, other]);
    claim(eng, pi, route, &vec![color; len]).unwrap();
    assert!(eng.pending_tunnel().is_none(), "{route}");
}

fn station_as(eng: &mut Engine, pi: usize, city: &str) {
    force_turn(eng, pi);
    eng.set_player_hand(pi, vec![Red]);
    build_station(eng, pi, city, &[Red]).unwrap();
}

fn completed(eng: &Engine, pi: usize) -> Vec<bool> {
    eng.player(pi)
        .unwrap()
        .tickets
        .values()
        .map(|t| t.completed.unwrap())
        .collect()
}

/// Gioca pescate dal mazzo fino a fine partita (il giocatore 0 innesca l'ultimo giro).
fn play_out(eng: &mut Engine) -> Vec<FinalScore> {
    eng.set_player_trains(0, 2);
    for _ in 0..50 {
        if eng.phase() == GamePhase::GameOver {
            break;
        }
        let events = eng
            .apply(Action::DrawTrainCard {
                player_index: eng.current_player(),
                source: DrawSource::Deck,
            })
            .unwrap();
        for event in events {
            if let GameEvent::GameOver { final_scores, .. } = event {
                return final_scores;
            }
        }
    }
    panic!("la partita non è finita");
}

/// P0 ha Dieppe–Paris, Bruxelles–Paris, Madrid–Pamplona, Barcelona–Pamplona e i biglietti
/// Dieppe–Madrid e Barcelona–Bruxelles; P1 ha Pamplona–Paris e Brest–Paris.
fn paris_scenario(station: bool) -> Engine {
    let mut eng = new_game(2);
    eng.set_player_tickets(0, &["dieppe_madrid", "barcelona_bruxelles"]);
    eng.set_player_tickets(1, &[]);
    claim_as(&mut eng, 0, "dieppe_paris_1", Purple);
    claim_as(&mut eng, 0, "bruxelles_paris_2", Yellow);
    claim_as(&mut eng, 0, "madrid_pamplona_1", Black);
    claim_as(&mut eng, 0, "barcelona_pamplona_1", Orange);
    assert_eq!(completed(&eng, 0), [false, false]);
    if station {
        station_as(&mut eng, 0, "paris");
        assert_eq!(
            completed(&eng, 0),
            [false, false],
            "nessuna rotta avversaria"
        );
    }
    claim_as(&mut eng, 1, "pamplona_paris_1", Blue);
    claim_as(&mut eng, 1, "brest_paris_1", Black);
    eng
}

#[test]
fn paris_station_completes_both_tickets_through_an_opponent_route() {
    let mut eng = paris_scenario(true);
    // L'ultima rotta avversaria è arrivata dopo la stazione: il completamento è live.
    assert_eq!(completed(&eng, 0), [true, true]);

    let scores = play_out(&mut eng);
    let p0 = &scores[0];
    assert_eq!((p0.ticket_completed, p0.ticket_failed), (16, 0));
    assert_eq!(p0.station_points, 8, "due stazioni non costruite");
    assert_eq!(
        p0.borrowed_routes,
        [("paris".to_string(), "pamplona_paris_1".to_string())]
    );
    // Rotte 1+2+4+2 punti; la rotta in prestito non allunga il percorso più lungo di
    // P0 (5 contro i 7 di P1), quindi il bonus va a P1.
    assert_eq!(p0.route_points, 9);
    assert_eq!(p0.longest_path_bonus, 0);
    assert_eq!(p0.total, 9 + 16 + 8);
    let p1 = &scores[1];
    assert_eq!(p1.longest_path_bonus, 10);
    assert_eq!(p1.station_points, 12);
    assert!(p1.borrowed_routes.is_empty());
    assert_eq!(p1.total, 11 + 12 + 10);
}

#[test]
fn without_a_station_the_same_tickets_fail_and_all_stations_score_four() {
    let mut eng = paris_scenario(false);
    assert_eq!(completed(&eng, 0), [false, false]);
    let scores = play_out(&mut eng);
    let p0 = &scores[0];
    assert_eq!((p0.ticket_completed, p0.ticket_failed), (0, -16));
    assert_eq!(p0.station_points, 12);
    assert!(p0.borrowed_routes.is_empty());
    assert_eq!(p0.total, 9 - 16 + 12);
}

#[test]
fn station_in_essen_needs_amsterdam_essen_to_be_claimed() {
    let mut eng = new_game(2);
    eng.set_player_tickets(0, &["amsterdam_pamplona"]);
    claim_as(&mut eng, 0, "pamplona_paris_1", Blue);
    claim_as(&mut eng, 0, "frankfurt_paris_1", Orange);
    claim_as(&mut eng, 0, "essen_frankfurt_1", Green);
    station_as(&mut eng, 0, "essen");
    assert_eq!(completed(&eng, 0), [false]);

    // Amsterdam–Essen ancora libera: la stazione non serve a nulla.
    let scores = play_out(&mut eng.clone());
    assert_eq!(scores[0].ticket_failed, -7);
    assert!(scores[0].borrowed_routes.is_empty());

    // L'avversario la reclama: il biglietto di P0 si completa subito.
    claim_as(&mut eng, 1, "amsterdam_essen_1", Yellow);
    assert_eq!(completed(&eng, 0), [true]);
    let scores = play_out(&mut eng);
    assert_eq!(scores[0].ticket_completed, 7);
    assert_eq!(
        scores[0].borrowed_routes,
        [("essen".to_string(), "amsterdam_essen_1".to_string())]
    );
}

#[test]
fn no_route_is_borrowed_when_the_ticket_is_complete_without_one() {
    let mut eng = new_game(2);
    eng.set_player_tickets(0, &["amsterdam_pamplona"]);
    claim_as(&mut eng, 0, "amsterdam_essen_1", Yellow);
    claim_as(&mut eng, 0, "essen_frankfurt_1", Green);
    claim_as(&mut eng, 0, "frankfurt_paris_1", Orange);
    claim_as(&mut eng, 0, "pamplona_paris_1", Blue);
    claim_as(&mut eng, 1, "brest_paris_1", Black);
    station_as(&mut eng, 0, "paris");
    let scores = play_out(&mut eng);
    assert_eq!(scores[0].ticket_completed, 7);
    assert!(
        scores[0].borrowed_routes.is_empty(),
        "prima 'nessuna rotta'"
    );
}

// ── Setup e biglietti ────────────────────────────────────────────────

#[test]
fn europe_setup_deals_one_long_and_three_regular_tickets() {
    let (ruleset, map) = europe();
    let long_ids: Vec<String> = map
        .tickets_in_play(&ruleset)
        .filter(|t| t.deck == "long")
        .map(|t| t.id.clone())
        .collect();
    let regular = map
        .tickets_in_play(&ruleset)
        .filter(|t| t.deck == "regular")
        .count();
    let mut eng = Engine::new(
        11,
        ruleset,
        map,
        (0..3).map(|i| format!("p{i}")).collect(),
        (0..3).map(|i| format!("P{i}")).collect(),
        default_player_colors(3),
        0,
    )
    .unwrap();
    for pi in 0..3 {
        let offered = eng.initial_tickets_for(pi).unwrap().to_vec();
        assert_eq!(offered.len(), 4);
        assert_eq!(offered.iter().filter(|t| long_ids.contains(t)).count(), 1);
        // Si tengono due biglietti: gli scarti di setup sono rimossi dalla partita.
        eng.apply(Action::ChooseTickets {
            player_index: pi,
            keep: offered[..2].to_vec(),
        })
        .unwrap();
    }
    assert_eq!(eng.ticket_pile().len(), regular - 9);
    assert!(eng.ticket_pile().iter().all(|t| !long_ids.contains(t)));

    // A metà partita si pescano solo biglietti regolari.
    eng.apply(Action::DrawTickets { player_index: 0 }).unwrap();
    assert_eq!(eng.pending_tickets().len(), 3);
    assert!(eng.pending_tickets().iter().all(|t| !long_ids.contains(t)));
}

// ── Validazione dei dati Europe ──────────────────────────────────────

/// Esito di `Ruleset::validate` sui dati Europe reali dopo la mutazione.
fn validate_mutated(
    mutate: impl FnOnce(&mut Ruleset, &mut MapData),
) -> Result<(), EngineInitError> {
    let (mut ruleset, mut map) = europe();
    mutate(&mut ruleset, &mut map);
    ruleset.validate(&map)
}

fn first_route_index(map: &MapData, pred: impl Fn(&Route) -> bool) -> usize {
    map.routes.iter().position(pred).unwrap()
}

#[test]
fn validate_accepts_shipped_europe_data() {
    let (ruleset, map) = europe();
    ruleset.validate(&map).unwrap();
}

#[test]
fn validate_rejects_ferries_beyond_route_length() {
    let mut expected = None;
    let err = validate_mutated(|_, map| {
        let i = first_route_index(map, |r| r.ferries > 0);
        let route = &mut map.routes[i];
        route.ferries = route.len + 1;
        expected = Some(EngineInitError::RouteFerriesExceedLength {
            route_id: route.id.clone(),
            ferries: route.len + 1,
            len: route.len,
        });
    });
    assert_eq!(err, Err(expected.unwrap()));
}

#[test]
fn validate_rejects_route_that_is_tunnel_and_ferry() {
    let mut expected = None;
    let err = validate_mutated(|_, map| {
        let i = first_route_index(map, |r| r.ferries > 0);
        map.routes[i].tunnel = true;
        expected = Some(EngineInitError::RouteTunnelAndFerry {
            route_id: map.routes[i].id.clone(),
        });
    });
    assert_eq!(err, Err(expected.unwrap()));
}

#[test]
fn validate_rejects_tunnel_routes_without_tunnels_config() {
    let first_tunnel = |map: &MapData| map.routes.iter().find(|r| r.tunnel).unwrap().id.clone();
    let (_, map) = europe();
    let expected = Err(EngineInitError::TunnelsConfigMissing {
        route_id: first_tunnel(&map),
    });
    assert_eq!(validate_mutated(|rs, _| rs.tunnels = None), expected);
    assert_eq!(
        validate_mutated(|rs, _| rs.tunnels.as_mut().unwrap().revealed = 0),
        expected
    );
}

#[test]
fn validate_rejects_station_costs_not_matching_count() {
    assert_eq!(
        validate_mutated(|rs, _| {
            rs.stations.as_mut().unwrap().costs.pop();
        }),
        Err(EngineInitError::StationCostsMismatch { count: 3, costs: 2 })
    );
}

#[test]
fn validate_rejects_a_zero_station_cost() {
    assert_eq!(
        validate_mutated(|rs, _| rs.stations.as_mut().unwrap().costs[1] = 0),
        Err(EngineInitError::StationCostZero { index: 1 })
    );
}
