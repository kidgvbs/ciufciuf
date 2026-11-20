//! Rotte di montagna e bonus `largest_network` (Legendary Asia): costo in treni pari a
//! lunghezza più simboli X, `pointsPerTrain` punti per treno scartato, rifiuto con riserva
//! insufficiente, ultimo giro e D28 contati con i treni di montagna, rete più grande (rami,
//! anelli, pari merito, Paesi), spareggio `mountainRoutes`, validazione dei dati.
//!
//! Mappa e ruleset sono costruiti in memoria.

use serde_json::{Value, json};
use ttr_core::error::GamePhase;
use ttr_core::event::FinalScore;
use ttr_core::scoring::{largest_network_size, largest_network_winners};
use ttr_core::types::{ClaimedRouteInfo, MapData, Route, Ruleset, TieBreaker};
use ttr_core::{
    Action, DrawSource, Engine, EngineError, EngineInitError, GameEvent, default_player_colors,
};
use ttr_proto::CardColor;

// ── Fixture ──────────────────────────────────────────────────────────

/// Id, estremi, lunghezza, colore, doppia, simboli di montagna.
type Spec = (
    &'static str,
    &'static str,
    &'static str,
    u32,
    &'static str,
    bool,
    u32,
);

const ROUTES: &[Spec] = &[
    ("a_b", "a", "b", 2, "Gray", false, 0),
    ("b_c", "b", "c", 3, "Gray", false, 2),
    // Doppia: la colorata ha la X, la grigia no.
    ("c_d_1", "c", "d", 2, "Red", true, 1),
    ("c_d_2", "c", "d", 2, "Gray", true, 0),
    ("m_n", "m", "n", 1, "Gray", false, 1),
    ("p_q", "p", "q", 1, "Gray", false, 0),
    // Rete: anello e–f–g con ramo g–h; triangolo i–j–k.
    ("e_f", "e", "f", 1, "Gray", false, 0),
    ("f_g", "f", "g", 1, "Gray", false, 0),
    ("e_g", "e", "g", 1, "Gray", false, 0),
    ("g_h", "g", "h", 1, "Gray", false, 0),
    ("i_j", "i", "j", 1, "Gray", false, 0),
    ("j_k", "j", "k", 1, "Gray", false, 0),
    ("i_k", "i", "k", 1, "Gray", false, 0),
    // Paese con due varchi: non collega h e i.
    ("h_xx", "h", "xx", 1, "Gray", false, 0),
    ("xx_i", "xx", "i", 1, "Gray", false, 0),
];

const CITIES: [&str; 16] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "m", "n", "p", "q", "xx",
];

fn route_json(spec: &Spec) -> Value {
    let &(id, from, to, len, color, double, mountains) = spec;
    let mut route = json!({
        "id": id, "from": from, "to": to, "len": len, "color": color, "double": double,
    });
    if mountains > 0 {
        route["mountains"] = json!(mountains);
    }
    route
}

fn map_json(routes: &[Spec]) -> Value {
    let cities: Vec<Value> = CITIES
        .iter()
        .map(|id| json!({"id": id, "name": id, "country": *id == "xx"}))
        .collect();
    let ticket = |id: &str, from: &str, to: &str| json!({"id": id, "from": from, "to": to, "points": 1, "deck": "regular", "sets": ["mini"]});
    let mut tickets = vec![ticket("t_a_b", "a", "b"), ticket("t_e_f", "e", "f")];
    tickets.extend((0..14).map(|i| ticket(&format!("t_fill{i:02}"), "m", "q")));
    json!({"cities": cities, "routes": routes.iter().map(route_json).collect::<Vec<_>>(), "tickets": tickets})
}

fn map() -> MapData {
    serde_json::from_value(map_json(ROUTES)).unwrap()
}

fn ruleset_json(bonuses: &Value, tie_breakers: Option<Value>) -> Value {
    let mut ruleset = json!({
        "name": "Mini", "mode": "Standard", "ticketSet": "mini", "map": "mini",
        "players": {"min": 2, "max": 2}, "trainsPerPlayer": 30,
        "deck": {"perColor": 12, "locomotives": 14},
        "ticketDecks": ["regular"],
        "initialTickets": {"dealt": [{"deck": "regular", "count": 4}], "keepMin": 2},
        "ticketDraw": {"deck": "regular", "count": 3, "keepMin": 1},
        "setupDiscards": "removed",
        "routePoints": {"1": 1, "2": 2, "3": 4},
        "mountains": {"pointsPerTrain": 2},
        "bonuses": bonuses, "doubleRouteMinPlayers": 2, "finalRoundIncludesTrigger": true
    });
    if let Some(keys) = tie_breakers {
        ruleset["tieBreakers"] = keys;
    }
    ruleset
}

fn explorer() -> Value {
    json!([{"type": "largest_network", "label": "Asian Explorer", "points": 10}])
}

fn ruleset(bonuses: &Value, tie_breakers: Option<Value>) -> Ruleset {
    serde_json::from_value(ruleset_json(bonuses, tie_breakers)).unwrap()
}

fn engine_on(map: MapData, ruleset: Ruleset) -> Engine {
    Engine::new(
        5,
        ruleset,
        map,
        vec!["p0".into(), "p1".into()],
        vec!["P0".into(), "P1".into()],
        default_player_colors(2),
        0,
    )
    .unwrap()
}

fn engine() -> Engine {
    engine_on(map(), ruleset(&json!([]), None))
}

fn engine_with(bonuses: &Value, tie_breakers: Option<Value>) -> Engine {
    engine_on(map(), ruleset(bonuses, tie_breakers))
}

fn route<'a>(eng: &'a Engine, id: &str) -> &'a Route {
    eng.get_route(id).unwrap()
}

fn try_claim(eng: &mut Engine, pi: usize, route_id: &str) -> Result<Vec<GameEvent>, EngineError> {
    let color = if route(eng, route_id).color == "Red" {
        CardColor::Red
    } else {
        CardColor::Blue
    };
    let cards = vec![color; route(eng, route_id).len as usize];
    eng.set_phase_for_test(GamePhase::PlayerTurn, pi);
    eng.set_player_hand(pi, cards.clone());
    eng.apply(Action::ClaimRoute {
        player_index: pi,
        route_id: route_id.into(),
        cards_discarded: cards,
        passengers: Default::default(),
        ferry_cards: 0,
    })
}

fn claim(eng: &mut Engine, pi: usize, route_id: &str) -> Vec<GameEvent> {
    try_claim(eng, pi, route_id).unwrap()
}

fn claim_all(eng: &mut Engine, pi: usize, routes: &[&str]) {
    for route_id in routes {
        claim(eng, pi, route_id);
    }
}

fn finish_game(eng: &mut Engine) -> Vec<FinalScore> {
    let current = eng.current_player();
    eng.set_phase_for_test(GamePhase::PlayerTurn, current);
    eng.set_player_trains(current, 2);
    for _ in 0..200 {
        let pi = eng.current_player();
        let events = eng
            .apply(Action::DrawTrainCard {
                player_index: pi,
                source: DrawSource::Deck,
            })
            .unwrap();
        for event in events {
            if let GameEvent::GameOver { final_scores, .. } = event {
                return final_scores;
            }
        }
    }
    panic!("la partita non è terminata");
}

fn last_round_trains(events: &[GameEvent]) -> Option<u32> {
    events.iter().find_map(|e| match e {
        GameEvent::LastRoundAnnounced {
            trains_remaining, ..
        } => Some(*trains_remaining),
        _ => None,
    })
}

// ── Treni e punti ────────────────────────────────────────────────────

/// Una rotta di montagna (len 3, 2 X) si paga con 3 carte e si posa come sempre, ma toglie
/// 3 + 2 treni e dà 4 punti per la lunghezza più 2 × 2 per i treni scartati.
#[test]
fn mountain_route_debits_length_plus_symbols_and_scores_two_per_train() {
    let mut eng = engine();
    let events = claim(&mut eng, 0, "b_c");
    let player = eng.player(0).unwrap();
    assert_eq!(player.trains_remaining, 30 - 5);
    assert_eq!(player.score, 4 + 2 * 2);
    assert_eq!(player.mountain_points, 4);
    assert_eq!(player.claimed_routes["b_c"].points, 4);
    assert_eq!(player.claimed_routes["b_c"].train_count, 3);
    // Le carte pagate sono solo la lunghezza: i treni scartati non sono carte.
    assert_eq!(eng.discard_cards().len(), 3);
    let claimed = events
        .iter()
        .find_map(|e| match e {
            GameEvent::RouteClaimed {
                points,
                trains_placed,
                ..
            } => Some((*points, *trains_placed)),
            _ => None,
        })
        .unwrap();
    assert_eq!(claimed, (8, 3));
}

/// Una rotta senza X resta com'era: nessun treno extra, nessun punto di montagna.
#[test]
fn plain_route_is_unchanged() {
    let mut eng = engine();
    claim(&mut eng, 0, "a_b");
    let player = eng.player(0).unwrap();
    assert_eq!(player.trains_remaining, 28);
    assert_eq!(player.score, 2);
    assert_eq!(player.mountain_points, 0);
}

/// Doppia colorata con X + grigia senza X: solo la colorata costa treni extra.
#[test]
fn only_the_colored_lane_of_a_double_has_the_symbol() {
    let mut eng = engine();
    claim(&mut eng, 0, "c_d_2");
    claim(&mut eng, 1, "c_d_1");
    let plain = eng.player(0).unwrap();
    assert_eq!((plain.trains_remaining, plain.score), (28, 2));
    let mountain = eng.player(1).unwrap();
    assert_eq!((mountain.trains_remaining, mountain.score), (27, 4));
    assert_eq!(mountain.mountain_points, 2);
}

/// Con meno di lunghezza + X treni la rotta è rifiutata (stato intatto) e non è tra le
/// mosse legali, anche se la lunghezza da sola entrerebbe; con lunghezza + X è legale.
#[test]
fn claim_is_refused_when_the_reserve_does_not_cover_the_symbols() {
    let mut eng = engine();
    eng.set_player_trains(0, 4);
    eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
    eng.set_player_hand(0, vec![CardColor::Blue; 3]);
    let routes = eng.legal_moves(0).routes;
    assert!(routes.contains(&"a_b".to_string()));
    assert!(!routes.contains(&"b_c".to_string()), "{routes:?}");

    let refused = eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "b_c".into(),
        cards_discarded: vec![CardColor::Blue; 3],
        passengers: Default::default(),
        ferry_cards: 0,
    });
    assert!(matches!(
        refused,
        Err(EngineError::InsufficientCards { .. })
    ));
    let player = eng.player(0).unwrap();
    assert_eq!((player.trains_remaining, player.score), (4, 0));
    assert_eq!(player.hand.len(), 3);
    assert!(eng.claimed_route_ids().is_empty());

    eng.set_player_trains(0, 5);
    assert!(eng.legal_moves(0).routes.contains(&"b_c".to_string()));
    let events = eng
        .apply(Action::ClaimRoute {
            player_index: 0,
            route_id: "b_c".into(),
            cards_discarded: vec![CardColor::Blue; 3],
            passengers: Default::default(),
            ferry_cards: 0,
        })
        .unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::RouteClaimed { .. }))
    );
    assert_eq!(eng.player(0).unwrap().trains_remaining, 0);
}

/// L'ultimo giro scatta con ≤ 2 treni dopo lo scarto: 7 − (3 + 2) = 2 innesca, 8 − 5 = 3 no.
#[test]
fn last_round_triggers_after_the_discarded_trains() {
    let mut eng = engine();
    eng.set_player_trains(0, 7);
    let events = claim(&mut eng, 0, "b_c");
    assert_eq!(last_round_trains(&events), Some(2));
    assert!(eng.last_round());

    let mut eng = engine();
    eng.set_player_trains(0, 8);
    let events = claim(&mut eng, 0, "b_c");
    assert_eq!(last_round_trains(&events), None);
    assert!(!eng.last_round());
}

/// D28: l'ultimo giro scatta se nessuno può più reclamare nessuna rotta, contando i treni
/// di montagna: con 4 treni la rotta len 3 + 2 X non è reclamabile (3 sarebbero bastati).
#[test]
fn nobody_can_claim_counts_the_mountain_trains() {
    let routes: [Spec; 2] = [
        ("p_q", "p", "q", 1, "Gray", false, 0),
        ("q_r", "q", "r", 3, "Gray", false, 2),
    ];
    let mut map_value = map_json(&routes);
    map_value["cities"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "r", "name": "r"}));
    let build = || {
        engine_on(
            serde_json::from_value(map_value.clone()).unwrap(),
            ruleset(&json!([]), None),
        )
    };

    let mut eng = build();
    eng.set_player_trains(0, 5);
    eng.set_player_trains(1, 4);
    let events = claim(&mut eng, 0, "p_q");
    assert_eq!(last_round_trains(&events), Some(4));

    // Con 5 treni il secondo giocatore può ancora prendere q_r: nessun ultimo giro.
    let mut eng = build();
    eng.set_player_trains(0, 5);
    eng.set_player_trains(1, 5);
    let events = claim(&mut eng, 0, "p_q");
    assert_eq!(last_round_trains(&events), None);
}

/// Il punteggio finale separa i punti di montagna da quelli delle rotte; il totale li somma.
#[test]
fn final_score_reports_mountain_points_separately() {
    let mut eng = engine();
    eng.set_player_tickets(0, &["t_a_b"]);
    eng.set_player_tickets(1, &[]);
    claim_all(&mut eng, 0, &["b_c", "a_b"]);
    assert_eq!(eng.player(0).unwrap().score, 2 + 4 + 4);
    let scores = finish_game(&mut eng);
    let first = &scores[0];
    assert_eq!(first.route_points, 2 + 4);
    assert_eq!(first.mountain_points, 4);
    assert_eq!(first.ticket_completed, 1);
    assert_eq!(first.total, 2 + 4 + 4 + 1);
    assert_eq!(scores[1].mountain_points, 0);
    assert_eq!(scores[1].total, 0);
}

// ── Rete più grande ──────────────────────────────────────────────────

/// Partita con i giocatori che reclamano le rotte indicate; punteggi finali.
fn explorer_game(first: &[&str], second: &[&str]) -> Vec<FinalScore> {
    let mut eng = engine_with(&explorer(), None);
    eng.set_player_tickets(0, &[]);
    eng.set_player_tickets(1, &[]);
    claim_all(&mut eng, 0, first);
    claim_all(&mut eng, 1, second);
    finish_game(&mut eng)
}

fn network(scores: &[FinalScore]) -> Vec<(u32, i32)> {
    scores
        .iter()
        .map(|s| (s.largest_network, s.largest_network_bonus))
        .collect()
}

/// Anello più ramo: 4 città contano una volta ciascuna; vince su una catena di 3.
#[test]
fn largest_network_counts_cities_of_branches_and_loops() {
    let scores = explorer_game(&["e_f", "f_g", "e_g", "g_h"], &["i_j", "j_k"]);
    assert_eq!(network(&scores), [(4, 10), (3, 0)]);
    assert_eq!(scores[0].total, 4 + 10);
}

/// Pari merito: il bonus va a tutti (un anello di 3 città vale 3, come una catena di 3).
#[test]
fn largest_network_tie_awards_everyone() {
    let scores = explorer_game(&["e_f", "f_g", "e_g"], &["i_j", "j_k"]);
    assert_eq!(network(&scores), [(3, 10), (3, 10)]);
}

/// Conta solo la componente più grande: due reti separate non si sommano.
#[test]
fn largest_network_takes_the_biggest_component_only() {
    let scores = explorer_game(&["e_f", "f_g", "i_j"], &["p_q"]);
    assert_eq!(network(&scores), [(3, 10), (2, 0)]);
}

/// Senza rotte nessuno prende il bonus.
#[test]
fn largest_network_is_zero_without_routes() {
    let scores = explorer_game(&[], &[]);
    assert_eq!(network(&scores), [(0, 0), (0, 0)]);
}

/// Un Paese è un nodo privato per rotta: h–Paese e Paese–i non uniscono h e i (rete di 2,
/// non 3); l'avversario con una catena di 3 città vince.
#[test]
fn largest_network_does_not_join_through_a_country() {
    let scores = explorer_game(&["h_xx", "xx_i"], &["e_f", "f_g"]);
    assert_eq!(network(&scores), [(2, 0), (3, 10)]);
}

/// Senza il bonus nel ruleset la rete non si calcola.
#[test]
fn largest_network_is_absent_without_the_bonus() {
    let mut eng = engine();
    claim_all(&mut eng, 0, &["e_f", "f_g"]);
    let scores = finish_game(&mut eng);
    assert_eq!(network(&scores), [(0, 0), (0, 0)]);
}

#[test]
fn largest_network_functions_on_claimed_routes() {
    let routes: std::collections::BTreeMap<String, Route> = map()
        .routes
        .into_iter()
        .map(|r| (r.id.clone(), r))
        .collect();
    let claimed = |ids: &[&str]| -> Vec<ClaimedRouteInfo> {
        ids.iter()
            .map(|id| ClaimedRouteInfo {
                route_id: (*id).into(),
                claimed_by: 0,
            })
            .collect()
    };
    let countries = std::iter::once("xx".to_string()).collect();
    assert_eq!(largest_network_size(&[], &routes, &countries), 0);
    assert_eq!(
        largest_network_size(&claimed(&["e_f", "f_g", "e_g", "g_h"]), &routes, &countries),
        4
    );
    // Il Paese è un nodo privato per rotta e conta una volta per componente.
    assert_eq!(
        largest_network_size(&claimed(&["g_h", "h_xx", "xx_i"]), &routes, &countries),
        3
    );
    assert_eq!(largest_network_winners(&[0, 0]), Vec::<usize>::new());
    assert_eq!(largest_network_winners(&[3, 5, 5]), [1, 2]);
}

// ── Spareggio ────────────────────────────────────────────────────────

fn winners(scores: &[FinalScore]) -> Vec<usize> {
    scores
        .iter()
        .filter(|s| s.winner)
        .map(|s| s.player_index)
        .collect()
}

/// A pari punti e biglietti completati vince chi ha preso più rotte di montagna.
#[test]
fn tie_break_mountain_routes_decides_after_completed_tickets() {
    let mut eng = engine_with(
        &json!([]),
        Some(json!(["completedTickets", "mountainRoutes"])),
    );
    eng.set_player_tickets(0, &[]);
    eng.set_player_tickets(1, &[]);
    // 3 + 1 contro 2 + 1 + 1: 4 punti a testa, una rotta di montagna contro nessuna.
    claim_all(&mut eng, 0, &["m_n", "j_k"]);
    claim_all(&mut eng, 1, &["a_b", "e_f", "f_g"]);
    let scores = finish_game(&mut eng);
    assert_eq!((scores[0].total, scores[1].total), (4, 4));
    assert_eq!(winners(&scores), [0]);
}

/// L'ordine delle chiavi conta: con `completedTickets` prima vince chi ha un biglietto in
/// più, con `mountainRoutes` prima vince chi ha la montagna.
#[test]
fn tie_break_order_follows_the_ruleset() {
    let outcome = |keys: Value| {
        let mut eng = engine_with(&json!([]), Some(keys));
        eng.set_player_tickets(0, &[]);
        eng.set_player_tickets(1, &["t_e_f"]);
        // 3 + 1 = 4 contro 1 + 2 + biglietto 1 = 4.
        claim_all(&mut eng, 0, &["m_n", "p_q"]);
        claim_all(&mut eng, 1, &["e_f", "a_b"]);
        let scores = finish_game(&mut eng);
        assert_eq!((scores[0].total, scores[1].total), (4, 4));
        winners(&scores)
    };
    assert_eq!(outcome(json!(["completedTickets", "mountainRoutes"])), [1]);
    assert_eq!(outcome(json!(["mountainRoutes", "completedTickets"])), [0]);
}

#[test]
fn mountain_routes_is_a_known_tie_breaker_key() {
    assert_eq!(
        TieBreaker::parse("mountainRoutes"),
        Some(TieBreaker::MountainRoutes)
    );
}

// ── Validazione e dati ───────────────────────────────────────────────

fn validate(map_value: &Value, ruleset_value: &Value) -> Result<(), EngineInitError> {
    let map: MapData = serde_json::from_value(map_value.clone()).unwrap();
    let ruleset: Ruleset = serde_json::from_value(ruleset_value.clone()).unwrap();
    ruleset.validate(&map)
}

#[test]
fn valid_mountain_data_passes_validation() {
    let ruleset = ruleset_json(
        &explorer(),
        Some(json!(["completedTickets", "mountainRoutes"])),
    );
    assert!(validate(&map_json(ROUTES), &ruleset).is_ok());
}

#[test]
fn mountains_cannot_exceed_the_length() {
    let routes: [Spec; 2] = [
        ("a_b", "a", "b", 2, "Gray", false, 3),
        ("p_q", "p", "q", 1, "Gray", false, 0),
    ];
    let err = validate(&map_json(&routes), &ruleset_json(&json!([]), None)).unwrap_err();
    assert!(
        matches!(&err, EngineInitError::RouteMountainsExceedLength { route_id, mountains: 3, len: 2 } if route_id == "a_b"),
        "{err:?}"
    );
}

#[test]
fn mountains_are_rejected_on_tunnels() {
    let mut map_value = map_json(ROUTES);
    map_value["routes"][1]["tunnel"] = json!(true);
    let mut ruleset = ruleset_json(&json!([]), None);
    ruleset["tunnels"] = json!({"revealed": 3});
    let err = validate(&map_value, &ruleset).unwrap_err();
    assert!(
        matches!(&err, EngineInitError::RouteMountainsOnTunnel { route_id } if route_id == "b_c"),
        "{err:?}"
    );
}

#[test]
fn mountain_routes_require_the_ruleset_section() {
    let mut ruleset = ruleset_json(&json!([]), None);
    ruleset.as_object_mut().unwrap().remove("mountains");
    let err = validate(&map_json(ROUTES), &ruleset).unwrap_err();
    assert!(
        matches!(&err, EngineInitError::MountainsConfigMissing { route_id } if route_id == "b_c"),
        "{err:?}"
    );
    ruleset["mountains"] = json!({"pointsPerTrain": 0});
    assert!(matches!(
        validate(&map_json(ROUTES), &ruleset),
        Err(EngineInitError::MountainsConfigMissing { .. })
    ));
}

#[test]
fn mountains_section_without_mountain_routes_is_rejected() {
    let routes: [Spec; 1] = [("a_b", "a", "b", 2, "Gray", false, 0)];
    let err = validate(&map_json(&routes), &ruleset_json(&json!([]), None)).unwrap_err();
    assert!(
        matches!(err, EngineInitError::MountainsConfigUnused),
        "{err:?}"
    );
}

#[test]
fn bonus_types_still_reject_unknown_names() {
    let bogus = json!([{"type": "largest_networks", "label": "x", "points": 10}]);
    let err = validate(&map_json(ROUTES), &ruleset_json(&bogus, None)).unwrap_err();
    assert!(
        matches!(&err, EngineInitError::UnknownBonusType { bonus_type } if bonus_type == "largest_networks"),
        "{err:?}"
    );
    let twice = json!([
        {"type": "largest_network", "label": "x", "points": 10},
        {"type": "largest_network", "label": "y", "points": 10},
    ]);
    assert!(matches!(
        validate(&map_json(ROUTES), &ruleset_json(&twice, None)),
        Err(EngineInitError::DuplicateBonusType { .. })
    ));
}

/// Il campo `mountains` non compare nei JSON delle rotte normali (mappe esistenti
/// byte-identiche) e si legge a 0 se assente.
#[test]
fn route_mountains_field_is_omitted_when_zero() {
    let map = map();
    let plain = serde_json::to_value(&map.routes[0]).unwrap();
    assert!(plain.get("mountains").is_none(), "{plain}");
    let mountain = serde_json::to_value(&map.routes[1]).unwrap();
    assert_eq!(mountain["mountains"], 2);
    assert_eq!(map.routes[0].train_cost(), 2);
    assert_eq!(map.routes[1].train_cost(), 5);
}
