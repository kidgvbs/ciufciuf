//! Locomotive e traghetti come dati del ruleset: pagamento delle rotte con restrizioni e
//! sostituzioni, pesca con `freeDraw`, mercato senza reset, validazione dei campi nuovi.
//! Mappa e ruleset sono costruiti in memoria, indipendenti dai JSON delle mappe.

use serde_json::{Value, json};
use ttr_core::error::GamePhase;
use ttr_core::types::{MapData, Ruleset};
use ttr_core::{Action, DrawSource, Engine, EngineError, EngineInitError, GameEvent};
use ttr_proto::CardColor::{self, Black, Blue, Green, Locomotive, Red};

fn route(id: &str, from: &str, to: &str, len: u32, color: &str) -> Value {
    json!({ "id": id, "from": from, "to": to, "len": len, "color": color, "double": false })
}

fn map_json() -> Value {
    let mut tunnel = route("tunnel_red", "B", "C", 2, "Red");
    tunnel["tunnel"] = json!(true);
    let mut ferry = route("ferry_blue", "C", "D", 3, "Blue");
    ferry["ferries"] = json!(1);
    let mut gray_ferry = route("ferry_gray", "D", "E", 3, "Gray");
    gray_ferry["ferries"] = json!(1);
    let mut long = route("long_gray", "E", "F", 9, "Gray");
    long["anyCardsPerSpace"] = json!(4);
    let tickets: Vec<Value> = (0..20)
        .map(|i| {
            json!({
                "id": format!("t{i}"), "from": "A", "to": "B", "points": 1,
                "deck": "regular", "sets": ["t"]
            })
        })
        .collect();
    let cities: Vec<Value> = ["A", "B", "C", "D", "E", "F", "G"]
        .iter()
        .map(|c| json!({ "id": c, "name": c }))
        .collect();
    json!({
        "cities": cities,
        "routes": [
            route("plain_red", "A", "B", 2, "Red"),
            tunnel,
            ferry,
            gray_ferry,
            long,
            route("plain_gray", "F", "G", 2, "Gray"),
        ],
        "tickets": tickets,
    })
}

fn ruleset_json(locomotives: Option<Value>, ferries: Option<Value>) -> Value {
    let mut rs = json!({
        "name": "Prova",
        "mode": "Standard",
        "ticketSet": "t",
        "map": "t",
        "players": { "min": 2, "max": 2 },
        "trainsPerPlayer": 45,
        "deck": { "perColor": 12, "locomotives": 14 },
        "ticketDecks": ["regular"],
        "initialTickets": { "dealt": [{ "deck": "regular", "count": 4 }], "keepMin": 2 },
        "ticketDraw": { "deck": "regular", "count": 3, "keepMin": 1 },
        "setupDiscards": "removed",
        "routePoints": { "1": 1, "2": 2, "3": 4, "9": 27 },
        "bonuses": [],
        "tunnels": { "revealed": 3 },
        "doubleRouteMinPlayers": 2,
        "finalRoundIncludesTrigger": true
    });
    if let Some(l) = locomotives {
        rs["locomotives"] = l;
    }
    if let Some(f) = ferries {
        rs["ferries"] = f;
    }
    rs
}

fn nordic_rules() -> Value {
    ruleset_json(
        Some(json!({ "usableOn": ["tunnel", "ferry"], "freeDraw": true, "faceUpReset": false })),
        Some(json!({ "anyCardsPerLocomotive": 3 })),
    )
}

fn swiss_rules() -> Value {
    ruleset_json(
        Some(json!({ "usableOn": ["tunnel"], "freeDraw": true })),
        None,
    )
}

fn engine_of(ruleset: &Value) -> Engine {
    let ruleset: Ruleset = serde_json::from_value(ruleset.clone()).unwrap();
    let map: MapData = serde_json::from_value(map_json()).unwrap();
    let mut eng = Engine::new(
        7,
        ruleset,
        map,
        vec!["p1".into(), "p2".into()],
        vec!["Uno".into(), "Due".into()],
        ttr_core::default_player_colors(2),
        0,
    )
    .unwrap();
    for pi in 0..2 {
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
    eng
}

fn cards(parts: &[(CardColor, usize)]) -> Vec<CardColor> {
    parts
        .iter()
        .flat_map(|(c, n)| std::iter::repeat_n(*c, *n))
        .collect()
}

fn claim(
    eng: &mut Engine,
    route_id: &str,
    paid: Vec<CardColor>,
) -> Result<Vec<GameEvent>, EngineError> {
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: route_id.into(),
        cards_discarded: paid,
        passengers: Default::default(),
    })
}

fn draw_face_up(
    eng: &mut Engine,
    player: usize,
    index: usize,
) -> Result<Vec<GameEvent>, EngineError> {
    eng.apply(Action::DrawTrainCard {
        player_index: player,
        source: DrawSource::FaceUp { index },
    })
}

fn is_claimable(eng: &Engine, route_id: &str) -> bool {
    eng.legal_moves(0).routes.iter().any(|r| r == route_id)
}

// ── Pagamento: Nordic ────────────────────────────────────────────────

#[test]
fn nordic_plain_route_refuses_a_locomotive() {
    let mut eng = engine_of(&nordic_rules());
    eng.set_player_hand(0, cards(&[(Red, 1), (Locomotive, 1)]));
    assert!(matches!(
        claim(&mut eng, "plain_red", cards(&[(Red, 1), (Locomotive, 1)])),
        Err(EngineError::InvalidPayload { .. })
    ));
    assert!(!is_claimable(&eng, "plain_red"));

    // Carta mancante: niente sostituzione con la locomotiva dove è vietata.
    assert!(matches!(
        claim(&mut eng, "plain_red", cards(&[(Red, 2)])),
        Err(EngineError::InsufficientCards { .. })
    ));
    // Lo stato non è cambiato dai rifiuti.
    assert_eq!(eng.player_hand(0).unwrap().len(), 2);

    eng.set_player_hand(0, cards(&[(Red, 2), (Locomotive, 1)]));
    assert!(is_claimable(&eng, "plain_red"));
    let events = claim(&mut eng, "plain_red", cards(&[(Red, 2)])).unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::RouteClaimed { cards_used, .. } if *cards_used == cards(&[(Red, 2)])
    )));
    assert_eq!(eng.player_hand(0).unwrap(), cards(&[(Locomotive, 1)]));
}

#[test]
fn nordic_tunnel_accepts_a_locomotive() {
    let mut eng = engine_of(&nordic_rules());
    eng.set_player_hand(0, cards(&[(Red, 1), (Locomotive, 1)]));
    assert!(is_claimable(&eng, "tunnel_red"));
    // Le carte rivelate non costano nulla: il tunnel si completa subito.
    eng.set_draw_pile(cards(&[(Black, 3)]));
    let events = claim(&mut eng, "tunnel_red", cards(&[(Red, 1), (Locomotive, 1)])).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::RouteClaimed { .. }))
    );
}

#[test]
fn nordic_ferry_three_any_cards_replace_the_locomotive() {
    let mut eng = engine_of(&nordic_rules());
    // Traghetto blu di 3 spazi, 1 simbolo locomotiva: 2 blu + loco...
    eng.set_player_hand(0, cards(&[(Blue, 2), (Locomotive, 1)]));
    assert!(is_claimable(&eng, "ferry_blue"));
    // ...oppure 2 blu + 3 carte qualsiasi.
    eng.set_player_hand(0, cards(&[(Blue, 2), (Red, 2), (Green, 1)]));
    assert!(is_claimable(&eng, "ferry_blue"));

    // Una carta avanzata (6 carte per 5 necessarie) è rifiutata.
    eng.set_player_hand(0, cards(&[(Blue, 2), (Red, 2), (Green, 2)]));
    assert!(matches!(
        claim(
            &mut eng,
            "ferry_blue",
            cards(&[(Blue, 2), (Red, 2), (Green, 2)])
        ),
        Err(EngineError::InvalidPayload { .. })
    ));
    // Meno carte degli spazi.
    assert!(matches!(
        claim(&mut eng, "ferry_blue", cards(&[(Blue, 2)])),
        Err(EngineError::InsufficientCards { .. })
    ));

    eng.set_player_hand(0, cards(&[(Blue, 2), (Red, 2), (Green, 1)]));
    let events = claim(
        &mut eng,
        "ferry_blue",
        cards(&[(Blue, 2), (Red, 2), (Green, 1)]),
    )
    .unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::RouteClaimed { cards_used, trains_placed: 3, .. } if cards_used.len() == 5
    )));
    assert!(eng.player_hand(0).unwrap().is_empty());
}

#[test]
fn nordic_gray_ferry_uses_one_color_and_groups() {
    let mut eng = engine_of(&nordic_rules());
    // 1 blu singola + locomotiva + gruppo di 3 qualsiasi.
    let pay = cards(&[(Blue, 1), (Locomotive, 1), (Red, 2), (Green, 1)]);
    eng.set_player_hand(0, pay.clone());
    assert!(is_claimable(&eng, "ferry_gray"));
    // Due colori singoli diversi non vanno.
    assert!(matches!(
        claim(
            &mut eng,
            "ferry_gray",
            cards(&[(Blue, 1), (Red, 1), (Locomotive, 1)])
        ),
        Err(EngineError::InvalidPayload { .. } | EngineError::InsufficientCards { .. })
    ));
    claim(&mut eng, "ferry_gray", pay).unwrap();
}

#[test]
fn nordic_long_route_seven_green_plus_eight_any() {
    let mut eng = engine_of(&nordic_rules());
    let pay = cards(&[(Green, 7), (Red, 3), (Locomotive, 3), (Blue, 2)]);
    assert_eq!(pay.len(), 15);
    eng.set_player_hand(0, pay.clone());
    assert!(is_claimable(&eng, "long_gray"));

    // Una locomotiva da sola non può pagare uno spazio: 8 verdi + 1 loco non vale.
    eng.set_player_hand(0, cards(&[(Green, 8), (Locomotive, 1)]));
    assert!(matches!(
        claim(&mut eng, "long_gray", cards(&[(Green, 8), (Locomotive, 1)])),
        Err(EngineError::InvalidPayload { .. })
    ));
    // Carte avanzate: 7 verdi + 7 qualsiasi non fanno due gruppi da 4.
    eng.set_player_hand(0, cards(&[(Green, 7), (Red, 7)]));
    assert!(matches!(
        claim(&mut eng, "long_gray", cards(&[(Green, 7), (Red, 7)])),
        Err(EngineError::InvalidPayload { .. })
    ));
    // Troppo poche carte.
    assert!(matches!(
        claim(&mut eng, "long_gray", cards(&[(Green, 8)])),
        Err(EngineError::InsufficientCards { .. })
    ));

    eng.set_player_hand(0, pay.clone());
    let events = claim(&mut eng, "long_gray", pay).unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::RouteClaimed { cards_used, points: 27, trains_placed: 9, .. }
            if cards_used.len() == 15
    )));
    assert!(eng.player_hand(0).unwrap().is_empty());
    assert_eq!(eng.player(0).unwrap().trains_remaining, 36);
}

#[test]
fn legal_routes_follow_substitutions_and_restrictions() {
    let mut eng = engine_of(&nordic_rules());
    // 9 carte rosse: rotta lunga pagabile solo come 9 singole (mai: serve un colore, ok).
    eng.set_player_hand(0, cards(&[(Red, 9)]));
    assert!(is_claimable(&eng, "long_gray"));
    // 8 locomotive: due gruppi da 4 = 2 spazi, non bastano.
    eng.set_player_hand(0, cards(&[(Locomotive, 8)]));
    assert!(!is_claimable(&eng, "long_gray"));
    // Solo locomotive: nessuna rotta normale.
    assert!(!is_claimable(&eng, "plain_gray"));
    assert!(!is_claimable(&eng, "plain_red"));
}

// ── Pagamento: Switzerland ───────────────────────────────────────────

#[test]
fn swiss_locomotive_only_on_tunnels() {
    let mut eng = engine_of(&swiss_rules());
    eng.set_player_hand(0, cards(&[(Red, 1), (Locomotive, 1)]));
    assert!(!is_claimable(&eng, "plain_red"));
    assert!(is_claimable(&eng, "tunnel_red"));
    assert!(matches!(
        claim(&mut eng, "plain_red", cards(&[(Red, 1), (Locomotive, 1)])),
        Err(EngineError::InvalidPayload { .. })
    ));
    eng.set_draw_pile(cards(&[(Black, 3)]));
    claim(&mut eng, "tunnel_red", cards(&[(Red, 1), (Locomotive, 1)])).unwrap();
}

#[test]
fn tunnel_extra_cost_follows_locomotive_rule() {
    // Tunnel che accetta locomotive: il costo extra si paga anche con loco.
    let mut eng = engine_of(&swiss_rules());
    eng.set_player_hand(0, cards(&[(Red, 2), (Locomotive, 1)]));
    eng.set_draw_pile(cards(&[(Black, 2), (Red, 1)]));
    claim(&mut eng, "tunnel_red", cards(&[(Red, 2)])).unwrap();
    assert_eq!(eng.phase(), GamePhase::TunnelResolution);
    assert!(eng.legal_moves(0).tunnel.unwrap().can_pay);
    eng.apply(Action::TunnelDecision {
        player_index: 0,
        pay: true,
        cards_discarded: vec![Locomotive],
    })
    .unwrap();
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
}

// ── Compatibilità: senza campi nuovi il motore non cambia ────────────

#[test]
fn classic_ruleset_keeps_locomotive_fallback_and_free_use() {
    let mut eng = engine_of(&ruleset_json(None, None));
    // Locomotiva ovunque, anche su rotte normali e come sostituta di una carta mancante.
    eng.set_player_hand(0, cards(&[(Red, 1), (Locomotive, 1)]));
    assert!(is_claimable(&eng, "plain_red"));
    claim(&mut eng, "plain_red", cards(&[(Red, 2)])).unwrap();
    assert!(eng.player_hand(0).unwrap().is_empty());
}

#[test]
fn classic_ruleset_keeps_loco_face_up_rules() {
    let mut eng = engine_of(&ruleset_json(None, None));
    eng.set_face_up_card(0, Some(Locomotive));
    eng.set_face_up_card(1, Some(Locomotive));
    let events = draw_face_up(&mut eng, 0, 0).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DrawTurnComplete { .. }))
    );
    assert_eq!(eng.current_player(), 1);

    eng.set_face_up_card(0, Some(Red));
    eng.set_face_up_card(1, Some(Locomotive));
    draw_face_up(&mut eng, 1, 0).unwrap();
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);
    assert!(!eng.legal_moves(1).draw_face_up.contains(&1));
    assert!(matches!(
        draw_face_up(&mut eng, 1, 1),
        Err(EngineError::LocomotiveAsSecondCard)
    ));
}

// ── Pesca: freeDraw e faceUpReset ────────────────────────────────────

#[test]
fn free_draw_lets_a_face_up_locomotive_be_first_or_second_card() {
    let mut eng = engine_of(&nordic_rules());
    eng.set_face_up_card(0, Some(Locomotive));
    eng.set_face_up_card(1, Some(Locomotive));
    let events = draw_face_up(&mut eng, 0, 0).unwrap();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::DrawTurnComplete { .. }))
    );
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);
    assert!(eng.legal_moves(0).draw_face_up.contains(&1));

    let events = draw_face_up(&mut eng, 0, 1).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DrawTurnComplete { .. }))
    );
    assert_eq!(eng.current_player(), 1);
    let hand = eng.player_hand(0).unwrap();
    assert!(hand.iter().filter(|c| **c == Locomotive).count() >= 2);
}

#[test]
fn face_up_reset_false_keeps_three_face_up_locomotives() {
    let mut eng = engine_of(&nordic_rules());
    for (i, card) in [Red, Locomotive, Locomotive, Locomotive, Blue]
        .into_iter()
        .enumerate()
    {
        eng.set_face_up_card(i, Some(card));
    }
    let events = draw_face_up(&mut eng, 0, 0).unwrap();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::FaceUpReset { .. }))
    );
    let locos = eng
        .face_up_cards()
        .iter()
        .filter(|c| **c == Some(Locomotive))
        .count();
    assert!(locos >= 3);
}

#[test]
fn face_up_reset_default_still_applies_with_free_draw() {
    let mut eng = engine_of(&swiss_rules());
    for (i, card) in [Red, Locomotive, Locomotive, Locomotive, Blue]
        .into_iter()
        .enumerate()
    {
        eng.set_face_up_card(i, Some(card));
    }
    let events = draw_face_up(&mut eng, 0, 0).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::FaceUpReset { .. }))
    );
    // freeDraw vale anche con il reset: la locomotiva scoperta non chiude il turno.
    let mut eng = engine_of(&swiss_rules());
    eng.set_face_up_card(0, Some(Locomotive));
    draw_face_up(&mut eng, 0, 0).unwrap();
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);
}

// ── Validazione ──────────────────────────────────────────────────────

fn validate(ruleset: &Value, map: &Value) -> Result<(), EngineInitError> {
    let ruleset: Ruleset = serde_json::from_value(ruleset.clone()).unwrap();
    let map: MapData = serde_json::from_value(map.clone()).unwrap();
    ruleset.validate(&map)
}

#[test]
fn validate_accepts_the_new_fields() {
    assert!(validate(&nordic_rules(), &map_json()).is_ok());
    assert!(validate(&swiss_rules(), &map_json()).is_ok());
}

#[test]
fn validate_rejects_bad_usable_on() {
    let bad = |usable: Value| {
        validate(
            &ruleset_json(Some(json!({ "usableOn": usable })), None),
            &map_json(),
        )
    };
    assert!(matches!(
        bad(json!([])),
        Err(EngineInitError::LocomotivesUsableOnEmpty)
    ));
    assert!(matches!(
        bad(json!(["tunnel", "tunnel"])),
        Err(EngineInitError::LocomotivesUsableOnDuplicate { .. })
    ));
    assert!(matches!(
        bad(json!(["road"])),
        Err(EngineInitError::LocomotivesUsableOnUnknown { .. })
    ));
}

#[test]
fn validate_rejects_small_any_card_groups() {
    assert!(matches!(
        validate(
            &ruleset_json(None, Some(json!({ "anyCardsPerLocomotive": 1 }))),
            &map_json()
        ),
        Err(EngineInitError::FerriesAnyCardsTooSmall { value: 1 })
    ));
    let mut map = map_json();
    map["routes"][4]["anyCardsPerSpace"] = json!(1);
    assert!(matches!(
        validate(&nordic_rules(), &map),
        Err(EngineInitError::RouteAnyCardsPerSpaceTooSmall { value: 1, .. })
    ));
}

#[test]
fn validate_rejects_tunnel_with_ferries_or_any_cards() {
    let mut map = map_json();
    map["routes"][1]["ferries"] = json!(1);
    assert!(matches!(
        validate(&nordic_rules(), &map),
        Err(EngineInitError::RouteTunnelAndFerry { route_id }) if route_id == "tunnel_red"
    ));
    let mut map = map_json();
    map["routes"][1]["anyCardsPerSpace"] = json!(4);
    assert!(matches!(
        validate(&nordic_rules(), &map),
        Err(EngineInitError::RouteTunnelAnyCardsPerSpace { route_id }) if route_id == "tunnel_red"
    ));
}
