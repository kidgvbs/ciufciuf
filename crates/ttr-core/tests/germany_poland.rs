//! Regole di Germany e Poland: gruppi paralleli da 2–4 binari (anche sempre aperti), Paesi
//! come vicoli ciechi, prima mano e pesca dei biglietti Germany (mazzo unito), carte Paese e
//! spareggi.
//!
//! Le regole sui dati reali usano `data/{maps,rulesets}/{germany,poland}.json`; le carte
//! Paese e gli spareggi, che richiedono partite controllate fino a `GameOver`, usano una
//! mappa minima.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use serde_json::json;
use ttr_core::error::GamePhase;
use ttr_core::event::FinalScore;
use ttr_core::types::{MapData, Ruleset, TicketDef};
use ttr_core::{
    Action, DrawSource, Engine, EngineError, EngineInitError, GameEvent, RouteColor,
    default_player_colors,
};
use ttr_proto::CardColor;

// ── Fixture ──────────────────────────────────────────────────────────

fn load<T: DeserializeOwned>(path: &str) -> T {
    let text = std::fs::read_to_string(format!("../../data/{path}.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn germany() -> (Ruleset, MapData) {
    (load("rulesets/germany"), load("maps/germany"))
}

fn poland() -> (Ruleset, MapData) {
    (load("rulesets/poland"), load("maps/poland"))
}

fn engine(data: &(Ruleset, MapData), players: usize) -> Engine {
    engine_seed(data, players, 7)
}

fn engine_seed(data: &(Ruleset, MapData), players: usize, seed: u64) -> Engine {
    Engine::new(
        seed,
        data.0.clone(),
        data.1.clone(),
        (0..players).map(|i| format!("p{i}")).collect(),
        (0..players).map(|i| format!("P{i}")).collect(),
        default_player_colors(players),
        0,
    )
    .unwrap()
}

/// Il giocatore `pi` reclama la rotta nel suo turno (forzato), pagando con carte in mano.
fn claim(eng: &mut Engine, pi: usize, route_id: &str) -> Result<Vec<GameEvent>, EngineError> {
    let route = eng.get_route(route_id).unwrap().clone();
    let card = match RouteColor::parse(&route.color).unwrap() {
        RouteColor::Colored(card) => card,
        RouteColor::Gray => CardColor::Red,
    };
    let cards = vec![card; route.len as usize];
    eng.set_phase_for_test(GamePhase::PlayerTurn, pi);
    eng.set_player_hand(pi, cards.clone());
    eng.apply(Action::ClaimRoute {
        player_index: pi,
        route_id: route_id.into(),
        cards_discarded: cards,
        passengers: Default::default(),
    })
}

/// Rotte che `pi` può reclamare con una mano ricca di carte di ogni colore.
fn legal_routes(eng: &mut Engine, pi: usize) -> Vec<String> {
    eng.set_phase_for_test(GamePhase::PlayerTurn, pi);
    let mut hand = Vec::new();
    for card in ttr_core::types::TRAIN_COLORS {
        hand.extend(std::iter::repeat_n(card, 6));
    }
    eng.set_player_hand(pi, hand);
    eng.legal_moves(pi).routes
}

fn counts(pairs: &[(&str, u32)]) -> BTreeMap<String, u32> {
    pairs.iter().map(|(d, n)| (d.to_string(), *n)).collect()
}

fn draw(pi: usize) -> Action {
    Action::DrawTickets { player_index: pi }
}

fn choose(pi: usize, keep: &[String]) -> Action {
    Action::ChooseTickets {
        player_index: pi,
        keep: keep.to_vec(),
    }
}

fn deck_of(map: &MapData, ticket_id: &str) -> String {
    map.tickets
        .iter()
        .find(|t| t.id == ticket_id)
        .unwrap()
        .deck
        .clone()
}

fn is_completed(eng: &Engine, pi: usize, ticket_id: &str) -> Option<bool> {
    eng.player(pi).unwrap().tickets[ticket_id].completed
}

// ── F1: gruppi paralleli ─────────────────────────────────────────────

/// Gruppo da 3 binari (Germany): con 3 giocatori un binario preso blocca gli altri, con 4
/// i binari sono aperti a giocatori diversi; un giocatore non ne ha mai due.
#[test]
fn triple_group_blocks_at_three_players_and_opens_at_four() {
    let data = germany();

    let mut eng = engine(&data, 3);
    claim(&mut eng, 0, "dortmund_dusseldorf_1").unwrap();
    for lane in ["dortmund_dusseldorf_2", "dortmund_dusseldorf_3"] {
        assert!(
            matches!(
                claim(&mut eng, 1, lane),
                Err(EngineError::RouteBlockedDouble { .. })
            ),
            "{lane} va bloccato con 3 giocatori"
        );
    }
    assert!(matches!(
        claim(&mut eng, 0, "dortmund_dusseldorf_2"),
        Err(EngineError::SamePlayerDouble { .. })
    ));
    for pi in 0..3 {
        let legal = legal_routes(&mut eng, pi);
        assert!(!legal.iter().any(|r| r.starts_with("dortmund_dusseldorf_")));
        // Un altro gruppo resta aperto.
        assert!(legal.contains(&"dusseldorf_koln_1".to_string()), "{pi}");
    }

    let mut eng = engine(&data, 4);
    claim(&mut eng, 0, "dortmund_dusseldorf_1").unwrap();
    claim(&mut eng, 1, "dortmund_dusseldorf_2").unwrap();
    // Il terzo binario è ancora aperto agli altri, non a chi ne ha già uno.
    let p0 = legal_routes(&mut eng, 0);
    let p2 = legal_routes(&mut eng, 2);
    assert!(!p0.contains(&"dortmund_dusseldorf_3".to_string()));
    assert!(p2.contains(&"dortmund_dusseldorf_3".to_string()));
    assert!(matches!(
        claim(&mut eng, 1, "dortmund_dusseldorf_3"),
        Err(EngineError::SamePlayerDouble { .. })
    ));
    claim(&mut eng, 2, "dortmund_dusseldorf_3").unwrap();
    assert!(matches!(
        claim(&mut eng, 3, "dortmund_dusseldorf_1"),
        Err(EngineError::RouteAlreadyClaimed { .. })
    ));
    for pi in 0..4 {
        let legal = legal_routes(&mut eng, pi);
        assert!(!legal.iter().any(|r| r.starts_with("dortmund_dusseldorf_")));
    }
}

/// Gruppi `alwaysOpen` (rotte verso i Paesi di Poland) restano aperti con 2 giocatori; gli
/// altri gruppi doppi no.
#[test]
fn always_open_groups_stay_open_with_two_players() {
    let data = poland();
    let mut eng = engine(&data, 2);

    claim(&mut eng, 0, "bialorus_siedlce_1").unwrap();
    claim(&mut eng, 1, "bialorus_siedlce_2").unwrap();
    for pi in 0..2 {
        assert!(matches!(
            claim(&mut eng, pi, "bialorus_siedlce_3"),
            Err(EngineError::SamePlayerDouble { .. })
        ));
        assert!(!legal_routes(&mut eng, pi).contains(&"bialorus_siedlce_3".to_string()));
    }

    // Gruppo da 4 sempre aperto.
    claim(&mut eng, 0, "czechy_wroclaw_1").unwrap();
    claim(&mut eng, 1, "czechy_wroclaw_4").unwrap();

    // Gruppo doppio senza `alwaysOpen`: con 2 giocatori il secondo binario è bloccato.
    claim(&mut eng, 0, "bydgoszcz_gdansk_1").unwrap();
    assert!(matches!(
        claim(&mut eng, 1, "bydgoszcz_gdansk_2"),
        Err(EngineError::RouteBlockedDouble { .. })
    ));
    assert!(!legal_routes(&mut eng, 1).contains(&"bydgoszcz_gdansk_2".to_string()));

    // Con 4 giocatori lo stesso gruppo è aperto.
    let mut eng = engine(&data, 4);
    claim(&mut eng, 0, "bydgoszcz_gdansk_1").unwrap();
    claim(&mut eng, 1, "bydgoszcz_gdansk_2").unwrap();
}

// ── F2: Paesi come vicoli ciechi ─────────────────────────────────────

fn extra_ticket(id: &str, from: &str, to: &str) -> TicketDef {
    TicketDef {
        id: id.into(),
        from: from.into(),
        to: Some(to.into()),
        points: Some(4),
        destinations: None,
        deck: "regular".into(),
        sets: vec!["deutschland".into()],
    }
}

/// «Even if Freiburg and Saarbrücken are both connected to France, they are not
/// considered connected together»; un biglietto verso il Paese vale con qualunque
/// rotta che vi entra.
#[test]
fn two_routes_into_the_same_country_are_not_connected() {
    let (ruleset, mut map) = germany();
    map.tickets.push(extra_ticket(
        "freiburg_saarbrucken",
        "freiburg",
        "saarbrucken",
    ));
    map.tickets.push(extra_ticket(
        "freiburg_frankreich",
        "freiburg",
        "frankreich",
    ));
    map.tickets.push(extra_ticket(
        "frankreich_saarbrucken",
        "frankreich",
        "saarbrucken",
    ));
    let mut eng = engine(&(ruleset, map), 2);
    eng.set_player_tickets(
        0,
        &[
            "freiburg_saarbrucken",
            "freiburg_frankreich",
            "frankreich_saarbrucken",
        ],
    );

    claim(&mut eng, 0, "frankreich_freiburg_1").unwrap();
    assert_eq!(is_completed(&eng, 0, "freiburg_frankreich"), Some(true));
    assert_eq!(is_completed(&eng, 0, "frankreich_saarbrucken"), Some(false));

    claim(&mut eng, 0, "frankreich_saarbrucken_1").unwrap();
    assert_eq!(is_completed(&eng, 0, "frankreich_saarbrucken"), Some(true));
    assert_eq!(is_completed(&eng, 0, "freiburg_frankreich"), Some(true));
    assert_eq!(
        is_completed(&eng, 0, "freiburg_saarbrucken"),
        Some(false),
        "Freiburg e Saarbrücken non sono collegate attraverso la Francia"
    );
}

/// Biglietto Paese–Paese (Dänemark–Niederlande): serve una rotta verso ciascun Paese nella
/// stessa rete.
#[test]
fn country_to_country_ticket_needs_one_network() {
    let data = germany();
    let chain = [
        "danemark_kiel_1",
        "hamburg_kiel_1",
        "bremen_hamburg_1",
        "bremen_emden_1",
    ];

    let mut eng = engine(&data, 2);
    eng.set_player_tickets(0, &["danemark_niederlande"]);
    for route in chain {
        claim(&mut eng, 0, route).unwrap();
        assert_eq!(is_completed(&eng, 0, "danemark_niederlande"), Some(false));
    }
    claim(&mut eng, 0, "emden_niederlande_1").unwrap();
    assert_eq!(is_completed(&eng, 0, "danemark_niederlande"), Some(true));

    // Una rotta verso ciascun Paese, ma in reti separate: il biglietto non è completato.
    let mut eng = engine(&data, 2);
    eng.set_player_tickets(0, &["danemark_niederlande"]);
    claim(&mut eng, 0, "danemark_kiel_1").unwrap();
    claim(&mut eng, 0, "emden_niederlande_1").unwrap();
    assert_eq!(is_completed(&eng, 0, "danemark_niederlande"), Some(false));
    // Un'altra rotta verso lo stesso Paese non collega le due reti.
    claim(&mut eng, 0, "bremerhaven_danemark_1").unwrap();
    assert_eq!(is_completed(&eng, 0, "danemark_niederlande"), Some(false));
}

// ── F3: Germany, prima mano 2 lunghi + 3 corti e mazzo unito ─────────

fn deck_counts(map: &MapData, ids: &[String]) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for id in ids {
        *counts.entry(deck_of(map, id)).or_insert(0) += 1;
    }
    counts
}

/// Id dei biglietti in gioco in Germany (set `deutschland`).
fn in_play(map: &MapData) -> BTreeSet<String> {
    map.tickets
        .iter()
        .filter(|t| t.sets.iter().any(|s| s == "deutschland"))
        .map(|t| t.id.clone())
        .collect()
}

/// Dei biglietti offerti a `pi` nel draft, il primo lungo e il primo corto.
fn long_and_regular(eng: &Engine, map: &MapData, pi: usize) -> Vec<String> {
    let offered = eng.initial_tickets_for(pi).unwrap();
    ["long", "regular"]
        .iter()
        .map(|deck| {
            offered
                .iter()
                .find(|id| deck_of(map, id) == *deck)
                .unwrap()
                .clone()
        })
        .collect()
}

/// Engine Germany a 2 giocatori col draft finito (ognuno tiene un lungo e un corto) e al
/// turno di P0.
fn germany_after_draft(map: &MapData, seed: u64) -> Engine {
    let data = germany();
    let mut eng = engine_seed(&data, 2, seed);
    for pi in 0..2 {
        let keep = long_and_regular(&eng, map, pi);
        eng.apply(choose(pi, &keep)).unwrap();
    }
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
    eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
    eng
}

/// La prima mano è di 2 lunghi + 3 corti per giocatore, offerti subito; si tengono almeno 2
/// biglietti e almeno 1 corto: 1 lungo + 1 corto e 2 corti vanno bene, solo lunghi no.
#[test]
fn initial_hand_is_two_long_and_three_regular_with_keep_rules() {
    let data = germany();
    let map = data.1.clone();
    let mut eng = engine(&data, 2);

    let offered0 = eng.initial_tickets_for(0).unwrap().to_vec();
    let offered1 = eng.initial_tickets_for(1).unwrap().to_vec();
    for offered in [&offered0, &offered1] {
        assert_eq!(
            deck_counts(&map, offered),
            counts(&[("long", 2), ("regular", 3)])
        );
    }

    // Niente pesca nel draft: i biglietti sono già offerti.
    assert!(matches!(
        eng.apply(draw(0)),
        Err(EngineError::WrongState { .. })
    ));
    let legal = eng.legal_moves(0);
    assert!(!legal.can_draw_tickets);
    let choice = legal.ticket_choice.unwrap();
    assert_eq!(choice.keep_min, 2);
    let regular0: Vec<String> = offered0
        .iter()
        .filter(|id| deck_of(&map, id) == "regular")
        .cloned()
        .collect();
    // Solo il gruppo dei corti ha un minimo; i lunghi non hanno limiti.
    assert_eq!(choice.limits.len(), 1);
    assert_eq!(choice.limits[0].ticket_ids, regular0);
    assert_eq!((choice.limits[0].min, choice.limits[0].max), (1, 3));

    // Solo lunghi: rifiutato anche se sono 2; un solo biglietto: sotto il minimo; stato intatto.
    let before = eng.serialize_state();
    let longs0: Vec<String> = offered0
        .iter()
        .filter(|id| deck_of(&map, id) == "long")
        .cloned()
        .collect();
    assert!(matches!(
        eng.apply(choose(0, &longs0)),
        Err(EngineError::TicketChoiceBelowDeckMin { deck, kept: 0, min: 1 }) if deck == "regular"
    ));
    assert!(matches!(
        eng.apply(choose(0, &regular0[..1])),
        Err(EngineError::TicketChoiceBelowMin { .. })
    ));
    assert_eq!(eng.serialize_state(), before);

    // P0: 1 lungo + 1 corto; P1: 2 corti.
    eng.apply(choose(0, &long_and_regular(&eng, &map, 0)))
        .unwrap();
    assert_eq!(eng.phase(), GamePhase::InitialDraft);
    let regular1: Vec<String> = offered1
        .iter()
        .filter(|id| deck_of(&map, id) == "regular")
        .cloned()
        .collect();
    let events = eng.apply(choose(1, &regular1[..2])).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::InitialDraftComplete))
    );
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
}

/// Finito il draft, i biglietti rimasti di entrambi i mazzi formano una sola pila (nessun
/// biglietto perso, scarti del draft compresi) e `ticket_deck_size` la conta.
#[test]
fn draft_ends_with_one_merged_ticket_pile() {
    let map = germany().1;
    let data = germany();
    let mut eng = engine(&data, 2);
    // Con il draft in corso i mazzi sono ancora separati e il totale li somma.
    let all = in_play(&map);
    assert_eq!(eng.ticket_deck_size(), all.len() - 10);
    assert!(!eng.ticket_pile_of("long").is_empty());

    let keep0 = long_and_regular(&eng, &map, 0);
    let regular1: Vec<String> = eng
        .initial_tickets_for(1)
        .unwrap()
        .iter()
        .filter(|id| deck_of(&map, id) == "regular")
        .take(2)
        .cloned()
        .collect();
    eng.apply(choose(0, &keep0)).unwrap();
    assert_eq!(eng.ticket_deck_size(), all.len() - 10 + 3);
    eng.apply(choose(1, &regular1)).unwrap();

    let pile = eng.ticket_pile_of("regular").to_vec();
    assert!(eng.ticket_pile_of("long").is_empty());
    assert_eq!(pile.len(), all.len() - 4);
    assert_eq!(eng.ticket_deck_size(), pile.len());
    let kept: BTreeSet<String> = keep0.into_iter().chain(regular1).collect();
    let in_pile: BTreeSet<String> = pile.iter().cloned().collect();
    assert_eq!(in_pile.len(), pile.len(), "nessun duplicato");
    assert!(in_pile.is_disjoint(&kept));
    assert_eq!(in_pile.union(&kept).cloned().collect::<BTreeSet<_>>(), all);
    // Mescolati insieme: lunghi e corti si alternano in cima.
    let top: Vec<&String> = pile.iter().take(12).collect();
    assert!(top.iter().any(|id| deck_of(&map, id) == "long"));
    assert!(top.iter().any(|id| deck_of(&map, id) == "regular"));
}

/// A partita in corso DRAW_TICKETS pesca 4 biglietti dalla cima della pila unita (anche
/// lunghi e corti insieme); gli scartati vanno in fondo alla pila unita, nell'ordine offerto.
#[test]
fn mid_game_draw_takes_four_from_the_merged_pile_and_returns_discards_to_its_bottom() {
    let map = germany().1;
    let mut mixed_seeds = 0;
    for seed in 1..=40 {
        let mut eng = germany_after_draft(&map, seed);
        let before = eng.ticket_pile_of("regular").to_vec();
        let legal = eng.legal_moves(0);
        assert!(legal.can_draw_tickets);
        assert!(matches!(
            eng.apply(draw(1)),
            Err(EngineError::NotYourTurn { .. })
        ));

        eng.apply(draw(0)).unwrap();
        assert_eq!(eng.phase(), GamePhase::TicketChoice);
        let offered = eng.pending_tickets().to_vec();
        assert_eq!(offered, before[..4]);
        assert_eq!(eng.ticket_deck_size(), before.len() - 4);
        let types = deck_counts(&map, &offered);
        if types.len() < 2 {
            continue;
        }
        mixed_seeds += 1;

        let legal = eng.legal_moves(0);
        assert_eq!(legal.ticket_choice.unwrap().keep_min, 1);
        assert!(!legal.can_draw_tickets);
        assert!(matches!(
            eng.apply(choose(0, &[])),
            Err(EngineError::TicketChoiceBelowMin { .. })
        ));
        let kept = offered[1].clone();
        let discarded: Vec<String> = offered.iter().filter(|id| **id != kept).cloned().collect();
        eng.apply(choose(0, std::slice::from_ref(&kept))).unwrap();

        let after = eng.ticket_pile_of("regular");
        assert_eq!(after.len(), before.len() - 1);
        assert_eq!(&after[after.len() - 3..], discarded.as_slice());
        assert_eq!(&after[..after.len() - 3], &before[4..]);
        // Anche gli scartati lunghi tornano nella pila unita, non nel loro mazzo.
        assert!(eng.ticket_pile_of("long").is_empty());
        assert!(eng.player(0).unwrap().tickets.contains_key(&kept));
    }
    assert!(mixed_seeds > 0, "nessun seed con una pesca mista");
}

/// Con pochi biglietti rimasti si pescano quelli che restano (almeno 1); senza, la pesca non
/// è ammessa.
#[test]
fn draw_is_capped_by_the_remaining_tickets() {
    let map = germany().1;
    let mut eng = germany_after_draft(&map, 7);
    let two: Vec<String> = eng.ticket_pile_of("regular")[..2].to_vec();
    eng.set_ticket_pile("regular", two.clone());
    assert_eq!(eng.ticket_deck_size(), 2);
    assert!(eng.legal_moves(0).can_draw_tickets);
    eng.apply(draw(0)).unwrap();
    assert_eq!(eng.pending_tickets(), two.as_slice());
    assert_eq!(eng.ticket_deck_size(), 0);
    eng.apply(choose(0, &two[..1])).unwrap();
    assert_eq!(eng.ticket_pile_of("regular"), &two[1..]);
    assert_eq!(eng.ticket_deck_size(), 1);

    let mut eng = germany_after_draft(&map, 7);
    eng.set_ticket_pile("regular", Vec::new());
    assert_eq!(eng.ticket_deck_size(), 0);
    assert!(!eng.legal_moves(0).can_draw_tickets);
    assert!(eng.apply(draw(0)).is_err());
}

/// Stesso seed e stesse azioni: stesso stato, anche rigiocando il registro delle azioni.
#[test]
fn merged_pile_is_deterministic_and_replayable() {
    let map = germany().1;
    let play = |seed: u64| {
        let mut eng = germany_after_draft(&map, seed);
        eng.apply(draw(0)).unwrap();
        let keep = eng.pending_tickets()[..2].to_vec();
        eng.apply(choose(0, &keep)).unwrap();
        eng
    };
    let a = play(11);
    let b = play(11);
    assert_eq!(a.serialize_state(), b.serialize_state());
    assert_eq!(a.ticket_pile_of("regular"), b.ticket_pile_of("regular"));
    assert_ne!(
        a.ticket_pile_of("regular"),
        play(12).ticket_pile_of("regular")
    );

    let data = germany();
    let mut replay = engine_seed(&data, 2, 11);
    for record in a.action_log() {
        replay.apply(record.action.clone()).unwrap();
    }
    assert_eq!(replay.serialize_state(), a.serialize_state());
}

/// Le regole classiche non cambiano: un solo mazzo di pesca, e le azioni salvate con i campi
/// vecchi (`counts`) si leggono ancora.
#[test]
fn classic_rulesets_draw_from_one_deck_and_old_actions_still_load() {
    let data = poland();
    let mut eng = engine(&data, 2);
    eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
    assert!(eng.legal_moves(0).can_draw_tickets);
    eng.apply(draw(0)).unwrap();
    assert_eq!(eng.pending_tickets().len(), 3);

    let legacy: Action =
        serde_json::from_str(r#"{"DrawTickets":{"player_index":1,"counts":{"long":1}}}"#).unwrap();
    assert!(matches!(&legacy, Action::DrawTickets { player_index: 1 }));
    assert_eq!(
        serde_json::to_string(&legacy).unwrap(),
        r#"{"DrawTickets":{"player_index":1}}"#
    );
}

// ── Mappa minima: carte Paese e spareggi ─────────────────────────────

fn mini_map() -> MapData {
    let city = |id: &str, country: bool| json!({"id": id, "name": id, "country": country});
    let route = |id: &str, from: &str, to: &str, len: u32, double: bool| {
        json!({
            "id": id, "from": from, "to": to, "len": len, "color": "Gray",
            "double": double, "alwaysOpen": double
        })
    };
    let ticket = |id: &str, from: &str, to: &str, points: u32| {
        json!({"id": id, "from": from, "to": to, "points": points, "deck": "regular",
               "sets": ["mini"]})
    };
    let mut cities: Vec<_> = "abcdefghxy"
        .chars()
        .map(|c| city(&c.to_string(), false))
        .collect();
    cities.extend(["niemcy", "rosja", "czechy", "bialorus"].map(|c| city(c, true)));
    let routes = vec![
        // Spareggi: due giocatori con gli stessi punti rotta (3 ciascuno).
        route("a_b_1", "a", "b", 1, false),
        route("c_d_1", "c", "d", 1, false),
        route("e_f_1", "e", "f", 2, false),
        route("g_h_1", "g", "h", 2, false),
        // Carte Paese: due hub, x e y, con gruppi sempre aperti verso i Paesi.
        route("x_y_1", "x", "y", 1, true),
        route("x_y_2", "x", "y", 1, true),
        route("x_y_3", "x", "y", 1, true),
        route("niemcy_y_1", "niemcy", "y", 1, true),
        route("niemcy_y_2", "niemcy", "y", 1, true),
        route("niemcy_y_3", "niemcy", "y", 1, true),
        route("rosja_x_1", "rosja", "x", 1, true),
        route("rosja_x_2", "rosja", "x", 1, true),
        route("czechy_y_1", "czechy", "y", 1, false),
        route("bialorus_x_1", "bialorus", "x", 1, false),
    ];
    let mut tickets = vec![
        ticket("t_ab6", "a", "b", 6),
        ticket("t_fail5", "a", "h", 5),
        ticket("t_cd2", "c", "d", 2),
        ticket("t_gh2", "g", "h", 2),
        ticket("t_fail3", "c", "h", 3),
        ticket("t_ef", "e", "f", 1),
        ticket("t_ad", "a", "d", 1),
        ticket("t_be", "b", "e", 1),
        ticket("t_cf", "c", "f", 1),
        ticket("t_dg", "d", "g", 1),
    ];
    // Abbastanza biglietti per 4 giocatori (4 ciascuno) con `Ruleset::validate`.
    tickets.extend((0..8).map(|i| ticket(&format!("t_fill{i}"), "e", "f", 1)));
    serde_json::from_value(json!({"cities": cities, "routes": routes, "tickets": tickets})).unwrap()
}

fn mini_ruleset(tie_breakers: Option<serde_json::Value>, country_cards: bool) -> Ruleset {
    let mut ruleset = json!({
        "name": "Mini", "mode": "Standard", "ticketSet": "mini", "map": "mini",
        "players": {"min": 2, "max": 4}, "trainsPerPlayer": 45,
        "deck": {"perColor": 12, "locomotives": 14},
        "ticketDecks": ["regular"],
        "initialTickets": {"dealt": [{"deck": "regular", "count": 4}], "keepMin": 2},
        "ticketDraw": {"deck": "regular", "count": 3, "keepMin": 1},
        "setupDiscards": "reshuffleBottom",
        "routePoints": {"1": 1, "2": 2},
        "bonuses": [], "doubleRouteMinPlayers": 4, "finalRoundIncludesTrigger": true
    });
    if let Some(keys) = tie_breakers {
        ruleset["tieBreakers"] = keys;
    }
    if country_cards {
        // Come il regolamento di Poland: pile di Germania, Russia, Cechia e Bielorussia.
        ruleset["countryCards"] = json!({
            "niemcy": [7, 4, 3], "rosja": [7, 4, 3],
            "czechy": [10, 7, 4, 2], "bialorus": [6, 4, 2]
        });
    }
    serde_json::from_value(ruleset).unwrap()
}

/// Porta la partita a `GameOver` con pescate dal mazzo; restituisce i punteggi finali.
fn finish_game(eng: &mut Engine) -> Vec<FinalScore> {
    let current = eng.current_player();
    eng.set_phase_for_test(GamePhase::PlayerTurn, current);
    // Con due treni il giocatore di turno innesca l'ultimo giro alla fine della sua mossa.
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

fn country_cards_in(events: &[GameEvent]) -> Option<Vec<(String, u32)>> {
    events.iter().find_map(|e| match e {
        GameEvent::CountryCardsTaken { cards, .. } => Some(cards.clone()),
        _ => None,
    })
}

fn cards(pairs: &[(&str, u32)]) -> Vec<(String, u32)> {
    pairs.iter().map(|(c, v)| (c.to_string(), *v)).collect()
}

/// Esempio del regolamento di Poland (esempi 1–2): chi unisce due Paesi nella stessa rete
/// prende la carta in cima a ogni Paese della rete, le pile esaurite non danno nulla e unire
/// due proprie reti con una rotta interna non dà carte.
#[test]
fn country_cards_follow_the_poland_rulebook_example() {
    let map = mini_map();
    let mut eng = Engine::new(
        3,
        mini_ruleset(None, true),
        map,
        (0..4).map(|i| format!("p{i}")).collect(),
        (0..4).map(|i| format!("P{i}")).collect(),
        default_player_colors(4),
        0,
    )
    .unwrap();
    let top = |eng: &Engine, country: &str| eng.country_stacks()[country].clone();
    assert_eq!(top(&eng, "niemcy"), [7, 4, 3]);

    // A collega Russia e Germania: Germania 7 e Russia 7. Prima di unire i Paesi, e la
    // rotta interna, non danno carte.
    assert_eq!(
        country_cards_in(&claim(&mut eng, 0, "rosja_x_1").unwrap()),
        None
    );
    assert_eq!(
        country_cards_in(&claim(&mut eng, 0, "x_y_1").unwrap()),
        None
    );
    let events = claim(&mut eng, 0, "niemcy_y_1").unwrap();
    assert_eq!(
        country_cards_in(&events),
        Some(cards(&[("niemcy", 7), ("rosja", 7)]))
    );
    // L'evento precede il cambio di turno.
    let position = |pred: fn(&GameEvent) -> bool| events.iter().position(pred).unwrap();
    assert!(
        position(|e| matches!(e, GameEvent::RouteClaimed { .. }))
            < position(|e| matches!(e, GameEvent::CountryCardsTaken { .. }))
    );
    assert!(
        position(|e| matches!(e, GameEvent::CountryCardsTaken { .. }))
            < position(|e| matches!(e, GameEvent::TurnAdvanced { .. }))
    );
    assert_eq!(top(&eng, "niemcy"), [4, 3]);

    // B collega Germania e Cechia: Germania 4 e Cechia 10.
    assert_eq!(
        country_cards_in(&claim(&mut eng, 1, "czechy_y_1").unwrap()),
        None
    );
    assert_eq!(
        country_cards_in(&claim(&mut eng, 1, "niemcy_y_2").unwrap()),
        Some(cards(&[("czechy", 10), ("niemcy", 4)]))
    );

    // A estende la rete alla Bielorussia: Germania 3, Russia 4, Bielorussia 6 (le carte dei
    // Paesi già posseduti si prendono di nuovo).
    assert_eq!(
        country_cards_in(&claim(&mut eng, 0, "bialorus_x_1").unwrap()),
        Some(cards(&[("bialorus", 6), ("niemcy", 3), ("rosja", 4)]))
    );
    assert!(top(&eng, "niemcy").is_empty());

    // Un terzo giocatore che raggiunge la Germania non prende nulla per la Germania, ma la
    // rotta interna x–y che unisce le sue reti non dà carte.
    assert_eq!(
        country_cards_in(&claim(&mut eng, 2, "rosja_x_2").unwrap()),
        None
    );
    assert_eq!(
        country_cards_in(&claim(&mut eng, 2, "x_y_2").unwrap()),
        None
    );
    assert_eq!(
        country_cards_in(&claim(&mut eng, 2, "niemcy_y_3").unwrap()),
        Some(cards(&[("rosja", 3)]))
    );
    assert_eq!(eng.country_stacks()["rosja"], Vec::<u32>::new());

    let held = |eng: &Engine, pi: usize| -> BTreeMap<String, Vec<u32>> {
        eng.player(pi).unwrap().country_cards.clone()
    };
    assert_eq!(held(&eng, 0)["niemcy"], [7, 3]);
    assert_eq!(held(&eng, 0)["rosja"], [7, 4]);
    assert_eq!(held(&eng, 0)["bialorus"], [6]);
    assert_eq!(held(&eng, 1)["niemcy"], [4]);
    assert_eq!(held(&eng, 1)["czechy"], [10]);
    assert_eq!(held(&eng, 2)["rosja"], [3]);
    assert!(held(&eng, 3).is_empty());

    // Fine partita: le carte Paese entrano nel totale.
    let scores = finish_game(&mut eng);
    let country_points: Vec<i32> = scores.iter().map(|s| s.country_points).collect();
    assert_eq!(country_points, [27, 14, 3, 0]);
    for fs in &scores {
        assert_eq!(
            fs.total,
            fs.route_points
                + fs.ticket_completed
                + fs.ticket_failed
                + fs.longest_path_bonus
                + fs.globetrotter_bonus
                + fs.station_points
                + fs.country_points
        );
    }
}

fn mini_engine(map: MapData) -> Engine {
    Engine::new(
        3,
        mini_ruleset(None, true),
        map,
        (0..4).map(|i| format!("p{i}")).collect(),
        (0..4).map(|i| format!("P{i}")).collect(),
        default_player_colors(4),
        0,
    )
    .unwrap()
}

/// Esempio 2 del regolamento: due reti già premiate unite da una rotta interna, senza
/// Paesi nuovi, non danno carte.
#[test]
fn joining_two_rewarded_networks_gives_no_country_cards() {
    let mut eng = mini_engine(mini_map());
    claim(&mut eng, 0, "rosja_x_1").unwrap();
    assert!(country_cards_in(&claim(&mut eng, 0, "bialorus_x_1").unwrap()).is_some());
    claim(&mut eng, 0, "niemcy_y_1").unwrap();
    assert!(country_cards_in(&claim(&mut eng, 0, "czechy_y_1").unwrap()).is_some());
    assert_eq!(
        country_cards_in(&claim(&mut eng, 0, "x_y_1").unwrap()),
        None
    );
}

/// Una seconda rotta verso un Paese già nella rete premiata (seconda città di confine, o
/// rotta interna) non assegna carte di nuovo.
#[test]
fn extra_border_route_to_a_country_already_in_the_network_gives_no_cards() {
    let mut map = serde_json::to_value(mini_map()).unwrap();
    map["cities"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "z", "name": "z", "country": false}));
    for (id, from, to) in [("x_z_1", "x", "z"), ("niemcy_z_1", "niemcy", "z")] {
        map["routes"].as_array_mut().unwrap().push(json!({
            "id": id, "from": from, "to": to, "len": 1, "color": "Gray",
            "double": false, "alwaysOpen": false
        }));
    }
    let mut eng = mini_engine(serde_json::from_value(map).unwrap());
    claim(&mut eng, 0, "rosja_x_1").unwrap();
    claim(&mut eng, 0, "niemcy_y_1").unwrap();
    assert_eq!(
        country_cards_in(&claim(&mut eng, 0, "x_y_1").unwrap()),
        Some(cards(&[("niemcy", 7), ("rosja", 7)]))
    );
    assert_eq!(
        country_cards_in(&claim(&mut eng, 0, "x_z_1").unwrap()),
        None
    );
    assert_eq!(
        country_cards_in(&claim(&mut eng, 0, "niemcy_z_1").unwrap()),
        None
    );
}

/// K2: la stessa rete finale dà le stesse carte in qualunque ordine di presa (D38).
#[test]
fn poland_country_cards_do_not_depend_on_the_claim_order() {
    let border = ["gorzow_wielkopolski_niemcy_1", "olsztyn_rosja_1"];
    let inner = [
        "gdansk_olsztyn_1",
        "bydgoszcz_gdansk_1",
        "bydgoszcz_pila_1",
        "gorzow_wielkopolski_pila_1",
    ];
    let held = |order: &[&str]| {
        let mut eng = engine(&poland(), 2);
        for route in order {
            claim(&mut eng, 0, route).unwrap();
        }
        eng.player(0).unwrap().country_cards.clone()
    };
    let border_first: Vec<&str> = border.iter().chain(&inner).copied().collect();
    let inner_first: Vec<&str> = inner.iter().chain(&border).copied().collect();
    let expected: BTreeMap<String, Vec<u32>> = [
        ("niemcy".to_string(), vec![7]),
        ("rosja".to_string(), vec![7]),
    ]
    .into();
    assert_eq!(held(&border_first), expected);
    assert_eq!(held(&inner_first), expected);
}

/// Senza `countryCards` nessuna carta Paese, anche con rotte verso i Paesi.
#[test]
fn no_country_cards_without_the_ruleset_field() {
    let mut eng = Engine::new(
        3,
        mini_ruleset(None, false),
        mini_map(),
        vec!["p0".into(), "p1".into()],
        vec!["P0".into(), "P1".into()],
        default_player_colors(2),
        0,
    )
    .unwrap();
    claim(&mut eng, 0, "rosja_x_1").unwrap();
    claim(&mut eng, 0, "x_y_1").unwrap();
    let events = claim(&mut eng, 0, "niemcy_y_1").unwrap();
    assert_eq!(country_cards_in(&events), None);
    assert!(eng.country_stacks().is_empty());
}

/// Chi vince a pari totale dipende dagli spareggi del ruleset.
#[test]
fn tie_breakers_decide_the_winner_between_equal_totals() {
    // P0: 3 punti rotta; un biglietto da 6 completato, uno da 5 fallito → totale 4,
    //     1 biglietto completato per 6 punti.
    // P1: 3 punti rotta; due biglietti da 2 completati, uno da 3 fallito → totale 4,
    //     2 biglietti completati per 4 punti.
    let play = |tie_breakers: Option<serde_json::Value>| -> Vec<(i32, bool)> {
        let mut eng = Engine::new(
            5,
            mini_ruleset(tie_breakers, false),
            mini_map(),
            vec!["p0".into(), "p1".into()],
            vec!["P0".into(), "P1".into()],
            default_player_colors(2),
            0,
        )
        .unwrap();
        eng.set_player_tickets(0, &["t_ab6", "t_fail5"]);
        eng.set_player_tickets(1, &["t_cd2", "t_gh2", "t_fail3"]);
        claim(&mut eng, 0, "a_b_1").unwrap();
        claim(&mut eng, 1, "c_d_1").unwrap();
        claim(&mut eng, 0, "e_f_1").unwrap();
        claim(&mut eng, 1, "g_h_1").unwrap();
        finish_game(&mut eng)
            .iter()
            .map(|s| (s.total, s.winner))
            .collect()
    };

    // Default: più biglietti completati (2 contro 1).
    assert_eq!(play(None), [(4, false), (4, true)]);
    assert_eq!(
        play(Some(json!(["completedTickets"]))),
        [(4, false), (4, true)]
    );
    // Poland: biglietti completati, poi punti dei biglietti completati.
    assert_eq!(
        play(Some(json!(["completedTickets", "completedTicketPoints"]))),
        [(4, false), (4, true)]
    );
    // Punti dei biglietti completati prima del loro numero (6 contro 4).
    assert_eq!(
        play(Some(json!(["completedTicketPoints", "completedTickets"]))),
        [(4, true), (4, false)]
    );
    assert_eq!(
        play(Some(json!(["completedTicketPoints"]))),
        [(4, true), (4, false)]
    );
    // Senza spareggi, o con uno che non distingue, i pari restano tutti vincitori.
    assert_eq!(play(Some(json!([]))), [(4, true), (4, true)]);
    assert_eq!(
        play(Some(json!(["longestPathBonus"]))),
        [(4, true), (4, true)]
    );
}

/// `tieBreakers` con chiavi sconosciute o ripetute è rifiutato alla creazione.
#[test]
fn tie_breakers_with_unknown_or_repeated_keys_are_rejected() {
    let expect = |keys: serde_json::Value| {
        mini_ruleset(Some(keys), false)
            .validate(&mini_map())
            .unwrap_err()
    };
    assert_eq!(
        expect(json!(["stations"])),
        EngineInitError::UnknownTieBreaker {
            key: "stations".into()
        }
    );
    assert_eq!(
        expect(json!(["completedTickets", "completedTickets"])),
        EngineInitError::DuplicateTieBreaker {
            key: "completedTickets".into()
        }
    );
    assert!(
        mini_ruleset(Some(json!(["completedTickets", "longestPathBonus"])), false)
            .validate(&mini_map())
            .is_ok()
    );
}

// ── Validazione dei campi nuovi ──────────────────────────────────────

/// I dati spediti di Germany e Poland superano `Ruleset::validate`.
#[test]
fn shipped_germany_and_poland_data_validate() {
    for (ruleset, map) in [germany(), poland()] {
        ruleset.validate(&map).unwrap();
    }
}

/// I campi nuovi incoerenti hanno errori dedicati.
#[test]
fn new_fields_are_validated() {
    let (ruleset, map) = germany();
    let check = |mutate: fn(&mut Ruleset), expected: EngineInitError| {
        let mut rs = ruleset.clone();
        mutate(&mut rs);
        assert_eq!(rs.validate(&map).unwrap_err(), expected);
    };
    check(
        |rs| rs.ticket_draw.merge_decks = vec!["nessuno".into()],
        EngineInitError::MergeDeckUnknown {
            deck: "nessuno".into(),
        },
    );
    check(
        |rs| rs.ticket_draw.merge_decks = vec!["long".into(), "long".into()],
        EngineInitError::MergeDeckDuplicate {
            deck: "long".into(),
        },
    );
    check(
        |rs| rs.ticket_draw.merge_decks = vec!["regular".into()],
        EngineInitError::MergeDeckDuplicate {
            deck: "regular".into(),
        },
    );
    check(
        |rs| rs.setup_discards = ttr_core::types::SetupDiscards::ReshuffleBottom,
        EngineInitError::SetupDiscardsNeedOwnDeck,
    );
    check(
        |rs| rs.ticket_draw.keep_min = 9,
        EngineInitError::DrawKeepMinExceedsCount {
            keep_min: 9,
            count: 4,
        },
    );
    check(
        |rs| rs.initial_tickets.keep_min = 6,
        EngineInitError::InitialKeepMinExceedsDealt {
            keep_min: 6,
            dealt: 5,
        },
    );
    check(
        |rs| rs.initial_tickets.dealt[1].keep_min = 4,
        EngineInitError::DealtKeepMinExceedsCount {
            deck: "regular".into(),
            keep_min: 4,
            count: 3,
        },
    );
    check(
        |rs| {
            rs.initial_tickets.dealt[0].keep_min = 2;
            rs.initial_tickets.dealt[0].keep_max = Some(1);
        },
        EngineInitError::DealtKeepMinExceedsKeepMax {
            deck: "long".into(),
            keep_min: 2,
            keep_max: 1,
        },
    );
    check(
        |rs| {
            rs.initial_tickets.dealt[0].keep_min = 1;
            rs.initial_tickets.dealt[1].keep_min = 2;
        },
        EngineInitError::DealtKeepMinSumExceedsKeepMin {
            deck_min: 3,
            keep_min: 2,
        },
    );
    let mut rs = ruleset.clone();
    rs.initial_tickets.dealt[1].count = 13;
    assert!(matches!(
        rs.validate(&map).unwrap_err(),
        EngineInitError::TicketDeckTooSmall { deck, needed: 65, .. } if deck == "regular"
    ));

    let (ruleset, map) = poland();
    let mut rs = ruleset.clone();
    rs.country_cards.insert("krakow".into(), vec![1]);
    assert_eq!(
        rs.validate(&map).unwrap_err(),
        EngineInitError::CountryCardsUnknownCountry {
            city_id: "krakow".into()
        }
    );
    let mut bad_map = map.clone();
    bad_map
        .routes
        .iter_mut()
        .find(|r| r.id == "czechy_wroclaw_2")
        .unwrap()
        .always_open = false;
    assert!(matches!(
        ruleset.validate(&bad_map).unwrap_err(),
        EngineInitError::DoubleGroupAlwaysOpenMismatch { .. }
    ));
}

// ── F4: passeggeri (Germany 2017) ────────────────────────────────────

fn germany_2017() -> (Ruleset, MapData) {
    (load("rulesets/germany_2017"), load("maps/germany"))
}

/// Passeggeri sulle città, sommati per colore.
fn passengers_on_board(eng: &Engine) -> BTreeMap<String, u32> {
    let mut total = BTreeMap::new();
    for colors in eng.passengers_on_cities().values() {
        for (color, n) in colors {
            *total.entry(color.clone()).or_insert(0) += n;
        }
    }
    total
}

/// Il setup distribuisce i 60 passeggeri (10 per colore) secondo `placement`, in modo
/// deterministico per seed; la modalità Deutschland non ne ha.
#[test]
fn passengers_are_placed_by_the_setup_diagram() {
    let data = germany_2017();
    let config = data.0.passengers.clone().unwrap();
    let eng = engine(&data, 3);
    for (city, count) in &config.placement {
        let on_city: u32 = eng.passengers_on_cities()[city].values().sum();
        assert_eq!(on_city, *count, "passeggeri su {city}");
    }
    let expected: BTreeMap<String, u32> = config.colors.iter().map(|c| (c.clone(), 10)).collect();
    assert_eq!(passengers_on_board(&eng), expected);
    assert_eq!(
        engine(&data, 3).passengers_on_cities(),
        eng.passengers_on_cities()
    );
    assert!(engine(&germany(), 3).passengers_on_cities().is_empty());
}

/// Al claim si prende un passeggero per estremo: il colore va scelto solo dove la città
/// ne ha di più colori, un colore assente è rifiutato e una città vuota non dà nulla.
#[test]
fn claiming_takes_one_passenger_per_end_and_asks_only_for_mixed_cities() {
    let data = germany_2017();
    let mut eng = engine(&data, 2);
    let route_id = "berlin_magdeburg_1";
    let berlin = eng.passengers_on_cities()["berlin"].clone();
    let magdeburg = eng.passengers_on_cities()["magdeburg"].clone();
    assert!(berlin.len() > 1, "con 5 passeggeri Berlino ha più colori");
    assert_eq!(magdeburg.values().sum::<u32>(), 1);

    let claim_with = |eng: &mut Engine, picks: &[(&str, &str)]| {
        eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
        eng.set_player_hand(0, vec![CardColor::Red; 3]);
        eng.apply(Action::ClaimRoute {
            player_index: 0,
            route_id: route_id.into(),
            cards_discarded: vec![CardColor::Red; 3],
            passengers: picks
                .iter()
                .map(|(c, col)| (c.to_string(), col.to_string()))
                .collect(),
        })
    };
    assert!(matches!(
        claim_with(&mut eng, &[]).unwrap_err(),
        EngineError::PassengerChoiceRequired { ref route_id, ref city_id }
            if route_id == "berlin_magdeburg_1" && city_id == "berlin"
    ));
    let absent = data
        .0
        .passengers
        .as_ref()
        .unwrap()
        .colors
        .iter()
        .find(|c| !berlin.contains_key(*c))
        .cloned()
        .unwrap();
    assert!(matches!(
        claim_with(&mut eng, &[("berlin", &absent)]).unwrap_err(),
        EngineError::PassengerNotAvailable { .. }
    ));

    let chosen = berlin.keys().next().unwrap().clone();
    let auto = magdeburg.keys().next().unwrap().clone();
    let events = claim_with(&mut eng, &[("berlin", &chosen)]).unwrap();
    let taken = events.iter().find_map(|e| match e {
        GameEvent::PassengersTaken { taken, .. } => Some(taken.clone()),
        _ => None,
    });
    assert_eq!(
        taken,
        Some(vec![
            ("berlin".to_string(), chosen.clone()),
            ("magdeburg".to_string(), auto.clone()),
        ])
    );
    assert!(!eng.passengers_on_cities().contains_key("magdeburg"));
    assert_eq!(
        eng.passengers_on_cities()["berlin"].values().sum::<u32>(),
        4
    );
    let held = &eng.player(0).unwrap().passengers;
    assert_eq!(held.values().sum::<u32>(), 2);

    // Magdeburg ora è vuota: la rotta per Leipzig prende solo il passeggero di Leipzig.
    let leipzig_before: u32 = eng.passengers_on_cities()["leipzig"].values().sum();
    eng.set_phase_for_test(GamePhase::PlayerTurn, 1);
    eng.set_player_hand(1, vec![CardColor::Yellow; 2]);
    let leipzig = &eng.passengers_on_cities()["leipzig"];
    let pick: BTreeMap<String, String> = (leipzig.len() > 1)
        .then(|| {
            (
                "leipzig".to_string(),
                leipzig.keys().next().unwrap().clone(),
            )
        })
        .into_iter()
        .collect();
    eng.apply(Action::ClaimRoute {
        player_index: 1,
        route_id: "leipzig_magdeburg_1".into(),
        cards_discarded: vec![CardColor::Yellow; 2],
        passengers: pick,
    })
    .unwrap();
    let leipzig_after: u32 = eng
        .passengers_on_cities()
        .get("leipzig")
        .map_or(0, |c| c.values().sum());
    assert_eq!(leipzig_after, leipzig_before - 1);
    assert_eq!(eng.player(1).unwrap().passengers.values().sum::<u32>(), 1);
}

/// Spareggio `passengers`: a pari totale vince chi ha raccolto più passeggeri.
#[test]
fn passengers_tie_breaker_counts_collected_passengers() {
    let play = |tie_breakers: serde_json::Value| -> Vec<(i32, i32, bool)> {
        let mut ruleset = mini_ruleset(Some(tie_breakers), false);
        // Un solo colore e premi a zero: i totali restano pari (1 punto rotta a testa).
        ruleset.passengers = Some(
            serde_json::from_value(json!({
                "colors": ["Red"], "perColor": 3,
                "placement": {"a": 1, "b": 1, "c": 1},
                "majorityPoints": [0, 0]
            }))
            .unwrap(),
        );
        let mut eng = Engine::new(
            5,
            ruleset,
            mini_map(),
            vec!["p0".into(), "p1".into()],
            vec!["P0".into(), "P1".into()],
            default_player_colors(2),
            0,
        )
        .unwrap();
        eng.set_player_tickets(0, &[]);
        eng.set_player_tickets(1, &[]);
        claim(&mut eng, 0, "a_b_1").unwrap();
        claim(&mut eng, 1, "c_d_1").unwrap();
        finish_game(&mut eng)
            .iter()
            .map(|s| (s.total, s.passenger_points, s.winner))
            .collect()
    };
    assert_eq!(play(json!(["passengers"])), [(1, 0, true), (1, 0, false)]);
    assert_eq!(play(json!([])), [(1, 0, true), (1, 0, true)]);
}

/// Le maggioranze entrano nel totale finale (dati reali Germany 2017).
#[test]
fn passenger_majorities_are_added_to_the_final_total() {
    let data = germany_2017();
    let mut eng = engine(&data, 2);
    eng.set_player_tickets(0, &[]);
    eng.set_player_tickets(1, &[]);
    eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
    eng.set_player_hand(0, vec![CardColor::Red; 3]);
    let berlin = eng.passengers_on_cities()["berlin"].clone();
    let chosen = berlin.keys().next().unwrap().clone();
    eng.apply(Action::ClaimRoute {
        player_index: 0,
        route_id: "berlin_magdeburg_1".into(),
        cards_discarded: vec![CardColor::Red; 3],
        passengers: [("berlin".to_string(), chosen)].into_iter().collect(),
    })
    .unwrap();
    let scores = finish_game(&mut eng);
    // P0 ha 1 o 2 colori (Berlino + Magdeburgo), da solo: 20 punti per colore.
    let colors = eng.player(0).unwrap().passengers.len() as i32;
    assert_eq!(scores[0].passenger_points, 20 * colors);
    assert_eq!(scores[1].passenger_points, 0);
    assert_eq!(
        scores[0].total,
        scores[0].route_points + scores[0].passenger_points
    );
}

/// Schema di preparazione 2017: Frankfurt 4 (doppie verso Würzburg e Mannheim), Mainz 1,
/// totale 60.
#[test]
fn passenger_placement_matches_the_setup_diagram() {
    let config = germany_2017().0.passengers.unwrap();
    assert_eq!(config.placement.values().sum::<u32>(), 60);
    assert_eq!(config.placement["frankfurt"], 4);
    assert_eq!(config.placement["mainz"], 1);
}

/// Europe 1912/Mega/Grandi Città: Angora–Parigi vale 17 (come il percorso minimo).
#[test]
fn angora_paris_ticket_is_worth_seventeen() {
    let map: MapData = load("maps/europe");
    let ticket = map.tickets.iter().find(|t| t.id == "angora_paris").unwrap();
    assert_eq!(ticket.max_points(), 17);
}
