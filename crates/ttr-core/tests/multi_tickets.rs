//! Biglietti multi-destinazione (Switzerland), scarti rimossi a metà partita
//! (`ticketDraw.discards`) e spareggio `longestPath`.
//!
//! Ruleset e mappa sono costruiti in memoria: due città di passaggio, quattro Paesi
//! (vicoli ciechi) e biglietti classici e multi-destinazione.

use serde_json::{Value, json};
use ttr_core::error::GamePhase;
use ttr_core::event::FinalScore;
use ttr_core::types::{MapData, Ruleset, TicketDef};
use ttr_core::{
    Action, DrawSource, Engine, EngineError, EngineInitError, GameEvent, default_player_colors,
};
use ttr_proto::CardColor;

// ── Fixture ──────────────────────────────────────────────────────────

fn map_json(tickets: Vec<Value>) -> Value {
    let city = |id: &str, country: bool| json!({"id": id, "name": id, "country": country});
    let route = |id: &str, from: &str, to: &str, len: u32| json!({"id": id, "from": from, "to": to, "len": len, "color": "Gray", "double": false});
    json!({
        "cities": [
            city("a", false), city("b", false), city("c", false), city("d", false),
            city("e", false), city("f", false),
            city("fr", true), city("de", true), city("it", true), city("at", true),
        ],
        "routes": [
            route("fr_a_1", "fr", "a", 1),
            route("a_de_1", "a", "de", 1),
            route("b_it_1", "b", "it", 1),
            route("a_b_1", "a", "b", 1),
            route("at_c_1", "at", "c", 1),
            route("c_d_1", "c", "d", 1),
            route("e_f_1", "e", "f", 2),
            route("d_e_1", "d", "e", 2),
        ],
        "tickets": tickets,
    })
}

fn classic(id: &str, from: &str, to: &str, points: u32) -> Value {
    json!({"id": id, "from": from, "to": to, "points": points, "deck": "regular", "sets": ["mini"]})
}

fn multi(id: &str, from: &str, destinations: &[(&str, u32)]) -> Value {
    let dests: Vec<Value> = destinations
        .iter()
        .map(|(to, points)| json!({"to": to, "points": points}))
        .collect();
    json!({"id": id, "from": from, "destinations": dests, "deck": "regular", "sets": ["mini"]})
}

/// Biglietti di base: due multi-destinazione e 13 classici di riempimento (15 in tutto:
/// 8 distribuiti a 2 giocatori, 7 restano nel mazzo).
fn base_tickets() -> Vec<Value> {
    let mut tickets = vec![
        multi("t_multi", "fr", &[("de", 5), ("it", 13), ("at", 14)]),
        multi("t_city_multi", "b", &[("fr", 4), ("it", 7)]),
    ];
    tickets.extend((0..13).map(|i| classic(&format!("t_fill{i:02}"), "e", "f", 1)));
    tickets
}

fn map_from(tickets: Vec<Value>) -> MapData {
    serde_json::from_value(map_json(tickets)).unwrap()
}

fn ruleset_json(discards: &str, tie_breakers: Option<Value>) -> Value {
    let mut ruleset = json!({
        "name": "Mini", "mode": "Standard", "ticketSet": "mini", "map": "mini",
        "players": {"min": 2, "max": 2}, "trainsPerPlayer": 45,
        "deck": {"perColor": 12, "locomotives": 14},
        "ticketDecks": ["regular"],
        "initialTickets": {"dealt": [{"deck": "regular", "count": 4}], "keepMin": 2},
        "ticketDraw": {"deck": "regular", "count": 3, "keepMin": 1, "discards": discards},
        "setupDiscards": "removed",
        "routePoints": {"1": 1, "2": 2},
        "bonuses": [], "doubleRouteMinPlayers": 3, "finalRoundIncludesTrigger": true
    });
    if let Some(keys) = tie_breakers {
        ruleset["tieBreakers"] = keys;
    }
    ruleset
}

fn ruleset(discards: &str, tie_breakers: Option<Value>) -> Ruleset {
    serde_json::from_value(ruleset_json(discards, tie_breakers)).unwrap()
}

fn engine_with(ruleset: Ruleset, map: MapData) -> Engine {
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
    engine_with(ruleset("bottom", None), map_from(base_tickets()))
}

/// Il giocatore `pi` reclama la rotta nel suo turno (forzato), pagando con carte in mano.
fn claim(eng: &mut Engine, pi: usize, route_id: &str) {
    let len = eng.get_route(route_id).unwrap().len as usize;
    let cards = vec![CardColor::Red; len];
    eng.set_phase_for_test(GamePhase::PlayerTurn, pi);
    eng.set_player_hand(pi, cards.clone());
    eng.apply(Action::ClaimRoute {
        player_index: pi,
        route_id: route_id.into(),
        cards_discarded: cards,
        passengers: Default::default(),
    })
    .unwrap();
}

fn completed(eng: &Engine, pi: usize, ticket_id: &str) -> Option<bool> {
    eng.player(pi).unwrap().tickets[ticket_id].completed
}

/// Porta la partita a `GameOver` con pescate dal mazzo; restituisce i punteggi finali.
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

type Outcome = (String, bool, Option<String>, i32);

fn outcomes(eng: &Engine, pi: usize) -> Vec<Outcome> {
    eng.final_ticket_results(pi)
        .iter()
        .map(|t| {
            (
                t.ticket_id.clone(),
                t.completed,
                t.destination.clone(),
                t.points,
            )
        })
        .collect()
}

fn outcome(id: &str, done: bool, to: Option<&str>, points: i32) -> Outcome {
    (id.to_string(), done, to.map(str::to_string), points)
}

// ── Completamento e punteggio ────────────────────────────────────────

/// Nessuna, una o più destinazioni collegate; partenza e destinazioni sono Paesi.
#[test]
fn multi_destination_completion_and_scoring() {
    let mut eng = engine();
    eng.set_player_tickets(0, &["t_multi", "t_city_multi"]);
    eng.set_player_tickets(1, &["t_multi"]);

    // Nessuna destinazione collegata.
    assert_eq!(completed(&eng, 0, "t_multi"), Some(false));
    assert_eq!(completed(&eng, 0, "t_city_multi"), Some(false));

    // Una sola: il Paese di partenza (fr) arriva fino alla Germania passando da `a`.
    claim(&mut eng, 0, "fr_a_1");
    assert_eq!(completed(&eng, 0, "t_multi"), Some(false), "fr–a non basta");
    claim(&mut eng, 0, "a_de_1");
    assert_eq!(completed(&eng, 0, "t_multi"), Some(true));
    assert_eq!(completed(&eng, 0, "t_city_multi"), Some(false));

    // Più destinazioni: anche l'Italia è collegata, vale la più alta (13, non 5).
    claim(&mut eng, 0, "b_it_1");
    claim(&mut eng, 0, "a_b_1");
    assert_eq!(completed(&eng, 0, "t_city_multi"), Some(true));

    // L'altro giocatore non ha nessuna destinazione collegata.
    claim(&mut eng, 1, "at_c_1");
    assert_eq!(completed(&eng, 1, "t_multi"), Some(false));

    let scores = finish_game(&mut eng);
    // P0: t_city_multi vale 7 (it, non 4 di fr), t_multi vale 13 (it, non 5 di de).
    assert_eq!(
        (scores[0].ticket_completed, scores[0].ticket_failed),
        (20, 0)
    );
    assert_eq!(
        outcomes(&eng, 0),
        [
            outcome("t_city_multi", true, Some("it"), 7),
            outcome("t_multi", true, Some("it"), 13),
        ]
    );
    // P1: nessuna collegata → perde il valore più basso (5), non il più alto.
    assert_eq!(
        (scores[1].ticket_completed, scores[1].ticket_failed),
        (0, -5)
    );
    assert_eq!(outcomes(&eng, 1), [outcome("t_multi", false, None, -5)]);
    for (pi, s) in scores.iter().enumerate() {
        assert_eq!(
            s.total,
            s.route_points + s.ticket_completed + s.ticket_failed,
            "totale del giocatore {pi}"
        );
    }
}

/// A pari punti tra destinazioni collegate vale la prima dell'elenco.
#[test]
fn multi_destination_tie_takes_the_first_listed() {
    let tickets = vec![
        multi("t_tie", "a", &[("fr", 6), ("de", 6)]),
        classic("t_a", "a", "b", 1),
    ];
    let mut tickets = tickets;
    tickets.extend((0..13).map(|i| classic(&format!("t_fill{i:02}"), "e", "f", 1)));
    let mut eng = engine_with(ruleset("bottom", None), map_from(tickets));
    eng.set_player_tickets(0, &["t_tie", "t_a"]);
    claim(&mut eng, 0, "fr_a_1");
    claim(&mut eng, 0, "a_de_1");
    claim(&mut eng, 0, "a_b_1");
    let scores = finish_game(&mut eng);
    assert_eq!(
        outcomes(&eng, 0),
        [
            outcome("t_a", true, Some("b"), 1),
            outcome("t_tie", true, Some("fr"), 6),
        ]
    );
    assert_eq!(scores[0].ticket_completed, 7);
}

// ── Scarti rimossi a metà partita ────────────────────────────────────

fn complete_initial_draft(eng: &mut Engine) {
    for pi in 0..eng.num_players() {
        let keep: Vec<String> = eng.initial_tickets_for(pi).unwrap()[..2].to_vec();
        eng.apply(Action::ChooseTickets {
            player_index: pi,
            keep,
        })
        .unwrap();
    }
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
}

/// Pesca a metà partita e tiene il primo biglietto; restituisce offerti e scartati.
fn draw_and_keep_first(eng: &mut Engine) -> (Vec<String>, Vec<String>) {
    let pi = eng.current_player();
    eng.apply(Action::DrawTickets { player_index: pi }).unwrap();
    let offered = eng.pending_tickets().to_vec();
    eng.apply(Action::ChooseTickets {
        player_index: pi,
        keep: offered[..1].to_vec(),
    })
    .unwrap();
    let discarded = offered[1..].to_vec();
    (offered, discarded)
}

#[test]
fn removed_discards_leave_the_game_mid_game() {
    let mut eng = engine_with(ruleset("removed", None), map_from(base_tickets()));
    complete_initial_draft(&mut eng);
    assert_eq!(eng.ticket_deck_size(), 7, "15 biglietti, 8 distribuiti");

    let (offered, discarded) = draw_and_keep_first(&mut eng);
    assert_eq!(offered.len(), 3);
    assert_eq!(
        eng.ticket_deck_size(),
        4,
        "gli scartati non tornano nel mazzo"
    );
    assert!(
        discarded.iter().all(|t| !eng.ticket_pile().contains(t)),
        "gli scartati sono usciti dalla partita"
    );
}

#[test]
fn bottom_discards_return_to_the_pile_by_default() {
    let mut eng = engine();
    complete_initial_draft(&mut eng);
    let (_, discarded) = draw_and_keep_first(&mut eng);
    assert_eq!(eng.ticket_deck_size(), 6);
    let pile = eng.ticket_pile();
    assert_eq!(&pile[pile.len() - 2..], discarded.as_slice());
}

/// Il mazzo può finire: con meno di 3 biglietti si pescano quelli rimasti, poi la pesca è rifiutata.
#[test]
fn removed_discards_can_exhaust_the_deck() {
    let mut eng = engine_with(ruleset("removed", None), map_from(base_tickets()));
    complete_initial_draft(&mut eng);

    let mut offered_sizes = Vec::new();
    for _ in 0..3 {
        offered_sizes.push(draw_and_keep_first(&mut eng).0.len());
    }
    assert_eq!(offered_sizes, [3, 3, 1]);
    assert_eq!(eng.ticket_deck_size(), 0);

    let pi = eng.current_player();
    assert!(!eng.legal_moves(pi).can_draw_tickets);
    assert!(matches!(
        eng.apply(Action::DrawTickets { player_index: pi }),
        Err(EngineError::InvalidPayload { .. })
    ));
}

/// `removed` è ammesso anche con `mergeDecks`: gli scartati escono dalla pila unita.
#[test]
fn removed_discards_are_valid_with_merged_decks() {
    let mut rs = ruleset_json("removed", None);
    rs["ticketDecks"] = json!(["regular", "long"]);
    rs["ticketDraw"]["mergeDecks"] = json!(["long"]);
    let mut tickets = base_tickets();
    tickets.extend((0..3).map(|i| {
        let mut t = classic(&format!("t_long{i}"), "e", "f", 20);
        t["deck"] = json!("long");
        t
    }));
    let ruleset: Ruleset = serde_json::from_value(rs).unwrap();
    ruleset.validate(&map_from(tickets)).unwrap();
}

// ── Spareggio longestPath ────────────────────────────────────────────

/// P0 ha una rotta da 2, P1 due rotte da 1 non collegate: stessi punti rotta (2), nessun
/// biglietto; il percorso più lungo (2 contro 1) decide solo con `longestPath`, anche se il
/// ruleset non ha il bonus `longest_path`.
#[test]
fn longest_path_tie_breaker_works_without_the_bonus() {
    let play = |tie_breakers: Option<Value>| -> Vec<(i32, bool, i32)> {
        let mut eng = engine_with(ruleset("bottom", tie_breakers), map_from(base_tickets()));
        eng.set_player_tickets(0, &[]);
        eng.set_player_tickets(1, &[]);
        claim(&mut eng, 0, "e_f_1");
        claim(&mut eng, 1, "fr_a_1");
        claim(&mut eng, 1, "at_c_1");
        finish_game(&mut eng)
            .iter()
            .map(|s| (s.total, s.winner, s.longest_path_bonus))
            .collect()
    };

    assert_eq!(
        play(Some(json!(["longestPath"]))),
        [(2, true, 0), (2, false, 0)]
    );
    assert_eq!(
        play(Some(json!(["completedTickets", "longestPath"]))),
        [(2, true, 0), (2, false, 0)]
    );
    // Senza la chiave (default: biglietti completati, bonus del percorso) restano pari.
    assert_eq!(play(None), [(2, true, 0), (2, true, 0)]);
    assert_eq!(
        play(Some(json!(["longestPathBonus"]))),
        [(2, true, 0), (2, true, 0)]
    );
}

// ── Validazione ──────────────────────────────────────────────────────

fn validate_with(ticket: Value) -> Result<(), EngineInitError> {
    let mut tickets = base_tickets();
    tickets[0] = ticket;
    ruleset("bottom", None).validate(&map_from(tickets))
}

#[test]
fn ticket_forms_are_validated() {
    let ok = |t: Value| validate_with(t).unwrap();
    ok(multi("m", "a", &[("fr", 1), ("de", 2)]));
    ok(classic("c", "a", "fr", 3));

    let both = json!({"id": "x", "from": "a", "to": "fr", "points": 3, "deck": "regular",
        "sets": ["mini"], "destinations": [{"to": "de", "points": 1}, {"to": "it", "points": 2}]});
    assert_eq!(
        validate_with(both),
        Err(EngineInitError::TicketFormsBoth {
            ticket_id: "x".into()
        })
    );
    let both_partial = json!({"id": "x", "from": "a", "points": 3, "deck": "regular",
        "sets": ["mini"], "destinations": [{"to": "de", "points": 1}, {"to": "it", "points": 2}]});
    assert_eq!(
        validate_with(both_partial),
        Err(EngineInitError::TicketFormsBoth {
            ticket_id: "x".into()
        })
    );

    let neither = json!({"id": "x", "from": "a", "deck": "regular", "sets": ["mini"]});
    let to_only = json!({"id": "x", "from": "a", "to": "fr", "deck": "regular", "sets": ["mini"]});
    let points_only =
        json!({"id": "x", "from": "a", "points": 3, "deck": "regular", "sets": ["mini"]});
    for t in [neither, to_only, points_only] {
        assert_eq!(
            validate_with(t),
            Err(EngineInitError::TicketFormMissing {
                ticket_id: "x".into()
            })
        );
    }

    assert_eq!(
        validate_with(multi("x", "a", &[("fr", 1)])),
        Err(EngineInitError::TicketDestinationsTooFew {
            ticket_id: "x".into(),
            count: 1
        })
    );
    assert_eq!(
        validate_with(multi("x", "a", &[])),
        Err(EngineInitError::TicketDestinationsTooFew {
            ticket_id: "x".into(),
            count: 0
        })
    );
    assert_eq!(
        validate_with(multi("x", "a", &[("fr", 1), ("de", 2), ("fr", 3)])),
        Err(EngineInitError::TicketDestinationDuplicate {
            ticket_id: "x".into(),
            city_id: "fr".into()
        })
    );
    assert_eq!(
        validate_with(multi("x", "a", &[("fr", 1), ("a", 2)])),
        Err(EngineInitError::TicketSameCity {
            ticket_id: "x".into(),
            city_id: "a".into()
        })
    );
    assert_eq!(
        validate_with(multi("x", "a", &[("fr", 1), ("nowhere", 2)])),
        Err(EngineInitError::TicketCityUnknown {
            ticket_id: "x".into(),
            city_id: "nowhere".into()
        })
    );
    assert_eq!(
        validate_with(multi("x", "nowhere", &[("fr", 1), ("de", 2)])),
        Err(EngineInitError::TicketCityUnknown {
            ticket_id: "x".into(),
            city_id: "nowhere".into()
        })
    );
}

/// I biglietti classici restano `{from, to, points}` nel JSON; i multi-destinazione non hanno
/// né `to` né `points`.
#[test]
fn ticket_json_roundtrip_keeps_both_forms() {
    let classic_def: TicketDef = serde_json::from_value(classic("c", "a", "b", 3)).unwrap();
    let multi_def: TicketDef =
        serde_json::from_value(multi("m", "fr", &[("de", 5), ("it", 13)])).unwrap();
    assert_eq!(classic_def.max_points(), 3);
    assert_eq!(multi_def.max_points(), 13);
    assert_eq!(
        multi_def.destinations().collect::<Vec<_>>(),
        [("de", 5), ("it", 13)]
    );

    assert_eq!(
        serde_json::to_value(&classic_def).unwrap(),
        classic("c", "a", "b", 3)
    );
    assert_eq!(
        serde_json::to_value(&multi_def).unwrap(),
        multi("m", "fr", &[("de", 5), ("it", 13)])
    );
}

/// `ticketDraw.discards` ha default `bottom` e accetta solo `bottom` | `removed`.
#[test]
fn discards_field_defaults_to_bottom() {
    let mut rs = ruleset_json("bottom", None);
    rs["ticketDraw"].as_object_mut().unwrap().remove("discards");
    let parsed: Ruleset = serde_json::from_value(rs).unwrap();
    assert_eq!(
        parsed.ticket_draw.discards,
        ttr_core::types::TicketDiscards::Bottom
    );

    let mut bad = ruleset_json("bottom", None);
    bad["ticketDraw"]["discards"] = json!("burn");
    assert!(serde_json::from_value::<Ruleset>(bad).is_err());
}
