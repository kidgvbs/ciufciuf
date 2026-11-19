//! Bonus `mandala` (India, Grand Tour): un biglietto qualifica se le sue due città sono
//! collegate da almeno due percorsi continui di rotte del giocatore senza rotte in comune
//! (flusso massimo ≥ 2 sulla rete delle sue rotte). Bonus cumulativo per numero di biglietti
//! qualificati, validazione della tabella.
//!
//! Mappa e ruleset sono costruiti in memoria: un K4 (a, b, c, d), un «otto» (due cicli che
//! condividono la città `m`), un ponte, una catena e un Paese a due varchi.

use serde_json::{Value, json};
use ttr_core::error::GamePhase;
use ttr_core::event::FinalScore;
use ttr_core::scoring::mandala_points;
use ttr_core::types::{MapData, Ruleset};
use ttr_core::{Action, DrawSource, Engine, EngineInitError, GameEvent, default_player_colors};
use ttr_proto::CardColor;

const TABLE: [u32; 5] = [5, 10, 20, 30, 40];

// ── Fixture ──────────────────────────────────────────────────────────

fn map_json() -> Value {
    let city = |id: &str, country: bool| json!({"id": id, "name": id, "country": country});
    let route = |id: &str| {
        let (from, to) = id.split_once('_').unwrap();
        json!({"id": id, "from": from, "to": to, "len": 1, "color": "Gray", "double": false})
    };
    let cities: Vec<Value> = [
        "a", "b", "c", "d", "s", "x", "y", "m", "z", "w", "t", "u", "p", "q", "e", "f",
    ]
    .iter()
    .map(|id| city(id, false))
    .chain([city("xx", true)])
    .collect();
    let routes: Vec<Value> = [
        // K4
        "a_b", "a_c", "a_d", "b_c", "b_d", "c_d",
        // otto: s–m per x e y, m–t per z e w
        "s_x", "x_m", "s_y", "y_m", "m_z", "z_t", "m_w", "w_t", // ponte e catena
        "t_u", "p_q", // Paese con due varchi, uniti da e–f
        "xx_e", "xx_f", "e_f",
    ]
    .iter()
    .map(|id| route(id))
    .collect();

    let ticket = |id: &str, from: &str, to: &str| json!({"id": id, "from": from, "to": to, "points": 1, "deck": "regular", "sets": ["mini"]});
    let multi = |id: &str, from: &str, dests: &[&str]| {
        let dests: Vec<Value> = dests
            .iter()
            .map(|to| json!({"to": to, "points": 1}))
            .collect();
        json!({"id": id, "from": from, "destinations": dests, "deck": "regular", "sets": ["mini"]})
    };
    let mut tickets = vec![
        ticket("t_a_b", "a", "b"),
        ticket("t_a_c", "a", "c"),
        ticket("t_a_d", "a", "d"),
        ticket("t_b_c", "b", "c"),
        ticket("t_b_d", "b", "d"),
        ticket("t_c_d", "c", "d"),
        ticket("t_s_t", "s", "t"),
        ticket("t_s_u", "s", "u"),
        ticket("t_p_q", "p", "q"),
        ticket("t_xx_e", "xx", "e"),
        multi("t_multi_hit", "s", &["u", "t"]),
        multi("t_multi_miss", "s", &["u", "p"]),
    ];
    tickets.extend((0..13).map(|i| ticket(&format!("t_fill{i:02}"), "e", "f")));
    json!({"cities": cities, "routes": routes, "tickets": tickets})
}

fn map() -> MapData {
    serde_json::from_value(map_json()).unwrap()
}

fn ruleset_json(bonuses: Value) -> Value {
    json!({
        "name": "Mini", "mode": "Standard", "ticketSet": "mini", "map": "mini",
        "players": {"min": 2, "max": 2}, "trainsPerPlayer": 45,
        "deck": {"perColor": 12, "locomotives": 14},
        "ticketDecks": ["regular"],
        "initialTickets": {"dealt": [{"deck": "regular", "count": 4}], "keepMin": 2},
        "ticketDraw": {"deck": "regular", "count": 3, "keepMin": 1},
        "setupDiscards": "removed",
        "routePoints": {"1": 1},
        "bonuses": bonuses, "doubleRouteMinPlayers": 3, "finalRoundIncludesTrigger": true
    })
}

fn mandala_bonus() -> Value {
    json!({"type": "mandala", "label": "Grand Tour", "points": 40, "table": TABLE})
}

fn ruleset(bonuses: Value) -> Ruleset {
    serde_json::from_value(ruleset_json(bonuses)).unwrap()
}

fn engine_with(ruleset: Ruleset) -> Engine {
    Engine::new(
        5,
        ruleset,
        map(),
        vec!["p0".into(), "p1".into()],
        vec!["P0".into(), "P1".into()],
        default_player_colors(2),
        0,
    )
    .unwrap()
}

fn engine() -> Engine {
    engine_with(ruleset(json!([mandala_bonus()])))
}

fn claim(eng: &mut Engine, pi: usize, route_id: &str) {
    let cards = vec![CardColor::Red];
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

fn claim_all(eng: &mut Engine, pi: usize, routes: &[&str]) {
    for route in routes {
        claim(eng, pi, route);
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

/// Id dei biglietti del giocatore che contano per il bonus, in ordine di id.
fn qualifying(eng: &Engine, pi: usize) -> Vec<String> {
    eng.final_ticket_results(pi)
        .iter()
        .filter(|t| t.mandala)
        .map(|t| t.ticket_id.clone())
        .collect()
}

const K4: [&str; 6] = ["a_b", "a_c", "a_d", "b_c", "b_d", "c_d"];
const K4_TICKETS: [&str; 6] = ["t_a_b", "t_a_c", "t_a_d", "t_b_c", "t_b_d", "t_c_d"];
const EIGHT: [&str; 8] = ["s_x", "x_m", "s_y", "y_m", "m_z", "z_t", "m_w", "w_t"];

// ── Qualifica dei biglietti ──────────────────────────────────────────

/// Un ciclo a–b–c–a collega a e c con due percorsi (a–c e a–b–c).
#[test]
fn cycle_qualifies_its_endpoints() {
    let mut eng = engine();
    eng.set_player_tickets(0, &["t_a_c", "t_a_d"]);
    claim_all(&mut eng, 0, &["a_b", "b_c", "a_c"]);
    let scores = finish_game(&mut eng);
    assert_eq!(qualifying(&eng, 0), ["t_a_c"]);
    assert_eq!(scores[0].mandala_bonus, 5);
}

/// Un percorso che passa da un ponte (c–d) non ha un secondo percorso disgiunto.
#[test]
fn path_through_a_bridge_does_not_qualify() {
    let mut eng = engine();
    eng.set_player_tickets(0, &["t_a_c", "t_a_d", "t_b_d"]);
    claim_all(&mut eng, 0, &["a_b", "b_c", "a_c", "c_d"]);
    let scores = finish_game(&mut eng);
    assert_eq!(qualifying(&eng, 0), ["t_a_c"]);
    assert_eq!(scores[0].mandala_bonus, 5);
}

/// Due cicli che condividono la città `m` (nessuna rotta): s–t ha due percorsi disgiunti
/// che si incontrano in `m`. Senza una delle rotte dell'otto, o con un ponte oltre `t`,
/// non qualifica.
#[test]
fn figure_eight_sharing_a_city_qualifies() {
    let mut eng = engine();
    eng.set_player_tickets(0, &["t_s_t", "t_s_u"]);
    claim_all(&mut eng, 0, &EIGHT);
    claim(&mut eng, 0, "t_u");
    let scores = finish_game(&mut eng);
    assert_eq!(qualifying(&eng, 0), ["t_s_t"], "s–u passa dal ponte t–u");
    assert_eq!(scores[0].mandala_bonus, 5);

    let mut eng = engine();
    eng.set_player_tickets(0, &["t_s_t"]);
    claim_all(&mut eng, 0, &EIGHT[..7]);
    let scores = finish_game(&mut eng);
    assert!(qualifying(&eng, 0).is_empty(), "manca w–t");
    assert_eq!(scores[0].mandala_bonus, 0);
}

/// Un solo percorso semplice, anche lungo, non basta; due percorsi interi su una catena no.
#[test]
fn single_path_does_not_qualify() {
    let mut eng = engine();
    eng.set_player_tickets(0, &["t_s_t", "t_p_q"]);
    claim_all(&mut eng, 0, &["s_x", "x_m", "m_z", "z_t", "p_q"]);
    finish_game(&mut eng);
    assert!(qualifying(&eng, 0).is_empty());
}

/// Contano solo le rotte del giocatore: con una rotta del ciclo in mano all'avversario
/// nessuno dei due ha i due percorsi.
#[test]
fn opponent_routes_do_not_count() {
    let mut eng = engine();
    eng.set_player_tickets(0, &["t_a_c"]);
    eng.set_player_tickets(1, &["t_a_c"]);
    claim_all(&mut eng, 0, &["a_b", "b_c"]);
    claim(&mut eng, 1, "a_c");
    let scores = finish_game(&mut eng);
    assert!(qualifying(&eng, 0).is_empty());
    assert!(qualifying(&eng, 1).is_empty());
    assert_eq!((scores[0].mandala_bonus, scores[1].mandala_bonus), (0, 0));
}

/// Un Paese non è mai un estremo che qualifica: i varchi sono nodi distinti, quindi xx–e
/// via il varco xx_e e via xx_f–f–e non sono due percorsi dallo stesso nodo.
#[test]
fn country_endpoints_never_qualify() {
    let mut eng = engine();
    eng.set_player_tickets(0, &["t_xx_e"]);
    claim_all(&mut eng, 0, &["xx_e", "xx_f", "e_f"]);
    let scores = finish_game(&mut eng);
    assert!(qualifying(&eng, 0).is_empty());
    assert_eq!(scores[0].mandala_bonus, 0);
}

/// Biglietto multi-destinazione: basta una destinazione che qualifica.
#[test]
fn multi_destination_qualifies_with_any_destination() {
    let mut eng = engine();
    eng.set_player_tickets(0, &["t_multi_hit", "t_multi_miss"]);
    claim_all(&mut eng, 0, &EIGHT);
    claim(&mut eng, 0, "t_u");
    finish_game(&mut eng);
    assert_eq!(qualifying(&eng, 0), ["t_multi_hit"]);
}

// ── Tabella e totale ─────────────────────────────────────────────────

#[test]
fn table_lookup_is_cumulative_and_saturates() {
    let expected = [0, 5, 10, 20, 30, 40, 40, 40, 40];
    for (count, &points) in expected.iter().enumerate() {
        assert_eq!(mandala_points(&TABLE, count), points, "{count} biglietti");
    }
    assert_eq!(mandala_points(&TABLE, 1_000), 40);
    assert_eq!(mandala_points(&[], 3), 0);
}

/// Con un K4 ogni coppia ha tre percorsi disgiunti: 6 biglietti qualificati, il sesto non
/// aggiunge nulla (40, non di più); 3 biglietti valgono 20. Il bonus si somma al totale.
#[test]
fn bonus_saturates_beyond_five_tickets_and_adds_to_total() {
    for (tickets, bonus) in [(3, 20), (5, 40), (6, 40)] {
        let mut eng = engine();
        eng.set_player_tickets(0, &K4_TICKETS[..tickets]);
        claim_all(&mut eng, 0, &K4);
        let scores = finish_game(&mut eng);
        assert_eq!(qualifying(&eng, 0).len(), tickets);
        let s = &scores[0];
        assert_eq!(s.mandala_bonus, bonus, "{tickets} biglietti");
        assert_eq!(
            s.total,
            s.route_points + s.ticket_completed + s.ticket_failed + bonus,
            "il bonus entra nel totale"
        );
    }
}

/// Senza la voce `mandala` nel ruleset nessun biglietto è marcato e il bonus è 0.
#[test]
fn no_bonus_without_the_ruleset_entry() {
    let mut eng = engine_with(ruleset(json!([])));
    eng.set_player_tickets(0, &K4_TICKETS);
    claim_all(&mut eng, 0, &K4);
    let scores = finish_game(&mut eng);
    assert!(qualifying(&eng, 0).is_empty());
    let s = &scores[0];
    assert_eq!(s.mandala_bonus, 0);
    assert_eq!(
        s.total,
        s.route_points + s.ticket_completed + s.ticket_failed
    );
}

/// Stessa partita, stesso esito: nessuna dipendenza dall'ordine di iterazione.
#[test]
fn scoring_is_deterministic() {
    let run = || {
        let mut eng = engine();
        eng.set_player_tickets(0, &K4_TICKETS);
        claim_all(&mut eng, 0, &["c_d", "a_b", "b_d", "a_c", "b_c", "a_d"]);
        let scores = finish_game(&mut eng);
        (scores[0].mandala_bonus, qualifying(&eng, 0))
    };
    assert_eq!(run(), run());
    assert_eq!(run().0, 40);
}

// ── Validazione ──────────────────────────────────────────────────────

fn validate(bonuses: Value) -> Result<(), EngineInitError> {
    ruleset(bonuses).validate(&map())
}

fn mandala(points: u32, table: &[u32]) -> Value {
    json!({"type": "mandala", "label": "Grand Tour", "points": points, "table": table})
}

#[test]
fn valid_mandala_table_is_accepted() {
    assert_eq!(validate(json!([mandala(40, &TABLE)])), Ok(()));
    assert_eq!(
        validate(json!([mandala(5, &[5, 5])])),
        Ok(()),
        "non decrescente"
    );
    assert_eq!(validate(json!([mandala(7, &[7])])), Ok(()));
}

#[test]
fn bad_mandala_tables_are_rejected() {
    assert_eq!(
        validate(json!([mandala(0, &[])])),
        Err(EngineInitError::MandalaTableEmpty)
    );
    let no_table = json!({"type": "mandala", "label": "Grand Tour", "points": 40});
    assert_eq!(
        validate(json!([no_table])),
        Err(EngineInitError::MandalaTableEmpty)
    );
    assert_eq!(
        validate(json!([mandala(10, &[5, 20, 10])])),
        Err(EngineInitError::MandalaTableDecreasing { index: 2 })
    );
    assert_eq!(
        validate(json!([mandala(30, &TABLE)])),
        Err(EngineInitError::MandalaPointsMismatch {
            points: 30,
            last: 40
        })
    );
}

#[test]
fn table_is_only_allowed_on_mandala() {
    let bonus = json!({"type": "longest_path", "label": "LP", "points": 10, "table": [10]});
    assert_eq!(
        validate(json!([bonus])),
        Err(EngineInitError::BonusTableNotAllowed {
            bonus_type: "longest_path".into()
        })
    );
}

#[test]
fn engine_new_rejects_a_bad_table() {
    let result = Engine::new(
        5,
        ruleset(json!([mandala(40, &[5, 3])])),
        map(),
        vec!["p0".into(), "p1".into()],
        vec!["P0".into(), "P1".into()],
        default_player_colors(2),
        0,
    );
    assert_eq!(
        result.err(),
        Some(EngineInitError::MandalaTableDecreasing { index: 1 })
    );
}

/// `table` è omessa dalla serializzazione quando vuota: il catalogo dei ruleset esistenti
/// non cambia.
#[test]
fn table_is_skipped_when_empty() {
    let rs = ruleset(json!([
        {"type": "longest_path", "label": "LP", "points": 10},
        mandala_bonus()
    ]));
    let out = serde_json::to_value(&rs.bonuses).unwrap();
    assert_eq!(
        out[0],
        json!({"type": "longest_path", "label": "LP", "points": 10})
    );
    assert_eq!(out[1], mandala_bonus());
}
