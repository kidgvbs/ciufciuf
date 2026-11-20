//! Switzerland: i varchi di confine verso uno stesso Paese sono indipendenti. Kreuzlingen ha
//! due bandierine verso la Germania (`deutschland_kreuzlingen_1/2`, `double: false`): chi
//! prende un varco non blocca l'altro, e lo stesso giocatore può prenderli entrambi.
//! Usa `data/{maps,rulesets}/switzerland.json`.

use ttr_core::error::GamePhase;
use ttr_core::types::{MapData, Ruleset};
use ttr_core::{Action, Engine, EngineError, GameEvent, default_player_colors};
use ttr_proto::CardColor;

const EAST: &str = "deutschland_kreuzlingen_1";
const WEST: &str = "deutschland_kreuzlingen_2";

fn load<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let text = std::fs::read_to_string(format!("../../data/{path}.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn engine(players: usize) -> Engine {
    Engine::new(
        7,
        load::<Ruleset>("rulesets/switzerland"),
        load::<MapData>("maps/switzerland"),
        (0..players).map(|i| format!("p{i}")).collect(),
        (0..players).map(|i| format!("P{i}")).collect(),
        default_player_colors(players),
        0,
    )
    .unwrap()
}

/// Il giocatore `pi` reclama la rotta (lunga 1) nel suo turno, pagando con una carta del suo colore.
fn claim(
    eng: &mut Engine,
    pi: usize,
    route_id: &str,
    color: CardColor,
) -> Result<Vec<GameEvent>, EngineError> {
    eng.set_phase_for_test(GamePhase::PlayerTurn, pi);
    eng.set_player_hand(pi, vec![color]);
    eng.apply(Action::ClaimRoute {
        player_index: pi,
        route_id: route_id.into(),
        cards_discarded: vec![color],
        passengers: Default::default(),
        ferry_cards: 0,
    })
}

#[test]
fn kreuzlingen_crossings_are_independent_for_two_players() {
    let mut eng = engine(2);
    assert!(!eng.blocked_route_ids().contains(&WEST.to_string()));

    claim(&mut eng, 0, EAST, CardColor::White).unwrap();

    // Il varco ovest resta libero e rivendicabile, anche da chi ha preso quello est.
    assert!(!eng.blocked_route_ids().contains(&WEST.to_string()));
    eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
    eng.set_player_hand(0, vec![CardColor::Orange]);
    assert!(eng.legal_moves(0).routes.contains(&WEST.to_string()));

    claim(&mut eng, 0, WEST, CardColor::Orange).unwrap();
    assert_eq!(eng.player(0).unwrap().claimed_routes.len(), 2);
}
