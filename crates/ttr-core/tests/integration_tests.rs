//! Test di integrazione per il motore CiufCiuf! (ttr-core).
//!
//! Copre la checklist del piano ciufciuf-v1.3.md §3.3, replay e fuzzing.

use std::collections::BTreeMap;

use ttr_core::error::GamePhase;
use ttr_core::types::{
    Bonus, City, ClaimedRouteInfo, DeckConfig, InitialTickets, MapData, PlayersConfig, Route,
    Ruleset, SetupDiscards, TicketDealtEntry, TicketDef, TicketDraw,
};
use ttr_core::{Action, DrawSource, Engine, EngineError, GameEvent};
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
    }
}

fn ticket(id: &str, from: &str, to: &str, points: u32) -> TicketDef {
    TicketDef {
        id: id.into(),
        from: from.into(),
        to: to.into(),
        points,
        deck: "regular".into(),
    }
}

fn city(id: &str) -> City {
    City {
        id: id.into(),
        name: id.into(),
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
        map: "mini".into(),
        players: PlayersConfig { min: 2, max: 5 },
        trains_per_player: 45,
        deck: DeckConfig {
            per_color: 12,
            locomotives: 14,
        },
        ticket_decks: vec!["regular".into()],
        initial_tickets: InitialTickets {
            dealt: vec![TicketDealtEntry {
                deck: "regular".into(),
                count: 4,
            }],
            keep_min: 2,
        },
        ticket_draw: TicketDraw {
            deck: "regular".into(),
            count: 3,
            keep_min: 1,
        },
        setup_discards: SetupDiscards::ReshuffleBottom,
        route_points: rp,
        bonuses: vec![Bonus {
            bonus_type: "longestPath".into(),
            label: "Longest Path".into(),
            points: 10,
        }],
        double_route_min_players: 4,
        final_round_includes_trigger: true,
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
    let mut eng = Engine::new(42, mini_ruleset(), mini_map(), ids, names, colors, 0);
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
    let mut eng = Engine::new(43, mini_ruleset(), mini_map(), ids, names, colors, 0);
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
    let mut eng = Engine::new(44, mini_ruleset(), mini_map(), ids, names, colors, 0);
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
    let mut eng = Engine::new(47, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);

    eng.apply(Action::DrawTickets { player_index: 0 }).unwrap();

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
    );
    complete_initial_draft(&mut eng);

    eng.set_player_hand(0, vec![CardColor::Red, CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red, CardColor::Red],
    })
    .unwrap();

    eng.set_player_hand(1, vec![CardColor::Blue, CardColor::Blue]);
    let r = eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "A_B_2".into(),
        cards_discarded: vec![CardColor::Blue, CardColor::Blue],
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
    );
    complete_initial_draft(&mut eng);

    eng.set_player_hand(0, vec![CardColor::Red, CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red, CardColor::Red],
    })
    .unwrap();

    eng.set_player_hand(1, vec![CardColor::Blue, CardColor::Blue]);
    let r = eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "A_B_2".into(),
        cards_discarded: vec![CardColor::Blue, CardColor::Blue],
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
    );
    complete_initial_draft(&mut eng);

    eng.set_player_hand(0, vec![CardColor::Red, CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red, CardColor::Red],
    })
    .unwrap();

    eng.set_player_hand(1, vec![CardColor::Blue]);
    eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "B_C_1".into(),
        cards_discarded: vec![CardColor::Blue],
    })
    .unwrap();

    eng.set_player_hand(0, vec![CardColor::Green, CardColor::Green]);
    let r = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_2".into(),
        cards_discarded: vec![CardColor::Green, CardColor::Green],
    });
    assert!(matches!(r, Err(EngineError::SamePlayerDouble { .. })));
}

#[test]
fn golden_last_round_includes_trigger() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(51, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);

    eng.set_player_trains(0, 3);
    eng.set_player_hand(0, vec![CardColor::Red]);
    let events = eng
        .apply(Action::ClaimRoute {
            player_index: 0,
            route_id: "A_B_1".into(),
            cards_discarded: vec![CardColor::Red],
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
/// Sequenza attesa (piano §6/D11, §1 blockquote):
/// 1. P1 (ultimo) claim con ≤2 treni → LastRoundAnnounced(trigger=1)
/// 2. Turno finale di P0 → TurnAdvanced
/// 3. Turno finale di P1 → GameOver
///
/// Nessun giro extra.
#[test]
fn golden_last_round_trigger_is_last_player() {
    let (ids, names, colors) = two_players();
    // first_player = 0, quindi l'ordine è P0 → P1
    let mut eng = Engine::new(171, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);
    assert_eq!(eng.current_player(), 0, "P0 inizia");

    // P0 fa un'azione normale (non trigger)
    eng.set_player_hand(0, vec![CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red],
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
    let mut eng = Engine::new(52, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);

    eng.set_player_hand(0, vec![CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red],
    })
    .unwrap();
    assert_eq!(eng.player(0).unwrap().score, 1);

    eng.set_player_hand(1, vec![CardColor::Blue, CardColor::Blue]);
    eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "B_C_1".into(),
        cards_discarded: vec![CardColor::Blue, CardColor::Blue],
    })
    .unwrap();
    assert_eq!(eng.player(1).unwrap().score, 2);

    eng.set_player_hand(0, vec![CardColor::Green, CardColor::Green]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_E_1".into(),
        cards_discarded: vec![CardColor::Green, CardColor::Green],
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
    })
    .unwrap();
    assert_eq!(eng.player(1).unwrap().score, 6);

    eng.set_player_hand(0, vec![CardColor::Red; 6]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_F_1".into(),
        cards_discarded: vec![CardColor::Red; 6],
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
    let mut eng = Engine::new(53, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);

    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    })
    .unwrap();
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);

    let r = eng.apply(Action::DrawTickets { player_index: 0 });
    assert!(r.is_err());
}

#[test]
fn golden_not_your_turn_error() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(54, mini_ruleset(), mini_map(), ids, names, colors, 0);
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
    let mut eng = Engine::new(55, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);

    let r = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "nonexistent".into(),
        cards_discarded: vec![CardColor::Red],
    });
    assert!(matches!(r, Err(EngineError::RouteNotFound { .. })));
}

#[test]
fn golden_insufficient_cards_for_route() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(56, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);

    let r = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![],
    });
    assert!(matches!(r, Err(EngineError::InsufficientCards { .. })));
}

#[test]
fn golden_wrong_color_for_route() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(57, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);
    eng.set_player_hand(0, vec![CardColor::Green]);

    // D_E_1 è Rossa, pago con Verde
    let r = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "D_E_1".into(),
        cards_discarded: vec![CardColor::Green],
    });
    assert!(r.is_err());
}

#[test]
fn golden_initial_draft_must_keep_minimum() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(58, mini_ruleset(), mini_map(), ids, names, colors, 0);

    let r = eng.apply(Action::ChooseTickets {
        player_index: 0,
        keep: vec!["A_D".into()],
    });
    assert!(matches!(r, Err(EngineError::TicketChoiceBelowMin { .. })));
}

#[test]
fn golden_first_player_is_respected() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(59, mini_ruleset(), mini_map(), ids, names, colors, 1);
    complete_initial_draft(&mut eng);
    assert_eq!(eng.current_player(), 1);
}

#[test]
fn golden_initial_draft_parallel() {
    let (ids, names, colors) = three_players();
    let mut eng = Engine::new(60, mini_ruleset(), mini_map(), ids, names, colors, 0);

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

/// F8 / E10: dopo un claim che connette alcune città, un
/// `DrawTickets` + `ChooseTickets` mid-game deve marcare ogni biglietto
/// tenuto con `completed: Some(_)` — mai `None` (E10 fix).
#[test]
fn golden_mid_game_ticket_completed_not_none() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(64, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);

    // Dai a P0 carte sufficienti per un claim: A_B_1 (len=1, Gray).
    eng.set_player_hand(0, vec![CardColor::Red, CardColor::Red]);

    // Claim A_B_1 → i biglietti dummy A→B saranno completati se pescati.
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red],
    })
    .unwrap();

    // Resetta treni per evitare last round.
    eng.set_player_trains(0, 40);

    // Dopo il claim di P0 il turno passa a P1.
    // Fai pescare biglietti a P1 per avanzare il turno e tornare a P0.
    eng.apply(Action::DrawTickets { player_index: 1 }).unwrap();
    let p1_offered = eng.pending_tickets().to_vec();
    let p1_keep = vec![p1_offered[0].clone()];
    eng.apply(Action::ChooseTickets {
        player_index: 1,
        keep: p1_keep,
    })
    .unwrap();

    // DrawTickets mid-game (turno di P0).
    eng.apply(Action::DrawTickets { player_index: 0 }).unwrap();
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

    // Invariante E10: ogni biglietto tenuto ha completed.is_some()
    // (Some(true) se completato, Some(false) altrimenti — mai None).
    let player = eng.player(0).unwrap();
    for tid in &keep {
        let ts = player
            .tickets
            .get(tid)
            .unwrap_or_else(|| panic!("biglietto {} non trovato nel giocatore", tid));
        assert!(
            ts.completed.is_some(),
            "E10: biglietto '{}' ha completed = {:?}, atteso Some(_)",
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
        "E10: almeno un dummy A→B deve risultare completed dopo ChooseTickets (A_B_1 claimato)"
    );
}

// ============================================================
// Sezione 2.5 — legalMoves gate + §3.3 test gap closures
// ============================================================

/// legalMoves in SecondCardDraw: niente rotte, niente ticket draw.
#[test]
fn golden_legal_moves_second_card_draw() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(61, mini_ruleset(), mini_map(), ids, names, colors, 0);
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
    let mut eng = Engine::new(62, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);

    eng.apply(Action::DrawTickets { player_index: 0 }).unwrap();
    assert_eq!(eng.phase(), GamePhase::TicketChoice);

    let lm = eng.legal_moves(0);
    assert!(lm.routes.is_empty(), "no routes in TicketChoice");
    assert!(lm.draw_face_up.is_empty(), "no face-up in TicketChoice");
    assert!(!lm.can_draw_deck, "no deck draw in TicketChoice");
    assert!(!lm.can_draw_tickets, "no ticket draw in TicketChoice");
}

/// R4.1: Esaurimento totale mazzo+scarti: la pescata si completa
/// parzialmente e il turno NON si perde.
#[test]
fn golden_total_deck_exhaustion_no_turn_loss() {
    let (ids, names, colors) = two_players();
    let mut eng = Engine::new(63, mini_ruleset(), mini_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);

    // Svuota mazzo E scarti
    eng.clear_deck_and_discards();

    // La pescata dal mazzo dovrebbe fallire perché non ci sono carte
    let r = eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    });
    assert!(
        matches!(r, Err(EngineError::DeckEmpty)),
        "deck+discards vuoti → DeckEmpty"
    );

    // Il giocatore può comunque reclamare una rotta
    eng.set_player_hand(0, vec![CardColor::Red]);
    let ev = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red],
    });
    assert!(
        ev.is_ok(),
        "può ancora reclamare dopo esaurimento deck: {:?}",
        ev.err()
    );

    // Può anche pescare biglietti
    eng.set_player_trains(1, 45); // reset trains so no last round
    let ev2 = eng.apply(Action::DrawTickets { player_index: 1 });
    assert!(
        ev2.is_ok(),
        "può pescare biglietti dopo esaurimento deck: {:?}",
        ev2.err()
    );
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
    );
    complete_initial_draft(&mut eng);

    // P0 claim A_B_1
    eng.set_player_hand(0, vec![CardColor::Red, CardColor::Red]);
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "A_B_1".into(),
        cards_discarded: vec![CardColor::Red, CardColor::Red],
    })
    .unwrap();

    // P1 claim B_C_1
    eng.set_player_hand(1, vec![CardColor::Blue]);
    eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "B_C_1".into(),
        cards_discarded: vec![CardColor::Blue],
    })
    .unwrap();

    // P2 prova A_B_2 → deve fallire (blocco a 3p)
    eng.set_player_hand(2, vec![CardColor::Green, CardColor::Green]);
    let r = eng.apply(Action::ClaimRoute {
        player_index: 2,
        route_id: "A_B_2".into(),
        cards_discarded: vec![CardColor::Green, CardColor::Green],
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
    let mut eng = Engine::new(65, mini_ruleset(), mini_map(), ids, names, colors, 0);

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
    );
    complete_initial_draft(&mut eng_a);

    let mut eng_b = Engine::new(
        seed,
        mini_ruleset(),
        mini_map(),
        ids.clone(),
        names.clone(),
        colors.clone(),
        0,
    );
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

    let mut eng_a = Engine::new(111, mini_ruleset(), mini_map(), ids_a, names_a, colors_a, 0);
    complete_initial_draft(&mut eng_a);
    let mut eng_b = Engine::new(222, mini_ruleset(), mini_map(), ids_b, names_b, colors_b, 0);
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

/// Gioca una partita self-play casuale. Restituisce (azioni_eseguite, stato).
fn play_random_game(seed: u64, num_players: usize) -> (usize, Engine) {
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    let (ids, names, colors) = match num_players {
        2 => {
            let ids: Vec<_> = (0..2).map(|i| format!("p{}", i + 1)).collect();
            let names: Vec<_> = (0..2).map(|i| format!("P{}", i + 1)).collect();
            (
                ids,
                names,
                vec![ttr_proto::PlayerColor::Red, ttr_proto::PlayerColor::Blue],
            )
        }
        3 => {
            let ids: Vec<_> = (0..3).map(|i| format!("p{}", i + 1)).collect();
            let names: Vec<_> = (0..3).map(|i| format!("P{}", i + 1)).collect();
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
        _ => {
            let ids: Vec<_> = (0..4).map(|i| format!("p{}", i + 1)).collect();
            let names: Vec<_> = (0..4).map(|i| format!("P{}", i + 1)).collect();
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
    };

    let mut eng = Engine::new(seed, usa_ruleset(), usa_map(), ids, names, colors, 0);
    complete_initial_draft(&mut eng);

    let mut rng = StdRng::seed_from_u64(seed ^ 0xDEADBEEF);
    let mut action_count = 0;
    let max_actions = 2000;

    while action_count < max_actions {
        if eng.phase() == GamePhase::GameOver {
            break;
        }

        let pi = eng.current_player();

        match eng.phase() {
            GamePhase::PlayerTurn | GamePhase::LastRound => {
                let lm = eng.legal_moves(pi);
                let mut choices: Vec<Action> = Vec::new();

                if lm.can_draw_deck {
                    choices.push(Action::DrawTrainCard {
                        player_index: pi,
                        source: DrawSource::Deck,
                    });
                }
                for &idx in &lm.draw_face_up {
                    choices.push(Action::DrawTrainCard {
                        player_index: pi,
                        source: DrawSource::FaceUp { index: idx },
                    });
                }
                if lm.can_draw_tickets {
                    choices.push(Action::DrawTickets { player_index: pi });
                }
                for rid in lm.routes.iter().take(10) {
                    if let Some(route) = eng.get_route(rid) {
                        let hand = eng.player(pi).unwrap().hand.clone();
                        if let Some(cards) = build_payment(&hand, &route.color, route.len as usize)
                        {
                            choices.push(Action::ClaimRoute {
                                player_index: pi,
                                route_id: rid.clone(),
                                cards_discarded: cards,
                            });
                        }
                    }
                }

                if choices.is_empty() {
                    break;
                }
                let idx = rng.gen_range(0..choices.len());
                if eng.apply(choices[idx].clone()).is_err() {
                    break;
                }
                action_count += 1;
            }
            GamePhase::SecondCardDraw => {
                let lm = eng.legal_moves(pi);
                let mut choices: Vec<Action> = Vec::new();
                if lm.can_draw_deck {
                    choices.push(Action::DrawTrainCard {
                        player_index: pi,
                        source: DrawSource::Deck,
                    });
                }
                for &idx in &lm.draw_face_up {
                    choices.push(Action::DrawTrainCard {
                        player_index: pi,
                        source: DrawSource::FaceUp { index: idx },
                    });
                }
                if choices.is_empty() {
                    break;
                }
                let idx = rng.gen_range(0..choices.len());
                if eng.apply(choices[idx].clone()).is_err() {
                    break;
                }
                action_count += 1;
            }
            GamePhase::TicketChoice => {
                let offered = eng.pending_tickets().to_vec();
                let keep_min = eng.ticket_draw_keep_min() as usize;
                let keep: Vec<String> = offered.iter().take(keep_min).cloned().collect();
                if eng
                    .apply(Action::ChooseTickets {
                        player_index: pi,
                        keep,
                    })
                    .is_err()
                {
                    break;
                }
                action_count += 1;
            }
            _ => break,
        }
    }

    (action_count, eng)
}

#[test]
fn fuzzing_200_games_invariants() {
    let num_games: usize = std::env::var("FUZZ_GAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);

    for game_idx in 0..num_games {
        let seed = 1000 + game_idx as u64;
        let np = match game_idx % 3 {
            0 => 2,
            1 => 3,
            _ => 4,
        };
        let (actions, eng) = play_random_game(seed, np);

        // Invariante 1: conservazione carte (110)
        let total_cards: usize = eng.draw_pile_len()
            + eng.face_up_cards().iter().filter(|c| c.is_some()).count()
            + eng.discards_len()
            + (0..eng.num_players())
                .map(|pi| eng.player(pi).unwrap().hand.len())
                .sum::<usize>();
        assert_eq!(
            total_cards, 110,
            "game {} (seed {}) conservazione carte: {} != 110",
            game_idx, seed, total_cards
        );

        // Invariante 2: nessuna rotta reclamata due volte
        let mut seen = std::collections::BTreeSet::new();
        for rid in eng.claimed_route_ids() {
            assert!(
                seen.insert(rid.clone()),
                "game {} rotta {} duplicata",
                game_idx,
                rid
            );
        }

        // Invariante 3: punteggi non negativi
        for pi in 0..eng.num_players() {
            assert!(
                eng.player(pi).unwrap().score >= 0,
                "game {} P{} punteggio negativo",
                game_idx,
                pi
            );
        }

        // Invariante 4: termine entro il limite
        assert!(
            actions < 2000,
            "game {} (seed {}) bloccata a {} azioni",
            game_idx,
            seed,
            actions
        );

        // Invariante 5: se è terminata normalmente, fase = GameOver
        assert_eq!(
            eng.phase(),
            GamePhase::GameOver,
            "game {} terminata con fase {:?} (azioni {})",
            game_idx,
            eng.phase(),
            actions
        );
    }
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

    for game_idx in 0..num_games {
        let seed = 2000 + game_idx as u64;
        let np = match game_idx % 4 {
            0 => 2,
            1 => 3,
            _ => 4,
        };
        let (actions, eng) = play_random_game(seed, np);

        let total_cards: usize = eng.draw_pile_len()
            + eng.face_up_cards().iter().filter(|c| c.is_some()).count()
            + eng.discards_len()
            + (0..eng.num_players())
                .map(|pi| eng.player(pi).unwrap().hand.len())
                .sum::<usize>();
        assert_eq!(total_cards, 110, "game {} carte", game_idx);
        assert!(actions < 2000, "game {} bloccata", game_idx);
        assert_eq!(
            eng.phase(),
            GamePhase::GameOver,
            "game {} non finita",
            game_idx
        );
    }
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
