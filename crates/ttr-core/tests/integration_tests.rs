//! Test di integrazione per il motore CiufCiuf! (ttr-core).
//!
//! Copre la checklist del piano ciufciuf-v1.3.md §3.3, replay e fuzzing.

use std::collections::BTreeMap;

use ttr_core::error::GamePhase;
use ttr_core::types::{
    Bonus, City, ClaimedRouteInfo, DeckConfig, InitialTickets, MapData, PlayersConfig, Route,
    Ruleset, SetupDiscards, TicketDealtEntry, TicketDef, TicketDraw,
};
use ttr_core::{Action, DrawSource, Engine, EngineError, EngineInitError, GameEvent};
use ttr_proto::CardColor;

// ============================================================
// Helpers
// ============================================================

fn route(id: &str, from: &str, to: &str, len: u32, color: &str, double: bool) -> Route {
    Route {
        id: id.into(),
        from: from.into(),
        to: to.into(),
        len,
        color: color.into(),
        double,
        always_open: false,
        tunnel: false,
        ferries: 0,
    }
}

fn ticket(id: &str, from: &str, to: &str, points: u32) -> TicketDef {
    TicketDef {
        id: id.into(),
        from: from.into(),
        to: to.into(),
        points,
        deck: "regular".into(),
        sets: vec!["mini".into()],
    }
}

fn city(id: &str) -> City {
    City {
        id: id.into(),
        name: id.into(),
        country: false,
    }
}

fn draw_tickets(player_index: usize) -> Action {
    Action::DrawTickets {
        player_index,
        counts: BTreeMap::new(),
    }
}

fn mini_map() -> MapData {
    let mut tickets = vec![
        ticket("A_D", "A", "D", 10),
        ticket("A_F", "A", "F", 15),
        ticket("B_D", "B", "D", 8),
        ticket("E_F", "E", "F", 12),
    ];
    for i in 0..16 {
        tickets.push(ticket(&format!("dummy_{}", i), "A", "B", 1));
    }
    MapData {
        cities: vec![
            city("A"),
            city("B"),
            city("C"),
            city("D"),
            city("E"),
            city("F"),
        ],
        routes: vec![
            route("A_B_1", "A", "B", 1, "Gray", false),
            route("B_C_1", "B", "C", 2, "Gray", false),
            route("C_D_1", "C", "D", 3, "Gray", false),
            route("D_E_1", "D", "E", 1, "Red", false),
            route("A_E_1", "A", "E", 2, "Gray", false),
            route("A_F_1", "A", "F", 6, "Gray", false),
        ],
        tickets,
    }
}

fn double_route_map() -> MapData {
    let mut tickets = vec![ticket("A_C", "A", "C", 5)];
    for i in 0..16 {
        tickets.push(ticket(&format!("dummy_{}", i), "A", "B", 1));
    }
    MapData {
        cities: vec![city("A"), city("B"), city("C")],
        routes: vec![
            route("A_B_1", "A", "B", 2, "Gray", true),
            route("A_B_2", "A", "B", 2, "Gray", true),
            route("B_C_1", "B", "C", 1, "Gray", false),
        ],
        tickets,
    }
}

fn mini_ruleset() -> Ruleset {
    let mut rp = BTreeMap::new();
    rp.insert("1".into(), 1);
    rp.insert("2".into(), 2);
    rp.insert("3".into(), 4);
    rp.insert("4".into(), 7);
    rp.insert("5".into(), 10);
    rp.insert("6".into(), 15);
    Ruleset {
        tie_breakers: None,
        passengers: None,
        country_cards: BTreeMap::new(),
        name: "Mini".into(),
        mode: "Standard".into(),
        mode_order: 0,
        ticket_set: "mini".into(),
        map: "mini".into(),
        players: PlayersConfig { min: 2, max: 5 },
        trains_per_player: 45,
        deck: DeckConfig {
            per_color: 12,
            locomotives: 14,
        },
        ticket_decks: vec!["regular".into()],
        initial_tickets: InitialTickets {
            choose_from: None,
            count: 0,
            dealt: vec![TicketDealtEntry {
                deck: "regular".into(),
                count: 4,
                keep_max: None,
            }],
            keep_min: 2,
        },
        ticket_draw: TicketDraw {
            choose_from: None,
            deck: "regular".into(),
            count: 3,
            keep_min: 1,
        },
        setup_discards: SetupDiscards::ReshuffleBottom,
        route_points: rp,
        bonuses: vec![Bonus {
            bonus_type: "longest_path".into(),
            label: "Longest Path".into(),
            points: 10,
        }],
        big_cities: vec![],
        double_route_min_players: 4,
        final_round_includes_trigger: true,
        stations: None,
        tunnels: None,
    }
}

fn strict_double_ruleset() -> Ruleset {
    let mut rs = mini_ruleset();
    rs.double_route_min_players = 4;
    rs
}
fn free_double_ruleset() -> Ruleset {
    let mut rs = mini_ruleset();
    rs.double_route_min_players = 2;
    rs
}

fn two_players() -> (Vec<String>, Vec<String>, Vec<ttr_proto::PlayerColor>) {
    (
        vec!["p1".into(), "p2".into()],
        vec!["One".into(), "Two".into()],
        vec![ttr_proto::PlayerColor::Red, ttr_proto::PlayerColor::Blue],
    )
}

fn three_players() -> (Vec<String>, Vec<String>, Vec<ttr_proto::PlayerColor>) {
    let ids: Vec<String> = (0..3).map(|i| format!("p{}", i + 1)).collect();
    let names: Vec<String> = (0..3).map(|i| format!("P{}", i + 1)).collect();
    (
        ids,
        names,
        vec![
            ttr_proto::PlayerColor::Red,
            ttr_proto::PlayerColor::Blue,
            ttr_proto::PlayerColor::Green,
        ],
    )
}

fn four_players() -> (Vec<String>, Vec<String>, Vec<ttr_proto::PlayerColor>) {
    let ids: Vec<String> = (0..4).map(|i| format!("p{}", i + 1)).collect();
    let names: Vec<String> = (0..4).map(|i| format!("P{}", i + 1)).collect();
    (
        ids,
        names,
        vec![
            ttr_proto::PlayerColor::Red,
            ttr_proto::PlayerColor::Blue,
            ttr_proto::PlayerColor::Green,
            ttr_proto::PlayerColor::Yellow,
        ],
    )
}

fn complete_initial_draft(engine: &mut Engine) {
    let n = engine.num_players();
    for pi in 0..n {
        let tkts = engine.initial_tickets_for(pi).unwrap().to_vec();
        let keep: Vec<String> = tkts.iter().take(2).cloned().collect();
        engine
            .apply(Action::ChooseTickets {
                player_index: pi,
                keep,
            })
            .unwrap();
    }
    assert_eq!(engine.phase(), GamePhase::PlayerTurn);
}

fn usa_map() -> MapData {
    let json = std::fs::read_to_string("../../data/maps/usa.json").unwrap();
    serde_json::from_str(&json).unwrap()
}
fn usa_ruleset() -> Ruleset {
    let json = std::fs::read_to_string("../../data/rulesets/usa_v2.json").unwrap();
    serde_json::from_str(&json).unwrap()
}

// ============================================================
// Sezione 1 — Golden Tests
// ============================================================

#[test]
fn golden_loco_face_up_is_single_draw() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(42, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    eng.set_face_up_card(0, Some(CardColor::Locomotive));

    let events = eng
        .apply(Action::DrawTrainCard {
            player_index: 0,
            source: DrawSource::FaceUp { index: 0 },
        })
        .unwrap();

    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::CardDrawnFromFaceUp { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DrawTurnComplete { .. }))
    );
    assert_eq!(eng.current_player(), 1);
    assert_ne!(eng.phase(), GamePhase::SecondCardDraw);
}

#[test]
fn golden_loco_not_second_card() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(43, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    eng.set_face_up_card(0, Some(CardColor::Locomotive));

    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    })
    .unwrap();
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);

    let r = eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::FaceUp { index: 0 },
    });
    assert!(matches!(r, Err(EngineError::LocomotiveAsSecondCard)));
}

#[test]
fn golden_loco_threshold_resets_pool() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(44, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    eng.set_face_up_card(0, Some(CardColor::Locomotive));
    eng.set_face_up_card(1, Some(CardColor::Locomotive));
    eng.set_face_up_card(2, Some(CardColor::Locomotive));
    eng.set_face_up_card(3, Some(CardColor::Red));

    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::FaceUp { index: 3 },
    })
    .unwrap();

    let loco_count = eng
        .face_up_cards()
        .iter()
        .filter(|c| matches!(c, Some(CardColor::Locomotive)))
        .count();
    assert!(
        loco_count < 3,
        "dopo reset <3 locomotive, trovate {}",
        loco_count
    );
}

#[test]
fn golden_ticket_discards_bottom_in_order() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(47, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    eng.apply(draw_tickets(0)).unwrap();

    let offered = eng.pending_tickets().to_vec();
    assert!(
        offered.len() >= 3,
        "servono >=3 ticket offerti, ne ho {}",
        offered.len()
    );
    let keep = vec![offered[0].clone()];
    let discarded: Vec<String> = offered.iter().skip(1).cloned().collect();

    eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep,
    })
    .unwrap();

    let pile = eng.ticket_pile();
    let n = discarded.len();
    assert!(pile.len() >= n);
    let bottom: Vec<String> = pile[pile.len() - n..].to_vec();
    assert_eq!(bottom, discarded, "scarti in ordine in fondo al mazzo");
}

#[test]
fn golden_double_route_blocked_2p() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(
        48,
        strict_double_ruleset(),
        double_route_map(),
        ids,
        names,
        colors,
        0,
    )
    .unwrap();
    complete_initial_draft(&mut eng);

    eng.set_player_hand(0, vec![CardColor::Red, CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red, CardColor::Red],
        passengers: Default::default(),
    })
    .unwrap();

    eng.set_player_hand(1, vec![CardColor::Blue, CardColor::Blue]);
    let r = eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "A_B_2".into(),
        cards_discarded: vec![CardColor::Blue, CardColor::Blue],
        passengers: Default::default(),
    });
    assert!(matches!(r, Err(EngineError::RouteBlockedDouble { .. })));
}

#[test]
fn golden_double_route_free_4p() {
    let (ids, names, colors) = four_players();
    let mut eng = Engine::new(
        49,
        free_double_ruleset(),
        double_route_map(),
        ids,
        names,
        colors,
        0,
    )
    .unwrap();
    complete_initial_draft(&mut eng);

    eng.set_player_hand(0, vec![CardColor::Red, CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red, CardColor::Red],
        passengers: Default::default(),
    })
    .unwrap();

    eng.set_player_hand(1, vec![CardColor::Blue, CardColor::Blue]);
    let r = eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "A_B_2".into(),
        cards_discarded: vec![CardColor::Blue, CardColor::Blue],
        passengers: Default::default(),
    });
    assert!(r.is_ok(), "doppia rotta libera a 4p: {:?}", r.err());
}

#[test]
fn golden_same_player_cannot_claim_both_doubles() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(
        50,
        free_double_ruleset(),
        double_route_map(),
        ids,
        names,
        colors,
        0,
    )
    .unwrap();
    complete_initial_draft(&mut eng);

    eng.set_player_hand(0, vec![CardColor::Red, CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red, CardColor::Red],
        passengers: Default::default(),
    })
    .unwrap();

    eng.set_player_hand(1, vec![CardColor::Blue]);
    eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "B_C_1".into(),
        cards_discarded: vec![CardColor::Blue],
        passengers: Default::default(),
    })
    .unwrap();

    eng.set_player_hand(0, vec![CardColor::Green, CardColor::Green]);
    let r = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_2".into(),
        cards_discarded: vec![CardColor::Green, CardColor::Green],
        passengers: Default::default(),
    });
    assert!(matches!(r, Err(EngineError::SamePlayerDouble { .. })));
}

#[test]
fn golden_last_round_includes_trigger() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(51, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    eng.set_player_trains(0, 3);
    eng.set_player_hand(0, vec![CardColor::Red]);
    let events = eng
        .apply(Action::ClaimRoute {
            player_index: 0,
            route_id: "A_B_1".into(),
            cards_discarded: vec![CardColor::Red],
            passengers: Default::default(),
        })
        .unwrap();

    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::LastRoundAnnounced { .. }))
    );
    assert_eq!(eng.last_round_trigger_player(), Some(0));
    assert!(eng.last_round());
    assert_eq!(eng.current_player(), 1);
    assert_eq!(eng.phase(), GamePhase::LastRound);

    // P1 ultimo turno: due pescate dal mazzo
    eng.apply(Action::DrawTrainCard {
        player_index: 1,
        source: DrawSource::Deck,
    })
    .unwrap();
    if eng.phase() == GamePhase::SecondCardDraw {
        eng.apply(Action::DrawTrainCard {
            player_index: 1,
            source: DrawSource::Deck,
        })
        .unwrap();
    }
    assert_eq!(eng.current_player(), 0, "dopo P1 tocca a P0 (trigger)");

    // P0 ultimo turno → fine partita
    let ev = eng
        .apply(Action::DrawTrainCard {
            player_index: 0,
            source: DrawSource::Deck,
        })
        .unwrap();
    if eng.phase() == GamePhase::SecondCardDraw {
        let ev2 = eng
            .apply(Action::DrawTrainCard {
                player_index: 0,
                source: DrawSource::Deck,
            })
            .unwrap();
        assert!(ev2.iter().any(|e| matches!(e, GameEvent::GameOver { .. })));
    } else {
        assert!(ev.iter().any(|e| matches!(e, GameEvent::GameOver { .. })));
    }
    assert_eq!(eng.phase(), GamePhase::GameOver);
}

/// Edge case: il trigger dell'ultimo giro è l'ultimo giocatore nell'ordine.
///
/// Sequenza attesa (piano §6):
/// 1. P1 (ultimo) claim con ≤2 treni → LastRoundAnnounced(trigger=1)
/// 2. Turno finale di P0 → TurnAdvanced
/// 3. Turno finale di P1 → GameOver
///
/// Nessun giro extra.
#[test]
fn golden_last_round_trigger_is_last_player() {
    let (ids, names, colors) = two_players();
    // first_player = 0, quindi l'ordine è P0 → P1
    let mut eng = Engine::new(171, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    assert_eq!(eng.current_player(), 0, "P0 inizia");

    // P0 fa un'azione normale (non trigger)
    eng.set_player_hand(0, vec![CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red],
        passengers: Default::default(),
    })
    .unwrap();
    assert_eq!(eng.current_player(), 1, "dopo P0 tocca a P1");

    // P1 (ultimo dell'ordine) claim: porta i treni a ≤2 → trigger
    eng.set_player_trains(1, 4);
    eng.set_player_hand(1, vec![CardColor::Blue, CardColor::Blue]);
    let events = eng
        .apply(Action::ClaimRoute {
            player_index: 1,
            route_id: "B_C_1".into(),
            cards_discarded: vec![CardColor::Blue, CardColor::Blue],
            passengers: Default::default(),
        })
        .unwrap();

    // Verify: LastRoundAnnounced con trigger=P1
    let last_round_ev = events
        .iter()
        .find(|e| matches!(e, GameEvent::LastRoundAnnounced { .. }));
    assert!(
        last_round_ev.is_some(),
        "P1 (ultimo) deve innescare LastRoundAnnounced"
    );
    assert_eq!(eng.last_round_trigger_player(), Some(1));
    assert!(eng.last_round());
    assert_eq!(eng.current_player(), 0, "dopo trigger di P1, tocca a P0");
    assert_eq!(eng.phase(), GamePhase::LastRound);

    // P0 fa il suo ultimo turno
    let ev0 = eng
        .apply(Action::DrawTrainCard {
            player_index: 0,
            source: DrawSource::Deck,
        })
        .unwrap();
    if eng.phase() == GamePhase::SecondCardDraw {
        // Seconda pescata completa il turno
        let ev0b = eng
            .apply(Action::DrawTrainCard {
                player_index: 0,
                source: DrawSource::Deck,
            })
            .unwrap();
        assert!(
            ev0b.iter()
                .any(|e| matches!(e, GameEvent::TurnAdvanced { .. })),
            "seconda pescata di P0 deve avanzare il turno"
        );
    } else {
        assert!(
            ev0.iter()
                .any(|e| matches!(e, GameEvent::TurnAdvanced { .. })),
            "turno di P0 deve avanzare"
        );
    }
    assert_eq!(
        eng.current_player(),
        1,
        "dopo turno finale di P0, tocca a P1 (trigger)"
    );
    assert_eq!(eng.phase(), GamePhase::LastRound, "ancora LastRound");

    // P1 fa il SUO ultimo turno → GameOver
    let ev1 = eng
        .apply(Action::DrawTrainCard {
            player_index: 1,
            source: DrawSource::Deck,
        })
        .unwrap();
    if eng.phase() == GamePhase::SecondCardDraw {
        let ev2 = eng
            .apply(Action::DrawTrainCard {
                player_index: 1,
                source: DrawSource::Deck,
            })
            .unwrap();
        assert!(
            ev2.iter().any(|e| matches!(e, GameEvent::GameOver { .. })),
            "dopo il turno finale di P1 (trigger), la partita deve finire"
        );
    } else {
        assert!(
            ev1.iter().any(|e| matches!(e, GameEvent::GameOver { .. })),
            "dopo il turno finale di P1 (trigger), la partita deve finire"
        );
    }
    assert_eq!(
        eng.phase(),
        GamePhase::GameOver,
        "nessun giro extra dopo il turno finale del trigger"
    );
}

#[test]
fn golden_route_point_table() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(52, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    eng.set_player_hand(0, vec![CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red],
        passengers: Default::default(),
    })
    .unwrap();
    assert_eq!(eng.player(0).unwrap().score, 1);

    eng.set_player_hand(1, vec![CardColor::Blue, CardColor::Blue]);
    eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "B_C_1".into(),
        cards_discarded: vec![CardColor::Blue, CardColor::Blue],
        passengers: Default::default(),
    })
    .unwrap();
    assert_eq!(eng.player(1).unwrap().score, 2);

    eng.set_player_hand(0, vec![CardColor::Green, CardColor::Green]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_E_1".into(),
        cards_discarded: vec![CardColor::Green, CardColor::Green],
        passengers: Default::default(),
    })
    .unwrap();
    assert_eq!(eng.player(0).unwrap().score, 3);

    eng.set_player_hand(
        1,
        vec![CardColor::Yellow, CardColor::Yellow, CardColor::Yellow],
    );
    eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "C_D_1".into(),
        cards_discarded: vec![CardColor::Yellow, CardColor::Yellow, CardColor::Yellow],
        passengers: Default::default(),
    })
    .unwrap();
    assert_eq!(eng.player(1).unwrap().score, 6);

    eng.set_player_hand(0, vec![CardColor::Red; 6]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_F_1".into(),
        cards_discarded: vec![CardColor::Red; 6],
        passengers: Default::default(),
    })
    .unwrap();
    assert_eq!(eng.player(0).unwrap().score, 18);
}

// ============================================================
// Sezione 2 — Error handling
// ============================================================

#[test]
fn golden_cannot_draw_tickets_in_second_card_draw() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(53, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    })
    .unwrap();
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);

    let r = eng.apply(draw_tickets(0));
    assert!(r.is_err());
}

#[test]
fn golden_not_your_turn_error() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(54, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    let r = eng.apply(Action::DrawTrainCard {
        player_index: 1,
        source: DrawSource::Deck,
    });
    assert!(matches!(r, Err(EngineError::NotYourTurn { .. })));
}

#[test]
fn golden_route_not_found_error() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(55, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    let r = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "nonexistent".into(),
        cards_discarded: vec![CardColor::Red],
        passengers: Default::default(),
    });
    assert!(matches!(r, Err(EngineError::RouteNotFound { .. })));
}

#[test]
fn golden_insufficient_cards_for_route() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(56, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    let r = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![],
        passengers: Default::default(),
    });
    assert!(matches!(r, Err(EngineError::InsufficientCards { .. })));
}

#[test]
fn golden_wrong_color_for_route() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(57, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    eng.set_player_hand(0, vec![CardColor::Green]);

    // D_E_1 è Rossa, pago con Verde
    let r = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "D_E_1".into(),
        cards_discarded: vec![CardColor::Green],
        passengers: Default::default(),
    });
    assert!(r.is_err());
}

/// Il vincitore è calcolato dal motore: totale più alto, non chi ha il
/// percorso più lungo; esattamente i migliori sono marcati `winner`.
#[test]
fn golden_game_over_marks_winner_by_total() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(57, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    // Ultimo giro innescato dal giocatore 0, poi tutti pescano fino alla fine.
    eng.set_player_trains(0, 3);
    eng.set_player_hand(0, vec![CardColor::Red]);
    let mut events = eng
        .apply(Action::ClaimRoute {
            player_index: 0,
            route_id: "A_B_1".into(),
            cards_discarded: vec![CardColor::Red],
            passengers: Default::default(),
        })
        .unwrap();
    while eng.phase() != GamePhase::GameOver {
        events.extend(
            eng.apply(Action::DrawTrainCard {
                player_index: eng.current_player(),
                source: DrawSource::Deck,
            })
            .unwrap(),
        );
    }
    let final_scores = events
        .iter()
        .find_map(|e| match e {
            GameEvent::GameOver { final_scores, .. } => Some(final_scores.clone()),
            _ => None,
        })
        .expect("evento GameOver");
    let best = final_scores.iter().map(|fs| fs.total).max().unwrap();
    assert!(final_scores.iter().any(|fs| fs.winner));
    for fs in &final_scores {
        if fs.winner {
            assert_eq!(fs.total, best, "un vincitore ha il totale massimo");
        } else {
            assert!(fs.total <= best);
        }
    }
}

#[test]
fn golden_initial_draft_must_keep_minimum() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(58, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();

    let r = eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep: vec!["A_D".into()],
    });
    assert!(matches!(r, Err(EngineError::TicketChoiceBelowMin { .. })));
}

#[test]
fn golden_first_player_is_respected() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(59, mini_ruleset(), mini_map(), ids, names, colors, 1).unwrap();
    complete_initial_draft(&mut eng);
    assert_eq!(eng.current_player(), 1);
}

#[test]
fn golden_initial_draft_parallel() {
    let (ids, names, colors) = three_players();
    let mut eng = Engine::new(60, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();

    for pi in 0..3 {
        let tickets = eng.initial_tickets_for(pi).unwrap().to_vec();
        let keep = tickets.iter().take(2).cloned().collect::<Vec<_>>();
        eng.apply(Action::ChooseTickets {
            player_index: pi,
            keep,
        })
        .unwrap();
    }
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
}

/// Dopo un claim che connette alcune città, un `DrawTickets` + `ChooseTickets`
/// mid-game deve marcare ogni biglietto tenuto con `completed: Some(_)`, mai
/// `None`.
#[test]
fn golden_mid_game_ticket_completed_not_none() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(64, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    // Dai a P0 carte sufficienti per un claim: A_B_1 (len=1, Gray).
    eng.set_player_hand(0, vec![CardColor::Red, CardColor::Red]);

    // Claim A_B_1 → i biglietti dummy A→B saranno completati se pescati.
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red],
        passengers: Default::default(),
    })
    .unwrap();

    // Resetta treni per evitare last round.
    eng.set_player_trains(0, 40);

    // Dopo il claim di P0 il turno passa a P1.
    // Fai pescare biglietti a P1 per avanzare il turno e tornare a P0.
    eng.apply(draw_tickets(1)).unwrap();
    let p1_offered = eng.pending_tickets().to_vec();
    let p1_keep = vec![p1_offered[0].clone()];
    eng.apply(Action::ChooseTickets {
        player_index: 1,
        keep: p1_keep,
    })
    .unwrap();

    // DrawTickets mid-game (turno di P0).
    eng.apply(draw_tickets(0)).unwrap();
    assert_eq!(eng.phase(), GamePhase::TicketChoice);

    let pending = eng.pending_tickets().to_vec();
    assert!(!pending.is_empty(), "ci devono essere biglietti in pending");

    // Scegli tutti i biglietti offerti per testarli tutti.
    let keep = pending.clone();
    eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep: keep.clone(),
    })
    .unwrap();

    // Ogni biglietto tenuto ha completed.is_some()
    // (Some(true) se completato, Some(false) altrimenti — mai None).
    let player = eng.player(0).unwrap();
    for tid in &keep {
        let ts = player
            .tickets
            .get(tid)
            .unwrap_or_else(|| panic!("biglietto {} non trovato nel giocatore", tid));
        assert!(
            ts.completed.is_some(),
            "biglietto '{}' ha completed = {:?}, atteso Some(_)",
            tid,
            ts.completed
        );
    }

    // Se tra i biglietti tenuti c'è un dummy A→B, deve essere Some(true).
    let dummy_completed = keep
        .iter()
        .filter(|tid| tid.starts_with("dummy_"))
        .any(|tid| player.tickets[tid].completed == Some(true));
    assert!(
        dummy_completed,
        "almeno un dummy A→B deve risultare completed dopo ChooseTickets (A_B_1 claimato)"
    );
}

// ============================================================
// Sezione 2.5 — legalMoves gate + §3.3 test gap closures
// ============================================================

/// legalMoves in SecondCardDraw: niente rotte, niente ticket draw.
#[test]
fn golden_legal_moves_second_card_draw() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(61, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    })
    .unwrap();
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);

    let lm = eng.legal_moves(0);
    assert!(
        lm.routes.is_empty(),
        "no claimable routes in SecondCardDraw"
    );
    assert!(
        !lm.can_draw_tickets,
        "cannot draw tickets in SecondCardDraw"
    );
    // can_draw_deck deve essere ancora vero
    assert!(lm.can_draw_deck, "can still draw from deck");
}

/// legalMoves in TicketChoice: tutto vuoto/false.
#[test]
fn golden_legal_moves_ticket_choice_empty() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(62, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    eng.apply(draw_tickets(0)).unwrap();
    assert_eq!(eng.phase(), GamePhase::TicketChoice);

    let lm = eng.legal_moves(0);
    assert!(lm.routes.is_empty(), "no routes in TicketChoice");
    assert!(lm.draw_face_up.is_empty(), "no face-up in TicketChoice");
    assert!(!lm.can_draw_deck, "no deck draw in TicketChoice");
    assert!(!lm.can_draw_tickets, "no ticket draw in TicketChoice");
}

fn has_no_moves(lm: &ttr_core::LegalMoves) -> bool {
    lm.routes.is_empty() && lm.draw_face_up.is_empty() && !lm.can_draw_deck && !lm.can_draw_tickets
}

/// legalMoves solo a chi è di turno: chi aspetta non vede mosse e il motore
/// continua a rifiutarle con `NotYourTurn`.
#[test]
fn golden_legal_moves_only_for_current_player() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(63, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    assert_eq!(eng.current_player(), 0);

    let current = eng.legal_moves(0);
    assert!(current.can_draw_deck, "chi è di turno può pescare");
    assert!(
        !current.draw_face_up.is_empty(),
        "chi è di turno vede le scoperte"
    );
    assert!(has_no_moves(&eng.legal_moves(1)), "PlayerTurn: chi aspetta");

    let claim = eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "A_B_1".into(),
        cards_discarded: eng.player(1).unwrap().hand[..1].to_vec(),
        passengers: Default::default(),
    });
    assert!(matches!(claim, Err(EngineError::NotYourTurn { .. })));
    let draw = eng.apply(Action::DrawTrainCard {
        player_index: 1,
        source: DrawSource::Deck,
    });
    assert!(matches!(draw, Err(EngineError::NotYourTurn { .. })));

    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    })
    .unwrap();
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);

    assert!(
        eng.legal_moves(0).can_draw_deck,
        "seconda pescata di chi è di turno"
    );
    assert!(
        has_no_moves(&eng.legal_moves(1)),
        "SecondCardDraw: chi aspetta"
    );
    let draw = eng.apply(Action::DrawTrainCard {
        player_index: 1,
        source: DrawSource::Deck,
    });
    assert!(matches!(draw, Err(EngineError::NotYourTurn { .. })));

    // A turno finito le mosse passano all'altro giocatore.
    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    })
    .unwrap();
    assert_eq!(eng.current_player(), 1);
    assert!(
        has_no_moves(&eng.legal_moves(0)),
        "turno passato: chi ha giocato"
    );
    assert!(
        eng.legal_moves(1).can_draw_deck,
        "turno passato: nuovo di turno"
    );
}

/// D27: con mazzo e scarti esauriti si può ancora reclamare e pescare
/// biglietti, e la pescata dal mazzo riesce con un mazzo nuovo.
#[test]
fn golden_total_deck_exhaustion_no_turn_loss() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(63, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    // Svuota mazzo E scarti
    eng.clear_deck_and_discards();

    // Il giocatore può comunque reclamare una rotta
    eng.set_player_hand(0, vec![CardColor::Red]);
    let ev = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red],
        passengers: Default::default(),
    });
    assert!(
        ev.is_ok(),
        "può ancora reclamare dopo esaurimento deck: {:?}",
        ev.err()
    );

    // Può anche pescare biglietti
    eng.set_player_trains(1, 45); // reset trains so no last round
    let ev2 = eng.apply(draw_tickets(1));
    assert!(
        ev2.is_ok(),
        "può pescare biglietti dopo esaurimento deck: {:?}",
        ev2.err()
    );
    let keep = vec![eng.pending_tickets()[0].clone()];
    eng.apply(Action::ChooseTickets {
        player_index: 1,
        keep,
    })
    .unwrap();

    // La pescata dal mazzo riesce con un mazzo nuovo
    eng.clear_deck_and_discards();
    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    })
    .unwrap();
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);
    assert_eq!(eng.draw_pile_len(), 109);
    assert_eq!(eng.discards_len(), 0);
}

/// R4.2: Rotta doppia bloccata a 3 giocatori.
#[test]
fn golden_double_route_blocked_3p() {
    let (ids, names, colors) = three_players();
    let mut eng = Engine::new(
        64,
        strict_double_ruleset(),
        double_route_map(),
        ids,
        names,
        colors,
        0,
    )
    .unwrap();
    complete_initial_draft(&mut eng);

    // P0 claim A_B_1
    eng.set_player_hand(0, vec![CardColor::Red, CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red, CardColor::Red],
        passengers: Default::default(),
    })
    .unwrap();

    // P1 claim B_C_1
    eng.set_player_hand(1, vec![CardColor::Blue]);
    eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "B_C_1".into(),
        cards_discarded: vec![CardColor::Blue],
        passengers: Default::default(),
    })
    .unwrap();

    // P2 prova A_B_2 → deve fallire (blocco a 3p)
    eng.set_player_hand(2, vec![CardColor::Green, CardColor::Green]);
    let r = eng.apply(Action::ClaimRoute {
        player_index: 2,
        route_id: "A_B_2".into(),
        cards_discarded: vec![CardColor::Green, CardColor::Green],
        passengers: Default::default(),
    });
    assert!(
        matches!(r, Err(EngineError::RouteBlockedDouble { .. })),
        "doppia bloccata a 3p: {:?}",
        r.err()
    );
}

/// R4.3: Scarti biglietti di setup in fondo al mazzo dopo initial draft
/// (regola `reshuffleBottom`).
#[test]
fn golden_setup_ticket_discards_bottom() {
    let (ids, names, colors) = two_players();
    // Seed fisso per riproducibilità
    let mut eng = Engine::new(65, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();

    // (snapshot pre-draft non serve per il test — lo teniamo commentato
    // per eventuali regression future)
    let _pile_before: Vec<String> = eng.ticket_pile().to_vec();

    // P0 sceglie i primi 2 biglietti offerti (tiene minimo)
    let tickets_p0 = eng.initial_tickets_for(0).unwrap().to_vec();
    let keep_p0: Vec<String> = tickets_p0.iter().take(2).cloned().collect();
    let discarded_p0: Vec<String> = tickets_p0.iter().skip(2).cloned().collect();
    eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep: keep_p0,
    })
    .unwrap();

    // P1 sceglie i primi 2
    let tickets_p1 = eng.initial_tickets_for(1).unwrap().to_vec();
    let keep_p1: Vec<String> = tickets_p1.iter().take(2).cloned().collect();
    let discarded_p1: Vec<String> = tickets_p1.iter().skip(2).cloned().collect();
    eng.apply(Action::ChooseTickets {
        player_index: 1,
        keep: keep_p1,
    })
    .unwrap();

    assert_eq!(eng.phase(), GamePhase::PlayerTurn, "draft completato");

    // Gli scarti devono essere in fondo al mazzo biglietti
    let pile_after: Vec<String> = eng.ticket_pile().to_vec();

    // Verifica di appartenenza: ogni scarto deve essere presente nella parte finale del mazzo
    let all_discarded: Vec<String> = [discarded_p0, discarded_p1].concat().into_iter().collect();

    for tid in &all_discarded {
        assert!(
            pile_after.contains(tid),
            "scarto {} deve essere nel mazzo biglietti dopo il draft",
            tid
        );
    }

    // Verifica di posizione: gli scarti devono stare in fondo (dopo tutti i ticket non ancora distribuiti)
    let n_discarded = all_discarded.len();
    assert!(
        pile_after.len() >= n_discarded,
        "il mazzo ha almeno {} biglietti",
        n_discarded
    );
    let bottom: Vec<String> = pile_after[pile_after.len() - n_discarded..].to_vec();

    // Con reshuffleBottom, gli scarti sono mescolati (l'ordine non è deterministico) ma sono in fondo
    for tid in &all_discarded {
        assert!(
            bottom.contains(tid),
            "scarto {} deve essere nel fondo del mazzo",
            tid
        );
    }

    // Tutti i biglietti non-scartati devono stare PRIMA del fondo
    let top: Vec<String> = pile_after[..pile_after.len() - n_discarded].to_vec();
    for tid in &top {
        assert!(
            !all_discarded.contains(tid),
            "biglietto non scartato {} non può stare tra gli scarti in fondo",
            tid
        );
    }
}

// ============================================================
// Sezione 3 — Replay determinism
// ============================================================

#[test]
fn replay_determinism_same_seed_same_state() {
    let seed = 999;
    let (ids, names, colors) = two_players();

    let mut eng_a = Engine::new(
        seed,
        mini_ruleset(),
        mini_map(),
        ids.clone(),
        names.clone(),
        colors.clone(),
        0,
    )
    .unwrap();
    complete_initial_draft(&mut eng_a);

    let mut eng_b = Engine::new(
        seed,
        mini_ruleset(),
        mini_map(),
        ids.clone(),
        names.clone(),
        colors.clone(),
        0,
    )
    .unwrap();
    complete_initial_draft(&mut eng_b);

    let actions: Vec<Action> = vec![
        Action::DrawTrainCard {
            player_index: 0,
            source: DrawSource::Deck,
        },
        Action::DrawTrainCard {
            player_index: 0,
            source: DrawSource::Deck,
        },
        Action::DrawTrainCard {
            player_index: 1,
            source: DrawSource::Deck,
        },
    ];

    for action in &actions {
        eng_a.apply(action.clone()).unwrap();
        eng_b.apply(action.clone()).unwrap();
    }

    assert_eq!(
        eng_a.serialize_state(),
        eng_b.serialize_state(),
        "stessi seed + azioni = stesso stato"
    );
}

#[test]
fn replay_determinism_different_seed() {
    let (ids_a, names_a, colors_a) = two_players();
    let (ids_b, names_b, colors_b) = two_players();

    let mut eng_a =
        Engine::new(111, mini_ruleset(), mini_map(), ids_a, names_a, colors_a, 0).unwrap();
    complete_initial_draft(&mut eng_a);
    let mut eng_b =
        Engine::new(222, mini_ruleset(), mini_map(), ids_b, names_b, colors_b, 0).unwrap();
    complete_initial_draft(&mut eng_b);

    let action = Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    };
    eng_a.apply(action.clone()).unwrap();
    eng_b.apply(action.clone()).unwrap();

    assert_ne!(
        eng_a.serialize_state(),
        eng_b.serialize_state(),
        "seed diversi = stato diverso"
    );
}

// ============================================================
// Sezione 4 — Fuzzing self-play (mappa reale USA)
// ============================================================

/// Costruisce un pagamento valido per una rotta a partire dalla mano.
/// Restituisce `Some` solo se la mano può davvero pagare la rotta.
fn build_payment(hand: &[CardColor], route_color: &str, len: usize) -> Option<Vec<CardColor>> {
    if hand.len() < len {
        return None;
    }

    let is_gray = route_color.eq_ignore_ascii_case("Gray");
    let target = match route_color {
        "Red" => Some(CardColor::Red),
        "Orange" => Some(CardColor::Orange),
        "Yellow" => Some(CardColor::Yellow),
        "Green" => Some(CardColor::Green),
        "Blue" => Some(CardColor::Blue),
        "Purple" => Some(CardColor::Purple),
        "Black" => Some(CardColor::Black),
        "White" => Some(CardColor::White),
        _ => None,
    };

    let chosen_color = if is_gray {
        let mut best: Option<(CardColor, usize)> = None;
        for c in hand {
            if *c == CardColor::Locomotive {
                continue;
            }
            let cnt = hand.iter().filter(|x| **x == *c).count();
            if best.map(|(_, n)| cnt > n).unwrap_or(true) {
                best = Some((*c, cnt));
            }
        }
        best.map(|(c, _)| c)
    } else {
        target
    };

    let mut payment: Vec<CardColor> = Vec::new();

    if let Some(c) = chosen_color {
        for card in hand {
            if *card == c && payment.len() < len {
                payment.push(*card);
            }
        }
    }

    for card in hand {
        if payment.len() == len {
            break;
        }
        if *card == CardColor::Locomotive {
            payment.push(*card);
        }
    }

    if payment.len() == len {
        Some(payment)
    } else {
        None
    }
}

/// Politiche di bot del fuzz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BotPolicy {
    Random,
    ClaimAsap,
    /// Reclama raramente: esaurisce mazzo e scarti e fa scattare D27.
    Hoarder,
}

const BOT_POLICIES: [BotPolicy; 3] = [BotPolicy::Random, BotPolicy::ClaimAsap, BotPolicy::Hoarder];

/// Limite di azioni delle partite che devono arrivare a `GameOver`.
const FUZZ_MAX_ACTIONS: usize = 2000;
/// Limite delle partite "accumula carte", che non possono finire da sole.
const FUZZ_MAX_ACTIONS_HOARDER: usize = 400;
/// Il bot "accumula carte" reclama in media una volta su tante occasioni.
const HOARDER_CLAIM_ONE_IN: u32 = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
enum FuzzMove {
    Deck,
    FaceUp(usize),
    Tickets,
    Route(String),
}

/// Partita di fuzz conclusa.
struct FuzzOutcome {
    engine: Engine,
    actions: usize,
    /// Quante volte è stato creato un mazzo nuovo (D27).
    fresh_decks: usize,
}

#[derive(Debug, Default)]
struct FuzzStats {
    games: usize,
    actions: usize,
    fresh_decks: usize,
    hoarder_fresh_decks: usize,
    /// Partite in cui l'ultimo giro è partito per D28.
    no_route_last_rounds: usize,
    replays: usize,
}

fn fuzz_players(n: usize) -> (Vec<String>, Vec<String>, Vec<ttr_proto::PlayerColor>) {
    (
        (0..n).map(|i| format!("p{}", i + 1)).collect(),
        (0..n).map(|i| format!("P{}", i + 1)).collect(),
        ttr_core::default_player_colors(n),
    )
}

fn total_train_cards(eng: &Engine) -> usize {
    eng.draw_pile_len()
        + eng.face_up_cards().iter().flatten().count()
        + eng.discards_len()
        + (0..eng.num_players())
            .map(|pi| eng.player(pi).unwrap().hand.len())
            .sum::<usize>()
}

/// Invarianti da verificare dopo ogni azione. Ritorna il totale delle carte.
fn check_fuzz_invariants(
    eng: &Engine,
    trains_per_player: u32,
    deck_size: usize,
    previous_trains: &mut [u32],
    previous_total: usize,
    ctx: &dyn Fn() -> String,
) -> usize {
    let total = total_train_cards(eng);
    assert!(
        total >= previous_total && total.is_multiple_of(deck_size),
        "{}: carte in gioco {} (prima {}), attesi multipli di {}",
        ctx(),
        total,
        previous_total,
        deck_size
    );

    assert!(
        eng.face_up_cards().iter().all(Option::is_some),
        "{}: slot scoperto vuoto {:?}",
        ctx(),
        eng.face_up_cards()
    );

    for (pi, previous) in previous_trains.iter_mut().enumerate() {
        let trains = eng.player(pi).unwrap().trains_remaining;
        assert!(
            trains <= *previous && trains <= trains_per_player,
            "{}: P{} treni {} (prima {}, massimo {})",
            ctx(),
            pi,
            trains,
            previous,
            trains_per_player
        );
        *previous = trains;

        if pi != eng.current_player() {
            assert!(
                has_no_moves(&eng.legal_moves(pi)),
                "{}: P{} non è di turno ma ha mosse {:?}",
                ctx(),
                pi,
                eng.legal_moves(pi)
            );
        }
    }
    total
}

/// Sceglie la mossa secondo la politica, tra quelle dichiarate legali.
fn pick_fuzz_move(
    policy: BotPolicy,
    lm: &ttr_core::LegalMoves,
    rng: &mut rand::rngs::StdRng,
) -> Option<FuzzMove> {
    use rand::Rng;

    let mut draws: Vec<FuzzMove> = lm
        .draw_face_up
        .iter()
        .copied()
        .map(FuzzMove::FaceUp)
        .collect();
    if lm.can_draw_deck {
        draws.push(FuzzMove::Deck);
    }
    let routes: Vec<FuzzMove> = lm.routes.iter().cloned().map(FuzzMove::Route).collect();

    let preferred = match policy {
        BotPolicy::Random => {
            let mut all = draws.clone();
            all.extend(routes.iter().cloned());
            if lm.can_draw_tickets {
                all.push(FuzzMove::Tickets);
            }
            all
        }
        BotPolicy::ClaimAsap if !routes.is_empty() => routes.clone(),
        BotPolicy::Hoarder if !routes.is_empty() && rng.gen_ratio(1, HOARDER_CLAIM_ONE_IN) => {
            routes.clone()
        }
        BotPolicy::ClaimAsap | BotPolicy::Hoarder => draws.clone(),
    };

    let candidates = if !preferred.is_empty() {
        preferred
    } else if !draws.is_empty() {
        draws
    } else if !routes.is_empty() {
        routes
    } else if lm.can_draw_tickets {
        vec![FuzzMove::Tickets]
    } else {
        return None;
    };
    let idx = rng.gen_range(0..candidates.len());
    Some(candidates[idx].clone())
}

/// Gioca una partita sulla mappa USA reale. Ogni blocco fa fallire il test.
fn play_fuzz_game(
    seed: u64,
    num_players: usize,
    policy: BotPolicy,
    ruleset: &Ruleset,
    map: &MapData,
) -> FuzzOutcome {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    let max_actions = match policy {
        BotPolicy::Hoarder => FUZZ_MAX_ACTIONS_HOARDER,
        BotPolicy::Random | BotPolicy::ClaimAsap => FUZZ_MAX_ACTIONS,
    };
    let deck_size = (ruleset.deck.per_color * 8 + ruleset.deck.locomotives) as usize;

    let (ids, names, colors) = fuzz_players(num_players);
    let mut eng = Engine::new(seed, ruleset.clone(), map.clone(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);

    let mut rng = StdRng::seed_from_u64(seed ^ 0xDEADBEEF);
    let mut previous_trains: Vec<u32> = (0..num_players)
        .map(|pi| eng.player(pi).unwrap().trains_remaining)
        .collect();
    let mut total = total_train_cards(&eng);
    assert_eq!(total, deck_size, "seed {}: carte dopo il setup", seed);
    let mut actions = 0;

    while actions < max_actions && eng.phase() != GamePhase::GameOver {
        let pi = eng.current_player();
        let phase = eng.phase();
        let step = actions;
        let ctx = || {
            format!(
                "seed={seed} politica={policy:?} giocatori={num_players} azione={step} fase={phase:?} giocatore={pi}"
            )
        };

        let action = match phase {
            GamePhase::PlayerTurn | GamePhase::LastRound | GamePhase::SecondCardDraw => {
                let lm = eng.legal_moves(pi);
                let Some(chosen) = pick_fuzz_move(policy, &lm, &mut rng) else {
                    panic!("{}: nessuna mossa legale", ctx());
                };
                match chosen {
                    FuzzMove::Deck => Action::DrawTrainCard {
                        player_index: pi,
                        source: DrawSource::Deck,
                    },
                    FuzzMove::FaceUp(index) => Action::DrawTrainCard {
                        player_index: pi,
                        source: DrawSource::FaceUp { index },
                    },
                    FuzzMove::Tickets => draw_tickets(pi),
                    FuzzMove::Route(route_id) => {
                        let route = eng.get_route(&route_id).unwrap();
                        let hand = &eng.player(pi).unwrap().hand;
                        let Some(cards_discarded) =
                            build_payment(hand, &route.color, route.len as usize)
                        else {
                            panic!("{}: rotta {} legale ma non pagabile", ctx(), route_id);
                        };
                        Action::ClaimRoute {
                            player_index: pi,
                            route_id,
                            cards_discarded,
                            passengers: Default::default(),
                        }
                    }
                }
            }
            GamePhase::TicketChoice => {
                let keep_min = eng.ticket_draw_keep_min() as usize;
                Action::ChooseTickets {
                    player_index: pi,
                    keep: eng
                        .pending_tickets()
                        .iter()
                        .take(keep_min)
                        .cloned()
                        .collect(),
                }
            }
            _ => {
                panic!("{}: fase inattesa", ctx())
            }
        };

        if let Err(e) = eng.apply(action.clone()) {
            panic!("{}: mossa legale {:?} rifiutata: {:?}", ctx(), action, e);
        }
        actions += 1;
        total = check_fuzz_invariants(
            &eng,
            ruleset.trains_per_player,
            deck_size,
            &mut previous_trains,
            total,
            &ctx,
        );
    }

    FuzzOutcome {
        fresh_decks: total / deck_size - 1,
        engine: eng,
        actions,
    }
}

/// Ricostruisce la partita da seed e `action_log` e confronta lo stato.
fn assert_replay_matches(original: &Engine, policy: BotPolicy, ruleset: &Ruleset, map: &MapData) {
    let seed = original.seed();
    let (ids, names, colors) = fuzz_players(original.num_players());
    let mut replayed =
        Engine::new(seed, ruleset.clone(), map.clone(), ids, names, colors, 0).unwrap();
    for (i, record) in original.action_log().iter().enumerate() {
        if let Err(e) = replayed.apply(record.action.clone()) {
            panic!("seed={seed} politica={policy:?}: replay azione {i}: {e:?}");
        }
    }
    assert_eq!(
        replayed.serialize_state(),
        original.serialize_state(),
        "seed={seed} politica={policy:?}: replay divergente"
    );
    // Lo stato serializzato non include l'ordine del mazzo: gli eventi sì.
    assert_eq!(
        serde_json::to_string(replayed.action_log()).unwrap(),
        serde_json::to_string(original.action_log()).unwrap(),
        "seed={seed} politica={policy:?}: eventi del replay divergenti"
    );
}

/// Gioca `num_games` partite ruotando politiche e numero di giocatori (2-5).
fn run_fuzz(num_games: usize, base_seed: u64) -> FuzzStats {
    let ruleset = usa_ruleset();
    let map = usa_map();
    let mut stats = FuzzStats::default();

    for game_idx in 0..num_games {
        let seed = base_seed + game_idx as u64;
        let policy = BOT_POLICIES[game_idx % BOT_POLICIES.len()];
        let np = 2 + (game_idx / BOT_POLICIES.len()) % 4;
        let FuzzOutcome {
            engine: eng,
            actions,
            fresh_decks,
        } = play_fuzz_game(seed, np, policy, &ruleset, &map);
        let label = format!("seed={seed} politica={policy:?} giocatori={np} azioni={actions}");

        let mut seen = std::collections::BTreeSet::new();
        for rid in eng.claimed_route_ids() {
            assert!(seen.insert(rid.clone()), "{label}: rotta {rid} duplicata");
        }

        match policy {
            BotPolicy::Random | BotPolicy::ClaimAsap => {
                assert_eq!(
                    eng.phase(),
                    GamePhase::GameOver,
                    "{label}: partita non terminata"
                );
                for pi in 0..eng.num_players() {
                    assert!(
                        eng.player(pi).unwrap().score >= 0,
                        "{label}: P{pi} punteggio negativo"
                    );
                }
            }
            // Ogni azione aggiunge una carta alle mani: oltre il mazzo intero
            // D27 deve essere scattato per forza.
            BotPolicy::Hoarder => {
                assert!(fresh_decks > 0, "{label}: D27 non è mai scattata");
                stats.hoarder_fresh_decks += fresh_decks;
            }
        }

        let triggered_by_no_routes = eng.action_log().iter().any(|record| {
            record.events.iter().any(|e| {
                matches!(e, GameEvent::LastRoundAnnounced { trains_remaining, .. } if *trains_remaining > 2)
            })
        });
        stats.no_route_last_rounds += usize::from(triggered_by_no_routes);

        assert_replay_matches(&eng, policy, &ruleset, &map);
        stats.replays += 1;
        stats.games += 1;
        stats.actions += actions;
        stats.fresh_decks += fresh_decks;
    }
    println!("fuzz: {stats:?}");
    stats
}

#[test]
fn fuzzing_200_games_invariants() {
    let num_games: usize = std::env::var("FUZZ_GAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    let stats = run_fuzz(num_games, 1000);
    assert_eq!(stats.games, num_games);
}

#[test]
fn fuzzing_1000_games_explicit() {
    let num_games = std::env::var("FUZZ_GAMES_FULL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if num_games == 0 {
        return;
    }
    let stats = run_fuzz(num_games, 2000);
    assert_eq!(stats.games, num_games);
}

#[test]
fn golden_longest_path_on_multigraph() {
    let mut routes_map: BTreeMap<String, Route> = BTreeMap::new();
    routes_map.insert("a_b_1".into(), route("a_b_1", "a", "b", 3, "Gray", false));
    routes_map.insert("b_c_1".into(), route("b_c_1", "b", "c", 2, "Gray", false));
    routes_map.insert("a_c_1".into(), route("a_c_1", "a", "c", 4, "Gray", false));

    let claimed: Vec<ClaimedRouteInfo> = vec![
        ClaimedRouteInfo {
            route_id: "a_b_1".into(),
            claimed_by: 0,
        },
        ClaimedRouteInfo {
            route_id: "b_c_1".into(),
            claimed_by: 0,
        },
        ClaimedRouteInfo {
            route_id: "a_c_1".into(),
            claimed_by: 0,
        },
    ];
    let cri_refs: Vec<&ClaimedRouteInfo> = claimed.iter().collect();
    let result = ttr_core::scoring::longest_continuous_path(&cri_refs, &routes_map);
    assert_eq!(result, 9, "triangolo: 3+2+4 = 9");
}

#[test]
fn golden_union_find_connectivity() {
    let mut uf = ttr_core::scoring::UnionFind::new();
    uf.union("A", "B");
    uf.union("B", "C");
    assert!(uf.connected("A", "C"));
    assert!(!uf.connected("A", "D"));
}

// ============================================================
// Regressioni di robustezza
// ============================================================

const L: CardColor = CardColor::Locomotive;

/// Motore post-draft con mazzo vuoto e scarti/scoperte imposti dal test.
fn deck_fixture(seed: u64, discards: Vec<CardColor>, face_up: [Option<CardColor>; 5]) -> Engine {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(seed, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    eng.drain_draw_pile();
    eng.set_discards(discards);
    for (i, card) in face_up.into_iter().enumerate() {
        eng.set_face_up_card(i, card);
    }
    eng
}

fn draw_face_up_0(eng: &mut Engine) {
    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::FaceUp { index: 0 },
    })
    .unwrap();
}

fn face_up_count(eng: &Engine) -> usize {
    eng.face_up_cards().iter().flatten().count()
}

fn loco_count(eng: &Engine) -> usize {
    eng.face_up_cards()
        .iter()
        .flatten()
        .filter(|c| **c == L)
        .count()
}

/// Scoperte, mazzo e scarti in forma compatta, per i valori di riferimento.
fn deck_fingerprint(eng: &Engine) -> String {
    let cards: String = eng
        .face_up_cards()
        .iter()
        .map(|c| match c {
            Some(CardColor::Red) => 'R',
            Some(CardColor::Orange) => 'O',
            Some(CardColor::Yellow) => 'Y',
            Some(CardColor::Green) => 'G',
            Some(CardColor::Blue) => 'B',
            Some(CardColor::Purple) => 'P',
            Some(CardColor::Black) => 'K',
            Some(CardColor::White) => 'W',
            Some(CardColor::Locomotive) => 'L',
            None => '-',
        })
        .collect();
    format!("{}/{}/{}", cards, eng.draw_pile_len(), eng.discards_len())
}

const RED: Option<CardColor> = Some(CardColor::Red);
const GREEN: Option<CardColor> = Some(CardColor::Green);
const LOCO: Option<CardColor> = Some(L);

/// Caso normale: la sequenza RNG del reset è quella dei valori di riferimento,
/// catturati dall'implementazione ricorsiva originale.
#[test]
fn loco_reset_normal_case_matches_recursive_rng_sequence() {
    let mut pool = vec![L; 12];
    pool.extend([
        CardColor::Red,
        CardColor::Blue,
        CardColor::Yellow,
        CardColor::Purple,
        CardColor::White,
        CardColor::Black,
    ]);
    let got: Vec<String> = (0..16u64)
        .map(|seed| {
            let mut eng = deck_fixture(seed, pool.clone(), [RED, LOCO, LOCO, LOCO, GREEN]);
            draw_face_up_0(&mut eng);
            deck_fingerprint(&eng)
        })
        .collect();
    let expected: [&str; 16] = [
        "LKLPW/12/5",
        "BLPYL/12/5",
        "LLKBP/2/15",
        "PLKWY/12/5",
        "WLLPR/12/5",
        "RBLYL/2/15",
        "RKLPY/2/15",
        "LLYKR/12/5",
        "KWRGP/7/10",
        "LPWGY/2/15",
        "LRGYL/17/0",
        "LYBLW/12/5",
        "PKLWL/7/10",
        "PLLWK/7/10",
        "RLWKL/7/10",
        "YLWGL/2/15",
    ];
    assert_eq!(got, expected);
}

/// Stessi input, stesso esito: il reset non introduce non determinismo.
#[test]
fn loco_reset_normal_case_is_deterministic() {
    let mut pool = vec![L; 10];
    pool.extend([CardColor::Blue, CardColor::Yellow, CardColor::White]);
    for seed in 0..20u64 {
        let mut a = deck_fixture(seed, pool.clone(), [RED, LOCO, LOCO, LOCO, GREEN]);
        let mut b = deck_fixture(seed, pool.clone(), [RED, LOCO, LOCO, LOCO, GREEN]);
        draw_face_up_0(&mut a);
        draw_face_up_0(&mut b);
        assert_eq!(a.serialize_state(), b.serialize_state(), "seed {}", seed);
        assert_eq!(deck_fingerprint(&a), deck_fingerprint(&b), "seed {}", seed);
    }
}

/// Esattamente 3 non-locomotive nel pool: non e' un caso senza uscita, il
/// reset continua finche' le scoperte scendono sotto soglia.
#[test]
fn loco_reset_three_non_locos_ends_below_threshold() {
    let mut pool = vec![L; 9];
    pool.extend([CardColor::Blue, CardColor::Yellow]);
    for seed in 0..20u64 {
        let mut eng = deck_fixture(seed, pool.clone(), [RED, LOCO, LOCO, LOCO, GREEN]);
        draw_face_up_0(&mut eng);
        assert!(
            loco_count(&eng) < 3,
            "seed {}: {}",
            seed,
            deck_fingerprint(&eng)
        );
    }
}

/// Pool con 2 non-locomotive: un solo rimescolamento, poi ci si ferma.
#[test]
fn loco_reset_deadend_two_non_locos_terminates() {
    for seed in 0..20u64 {
        let mut eng = deck_fixture(seed, vec![L; 10], [RED, LOCO, LOCO, RED, RED]);
        draw_face_up_0(&mut eng);
        assert_eq!(eng.phase(), GamePhase::SecondCardDraw);
        assert_eq!(face_up_count(&eng), 5, "seed {}", seed);
        assert_eq!(eng.draw_pile_len(), 9, "seed {}", seed);
        assert_eq!(eng.discards_len(), 0, "seed {}", seed);
    }
}

/// Pool di sole locomotive: le scoperte restano tutte locomotive.
#[test]
fn loco_reset_deadend_only_locos_terminates() {
    let mut eng = deck_fixture(7, vec![L; 9], [RED, LOCO, LOCO, LOCO, LOCO]);
    draw_face_up_0(&mut eng);
    assert_eq!(deck_fingerprint(&eng), "LLLLL/8/0");
}

/// Pool con meno di 5 carte durante la riscoperta: le carte mancanti arrivano
/// dal mazzo nuovo (D27), che sblocca il reset: le scoperte vengono
/// ricontrollate una volta e scendono sotto la soglia.
/// Usa lo slot `None` nelle scoperte (non raggiungibile in partita) solo per
/// arrivare alla riscoperta con un pool di 4 carte.
#[test]
fn loco_reset_deadend_small_pool_uses_fresh_deck_and_terminates() {
    for seed in 0..20u64 {
        let mut eng = deck_fixture(seed, vec![L], [RED, LOCO, LOCO, LOCO, None]);
        draw_face_up_0(&mut eng);
        assert_eq!(eng.phase(), GamePhase::SecondCardDraw);
        assert_eq!(face_up_count(&eng), 5, "seed {}", seed);
        assert!(
            loco_count(&eng) < 3,
            "seed {}: {}",
            seed,
            deck_fingerprint(&eng)
        );
        // 4 locomotive del pool più il mazzo nuovo da 110, RED in mano.
        assert_eq!(
            eng.draw_pile_len() + eng.discards_len() + face_up_count(&eng),
            114,
            "seed {}",
            seed
        );
    }
}

/// D27: mazzo e scarti vuoti, la pesca dal mazzo riesce e il mazzo si riempie.
#[test]
fn deck_draw_with_empty_pool_builds_fresh_deck() {
    let mut eng = deck_fixture(42, vec![], [RED, RED, GREEN, GREEN, RED]);
    let hand_before = eng.player_hand(0).unwrap().len();
    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    })
    .unwrap();
    assert_eq!(eng.player_hand(0).unwrap().len(), hand_before + 1);
    assert_eq!(eng.draw_pile_len(), 109);
    assert_eq!(eng.discards_len(), 0);
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);
}

/// D27: la sostituzione di una scoperta con pool vuoto non lascia lo slot vuoto.
#[test]
fn face_up_replacement_with_empty_pool_fills_slot() {
    let mut eng = deck_fixture(42, vec![], [RED, RED, GREEN, GREEN, RED]);
    draw_face_up_0(&mut eng);
    assert_eq!(face_up_count(&eng), 5);
    assert_eq!(eng.draw_pile_len(), 109);
    assert_eq!(eng.discards_len(), 0);
}

/// D27: in `SecondCardDraw` con sole locomotive scoperte e pool vuoto resta
/// disponibile la pesca dal mazzo, e il mazzo nuovo fa ricontrollare il mercato.
#[test]
fn second_draw_with_only_locos_and_empty_pool_can_draw_deck() {
    let mut eng = deck_fixture(42, vec![L; 20], [RED, RED, GREEN, GREEN, RED]);
    draw_face_up_0(&mut eng);
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);
    eng.clear_deck_and_discards();
    for i in 0..5 {
        eng.set_face_up_card(i, LOCO);
    }

    let lm = eng.legal_moves(0);
    assert!(lm.draw_face_up.is_empty());
    assert!(lm.can_draw_deck);

    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    })
    .unwrap();
    assert_eq!(eng.current_player(), 1);
    assert!(loco_count(&eng) < 3, "{}", deck_fingerprint(&eng));
    // 109 dopo la pesca, poi le 5 locomotive negli scarti e 5 nuove scoperte.
    assert_eq!(eng.draw_pile_len() + eng.discards_len(), 109);
    assert_eq!(face_up_count(&eng), 5);
}

/// J1: un mercato rimasto con ≥3 locomotive dopo un reset senza uscita (D26)
/// viene ricontrollato quando una pesca dal mazzo coperto crea il mazzo nuovo
/// (D27), dopo che la carta è finita in mano.
#[test]
fn deck_draw_that_builds_fresh_deck_rechecks_market() {
    for seed in 0..20u64 {
        let mut eng = deck_fixture(seed, vec![], [LOCO, LOCO, LOCO, RED, GREEN]);
        eng.set_draw_pile(vec![CardColor::Red]);

        let first = eng
            .apply(Action::DrawTrainCard {
                player_index: 0,
                source: DrawSource::Deck,
            })
            .unwrap();
        assert!(
            !first
                .iter()
                .any(|e| matches!(e, GameEvent::FaceUpReset { .. })),
            "seed {seed}: pesca senza mazzo nuovo non tocca il mercato"
        );
        assert_eq!(loco_count(&eng), 3);

        let second = eng
            .apply(Action::DrawTrainCard {
                player_index: 0,
                source: DrawSource::Deck,
            })
            .unwrap();
        let reshuffled = second
            .iter()
            .position(|e| matches!(e, GameEvent::DeckReshuffled))
            .expect("mazzo nuovo D27");
        let drawn = second
            .iter()
            .position(|e| matches!(e, GameEvent::CardDrawnFromDeck { .. }))
            .expect("carta pescata");
        let reset = second
            .iter()
            .position(|e| matches!(e, GameEvent::FaceUpReset { .. }))
            .expect("mercato ricontrollato");
        assert!(
            reshuffled < drawn && drawn < reset,
            "seed {seed}: {second:?}"
        );
        assert!(
            loco_count(&eng) < 3,
            "seed {seed}: {}",
            deck_fingerprint(&eng)
        );
        assert_eq!(face_up_count(&eng), 5);
        assert_eq!(eng.current_player(), 1);
    }
}

/// J1, controllo: con il pool in stato senza uscita e il mazzo non vuoto la
/// pesca dal mazzo non ricontrolla il mercato (niente rimescolamenti a vuoto).
#[test]
fn deck_draw_without_fresh_deck_leaves_dead_end_market() {
    let mut eng = deck_fixture(3, vec![L], [LOCO, LOCO, LOCO, RED, GREEN]);
    eng.set_draw_pile(vec![CardColor::Red, CardColor::Locomotive]);
    for _ in 0..2 {
        let events = eng
            .apply(Action::DrawTrainCard {
                player_index: 0,
                source: DrawSource::Deck,
            })
            .unwrap();
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, GameEvent::FaceUpReset { .. })),
            "{events:?}"
        );
    }
    assert_eq!(loco_count(&eng), 3);
}

/// Mappa con una doppia, una rotta corta e una da 6, con 5 treni a testa e
/// 3 giocatori. P0 e P1 reclamano la doppia e la rotta corta: restano la
/// sorella bloccata e la rotta troppo lunga, tranne per chi conserva 6 treni.
fn no_routes_left_fixture(extra_train_player: Option<usize>) -> (Engine, Vec<GameEvent>) {
    let mut map = double_route_map();
    map.routes.push(route("C_D_1", "C", "D", 6, "Gray", false));
    map.cities.push(city("D"));
    let (ids, names, colors) = three_players();
    let mut eng = Engine::new(71, strict_double_ruleset(), map, ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    for pi in 0..3 {
        eng.set_player_trains(pi, 5);
    }
    if let Some(pi) = extra_train_player {
        // Gli restano 6 treni anche dopo la rotta che reclama nel fixture.
        eng.set_player_trains(pi, 6 + [2, 1, 0][pi]);
    }

    eng.set_player_hand(0, vec![CardColor::Red; 2]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red; 2],
        passengers: Default::default(),
    })
    .unwrap();
    eng.set_player_hand(1, vec![CardColor::Blue]);
    let events = eng
        .apply(Action::ClaimRoute {
            player_index: 1,
            route_id: "B_C_1".into(),
            cards_discarded: vec![CardColor::Blue],
            passengers: Default::default(),
        })
        .unwrap();
    (eng, events)
}

fn draw_two_from_deck(eng: &mut Engine, player_index: usize) -> Vec<GameEvent> {
    let draw = Action::DrawTrainCard {
        player_index,
        source: DrawSource::Deck,
    };
    eng.apply(draw.clone()).unwrap();
    eng.apply(draw).unwrap()
}

/// D28: nessuna rotta reclamabile da nessuno, parte l'ultimo giro e ognuno,
/// incluso chi lo innesca, fa un ultimo turno.
#[test]
fn golden_last_round_when_no_route_is_claimable() {
    let (mut eng, events) = no_routes_left_fixture(None);

    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::LastRoundAnnounced {
                trigger_player: 1,
                trains_remaining: 4,
            }
        )),
        "{:?}",
        events
    );
    assert_eq!(eng.last_round_trigger_player(), Some(1));
    assert_eq!(eng.phase(), GamePhase::LastRound);
    assert_eq!(eng.current_player(), 2);

    for pi in [2, 0] {
        let events = draw_two_from_deck(&mut eng, pi);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, GameEvent::LastRoundAnnounced { .. })),
            "annuncio ripetuto: {:?}",
            events
        );
        assert_eq!(eng.phase(), GamePhase::LastRound);
    }
    let events = draw_two_from_deck(&mut eng, 1);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::GameOver { .. }))
    );
    assert_eq!(eng.phase(), GamePhase::GameOver);
}

/// D28: basta una rotta aperta per un solo giocatore e l'ultimo giro non parte.
#[test]
fn no_last_round_while_one_player_can_still_claim() {
    for pi in 0..3 {
        let (eng, events) = no_routes_left_fixture(Some(pi));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, GameEvent::LastRoundAnnounced { .. })),
            "a P{} restano 6 treni: {:?}",
            pi,
            events
        );
        assert!(!eng.last_round(), "P{}", pi);
        assert_eq!(eng.phase(), GamePhase::PlayerTurn, "P{}", pi);
    }
}

#[test]
fn choose_tickets_rejects_duplicates_in_initial_draft() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(42, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    let offered = eng.initial_tickets_for(0).unwrap().to_vec();

    let res = eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep: vec![offered[0].clone(), offered[0].clone()],
    });
    assert!(
        matches!(res, Err(EngineError::InvalidPayload { .. })),
        "duplicati nel draft iniziale devono essere rifiutati: {:?}",
        res
    );

    // Il rifiuto non consuma il draft: la scelta corretta passa ancora.
    eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep: vec![offered[0].clone(), offered[1].clone()],
    })
    .unwrap();
}

#[test]
fn choose_tickets_rejects_duplicates_mid_game() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(42, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    eng.apply(draw_tickets(0)).unwrap();
    let offered = eng.pending_tickets().to_vec();

    let res = eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep: vec![offered[0].clone(), offered[0].clone()],
    });
    assert!(
        matches!(res, Err(EngineError::InvalidPayload { .. })),
        "duplicati a meta' partita devono essere rifiutati: {:?}",
        res
    );
    assert_eq!(eng.phase(), GamePhase::TicketChoice);
}

/// Claim di `C_D_1` (grigia, 3) con la mano e la dichiarazione indicate.
fn claim_c_d(eng: &mut Engine, hand: Vec<CardColor>, declared: Vec<CardColor>) -> Vec<CardColor> {
    eng.set_player_hand(0, hand);
    let events = eng
        .apply(Action::ClaimRoute {
            player_index: 0,
            route_id: "C_D_1".into(),
            cards_discarded: declared,
            passengers: Default::default(),
        })
        .unwrap();
    events
        .iter()
        .find_map(|e| match e {
            GameEvent::RouteClaimed { cards_used, .. } => Some(cards_used.clone()),
            _ => None,
        })
        .expect("evento RouteClaimed")
}

#[test]
fn claim_records_locomotives_actually_removed() {
    let mut eng = deck_fixture(42, vec![], [RED, RED, RED, GREEN, GREEN]);
    eng.set_player_hand(1, vec![]);

    let used = claim_c_d(&mut eng, vec![L; 3], vec![CardColor::Red; 3]);
    assert_eq!(used, vec![L; 3], "cards_used deve riportare le carte vere");
    assert_eq!(eng.player_hand(0).unwrap(), &[] as &[CardColor]);

    // Gli scarti non hanno un getter: si ripescano. Il mazzo e' vuoto, quindi
    // le uniche carte disponibili sono quelle appena scartate.
    let deck_draw = |eng: &mut Engine, player_index: usize| {
        eng.apply(Action::DrawTrainCard {
            player_index,
            source: DrawSource::Deck,
        })
        .unwrap();
    };
    deck_draw(&mut eng, 1);
    deck_draw(&mut eng, 1);
    deck_draw(&mut eng, 0);
    assert_eq!(eng.player_hand(1).unwrap(), &[L, L]);
    assert_eq!(eng.player_hand(0).unwrap(), &[L]);
}

#[test]
fn claim_records_mixed_payment_in_declaration_order() {
    let mut eng = deck_fixture(42, vec![], [RED, RED, RED, GREEN, GREEN]);
    let used = claim_c_d(
        &mut eng,
        vec![L, CardColor::Red, L],
        vec![CardColor::Red; 3],
    );
    assert_eq!(used, vec![CardColor::Red, L, L]);
    assert_eq!(eng.discards_len(), 3);
}

#[test]
fn ticket_decks_are_dealt_per_ruleset_and_draw_pile_is_draw_deck_only() {
    let mut rs = mini_ruleset();
    rs.ticket_decks = vec!["long".into(), "regular".into()];
    rs.initial_tickets.dealt = vec![
        TicketDealtEntry {
            deck: "long".into(),
            count: 1,
            keep_max: None,
        },
        TicketDealtEntry {
            deck: "regular".into(),
            count: 3,
            keep_max: None,
        },
    ];
    rs.ticket_draw.deck = "regular".into();
    let mut map = mini_map();
    for i in 0..3 {
        let mut t = ticket(&format!("long_{}", i), "A", "F", 20);
        t.deck = "long".into();
        map.tickets.push(t);
    }
    let (ids, names, colors) = two_players();
    let eng = Engine::new(7, rs, map.clone(), ids, names, colors, 0).unwrap();
    let deck_of = |id: &String| {
        map.tickets
            .iter()
            .find(|t| &t.id == id)
            .unwrap()
            .deck
            .clone()
    };
    for pi in 0..2 {
        let offered = eng.initial_tickets_for(pi).unwrap().to_vec();
        assert_eq!(offered.iter().filter(|t| deck_of(t) == "long").count(), 1);
        assert_eq!(
            offered.iter().filter(|t| deck_of(t) == "regular").count(),
            3
        );
    }
    assert!(eng.ticket_pile().iter().all(|t| deck_of(t) == "regular"));
}

// ============================================================
// Configurazione: Engine::new e Ruleset::validate
// ============================================================

fn init_error(rs: Ruleset, map: MapData, players: usize, first_player: usize) -> EngineInitError {
    let (ids, names, colors) = fuzz_players(players);
    Engine::new(1, rs, map, ids, names, colors, first_player)
        .err()
        .expect("Engine::new doveva fallire")
}

#[test]
fn init_rejects_mismatched_player_lists() {
    let (ids, mut names, colors) = two_players();
    names.pop();
    let err = Engine::new(1, mini_ruleset(), mini_map(), ids, names, colors, 0)
        .err()
        .unwrap();
    assert_eq!(
        err,
        EngineInitError::PlayerListsMismatch {
            ids: 2,
            names: 1,
            colors: 2
        }
    );
}

#[test]
fn init_rejects_player_count_out_of_range() {
    let err = Engine::new(1, mini_ruleset(), mini_map(), vec![], vec![], vec![], 0)
        .err()
        .unwrap();
    assert_eq!(
        err,
        EngineInitError::PlayerCountOutOfRange {
            count: 0,
            min: 2,
            max: 5
        }
    );
    assert!(matches!(
        init_error(mini_ruleset(), mini_map(), 1, 0),
        EngineInitError::PlayerCountOutOfRange { count: 1, .. }
    ));
    let six = vec![ttr_proto::PlayerColor::Red; 6];
    let err = Engine::new(
        1,
        mini_ruleset(),
        mini_map(),
        (0..6).map(|i| format!("p{i}")).collect(),
        (0..6).map(|i| format!("P{i}")).collect(),
        six,
        0,
    )
    .err()
    .unwrap();
    assert!(matches!(
        err,
        EngineInitError::PlayerCountOutOfRange { count: 6, .. }
    ));
}

#[test]
fn init_rejects_first_player_out_of_range() {
    assert_eq!(
        init_error(mini_ruleset(), mini_map(), 2, 7),
        EngineInitError::FirstPlayerOutOfRange {
            first_player: 7,
            players: 2
        }
    );
}

#[test]
fn validate_rejects_invalid_player_range() {
    let mut rs = mini_ruleset();
    rs.players = PlayersConfig { min: 3, max: 2 };
    assert_eq!(
        rs.validate(&mini_map()),
        Err(EngineInitError::InvalidPlayerRange { min: 3, max: 2 })
    );
    rs.players = PlayersConfig { min: 0, max: 2 };
    assert!(matches!(
        rs.validate(&mini_map()),
        Err(EngineInitError::InvalidPlayerRange { .. })
    ));
}

#[test]
fn init_rejects_final_round_without_trigger() {
    let mut rs = mini_ruleset();
    rs.final_round_includes_trigger = false;
    assert_eq!(
        init_error(rs.clone(), mini_map(), 2, 0),
        EngineInitError::UnsupportedFinalRound
    );
    assert_eq!(
        rs.validate(&mini_map()),
        Err(EngineInitError::UnsupportedFinalRound)
    );
}

#[test]
fn init_rejects_train_deck_too_small() {
    let mut rs = mini_ruleset();
    rs.deck = DeckConfig {
        per_color: 0,
        locomotives: 0,
    };
    assert_eq!(
        init_error(rs.clone(), mini_map(), 2, 0),
        EngineInitError::TrainDeckTooSmall {
            have: 0,
            needed: 13
        }
    );
    assert_eq!(
        rs.validate(&mini_map()),
        Err(EngineInitError::TrainDeckTooSmall {
            have: 0,
            needed: 25
        })
    );
}

#[test]
fn init_rejects_non_canonical_route_color() {
    let mut map = mini_map();
    map.routes[0].color = "gray".into();
    let expected = EngineInitError::InvalidRouteColor {
        route_id: "A_B_1".into(),
        color: "gray".into(),
    };
    assert_eq!(init_error(mini_ruleset(), map.clone(), 2, 0), expected);
    assert_eq!(mini_ruleset().validate(&map), Err(expected));
}

#[test]
fn init_rejects_unknown_bonus_type() {
    let mut rs = mini_ruleset();
    rs.bonuses[0].bonus_type = "longestPath".into();
    let expected = EngineInitError::UnknownBonusType {
        bonus_type: "longestPath".into(),
    };
    assert_eq!(init_error(rs.clone(), mini_map(), 2, 0), expected);
    assert_eq!(rs.validate(&mini_map()), Err(expected));
}

#[test]
fn init_rejects_duplicate_ticket_deck() {
    let mut rs = mini_ruleset();
    rs.ticket_decks = vec!["regular".into(), "regular".into()];
    let expected = EngineInitError::DuplicateTicketDeck {
        deck: "regular".into(),
    };
    assert_eq!(init_error(rs.clone(), mini_map(), 2, 0), expected);
    assert_eq!(rs.validate(&mini_map()), Err(expected));
}

#[test]
fn init_rejects_dealt_deck_not_in_ticket_decks() {
    let mut rs = mini_ruleset();
    rs.initial_tickets.dealt = vec![TicketDealtEntry {
        deck: "long".into(),
        count: 4,
        keep_max: None,
    }];
    let expected = EngineInitError::DealtTicketDeckUnknown {
        deck: "long".into(),
    };
    assert_eq!(init_error(rs.clone(), mini_map(), 2, 0), expected);
    assert_eq!(rs.validate(&mini_map()), Err(expected));
}

#[test]
fn init_rejects_draw_deck_not_in_ticket_decks() {
    let mut rs = mini_ruleset();
    rs.ticket_draw.deck = "long".into();
    let expected = EngineInitError::DrawTicketDeckUnknown {
        deck: "long".into(),
    };
    assert_eq!(init_error(rs.clone(), mini_map(), 2, 0), expected);
    assert_eq!(rs.validate(&mini_map()), Err(expected));
}

#[test]
fn init_rejects_ticket_with_unlisted_deck() {
    let mut map = mini_map();
    map.tickets[1].deck = "long".into();
    let expected = EngineInitError::TicketDeckNotListed {
        ticket_id: "A_F".into(),
        deck: "long".into(),
    };
    assert_eq!(init_error(mini_ruleset(), map.clone(), 2, 0), expected);
    assert_eq!(mini_ruleset().validate(&map), Err(expected));
}

#[test]
fn init_rejects_ticket_deck_too_small() {
    let mut rs = mini_ruleset();
    rs.initial_tickets.dealt[0].count = 11;
    assert_eq!(
        init_error(rs.clone(), mini_map(), 2, 0),
        EngineInitError::TicketDeckTooSmall {
            deck: "regular".into(),
            have: 20,
            needed: 22
        }
    );
    assert_eq!(
        rs.validate(&mini_map()),
        Err(EngineInitError::TicketDeckTooSmall {
            deck: "regular".into(),
            have: 20,
            needed: 55
        })
    );
}

#[test]
fn init_rejects_initial_keep_min_above_dealt() {
    let mut rs = mini_ruleset();
    rs.initial_tickets.keep_min = 5;
    let expected = EngineInitError::InitialKeepMinExceedsDealt {
        keep_min: 5,
        dealt: 4,
    };
    assert_eq!(init_error(rs.clone(), mini_map(), 2, 0), expected);
    assert_eq!(rs.validate(&mini_map()), Err(expected));
}

#[test]
fn init_rejects_draw_keep_min_above_count() {
    let mut rs = mini_ruleset();
    rs.ticket_draw.keep_min = 4;
    let expected = EngineInitError::DrawKeepMinExceedsCount {
        keep_min: 4,
        count: 3,
    };
    assert_eq!(init_error(rs.clone(), mini_map(), 2, 0), expected);
    assert_eq!(rs.validate(&mini_map()), Err(expected));
}

#[test]
fn validate_accepts_shipped_usa_data() {
    usa_ruleset().validate(&usa_map()).unwrap();
}

#[test]
fn init_error_messages_are_italian() {
    let msg = init_error(mini_ruleset(), mini_map(), 2, 7).to_string();
    assert!(msg.contains("primo giocatore"), "{msg}");
}

// ============================================================
// Eventi di reset e rimescolamento
// ============================================================

fn apply_events(eng: &mut Engine, source: DrawSource) -> Vec<GameEvent> {
    eng.apply(Action::DrawTrainCard {
        player_index: eng.current_player(),
        source,
    })
    .unwrap()
}

#[test]
fn loco_reset_emits_event() {
    let mut pool = vec![L; 12];
    pool.extend([
        CardColor::Red,
        CardColor::Blue,
        CardColor::Yellow,
        CardColor::Purple,
        CardColor::White,
        CardColor::Black,
    ]);
    for seed in 0..8u64 {
        let mut eng = deck_fixture(seed, pool.clone(), [RED, LOCO, LOCO, LOCO, GREEN]);
        let events = apply_events(&mut eng, DrawSource::FaceUp { index: 0 });
        let reset = events.iter().rev().find_map(|e| match e {
            GameEvent::FaceUpReset {
                discarded,
                new_face_up,
            } => Some((discarded.clone(), new_face_up.clone())),
            _ => None,
        });
        let (discarded, new_face_up) = reset.expect("FaceUpReset atteso");
        assert_eq!(discarded.len(), 5, "seed {seed}");
        assert_eq!(new_face_up.len(), 5, "seed {seed}");
        assert_eq!(new_face_up, eng.face_up_cards(), "seed {seed}");
    }
}

#[test]
fn no_loco_reset_event_below_threshold() {
    let mut eng = deck_fixture(3, vec![CardColor::Blue; 10], [RED, GREEN, GREEN, RED, RED]);
    let events = apply_events(&mut eng, DrawSource::FaceUp { index: 0 });
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::FaceUpReset { .. }))
    );
}

#[test]
fn reshuffle_emits_event() {
    // Scarti rimescolati dentro la pesca dal mazzo coperto.
    let mut eng = deck_fixture(5, vec![CardColor::Blue; 10], [RED; 5]);
    let events = apply_events(&mut eng, DrawSource::Deck);
    let reshuffled = events
        .iter()
        .position(|e| matches!(e, GameEvent::DeckReshuffled))
        .expect("DeckReshuffled atteso");
    let drawn = events
        .iter()
        .position(|e| matches!(e, GameEvent::CardDrawnFromDeck { .. }))
        .unwrap();
    assert!(reshuffled < drawn);

    // Mazzo e scarti vuoti: mazzo nuovo (D27).
    let mut eng = deck_fixture(5, vec![], [RED; 5]);
    let events = apply_events(&mut eng, DrawSource::Deck);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DeckReshuffled))
    );

    // Mazzo pieno: nessun rimescolamento.
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(5, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    let events = apply_events(&mut eng, DrawSource::Deck);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::DeckReshuffled))
    );
}

#[test]
fn reshuffle_during_face_up_replacement_precedes_reveal() {
    let mut eng = deck_fixture(9, vec![CardColor::Blue; 4], [RED, GREEN, GREEN, RED, RED]);
    let events = apply_events(&mut eng, DrawSource::FaceUp { index: 0 });
    let reshuffled = events
        .iter()
        .position(|e| matches!(e, GameEvent::DeckReshuffled))
        .expect("DeckReshuffled atteso");
    let revealed = events
        .iter()
        .position(|e| matches!(e, GameEvent::FaceUpRevealed { index: 0, .. }))
        .expect("FaceUpRevealed atteso");
    assert!(reshuffled < revealed);
}

// ============================================================
// Stato pendente, colori, bonus
// ============================================================

#[test]
fn pending_tickets_empty_after_choice() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(21, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    for pi in 0..2 {
        assert!(!eng.initial_tickets_for(pi).unwrap().is_empty());
    }
    complete_initial_draft(&mut eng);
    for pi in 0..2 {
        assert!(eng.initial_tickets_for(pi).unwrap().is_empty());
    }

    eng.apply(draw_tickets(0)).unwrap();
    let keep = vec![eng.pending_tickets()[0].clone()];
    assert!(!eng.pending_tickets().is_empty());
    eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep,
    })
    .unwrap();
    assert!(eng.pending_tickets().is_empty());
}

#[test]
fn legal_and_apply_agree_on_gray() {
    let claim = |eng: &mut Engine, cards: Vec<CardColor>| {
        eng.apply(Action::ClaimRoute {
            player_index: 0,
            route_id: "C_D_1".into(),
            cards_discarded: cards,
            passengers: Default::default(),
        })
    };
    let route_is_legal = |eng: &Engine| eng.legal_moves(0).routes.iter().any(|r| r == "C_D_1");

    // Tre carte dello stesso colore pagano una rotta grigia da 3.
    let mut eng = deck_fixture(42, vec![], [RED; 5]);
    eng.set_player_hand(0, vec![CardColor::Blue; 3]);
    assert!(route_is_legal(&eng));
    claim(&mut eng, vec![CardColor::Blue; 3]).unwrap();

    // Colori misti senza locomotive: né legale né accettata.
    let mut eng = deck_fixture(42, vec![], [RED; 5]);
    eng.set_player_hand(0, vec![CardColor::Blue, CardColor::Blue, CardColor::Green]);
    assert!(!route_is_legal(&eng));
    assert!(
        claim(
            &mut eng,
            vec![CardColor::Blue, CardColor::Blue, CardColor::Green]
        )
        .is_err()
    );

    // Con una locomotiva il pagamento torna possibile.
    let mut eng = deck_fixture(42, vec![], [RED; 5]);
    eng.set_player_hand(0, vec![CardColor::Blue, CardColor::Blue, L]);
    assert!(route_is_legal(&eng));
    claim(&mut eng, vec![CardColor::Blue, CardColor::Blue, L]).unwrap();
}

#[test]
fn longest_path_bonus_is_found_by_type() {
    let mut rs = mini_ruleset();
    rs.bonuses = vec![
        Bonus {
            bonus_type: "globetrotter".into(),
            label: "Globetrotter".into(),
            points: 99,
        },
        Bonus {
            bonus_type: "longest_path".into(),
            label: "Longest Path".into(),
            points: 7,
        },
    ];
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(57, rs, mini_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    eng.set_player_trains(0, 3);
    eng.set_player_hand(0, vec![CardColor::Red]);
    let mut events = eng
        .apply(Action::ClaimRoute {
            player_index: 0,
            route_id: "A_B_1".into(),
            cards_discarded: vec![CardColor::Red],
            passengers: Default::default(),
        })
        .unwrap();
    while eng.phase() != GamePhase::GameOver {
        events.extend(apply_events(&mut eng, DrawSource::Deck));
    }
    let bonus = events.iter().find_map(|e| match e {
        GameEvent::GameOver {
            longest_path_bonus, ..
        } => Some(*longest_path_bonus),
        _ => None,
    });
    assert_eq!(bonus, Some(7));
}

// ============================================================
// Ordine di ticket_decks e RNG
// ============================================================

fn two_deck_map() -> MapData {
    let mut map = mini_map();
    for i in 0..3 {
        let mut t = ticket(&format!("long_{i}"), "A", "F", 20);
        t.deck = "long".into();
        map.tickets.push(t);
    }
    map
}

fn two_deck_ruleset(order: [&str; 2]) -> Ruleset {
    let mut rs = mini_ruleset();
    rs.ticket_decks = order.iter().map(|d| (*d).to_string()).collect();
    rs.initial_tickets.dealt = vec![
        TicketDealtEntry {
            deck: "long".into(),
            count: 1,
            keep_max: None,
        },
        TicketDealtEntry {
            deck: "regular".into(),
            count: 3,
            keep_max: None,
        },
    ];
    rs
}

fn initial_offers(rs: Ruleset) -> Vec<Vec<String>> {
    let (ids, names, colors) = two_players();
    let eng = Engine::new(7, rs, two_deck_map(), ids, names, colors, 0).unwrap();
    (0..2)
        .map(|pi| eng.initial_tickets_for(pi).unwrap().to_vec())
        .collect()
}

/// L'ordine di `ticket_decks` fissa l'ordine degli shuffle dell'RNG: cambiarlo
/// cambia i biglietti distribuiti e quindi il replay dei save esistenti.
#[test]
fn ticket_decks_order_determines_initial_tickets() {
    let regular_first = initial_offers(two_deck_ruleset(["regular", "long"]));
    let expected: Vec<Vec<String>> = GOLDEN_REGULAR_FIRST
        .iter()
        .map(|offer| offer.iter().map(|t| (*t).to_string()).collect())
        .collect();
    assert_eq!(regular_first, expected);

    let long_first = initial_offers(two_deck_ruleset(["long", "regular"]));
    let expected: Vec<Vec<String>> = GOLDEN_LONG_FIRST
        .iter()
        .map(|offer| offer.iter().map(|t| (*t).to_string()).collect())
        .collect();
    assert_eq!(long_first, expected);
    assert_ne!(regular_first, long_first);
}

const GOLDEN_REGULAR_FIRST: [[&str; 4]; 2] = [
    ["long_2", "E_F", "dummy_9", "dummy_2"],
    ["long_0", "dummy_3", "dummy_4", "dummy_8"],
];
const GOLDEN_LONG_FIRST: [[&str; 4]; 2] = [
    ["long_0", "dummy_6", "dummy_13", "dummy_14"],
    ["long_1", "dummy_3", "dummy_4", "dummy_8"],
];

// ============================================================
// Coerenza di mappa, ruleset e giocatori (E2, E4, E5)
// ============================================================

fn dense_map(cities: usize) -> MapData {
    let ids: Vec<String> = (0..cities).map(|i| format!("c{i}")).collect();
    let mut routes = Vec::new();
    for a in 0..cities {
        for b in (a + 1)..cities {
            routes.push(route(
                &format!("{}_{}_1", ids[a], ids[b]),
                &ids[a],
                &ids[b],
                1,
                "Gray",
                false,
            ));
        }
    }
    let mut map = mini_map();
    map.cities = ids.iter().map(|id| city(id)).collect();
    map.routes = routes;
    for t in &mut map.tickets {
        t.from = "c0".into();
        t.to = "c1".into();
    }
    map
}

#[test]
fn engine_new_rejects_route_to_unknown_city() {
    let mut map = mini_map();
    map.routes[0].to = "Z".into();
    assert_eq!(
        init_error(mini_ruleset(), map, 2, 0),
        EngineInitError::RouteCityUnknown {
            route_id: "A_B_1".into(),
            city_id: "Z".into()
        }
    );
}

#[test]
fn engine_new_rejects_route_same_city() {
    let mut map = mini_map();
    map.routes[0].to = "A".into();
    assert_eq!(
        init_error(mini_ruleset(), map, 2, 0),
        EngineInitError::RouteSameCity {
            route_id: "A_B_1".into(),
            city_id: "A".into()
        }
    );
}

#[test]
fn engine_new_rejects_ticket_to_unknown_city() {
    let mut map = mini_map();
    map.tickets[0].from = "Z".into();
    assert_eq!(
        init_error(mini_ruleset(), map, 2, 0),
        EngineInitError::TicketCityUnknown {
            ticket_id: "A_D".into(),
            city_id: "Z".into()
        }
    );
}

#[test]
fn engine_new_rejects_ticket_with_same_endpoints() {
    let mut map = mini_map();
    map.tickets[0].to = "A".into();
    assert_eq!(
        init_error(mini_ruleset(), map, 2, 0),
        EngineInitError::TicketSameCity {
            ticket_id: "A_D".into(),
            city_id: "A".into()
        }
    );
}

#[test]
fn engine_new_rejects_duplicate_ticket_id() {
    let mut map = mini_map();
    map.tickets[1].id = "A_D".into();
    assert_eq!(
        init_error(mini_ruleset(), map, 2, 0),
        EngineInitError::DuplicateTicketId {
            ticket_id: "A_D".into()
        }
    );
}

#[test]
fn engine_new_rejects_duplicate_city_id() {
    let mut map = mini_map();
    map.cities.push(city("A"));
    assert_eq!(
        init_error(mini_ruleset(), map, 2, 0),
        EngineInitError::DuplicateCityId {
            city_id: "A".into()
        }
    );
}

#[test]
fn engine_new_rejects_duplicate_route_id() {
    let mut map = mini_map();
    map.routes.push(map.routes[0].clone());
    assert_eq!(
        init_error(mini_ruleset(), map, 2, 0),
        EngineInitError::DuplicateRouteId {
            route_id: "A_B_1".into()
        }
    );
}

#[test]
fn engine_new_rejects_double_route_without_pair_suffix() {
    let mut map = double_route_map();
    map.routes[0].id = "A_B_first".into();
    assert_eq!(
        init_error(mini_ruleset(), map, 2, 0),
        EngineInitError::DoubleRouteBadId {
            route_id: "A_B_first".into()
        }
    );
}

#[test]
fn engine_new_rejects_double_route_with_mismatched_sister() {
    let mut longer = double_route_map();
    longer.routes[1].len = 3;
    let mut other_ends = double_route_map();
    other_ends.routes[1].to = "C".into();
    let mut single = double_route_map();
    single.routes.remove(1);
    let mut not_double = double_route_map();
    not_double.routes[1].double = false;
    for map in [longer, other_ends, single, not_double] {
        assert!(
            matches!(
                init_error(mini_ruleset(), map, 2, 0),
                EngineInitError::DoubleRouteUnpaired { .. }
            ),
            "una coppia incoerente va rifiutata"
        );
    }
}

#[test]
fn engine_new_rejects_route_length_zero() {
    let mut map = mini_map();
    map.routes[0].len = 0;
    assert_eq!(
        init_error(mini_ruleset(), map, 2, 0),
        EngineInitError::RouteLengthZero {
            route_id: "A_B_1".into()
        }
    );
}

#[test]
fn engine_new_rejects_route_length_missing_from_route_points() {
    let mut rs = mini_ruleset();
    rs.route_points.remove("6");
    assert_eq!(
        init_error(rs, mini_map(), 2, 0),
        EngineInitError::RouteLengthUnscored {
            route_id: "A_F_1".into(),
            len: 6
        }
    );
}

#[test]
fn engine_new_rejects_zero_trains_per_player() {
    let mut rs = mini_ruleset();
    rs.trains_per_player = 0;
    assert_eq!(
        init_error(rs, mini_map(), 2, 0),
        EngineInitError::ZeroTrainsPerPlayer
    );
}

#[test]
fn validate_rejects_empty_ruleset_name() {
    let mut rs = mini_ruleset();
    rs.name = "  ".into();
    assert_eq!(
        rs.validate(&mini_map()),
        Err(EngineInitError::EmptyRulesetName)
    );
}

#[test]
fn engine_new_rejects_duplicate_player_ids() {
    let (_, names, colors) = two_players();
    let err = Engine::new(
        1,
        mini_ruleset(),
        mini_map(),
        vec!["p1".into(), "p1".into()],
        names,
        colors,
        0,
    )
    .err()
    .unwrap();
    assert_eq!(
        err,
        EngineInitError::DuplicatePlayerId {
            player_id: "p1".into()
        }
    );
}

#[test]
fn engine_new_rejects_duplicate_player_colors() {
    let (ids, names, _) = two_players();
    let red = ttr_proto::PlayerColor::Red;
    let err = Engine::new(1, mini_ruleset(), mini_map(), ids, names, vec![red, red], 0)
        .err()
        .unwrap();
    assert_eq!(
        err,
        EngineInitError::DuplicatePlayerColor {
            color: "Red".into()
        }
    );
}

#[test]
fn engine_new_rejects_ticket_draw_count_zero() {
    let mut rs = mini_ruleset();
    rs.ticket_draw.count = 0;
    rs.ticket_draw.keep_min = 0;
    assert_eq!(
        init_error(rs, mini_map(), 2, 0),
        EngineInitError::TicketDrawCountZero
    );
}

#[test]
fn engine_new_rejects_too_many_locomotives() {
    let mut rs = mini_ruleset();
    rs.deck.per_color = 5;
    assert_eq!(
        init_error(rs.clone(), mini_map(), 2, 0),
        EngineInitError::TrainDeckLocomotiveHeavy {
            per_color: 5,
            locomotives: 14
        }
    );
    rs.deck.per_color = 6;
    Engine::new(
        1,
        rs,
        mini_map(),
        two_players().0,
        two_players().1,
        two_players().2,
        0,
    )
    .unwrap();
}

#[test]
fn engine_new_rejects_map_too_dense_for_longest_path() {
    let err = init_error(mini_ruleset(), dense_map(10), 2, 0);
    assert!(
        matches!(err, EngineInitError::LongestPathTooDense { .. }),
        "{err:?}"
    );
}

#[test]
fn engine_new_accepts_dense_map_when_trains_limit_claims() {
    let mut rs = mini_ruleset();
    rs.trains_per_player = 12;
    let (ids, names, colors) = two_players();
    Engine::new(1, rs, dense_map(10), ids, names, colors, 0).unwrap();
}

/// Highest estimate the shipped USA map may have: `MAX_CYCLOMATIC_NUMBER` with margin.
#[test]
fn usa_longest_path_estimate_is_within_the_threshold() {
    let estimate = usa_ruleset().max_cyclomatic_estimate(&usa_map());
    assert!(
        estimate < ttr_core::scoring::MAX_CYCLOMATIC_NUMBER,
        "stima USA {estimate}"
    );
}

/// The USA estimate is pinned so that a map change shows up here.
#[test]
fn usa_density_estimate_is_pinned() {
    assert_eq!(
        usa_ruleset().max_cyclomatic_estimate(&usa_map()),
        13,
        "update the pinned value only after verifying time and exactness of the longest \
         path search (e.g. run the `--ignored` timing test in release)"
    );
}

/// Route lengths of the USA map, cycled over the routes of the synthetic maps.
const USA_LENGTHS: [u32; 12] = [2, 3, 2, 4, 2, 1, 3, 5, 2, 6, 3, 4];

/// Planar grid of `side` x `side` cities without two opposite corners (47 cities for
/// `side` 7) whose first `doubled` links are double routes, named `{prefix}...`.
fn grid_routes(prefix: &str, side: usize, doubled: usize) -> (Vec<City>, Vec<Route>) {
    let skipped = |x: usize, y: usize| (x, y) == (0, 0) || (x, y) == (side - 1, side - 1);
    let name = |x: usize, y: usize| format!("{prefix}{x}_{y}");
    let mut cities = Vec::new();
    let mut routes = Vec::new();
    let mut link = 0;
    for x in 0..side {
        for y in 0..side {
            if skipped(x, y) {
                continue;
            }
            cities.push(city(&name(x, y)));
            for (nx, ny) in [(x + 1, y), (x, y + 1)] {
                if nx >= side || ny >= side || skipped(nx, ny) {
                    continue;
                }
                let len = USA_LENGTHS[link % USA_LENGTHS.len()];
                let (from, to) = (name(x, y), name(nx, ny));
                if link < doubled {
                    for n in [1, 2] {
                        routes.push(route(
                            &format!("{from}_{to}_{n}"),
                            &from,
                            &to,
                            len,
                            "Gray",
                            true,
                        ));
                    }
                } else {
                    routes.push(route(
                        &format!("{from}_{to}_1"),
                        &from,
                        &to,
                        len,
                        "Gray",
                        false,
                    ));
                }
                link += 1;
            }
        }
    }
    (cities, routes)
}

fn grid_map(components: usize) -> MapData {
    let mut map = usa_map();
    map.cities.clear();
    map.routes.clear();
    for component in 0..components {
        let (cities, routes) = grid_routes(&format!("g{component}"), 7, 20);
        map.cities.extend(cities);
        map.routes.extend(routes);
    }
    for t in &mut map.tickets {
        t.from = "g00_1".into();
        t.to = "g01_0".into();
    }
    map
}

/// A 47-city, 100-route planar grid, the shape of Europe, passes with a margin, and a
/// disjoint copy of it does not change the estimate: the bound is per connected component.
#[test]
fn planar_grid_estimate_is_per_component() {
    let rs = usa_ruleset();
    let single = grid_map(1);
    assert_eq!((single.cities.len(), single.routes.len()), (47, 100));
    let estimate = rs.max_cyclomatic_estimate(&single);
    assert!(
        estimate < ttr_core::scoring::MAX_CYCLOMATIC_NUMBER,
        "stima griglia {estimate}"
    );
    rs.validate(&single).unwrap();

    let doubled = grid_map(2);
    assert_eq!(doubled.cities.len(), 94);
    assert_eq!(rs.max_cyclomatic_estimate(&doubled), estimate);
    rs.validate(&doubled).unwrap();
}

#[test]
fn huge_locomotive_count_is_rejected_without_building_the_deck() {
    let mut rs = mini_ruleset();
    rs.deck.locomotives = 1_000_000_000;
    assert!(matches!(
        init_error(rs, mini_map(), 2, 0),
        EngineInitError::TrainDeckLocomotiveHeavy { .. }
    ));
}

/// Gli scarti del setup di un mazzo diverso da quello di pesca non rientrano nella pila.
#[test]
fn setup_discards_of_other_ticket_decks_stay_out_of_draw_pile() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(
        7,
        two_deck_ruleset(["regular", "long"]),
        two_deck_map(),
        ids,
        names,
        colors,
        0,
    )
    .unwrap();
    let is_regular = |id: &String| !id.starts_with("long_");
    let mut returned = 0;
    for pi in 0..2 {
        let offered = eng.initial_tickets_for(pi).unwrap().to_vec();
        returned += offered.iter().take(2).filter(|t| is_regular(t)).count();
        eng.apply(Action::ChooseTickets {
            player_index: pi,
            keep: offered[2..].to_vec(),
        })
        .unwrap();
    }
    assert!(eng.ticket_pile().iter().all(is_regular));
    // 20 regular tickets, 6 dealt, plus the regular discards.
    assert_eq!(eng.ticket_pile().len(), 20 - 6 + returned);
    assert!(
        returned < 4,
        "il test deve scartare almeno un biglietto long"
    );
}

// ============================================================
// Modalità: set di biglietti, keepMax, globetrotter, ticket_choice
// ============================================================

/// Mega-Europe in piccolo: 2 biglietti long (tenerne al massimo 1) + 5 regular, keepMin 3.
fn mega_like() -> (Ruleset, MapData) {
    let mut rs = mini_ruleset();
    rs.players.max = 2;
    rs.ticket_decks = vec!["long".into(), "regular".into()];
    rs.initial_tickets = InitialTickets {
        choose_from: None,
        count: 0,
        dealt: vec![
            TicketDealtEntry {
                deck: "long".into(),
                count: 2,
                keep_max: Some(1),
            },
            TicketDealtEntry {
                deck: "regular".into(),
                count: 5,
                keep_max: None,
            },
        ],
        keep_min: 3,
    };
    let mut map = mini_map();
    for i in 0..4 {
        let mut t = ticket(&format!("long_{i}"), "A", "F", 20);
        t.deck = "long".into();
        map.tickets.push(t);
    }
    (rs, map)
}

fn is_long(id: &str) -> bool {
    id.starts_with("long_")
}

fn mega_engine(first_player: usize) -> Engine {
    let (rs, map) = mega_like();
    let (ids, names, colors) = two_players();
    Engine::new(11, rs, map, ids, names, colors, first_player).unwrap()
}

/// Offerta di `pi`, divisa in (long, regular) nell'ordine di offerta.
fn offer_split(eng: &Engine, pi: usize) -> (Vec<String>, Vec<String>) {
    eng.initial_tickets_for(pi)
        .unwrap()
        .iter()
        .cloned()
        .partition(|id| is_long(id))
}

fn keep_of(long: &[String], regular: &[String], n_long: usize, n_regular: usize) -> Vec<String> {
    long.iter()
        .take(n_long)
        .chain(regular.iter().take(n_regular))
        .cloned()
        .collect()
}

#[test]
fn initial_draft_enforces_keep_max_per_deck() {
    let mut eng = mega_engine(0);
    let (long, regular) = offer_split(&eng, 0);
    assert_eq!((long.len(), regular.len()), (2, 5));

    // 2 long + 1 regular = 3 tenuti (keepMin ok) ma 2 long superano keepMax 1.
    let err = eng
        .apply(Action::ChooseTickets {
            player_index: 0,
            keep: keep_of(&long, &regular, 2, 1),
        })
        .unwrap_err();
    assert!(
        matches!(
            err,
            EngineError::TicketChoiceAboveDeckMax { ref deck, kept: 2, max: 1 } if deck == "long"
        ),
        "{err:?}"
    );
    assert!(err.to_string().contains("massimo"), "{err}");
    // Lo stato non cambia: la scelta è ancora da fare.
    assert!(eng.legal_moves(0).ticket_choice.is_some());
    assert_eq!(eng.initial_tickets_for(0).unwrap().len(), 7);

    // 0 long + 2 regular: sotto keepMin 3.
    let err = eng
        .apply(Action::ChooseTickets {
            player_index: 0,
            keep: keep_of(&long, &regular, 0, 2),
        })
        .unwrap_err();
    assert!(matches!(
        err,
        EngineError::TicketChoiceBelowMin { kept: 2, min: 3 }
    ));

    // 1 long + 2 regular: ok.
    eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep: keep_of(&long, &regular, 1, 2),
    })
    .unwrap();
    assert_eq!(eng.player(0).unwrap().tickets.len(), 3);

    // 0 long + 3 regular: ok.
    let (long, regular) = offer_split(&eng, 1);
    eng.apply(Action::ChooseTickets {
        player_index: 1,
        keep: keep_of(&long, &regular, 0, 3),
    })
    .unwrap();
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
}

#[test]
fn legal_moves_list_ticket_choice_to_every_player_who_has_not_drafted() {
    // Il giocatore 1 è di turno: il giocatore 0 non lo è, ma sceglie comunque.
    let mut eng = mega_engine(1);
    assert_eq!(eng.phase(), GamePhase::InitialDraft);
    for pi in 0..2 {
        let (long, _) = offer_split(&eng, pi);
        let lm = eng.legal_moves(pi);
        let choice = lm.ticket_choice.expect("ticket_choice in InitialDraft");
        assert_eq!(choice.keep_min, 3);
        assert_eq!(choice.limits.len(), 1);
        assert_eq!(choice.limits[0].ticket_ids, long);
        assert_eq!(choice.limits[0].max, 1);
        // Nessun'altra mossa è esposta.
        assert!(lm.routes.is_empty() && lm.draw_face_up.is_empty());
        assert!(!lm.can_draw_deck && !lm.can_draw_tickets);
        assert!(lm.station_cities.is_empty() && lm.station_cost == 0 && lm.tunnel.is_none());
    }
    // Fuori dai giocatori: nulla.
    assert!(eng.legal_moves(2).ticket_choice.is_none());

    // La mossa elencata è accettata.
    let (long, regular) = offer_split(&eng, 0);
    let choice = eng.legal_moves(0).ticket_choice.unwrap();
    eng.clone()
        .apply(Action::ChooseTickets {
            player_index: 0,
            keep: keep_of(
                &long,
                &regular,
                choice.limits[0].max as usize,
                choice.keep_min as usize - 1,
            ),
        })
        .unwrap();

    // Dopo la scelta del giocatore 0 resta solo quella del giocatore 1.
    eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep: keep_of(&long, &regular, 1, 2),
    })
    .unwrap();
    assert!(eng.legal_moves(0).ticket_choice.is_none());
    assert!(eng.legal_moves(1).ticket_choice.is_some());
}

#[test]
fn legal_moves_ticket_choice_without_limits_and_mid_game() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(5, mini_ruleset(), mini_map(), ids, names, colors, 0).unwrap();
    let choice = eng.legal_moves(1).ticket_choice.unwrap();
    assert_eq!(choice.keep_min, 2);
    assert!(choice.limits.is_empty());
    complete_initial_draft(&mut eng);
    assert!(eng.legal_moves(0).ticket_choice.is_none());

    eng.apply(draw_tickets(0)).unwrap();
    assert_eq!(eng.phase(), GamePhase::TicketChoice);
    let lm = eng.legal_moves(0);
    let choice = lm.ticket_choice.expect("ticket_choice in TicketChoice");
    assert_eq!(choice.keep_min, 1);
    assert!(choice.limits.is_empty());
    assert!(!lm.can_draw_deck && lm.routes.is_empty());
    assert!(eng.legal_moves(1).ticket_choice.is_none());
}

/// Mappa con biglietti fuori set mescolati a quelli in gioco.
fn set_filter_map() -> MapData {
    let mut map = mini_map();
    let mut mixed: Vec<TicketDef> = Vec::new();
    for i in 0..10 {
        let mut t = ticket(&format!("off_{i}"), "A", "B", 1);
        t.sets = vec!["other".into()];
        mixed.push(t);
    }
    let mut both = ticket("both_0", "A", "B", 1);
    both.sets = vec!["other".into(), "mini".into()];
    mixed.push(both);
    mixed.append(&mut map.tickets);
    map.tickets = mixed;
    map
}

#[test]
fn ticket_set_filter_never_deals_an_out_of_set_ticket() {
    let map = set_filter_map();
    let in_play = 21; // 20 di mini_map + both_0
    for seed in 0..20u64 {
        let (ids, names, colors) = two_players();
        let mut eng =
            Engine::new(seed, mini_ruleset(), map.clone(), ids, names, colors, 0).unwrap();
        let mut seen: Vec<String> = eng.ticket_pile().to_vec();
        for pi in 0..2 {
            seen.extend(eng.initial_tickets_for(pi).unwrap().iter().cloned());
        }
        assert_eq!(seen.len(), in_play, "seed {seed}");
        assert!(seen.iter().all(|t| !t.starts_with("off_")), "seed {seed}");

        complete_initial_draft(&mut eng);
        eng.apply(draw_tickets(0)).unwrap();
        assert!(
            eng.pending_tickets().iter().all(|t| !t.starts_with("off_")),
            "seed {seed}"
        );
    }
}

#[test]
fn ticket_set_filter_drives_deck_validation() {
    // Il set "other" ha 11 biglietti: bastano per 2 giocatori ma non per 5 (20 necessari).
    let mut rs = mini_ruleset();
    rs.ticket_set = "other".into();
    let map = set_filter_map();
    assert_eq!(
        rs.validate(&map),
        Err(EngineInitError::TicketDeckTooSmall {
            deck: "regular".into(),
            have: 11,
            needed: 20
        })
    );

    // Un set senza biglietti è un errore dedicato.
    rs.ticket_set = "nessuno".into();
    let expected = EngineInitError::EmptyTicketSet {
        set: "nessuno".into(),
    };
    assert_eq!(rs.validate(&map), Err(expected.clone()));
    assert_eq!(init_error(rs, map.clone(), 2, 0), expected);

    // Un biglietto fuori set non può far fallire i controlli sui deck del set.
    let mut map = mini_map();
    let mut t = ticket("fuori", "A", "B", 1);
    t.deck = "long".into();
    t.sets = vec!["other".into()];
    map.tickets.push(t);
    mini_ruleset().validate(&map).unwrap();
}

#[test]
fn init_validates_mode_sets_big_cities_and_keep_rules() {
    let mut rs = mini_ruleset();
    rs.mode = " ".into();
    assert_eq!(
        rs.validate(&mini_map()),
        Err(EngineInitError::EmptyRulesetMode)
    );

    let mut map = mini_map();
    map.tickets[2].sets = vec![];
    let expected = EngineInitError::TicketSetsEmpty {
        ticket_id: "B_D".into(),
    };
    assert_eq!(init_error(mini_ruleset(), map.clone(), 2, 0), expected);
    assert_eq!(mini_ruleset().validate(&map), Err(expected));

    let mut map = mini_map();
    map.tickets[2].sets = vec!["mini".into(), "mini".into()];
    let expected = EngineInitError::DuplicateTicketSet {
        ticket_id: "B_D".into(),
        set: "mini".into(),
    };
    assert_eq!(init_error(mini_ruleset(), map, 2, 0), expected);

    let mut rs = mini_ruleset();
    rs.big_cities = vec!["A".into(), "Z".into()];
    let expected = EngineInitError::BigCityUnknown {
        city_id: "Z".into(),
    };
    assert_eq!(init_error(rs.clone(), mini_map(), 2, 0), expected);
    rs.big_cities = vec!["A".into(), "B".into(), "A".into()];
    let expected = EngineInitError::DuplicateBigCity {
        city_id: "A".into(),
    };
    assert_eq!(init_error(rs.clone(), mini_map(), 2, 0), expected);
    rs.big_cities = vec!["A".into(), "B".into()];
    rs.validate(&mini_map()).unwrap();

    let mut rs = mini_ruleset();
    rs.bonuses.push(rs.bonuses[0].clone());
    let expected = EngineInitError::DuplicateBonusType {
        bonus_type: "longest_path".into(),
    };
    assert_eq!(init_error(rs.clone(), mini_map(), 2, 0), expected);
    // Nessun bonus: ammesso.
    rs.bonuses.clear();
    rs.validate(&mini_map()).unwrap();

    let (mut rs, map) = mega_like();
    rs.validate(&map).unwrap();
    rs.initial_tickets.dealt[0].keep_max = Some(3);
    let expected = EngineInitError::DealtKeepMaxExceedsCount {
        deck: "long".into(),
        keep_max: 3,
        count: 2,
    };
    assert_eq!(init_error(rs.clone(), map.clone(), 2, 0), expected);
    assert!(expected.to_string().contains("keepMax"));
    // 1 long tenibile + 5 regular = 6 < keepMin 7 (ma 7 sono distribuiti).
    rs.initial_tickets.dealt[0].keep_max = Some(1);
    rs.initial_tickets.keep_min = 7;
    let expected = EngineInitError::InitialKeepMinUnreachable {
        keep_min: 7,
        max_keep: 6,
    };
    assert_eq!(init_error(rs.clone(), map.clone(), 2, 0), expected);
    assert_eq!(rs.validate(&map), Err(expected));
}

/// Mini mappa con un biglietto B-C in più, per far completare un biglietto al giocatore 1.
fn bonus_map() -> MapData {
    let mut map = mini_map();
    map.tickets.push(ticket("B_C_t", "B", "C", 3));
    map
}

/// Gioca l'ultimo giro: il giocatore 0 prende A-B (innesca la fine), il 1 prende B-C.
/// Restituisce l'evento `GameOver` con le tessere `tickets` assegnate ai due giocatori.
fn finish_game(rs: Ruleset, tickets: [&[&str]; 2]) -> GameEvent {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(57, rs, bonus_map(), ids, names, colors, 0).unwrap();
    complete_initial_draft(&mut eng);
    for (pi, ids) in tickets.iter().enumerate() {
        eng.set_player_tickets(pi, ids);
    }
    eng.set_player_trains(0, 3);
    eng.set_player_hand(0, vec![CardColor::Red]);
    eng.set_player_hand(1, vec![CardColor::Blue, CardColor::Blue]);
    let mut events = eng
        .apply(Action::ClaimRoute {
            player_index: 0,
            route_id: "A_B_1".into(),
            cards_discarded: vec![CardColor::Red],
            passengers: Default::default(),
        })
        .unwrap();
    events.extend(
        eng.apply(Action::ClaimRoute {
            player_index: 1,
            route_id: "B_C_1".into(),
            cards_discarded: vec![CardColor::Blue, CardColor::Blue],
            passengers: Default::default(),
        })
        .unwrap(),
    );
    while eng.phase() != GamePhase::GameOver {
        events.extend(apply_events(&mut eng, DrawSource::Deck));
    }
    events
        .into_iter()
        .find(|e| matches!(e, GameEvent::GameOver { .. }))
        .expect("evento GameOver")
}

fn globetrotter_ruleset(with_longest_path: bool) -> Ruleset {
    let mut rs = mini_ruleset();
    rs.bonuses = vec![Bonus {
        bonus_type: "globetrotter".into(),
        label: "Globetrotter".into(),
        points: 15,
    }];
    if with_longest_path {
        rs.bonuses.push(Bonus {
            bonus_type: "longest_path".into(),
            label: "Longest Path".into(),
            points: 10,
        });
    }
    rs
}

fn game_over_scores(event: &GameEvent) -> &[ttr_core::event::FinalScore] {
    match event {
        GameEvent::GameOver { final_scores, .. } => final_scores,
        _ => unreachable!(),
    }
}

#[test]
fn globetrotter_goes_to_the_player_with_most_completed_tickets() {
    let event = finish_game(
        globetrotter_ruleset(false),
        [&["dummy_0", "dummy_1"], &["B_C_t"]],
    );
    let scores = game_over_scores(&event);
    assert_eq!(
        scores
            .iter()
            .map(|s| s.globetrotter_bonus)
            .collect::<Vec<_>>(),
        [15, 0]
    );
    for s in scores {
        assert_eq!(
            s.total,
            s.route_points
                + s.ticket_completed
                + s.ticket_failed
                + s.longest_path_bonus
                + s.globetrotter_bonus
                + s.station_points
        );
    }
    assert!(scores[0].winner && !scores[1].winner);
}

#[test]
fn globetrotter_tie_gives_the_bonus_to_everyone_tied() {
    let event = finish_game(globetrotter_ruleset(false), [&["dummy_0"], &["B_C_t"]]);
    let scores = game_over_scores(&event);
    assert_eq!(
        scores
            .iter()
            .map(|s| s.globetrotter_bonus)
            .collect::<Vec<_>>(),
        [15, 15]
    );
}

#[test]
fn globetrotter_is_not_awarded_when_nobody_completed_a_ticket() {
    let event = finish_game(globetrotter_ruleset(false), [&["A_F"], &["A_D"]]);
    let scores = game_over_scores(&event);
    assert!(scores.iter().all(|s| s.globetrotter_bonus == 0));
    // Un ruleset senza il bonus non lo assegna mai.
    let event = finish_game(mini_ruleset(), [&["dummy_0"], &["A_D"]]);
    assert!(
        game_over_scores(&event)
            .iter()
            .all(|s| s.globetrotter_bonus == 0)
    );
}

#[test]
fn ruleset_without_longest_path_scores_no_path_bonus() {
    let event = finish_game(globetrotter_ruleset(false), [&["dummy_0"], &["B_C_t"]]);
    let GameEvent::GameOver {
        final_scores,
        longest_path_players,
        longest_path_bonus,
    } = &event
    else {
        unreachable!()
    };
    assert!(longest_path_players.is_empty());
    assert_eq!(*longest_path_bonus, 0);
    assert!(final_scores.iter().all(|s| s.longest_path_bonus == 0));

    // Con il bonus, invece, entrambi i percorsi (1 e 2 treni) contano: vince il più lungo.
    let event = finish_game(globetrotter_ruleset(true), [&["dummy_0"], &["B_C_t"]]);
    let GameEvent::GameOver {
        longest_path_players,
        longest_path_bonus,
        final_scores,
    } = &event
    else {
        unreachable!()
    };
    assert_eq!(longest_path_players, &[1]);
    assert_eq!(*longest_path_bonus, 10);
    assert_eq!(final_scores[1].longest_path_bonus, 10);

    // Senza alcun bonus il ruleset è ammesso e la partita finisce.
    let mut rs = mini_ruleset();
    rs.bonuses.clear();
    let event = finish_game(rs, [&["dummy_0"], &["B_C_t"]]);
    assert!(
        game_over_scores(&event)
            .iter()
            .all(|s| { s.longest_path_bonus == 0 && s.globetrotter_bonus == 0 })
    );
}

#[test]
fn globetrotter_winners_handles_ties_and_zero() {
    use ttr_core::scoring::{PlayerScore, globetrotter_winners};
    let score = |completed_count: u32| PlayerScore {
        route_points: 0,
        completed_tickets: 0,
        completed_count,
        failed_tickets: 0,
        longest_path: 0,
        longest_path_exact: true,
        longest_path_routes: Vec::new(),
        borrowed_routes: Vec::new(),
    };
    let winners = |counts: &[u32]| {
        let scores: Vec<PlayerScore> = counts.iter().map(|&c| score(c)).collect();
        globetrotter_winners(&scores)
    };
    assert_eq!(winners(&[1, 3, 2]), [1]);
    assert_eq!(winners(&[2, 3, 3]), [1, 2]);
    assert!(winners(&[0, 0, 0]).is_empty());
    assert!(winners(&[]).is_empty());
}
