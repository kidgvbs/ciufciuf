//! Carte Traghetto e bonus Regioni (Italy): pesca (limite in mano, turno intero, mai nella
//! seconda pescata, anche nell'ultimo giro), ritorno nel mazzetto, pagamento delle onde con
//! 1 o 2 onde per carta e rifiuti, Regioni per rete (reti separate, Regioni doppie complete e
//! incomplete, soglie, Paesi), validazione dei dati e compatibilità dei save.
//!
//! Mappa e ruleset sono costruiti in memoria; i dati veri di Italy sono controllati in coda.

use serde_json::{Value, json};
use ttr_core::error::GamePhase;
use ttr_core::event::FinalScore;
use ttr_core::types::{MapData, Ruleset};
use ttr_core::{
    Action, DrawSource, Engine, EngineError, EngineInitError, GameEvent, default_player_colors,
};
use ttr_proto::CardColor;
use ttr_proto::CardColor::{Blue, Locomotive, Red};

// ── Fixture ──────────────────────────────────────────────────────────

/// Catena `c00`…`c17`, una Regione per città (`r00`…`r16`; `c17` è ancora in `r16`).
const CHAIN: usize = 18;

fn region_of(index: usize) -> String {
    format!("r{:02}", index.min(16))
}

fn chain_city(index: usize) -> String {
    format!("c{index:02}")
}

fn chain_route(index: usize) -> String {
    format!("{}_{}", chain_city(index), chain_city(index + 1))
}

/// Id, estremi, lunghezza, onde.
const SPECIAL_ROUTES: &[(&str, &str, &str, u32, u32)] = &[
    ("w1_w2", "w1", "w2", 4, 2),
    ("w3_w4", "w3", "w4", 3, 1),
    ("w5_w6", "w5", "w6", 2, 2),
    ("p_q", "p", "q", 2, 0),
    ("c03_xx", "c03", "xx", 1, 0),
    ("c11_xx", "c11", "xx", 1, 0),
];

fn map_json(with_regions: bool, with_ferries: bool) -> Value {
    let mut cities: Vec<Value> = (0..CHAIN)
        .map(|i| json!({"id": chain_city(i), "name": chain_city(i), "region": region_of(i)}))
        .collect();
    for id in ["w1", "w2", "w3", "w4", "w5", "w6", "p", "q"] {
        cities.push(json!({"id": id, "name": id, "region": "r00"}));
    }
    cities.push(json!({"id": "xx", "name": "xx", "country": true}));
    if !with_regions {
        for city in &mut cities {
            city.as_object_mut().unwrap().remove("region");
        }
    }
    let route = |id: &str, from: &str, to: &str, len: u32, ferries: u32| {
        json!({
            "id": id, "from": from, "to": to, "len": len, "color": "Gray", "double": false,
            "ferries": if with_ferries { ferries } else { 0 },
        })
    };
    let mut routes: Vec<Value> = (0..CHAIN - 1)
        .map(|i| route(&chain_route(i), &chain_city(i), &chain_city(i + 1), 1, 0))
        .collect();
    routes.extend(
        SPECIAL_ROUTES
            .iter()
            .map(|&(id, from, to, len, ferries)| route(id, from, to, len, ferries)),
    );
    let ticket = |id: &str| json!({"id": id, "from": "c00", "to": "c01", "points": 1, "deck": "regular", "sets": ["mini"]});
    let tickets: Vec<Value> = (0..16).map(|i| ticket(&format!("t{i:02}"))).collect();
    json!({"cities": cities, "routes": routes, "tickets": tickets})
}

fn regions_bonus() -> Value {
    json!({
        "type": "regions", "label": "Regioni", "points": 56, "minRegions": 5,
        "table": [1, 2, 4, 7, 11, 16, 22, 29, 37, 46, 56],
        "doubleRegions": ["r15", "r16"],
    })
}

fn ruleset_json() -> Value {
    json!({
        "name": "Mini", "mode": "Standard", "ticketSet": "mini", "map": "mini",
        "players": {"min": 2, "max": 2}, "trainsPerPlayer": 45,
        "deck": {"perColor": 12, "locomotives": 14},
        "ticketDecks": ["regular"],
        "initialTickets": {"dealt": [{"deck": "regular", "count": 4}], "keepMin": 2},
        "ticketDraw": {"deck": "regular", "count": 3, "keepMin": 1},
        "setupDiscards": "reshuffleBottom",
        "routePoints": {"1": 1, "2": 2, "3": 4, "4": 7},
        "ferryCards": {"count": 4, "maxInHand": 2, "wavesPerCard": 2},
        "bonuses": [regions_bonus()],
        "doubleRouteMinPlayers": 2, "finalRoundIncludesTrigger": true,
    })
}

fn map() -> MapData {
    serde_json::from_value(map_json(true, true)).unwrap()
}

fn ruleset() -> Ruleset {
    serde_json::from_value(ruleset_json()).unwrap()
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
    let mut eng = engine_on(map(), ruleset());
    eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
    eng
}

/// Senza carte Traghetto né Regioni: le onde sono locomotive obbligatorie come sempre.
fn plain_engine() -> Engine {
    let mut value = ruleset_json();
    let object = value.as_object_mut().unwrap();
    object.remove("ferryCards");
    object.insert("bonuses".into(), json!([]));
    let mut eng = engine_on(
        serde_json::from_value(map_json(false, true)).unwrap(),
        serde_json::from_value(value).unwrap(),
    );
    eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
    eng
}

fn draw_ferry(eng: &mut Engine, pi: usize) -> Result<Vec<GameEvent>, EngineError> {
    eng.apply(Action::DrawFerryCard { player_index: pi })
}

fn try_claim(
    eng: &mut Engine,
    pi: usize,
    route_id: &str,
    cards: &[CardColor],
    ferry_cards: u32,
) -> Result<Vec<GameEvent>, EngineError> {
    eng.set_phase_for_test(GamePhase::PlayerTurn, pi);
    eng.apply(Action::ClaimRoute {
        player_index: pi,
        route_id: route_id.into(),
        cards_discarded: cards.to_vec(),
        passengers: Default::default(),
        ferry_cards,
    })
}

/// Reclama una rotta grigia pagandola con carte blu.
fn claim_plain(eng: &mut Engine, pi: usize, route_id: &str) {
    let len = eng.get_route(route_id).unwrap().len as usize;
    let cards = vec![Blue; len];
    eng.set_player_hand(pi, cards.clone());
    try_claim(eng, pi, route_id, &cards, 0).unwrap();
}

/// Reclama la catena da `c{from}` a `c{to}` (estremi inclusi).
fn claim_chain(eng: &mut Engine, pi: usize, from: usize, to: usize) {
    for i in from..to {
        claim_plain(eng, pi, &chain_route(i));
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

fn ferry_cards(eng: &Engine, pi: usize) -> u32 {
    eng.player(pi).unwrap().ferry_cards
}

// ── Pesca delle carte Traghetto ──────────────────────────────────────

/// La pesca è un turno intero: una carta in mano, mazzetto di una in meno, turno al successivo.
#[test]
fn drawing_a_ferry_card_takes_a_whole_turn() {
    let mut eng = engine();
    assert_eq!(eng.ferry_pile_size(), 4);
    let events = draw_ferry(&mut eng, 0).unwrap();
    assert!(matches!(
        events[0],
        GameEvent::FerryCardDrawn { player_index: 0 }
    ));
    assert!(matches!(
        events[1],
        GameEvent::TurnAdvanced {
            previous_player: 0,
            next_player: 1
        }
    ));
    assert_eq!(ferry_cards(&eng, 0), 1);
    assert_eq!(eng.ferry_pile_size(), 3);
    assert_eq!(eng.current_player(), 1);
    assert_eq!(eng.phase(), GamePhase::PlayerTurn);
    // Nessun'altra carta: la mano di carte treno non cambia.
    assert_eq!(eng.player(0).unwrap().hand.len(), 4);
}

/// Al massimo 2 carte Traghetto in mano: la terza pesca è rifiutata e non è tra le mosse legali.
#[test]
fn a_player_holds_at_most_two_ferry_cards() {
    let mut eng = engine();
    draw_ferry(&mut eng, 0).unwrap();
    eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
    assert!(eng.legal_moves(0).can_draw_ferry_card);
    draw_ferry(&mut eng, 0).unwrap();
    eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
    assert_eq!(ferry_cards(&eng, 0), 2);
    assert!(!eng.legal_moves(0).can_draw_ferry_card);

    let refused = draw_ferry(&mut eng, 0);
    assert!(matches!(
        refused,
        Err(EngineError::FerryCardsHandFull { max: 2 })
    ));
    assert_eq!(ferry_cards(&eng, 0), 2);
    assert_eq!(eng.current_player(), 0);
    assert_eq!(eng.ferry_pile_size(), 2);
}

/// Solo come azione di un turno nuovo: mai nella seconda pescata, in scelta biglietti, nel
/// draft iniziale o fuori turno.
#[test]
fn ferry_draw_is_only_allowed_at_the_start_of_a_turn() {
    let mut eng = engine();
    eng.apply(Action::DrawTrainCard {
        player_index: 0,
        source: DrawSource::Deck,
    })
    .unwrap();
    assert_eq!(eng.phase(), GamePhase::SecondCardDraw);
    assert!(!eng.legal_moves(0).can_draw_ferry_card);
    assert!(matches!(
        draw_ferry(&mut eng, 0),
        Err(EngineError::WrongState {
            state: GamePhase::SecondCardDraw
        })
    ));

    let mut eng = engine();
    eng.apply(Action::DrawTickets { player_index: 0 }).unwrap();
    assert_eq!(eng.phase(), GamePhase::TicketChoice);
    assert!(!eng.legal_moves(0).can_draw_ferry_card);
    assert!(matches!(
        draw_ferry(&mut eng, 0),
        Err(EngineError::WrongState {
            state: GamePhase::TicketChoice
        })
    ));

    let mut eng = engine_on(map(), ruleset());
    assert_eq!(eng.phase(), GamePhase::InitialDraft);
    assert!(matches!(
        draw_ferry(&mut eng, 0),
        Err(EngineError::WrongState {
            state: GamePhase::InitialDraft
        })
    ));

    let mut eng = engine();
    assert!(matches!(
        draw_ferry(&mut eng, 1),
        Err(EngineError::NotYourTurn { .. })
    ));
    assert!(!eng.legal_moves(1).can_draw_ferry_card);
    assert_eq!(ferry_cards(&eng, 1), 0);
}

/// La pesca vale anche nell'ultimo giro, e può essere l'ultimo turno della partita.
#[test]
fn ferry_draw_is_allowed_in_the_last_round() {
    let mut eng = engine();
    eng.set_player_trains(0, 3);
    claim_plain(&mut eng, 0, "c00_c01");
    assert!(eng.last_round());
    assert_eq!(eng.phase(), GamePhase::LastRound);
    assert_eq!(eng.current_player(), 1);
    assert!(eng.legal_moves(1).can_draw_ferry_card);

    let events = draw_ferry(&mut eng, 1).unwrap();
    assert!(matches!(events[0], GameEvent::FerryCardDrawn { .. }));
    assert_eq!(eng.phase(), GamePhase::LastRound);
    assert_eq!(eng.current_player(), 0);

    // Chi ha innescato gioca l'ultimo turno: pescare una carta Traghetto lo chiude.
    let events = draw_ferry(&mut eng, 0).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::GameOver { .. }))
    );
    assert_eq!(eng.phase(), GamePhase::GameOver);
}

/// Senza `ferryCards` nel ruleset l'azione non esiste.
#[test]
fn ferry_draw_does_not_exist_without_the_config() {
    let mut eng = plain_engine();
    assert!(!eng.legal_moves(0).can_draw_ferry_card);
    assert!(matches!(
        draw_ferry(&mut eng, 0),
        Err(EngineError::FerryCardsDisabled)
    ));
    assert_eq!(eng.current_player(), 0);
    assert_eq!(eng.ferry_pile_size(), 0);
}

// ── Pagamento delle onde ─────────────────────────────────────────────

/// Una carta su un'onda sola: `w3_w4` (3 spazi, 1 onda) = carta Traghetto + 2 carte colore.
#[test]
fn one_ferry_card_pays_one_wave() {
    let mut eng = engine();
    eng.set_player_hand(0, vec![Blue, Blue, Red]);
    eng.set_player_ferry_cards(0, 1);
    let discards = eng.discard_cards().len();
    try_claim(&mut eng, 0, "w3_w4", &[Blue, Blue], 1).unwrap();
    let player = eng.player(0).unwrap();
    assert_eq!(player.hand, [Red]);
    assert_eq!(player.ferry_cards, 0);
    assert_eq!((player.trains_remaining, player.score), (42, 4));
    assert_eq!(eng.discard_cards().len(), discards + 2);
    assert_eq!(eng.discard_cards()[discards..], [Blue, Blue]);
}

/// Una carta su due onde: `w5_w6` (2 spazi, 2 onde) si paga con una sola carta Traghetto.
#[test]
fn one_ferry_card_pays_two_waves() {
    let mut eng = engine();
    eng.set_player_ferry_cards(0, 1);
    eng.set_player_hand(0, vec![]);
    let discards = eng.discard_cards().len();
    try_claim(&mut eng, 0, "w5_w6", &[], 1).unwrap();
    let player = eng.player(0).unwrap();
    assert_eq!((player.trains_remaining, player.score), (43, 2));
    assert_eq!(player.ferry_cards, 0);
    assert_eq!(eng.discard_cards().len(), discards);
}

/// Due carte su due onde: ognuna ne copre una.
#[test]
fn two_ferry_cards_pay_two_waves() {
    let mut eng = engine();
    eng.set_player_ferry_cards(0, 2);
    eng.set_player_hand(0, vec![]);
    try_claim(&mut eng, 0, "w5_w6", &[], 2).unwrap();
    assert_eq!(ferry_cards(&eng, 0), 0);
    assert_eq!(eng.player(0).unwrap().score, 2);
    // Le due carte sono tornate nel mazzetto.
    assert_eq!(eng.ferry_pile_size(), 4);
}

/// `w1_w2` (4 spazi, 2 onde): una carta su un'onda e una locomotiva sull'altra, oppure una
/// carta su entrambe; la locomotiva può anche stare su uno spazio colore.
#[test]
fn ferry_card_and_locomotive_combine_on_the_same_route() {
    // Carta su un'onda + locomotiva sull'altra + 2 carte colore.
    let mut eng = engine();
    eng.set_player_ferry_cards(0, 1);
    eng.set_player_hand(0, vec![Blue, Blue, Locomotive]);
    try_claim(&mut eng, 0, "w1_w2", &[Blue, Blue, Locomotive], 1).unwrap();
    assert_eq!(eng.player(0).unwrap().score, 7);
    assert_eq!(ferry_cards(&eng, 0), 0);
    assert!(eng.player(0).unwrap().hand.is_empty());

    // Una carta sulle due onde + 2 carte colore.
    let mut eng = engine();
    eng.set_player_ferry_cards(0, 1);
    eng.set_player_hand(0, vec![Blue, Blue]);
    try_claim(&mut eng, 0, "w1_w2", &[Blue, Blue], 1).unwrap();
    assert_eq!(eng.player(0).unwrap().score, 7);

    // Locomotiva sull'onda e una seconda su uno spazio colore: 1 carta + 3 carte.
    let mut eng = engine();
    eng.set_player_ferry_cards(0, 1);
    eng.set_player_hand(0, vec![Blue, Locomotive, Locomotive]);
    try_claim(&mut eng, 0, "w1_w2", &[Blue, Locomotive, Locomotive], 1).unwrap();
    assert_eq!(eng.player(0).unwrap().score, 7);
}

/// Senza carte Traghetto le onde restano locomotive obbligatorie (regola di Europe/India).
#[test]
fn waves_without_ferry_cards_still_need_locomotives() {
    let mut eng = plain_engine();
    eng.set_player_hand(0, vec![Blue, Blue, Blue, Locomotive]);
    let refused = try_claim(&mut eng, 0, "w1_w2", &[Blue, Blue, Blue, Locomotive], 0);
    assert!(matches!(
        refused,
        Err(EngineError::InsufficientCards { .. })
    ));
    try_claim(&mut eng, 0, "w1_w2", &[], 0).unwrap_err();

    eng.set_player_hand(0, vec![Blue, Blue, Locomotive, Locomotive]);
    try_claim(
        &mut eng,
        0,
        "w1_w2",
        &[Blue, Blue, Locomotive, Locomotive],
        0,
    )
    .unwrap();
    assert_eq!(eng.player(0).unwrap().score, 7);
}

/// Stato di un giocatore che un rifiuto non deve toccare.
fn snapshot(eng: &Engine, pi: usize) -> (Vec<CardColor>, u32, u32, i32, usize) {
    let player = eng.player(pi).unwrap();
    (
        player.hand.clone(),
        player.ferry_cards,
        player.trains_remaining,
        player.score,
        eng.claimed_route_ids().len(),
    )
}

fn assert_refused(
    eng: &mut Engine,
    route_id: &str,
    cards: &[CardColor],
    ferry: u32,
) -> EngineError {
    let before = snapshot(eng, 0);
    let error = try_claim(eng, 0, route_id, cards, ferry).unwrap_err();
    assert_eq!(snapshot(eng, 0), before, "stato cambiato da un rifiuto");
    assert_eq!(eng.current_player(), 0);
    error
}

/// Le carte Traghetto non valgono sulle rotte senza onde.
#[test]
fn ferry_card_is_refused_on_a_route_without_waves() {
    let mut eng = engine();
    eng.set_player_ferry_cards(0, 1);
    eng.set_player_hand(0, vec![Blue, Blue]);
    for cards in [vec![Blue], vec![Blue, Blue]] {
        let error = assert_refused(&mut eng, "p_q", &cards, 1);
        assert!(
            matches!(error, EngineError::InvalidPayload { .. }),
            "{error}"
        );
    }
    // Neanche su una rotta di catena.
    let error = assert_refused(&mut eng, "c00_c01", &[], 1);
    assert!(
        matches!(error, EngineError::InvalidPayload { .. }),
        "{error}"
    );
}

/// Più carte Traghetto delle onde da coprire sono rifiutate, anche se in mano.
#[test]
fn more_ferry_cards_than_waves_are_refused() {
    let mut eng = engine();
    eng.set_player_ferry_cards(0, 2);
    eng.set_player_hand(0, vec![Blue]);
    // `w3_w4` ha una sola onda.
    let error = assert_refused(&mut eng, "w3_w4", &[Blue], 2);
    assert!(
        matches!(error, EngineError::InvalidPayload { .. }),
        "{error}"
    );
    // Le due carte su un'onda sola sono troppe anche con una carta colore in più.
    eng.set_player_hand(0, vec![Blue, Blue]);
    let error = assert_refused(&mut eng, "w3_w4", &[Blue, Blue], 2);
    assert!(
        matches!(error, EngineError::InvalidPayload { .. }),
        "{error}"
    );
}

/// Più carte Traghetto di quelle in mano sono rifiutate.
#[test]
fn more_ferry_cards_than_owned_are_refused() {
    let mut eng = engine();
    eng.set_player_ferry_cards(0, 1);
    eng.set_player_hand(0, vec![]);
    let error = assert_refused(&mut eng, "w5_w6", &[], 2);
    assert!(
        matches!(error, EngineError::InsufficientCards { .. }),
        "{error}"
    );
    // Nessuna carta Traghetto in mano e nessuna locomotiva: non si può nemmeno tentare.
    eng.set_player_ferry_cards(0, 0);
    let error = assert_refused(&mut eng, "w5_w6", &[], 1);
    assert!(
        matches!(error, EngineError::InsufficientCards { .. }),
        "{error}"
    );
}

/// Una carta colore non paga un'onda: serve una locomotiva o una carta Traghetto.
#[test]
fn a_colored_card_cannot_pay_a_wave() {
    let mut eng = engine();
    eng.set_player_ferry_cards(0, 1);
    eng.set_player_hand(0, vec![Blue, Blue, Blue, Blue]);
    // Senza carta Traghetto: 3 carte blu su `w3_w4` lasciano scoperta l'onda.
    let error = assert_refused(&mut eng, "w3_w4", &[Blue, Blue, Blue], 0);
    assert!(
        matches!(error, EngineError::InsufficientCards { .. }),
        "{error}"
    );
    // Con la carta Traghetto le carte colore sono una di troppo.
    let error = assert_refused(&mut eng, "w3_w4", &[Blue, Blue, Blue], 1);
    assert!(
        matches!(error, EngineError::InsufficientCards { .. }),
        "{error}"
    );
    // `w1_w2` con 1 carta Traghetto e 3 carte blu: restano un'onda o uno spazio senza copertura.
    let error = assert_refused(&mut eng, "w1_w2", &[Blue, Blue, Blue], 1);
    assert!(
        matches!(error, EngineError::InvalidPayload { .. }),
        "{error}"
    );
}

/// Carte di due colori e carte che non si hanno restano rifiutate come sempre.
#[test]
fn ordinary_payment_rules_still_apply_with_ferry_cards() {
    let mut eng = engine();
    eng.set_player_ferry_cards(0, 1);
    eng.set_player_hand(0, vec![Blue, Red]);
    let error = assert_refused(&mut eng, "w3_w4", &[Blue, Red], 1);
    assert!(
        matches!(error, EngineError::InvalidPayload { .. }),
        "{error}"
    );
    let error = assert_refused(&mut eng, "w3_w4", &[Blue, Blue], 1);
    assert!(
        matches!(error, EngineError::InsufficientCards { .. }),
        "{error}"
    );
}

/// Senza `ferryCards` nel ruleset una carta Traghetto dichiarata è rifiutata.
#[test]
fn declared_ferry_cards_need_the_config() {
    let mut eng = plain_engine();
    eng.set_player_hand(0, vec![]);
    let error = assert_refused(&mut eng, "w5_w6", &[], 1);
    assert!(matches!(error, EngineError::FerryCardsDisabled), "{error}");
}

/// Le rotte reclamabili contano le carte Traghetto in mano.
#[test]
fn legal_routes_count_the_ferry_cards() {
    let mut eng = engine();
    eng.set_player_hand(0, vec![]);
    assert!(!eng.legal_moves(0).routes.contains(&"w5_w6".to_string()));
    eng.set_player_ferry_cards(0, 1);
    let routes = eng.legal_moves(0).routes;
    assert!(routes.contains(&"w5_w6".to_string()), "{routes:?}");
    // Le altre onde hanno anche spazi colore da pagare.
    assert!(!routes.contains(&"w3_w4".to_string()));
    assert!(!routes.contains(&"w1_w2".to_string()));
    // Mai su una rotta senza onde.
    assert!(!routes.contains(&"p_q".to_string()));

    eng.set_player_hand(0, vec![Blue, Blue]);
    let routes = eng.legal_moves(0).routes;
    assert!(routes.contains(&"w3_w4".to_string()), "{routes:?}");
    assert!(routes.contains(&"w1_w2".to_string()), "{routes:?}");

    // Senza carta Traghetto, solo carte colore: le onde non sono pagabili.
    eng.set_player_ferry_cards(0, 0);
    let routes = eng.legal_moves(0).routes;
    assert!(!routes.contains(&"w3_w4".to_string()), "{routes:?}");
    assert!(!routes.contains(&"w1_w2".to_string()), "{routes:?}");
    assert!(routes.contains(&"p_q".to_string()));
}

/// Le carte usate tornano nel mazzetto, che così si può riempire di nuovo.
#[test]
fn used_ferry_cards_return_to_the_pile() {
    let mut eng = engine();
    for pi in [0, 1, 0, 1] {
        eng.set_phase_for_test(GamePhase::PlayerTurn, pi);
        draw_ferry(&mut eng, pi).unwrap();
    }
    assert_eq!((ferry_cards(&eng, 0), ferry_cards(&eng, 1)), (2, 2));
    assert_eq!(eng.ferry_pile_size(), 0);

    eng.set_player_hand(0, vec![]);
    try_claim(&mut eng, 0, "w5_w6", &[], 2).unwrap();
    assert_eq!(ferry_cards(&eng, 0), 0);
    assert_eq!(eng.ferry_pile_size(), 2);

    eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
    draw_ferry(&mut eng, 0).unwrap();
    assert_eq!(ferry_cards(&eng, 0), 1);
    assert_eq!(eng.ferry_pile_size(), 1);
}

// ── Bonus Regioni ────────────────────────────────────────────────────

fn standing(eng: &Engine, pi: usize) -> (Vec<u32>, i32) {
    let standing = eng.region_standing(pi);
    (standing.networks, standing.bonus)
}

/// Meno di 5 Regioni: nessun punto; con 5 la prima voce della tabella.
#[test]
fn fewer_than_five_regions_score_nothing() {
    let mut eng = engine();
    claim_chain(&mut eng, 0, 0, 3);
    assert_eq!(standing(&eng, 0), (vec![4], 0));
    claim_chain(&mut eng, 0, 3, 4);
    assert_eq!(standing(&eng, 0), (vec![5], 1));
    claim_chain(&mut eng, 0, 4, 5);
    assert_eq!(standing(&eng, 0), (vec![6], 2));
    // L'altro giocatore non ha rotte.
    assert_eq!(standing(&eng, 1), (vec![], 0));
}

/// Reti separate si sommano, ognuna col suo punteggio; una rotta che le unisce le fonde.
#[test]
fn separate_networks_are_scored_separately() {
    let mut eng = engine();
    claim_chain(&mut eng, 0, 0, 5);
    assert_eq!(standing(&eng, 0), (vec![6], 2));
    claim_chain(&mut eng, 0, 8, 13);
    assert_eq!(standing(&eng, 0), (vec![6, 6], 4));

    // Una rete dell'avversario (c14, c15: 2 Regioni + la doppia r15) non conta per il giocatore.
    claim_plain(&mut eng, 1, "c14_c15");
    assert_eq!(standing(&eng, 1), (vec![3], 0));
    assert_eq!(standing(&eng, 0), (vec![6, 6], 4));

    // Le due reti si congiungono: 14 Regioni (c00…c13) valgono 46, non 2 + 2.
    claim_chain(&mut eng, 0, 5, 8);
    assert_eq!(standing(&eng, 0), (vec![14], 46));
}

/// Da 15 Regioni in su il punteggio resta quello dell'ultima voce.
#[test]
fn regions_above_fifteen_saturate_the_table() {
    let mut eng = engine();
    claim_chain(&mut eng, 0, 0, 14);
    assert_eq!(standing(&eng, 0), (vec![15], 56));
    // `c15` è una Regione doppia con una sola città: 16 Regioni + 1 = 17, sempre 56.
    claim_chain(&mut eng, 0, 14, 15);
    assert_eq!(standing(&eng, 0), (vec![17], 56));
}

/// Una Regione doppia vale 2 solo se tutte le sue città sono nella rete.
#[test]
fn a_double_region_counts_twice_only_when_complete() {
    let mut eng = engine();
    // c11…c16: r11…r16 sono 6 Regioni; r15 (solo `c15`) è completa = +1; r16 (`c16` e `c17`)
    // no: 7 Regioni, 4 punti.
    claim_chain(&mut eng, 0, 11, 16);
    assert_eq!(standing(&eng, 0), (vec![7], 4));
    // Con `c17` anche r16 è completa: 6 Regioni + 2 doppie = 8, 7 punti.
    claim_chain(&mut eng, 0, 16, 17);
    assert_eq!(standing(&eng, 0), (vec![8], 7));
}

/// Due rotte verso lo stesso Paese non collegano le due reti, e il Paese non è una Regione.
#[test]
fn countries_do_not_join_networks() {
    let mut eng = engine();
    claim_chain(&mut eng, 0, 0, 3);
    claim_plain(&mut eng, 0, "c03_xx");
    claim_chain(&mut eng, 0, 8, 11);
    claim_plain(&mut eng, 0, "c11_xx");
    // Se il Paese le unisse sarebbero 8 Regioni (7 punti): restano due reti da 4.
    assert_eq!(standing(&eng, 0), (vec![4, 4], 0));
    // Il Paese non aggiunge una Regione alla rete che lo raggiunge.
    claim_chain(&mut eng, 0, 3, 4);
    assert_eq!(standing(&eng, 0), (vec![5, 4], 1));
}

/// A fine partita il bonus è in `total` e `region_networks` elenca le reti in ordine
/// decrescente; la rete live coincide.
#[test]
fn final_scores_carry_the_regions_bonus() {
    let mut eng = engine();
    claim_chain(&mut eng, 0, 0, 5);
    claim_chain(&mut eng, 0, 8, 14);
    let live = standing(&eng, 0);
    assert_eq!(live, (vec![7, 6], 4 + 2));
    let route_points = eng.player(0).unwrap().score;
    assert_eq!(route_points, 11);

    let scores = finish_game(&mut eng);
    assert_eq!(scores[0].region_networks, [7, 6]);
    assert_eq!(scores[0].regions_bonus, 6);
    assert_eq!(scores[0].route_points, 11);
    assert_eq!(scores[0].total, 11 + 6);
    assert!(scores[1].region_networks.is_empty());
    assert_eq!(scores[1].regions_bonus, 0);
    assert!(scores[0].winner && !scores[1].winner);
}

/// Senza il bonus nel ruleset: nessuna rete, nessun punto.
#[test]
fn without_the_bonus_nothing_is_reported() {
    let mut eng = plain_engine();
    claim_chain(&mut eng, 0, 0, 6);
    assert_eq!(standing(&eng, 0), (vec![], 0));
    let scores = finish_game(&mut eng);
    assert!(scores[0].region_networks.is_empty());
    assert_eq!(scores[0].regions_bonus, 0);
    assert_eq!(scores[0].total, scores[0].route_points);
}

// ── Validazione ──────────────────────────────────────────────────────

fn validate(
    edit_ruleset: impl FnOnce(&mut Value),
    edit_map: impl FnOnce(&mut Value),
) -> Result<(), EngineInitError> {
    let mut rules = ruleset_json();
    edit_ruleset(&mut rules);
    let mut map = map_json(true, true);
    edit_map(&mut map);
    let rules: Ruleset = serde_json::from_value(rules).unwrap();
    let map: MapData = serde_json::from_value(map).unwrap();
    rules.validate(&map)
}

fn nothing(_: &mut Value) {}

#[test]
fn the_fixture_validates() {
    assert_eq!(validate(nothing, nothing), Ok(()));
}

#[test]
fn ferry_cards_fields_must_be_positive_and_enough_for_everyone() {
    for field in ["count", "maxInHand", "wavesPerCard"] {
        let result = validate(|r| r["ferryCards"][field] = json!(0), nothing);
        assert_eq!(result, Err(EngineInitError::FerryCardsFieldZero { field }));
    }
    // 2 giocatori × 2 carte in mano = 4 carte: 3 non bastano.
    assert_eq!(
        validate(|r| r["ferryCards"]["count"] = json!(3), nothing),
        Err(EngineInitError::FerryCardsPoolTooSmall {
            count: 3,
            needed: 4
        })
    );
    assert_eq!(
        validate(|r| r["ferryCards"]["count"] = json!(4), nothing),
        Ok(())
    );
}

#[test]
fn ferry_cards_need_routes_with_waves() {
    let no_waves = |map: &mut Value| {
        for route in map["routes"].as_array_mut().unwrap() {
            route["ferries"] = json!(0);
        }
    };
    assert_eq!(
        validate(nothing, no_waves),
        Err(EngineInitError::FerryCardsUnused)
    );
}

#[test]
fn regions_bonus_needs_a_region_on_every_city_and_none_on_countries() {
    let region_of_city = |id: &'static str, region: Option<&'static str>| {
        move |map: &mut Value| {
            for city in map["cities"].as_array_mut().unwrap() {
                if city["id"] == id {
                    match region {
                        Some(r) => city["region"] = json!(r),
                        None => {
                            city.as_object_mut().unwrap().remove("region");
                        }
                    }
                }
            }
        }
    };
    assert_eq!(
        validate(nothing, region_of_city("c04", None)),
        Err(EngineInitError::CityRegionMissing {
            city_id: "c04".into()
        })
    );
    assert_eq!(
        validate(nothing, region_of_city("xx", Some("r00"))),
        Err(EngineInitError::CountryHasRegion {
            city_id: "xx".into()
        })
    );
}

#[test]
fn regions_bonus_parameters_are_checked() {
    assert_eq!(
        validate(|r| r["bonuses"][0]["minRegions"] = json!(0), nothing),
        Err(EngineInitError::RegionsMinRegionsInvalid)
    );
    assert_eq!(
        validate(
            |r| {
                r["bonuses"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("minRegions");
            },
            nothing
        ),
        Err(EngineInitError::RegionsMinRegionsInvalid)
    );
    assert_eq!(
        validate(|r| r["bonuses"][0]["table"] = json!([]), nothing),
        Err(EngineInitError::RegionsTableEmpty)
    );
    assert_eq!(
        validate(|r| r["bonuses"][0]["table"] = json!([1, 4, 2, 56]), nothing),
        Err(EngineInitError::RegionsTableDecreasing { index: 2 })
    );
    assert_eq!(
        validate(|r| r["bonuses"][0]["points"] = json!(50), nothing),
        Err(EngineInitError::RegionsPointsMismatch {
            points: 50,
            last: 56
        })
    );
    assert_eq!(
        validate(
            |r| r["bonuses"][0]["doubleRegions"] = json!(["r15", "r15"]),
            nothing
        ),
        Err(EngineInitError::RegionsDoubleDuplicate {
            region: "r15".into()
        })
    );
    assert_eq!(
        validate(
            |r| r["bonuses"][0]["doubleRegions"] = json!(["atlantide"]),
            nothing
        ),
        Err(EngineInitError::RegionsDoubleUnknown {
            region: "atlantide".into()
        })
    );
}

#[test]
fn region_fields_are_not_allowed_elsewhere() {
    // `minRegions` su un altro bonus.
    assert_eq!(
        validate(
            |r| {
                r["bonuses"] = json!([regions_bonus(), {
                    "type": "longest_path", "label": "LP", "points": 10, "minRegions": 5
                }]);
            },
            nothing
        ),
        Err(EngineInitError::RegionsFieldsNotAllowed {
            bonus_type: "longest_path".into()
        })
    );
    // Città con Regione ma ruleset senza il bonus.
    assert_eq!(
        validate(|r| r["bonuses"] = json!([]), nothing),
        Err(EngineInitError::CityRegionUnused {
            city_id: "c00".into()
        })
    );
}

// ── Compatibilità di save e dati ─────────────────────────────────────

/// Le azioni esistenti si serializzano e si leggono come prima: `ferry_cards` a 0 non compare
/// nel log, e un log senza il campo vale 0.
#[test]
fn claim_route_without_ferry_cards_keeps_its_old_shape() {
    let action = Action::ClaimRoute {
        player_index: 1,
        route_id: "a_b".into(),
        cards_discarded: vec![Blue],
        passengers: Default::default(),
        ferry_cards: 0,
    };
    let json = serde_json::to_value(&action).unwrap();
    assert!(
        !json["ClaimRoute"]
            .as_object()
            .unwrap()
            .contains_key("ferry_cards"),
        "{json}"
    );
    let with = serde_json::to_value(Action::ClaimRoute {
        player_index: 1,
        route_id: "a_b".into(),
        cards_discarded: vec![],
        passengers: Default::default(),
        ferry_cards: 2,
    })
    .unwrap();
    assert_eq!(with["ClaimRoute"]["ferry_cards"], json!(2));
    let old: Action = serde_json::from_value(
        json!({"ClaimRoute": {"player_index": 1, "route_id": "a_b", "cards_discarded": ["BLUE"]}}),
    )
    .unwrap();
    assert!(matches!(
        old,
        Action::ClaimRoute {
            ferry_cards: 0,
            player_index: 1,
            ..
        }
    ));
    let draw = serde_json::to_value(Action::DrawFerryCard { player_index: 0 }).unwrap();
    let back: Action = serde_json::from_value(draw).unwrap();
    assert!(matches!(back, Action::DrawFerryCard { player_index: 0 }));
}

/// I dati veri di Italy: contratto di mappa e ruleset.
#[test]
fn the_italy_data_matches_the_contract() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let map: MapData = serde_json::from_str(
        &std::fs::read_to_string(root.join("maps/italy.json")).expect("data/maps/italy.json"),
    )
    .unwrap();
    let rules: Ruleset = serde_json::from_str(
        &std::fs::read_to_string(root.join("rulesets/italy.json"))
            .expect("data/rulesets/italy.json"),
    )
    .unwrap();
    rules.validate(&map).unwrap();

    let mut countries: Vec<&str> = map
        .cities
        .iter()
        .filter(|c| c.country)
        .map(|c| c.id.as_str())
        .collect();
    countries.sort_unstable();
    assert_eq!(
        countries,
        ["austria", "croazia", "francia", "monaco", "svizzera"]
    );
    assert_eq!(map.cities.len(), 34 + 5);
    let regions: std::collections::BTreeSet<&str> = map
        .cities
        .iter()
        .filter_map(|c| c.region.as_deref())
        .collect();
    assert_eq!(regions.len(), 17);
    assert_eq!(map.routes.len(), 128);
    let ferry: Vec<_> = map.routes.iter().filter(|r| r.ferries > 0).collect();
    assert_eq!(ferry.len(), 21);
    assert!(ferry.iter().all(|r| r.color == "Gray" && r.ferries <= 4));
    assert_eq!(map.tickets.len(), 56);

    let ferry_cards = rules.ferry_cards.as_ref().expect("ferryCards");
    assert_eq!(
        (
            ferry_cards.count,
            ferry_cards.max_in_hand,
            ferry_cards.waves_per_card
        ),
        (10, 2, 2)
    );
    let bonus = rules.regions_bonus().expect("regions");
    assert_eq!(bonus.min_regions, Some(5));
    assert_eq!(bonus.points, 56);
    assert_eq!(bonus.double_regions, ["puglia", "sardegna", "sicilia"]);
}
