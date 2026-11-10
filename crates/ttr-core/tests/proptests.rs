//! Test property-based sul motore: conservazione, azioni fallite,
//! legal-vs-apply e replay, su partite casuali sulla mappa USA reale.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use proptest::prelude::*;
use ttr_core::error::GamePhase;
use ttr_core::types::{MapData, Ruleset, TRAIN_COLORS};
use ttr_core::{Action, DrawSource, Engine, GameEvent, RouteColor, default_player_colors};
use ttr_proto::CardColor;

/// Carte di un mazzo completo USA (12 × 8 colori + 14 locomotive).
const FULL_DECK: usize = 110;

/// Massimo di mosse per partita casuale.
const MAX_STEPS: usize = 400;

static DATA: LazyLock<(Ruleset, MapData)> = LazyLock::new(|| {
    let ruleset = std::fs::read_to_string("../../data/rulesets/usa_v2.json").unwrap();
    let map = std::fs::read_to_string("../../data/maps/usa.json").unwrap();
    (
        serde_json::from_str(&ruleset).unwrap(),
        serde_json::from_str(&map).unwrap(),
    )
});

fn data() -> &'static (Ruleset, MapData) {
    &DATA
}

fn new_engine(seed: u64, players: usize) -> Engine {
    let (ruleset, map) = data();
    Engine::new(
        seed,
        ruleset.clone(),
        map.clone(),
        (0..players).map(|i| format!("p{i}")).collect(),
        (0..players).map(|i| format!("P{i}")).collect(),
        default_player_colors(players),
        0,
    )
    .unwrap()
}

/// Pagamento valido per la rotta con le carte in mano, se esiste.
fn build_payment(hand: &[CardColor], color: RouteColor, len: usize) -> Option<Vec<CardColor>> {
    let count = |c: CardColor| hand.iter().filter(|h| **h == c).count();
    let locos = count(CardColor::Locomotive);
    let colors: Vec<CardColor> = match color {
        RouteColor::Colored(c) => vec![c],
        RouteColor::Gray => TRAIN_COLORS.to_vec(),
    };
    for c in colors {
        if count(c) + locos >= len {
            let mut payment = vec![c; count(c).min(len)];
            payment.resize(len, CardColor::Locomotive);
            return Some(payment);
        }
    }
    (locos >= len).then(|| vec![CardColor::Locomotive; len])
}

fn route_payment(eng: &Engine, player: usize, route_id: &str) -> Option<Vec<CardColor>> {
    let route = eng.get_route(route_id)?;
    let color = RouteColor::parse(&route.color)?;
    build_payment(eng.player_hand(player)?, color, route.len as usize)
}

/// Azioni sensate per la fase corrente, usate per far avanzare la partita.
fn progress_actions(eng: &Engine) -> Vec<Action> {
    let (ruleset, _) = data();
    match eng.phase() {
        GamePhase::InitialDraft => {
            let mut actions = Vec::new();
            for pi in 0..eng.num_players() {
                let offered = eng.initial_tickets_for(pi).unwrap_or_default();
                if offered.is_empty() {
                    continue;
                }
                let keep_min = ruleset.initial_tickets.keep_min as usize;
                for k in [keep_min, offered.len()] {
                    actions.push(Action::ChooseTickets {
                        player_index: pi,
                        keep: offered[..k].to_vec(),
                    });
                }
            }
            actions
        }
        GamePhase::TicketChoice => {
            let pending = eng.pending_tickets();
            let keep_min = eng.ticket_draw_keep_min() as usize;
            [keep_min, pending.len()]
                .into_iter()
                .map(|k| Action::ChooseTickets {
                    player_index: eng.current_player(),
                    keep: pending[..k].to_vec(),
                })
                .collect()
        }
        GamePhase::GameOver => Vec::new(),
        _ => {
            let pi = eng.current_player();
            let legal = eng.legal_moves(pi);
            let mut actions = Vec::new();
            for route_id in &legal.routes {
                let cards = route_payment(eng, pi, route_id).expect("rotta legale non pagabile");
                actions.push(Action::ClaimRoute {
                    player_index: pi,
                    route_id: route_id.clone(),
                    cards_discarded: cards,
                });
            }
            for &index in &legal.draw_face_up {
                actions.push(Action::DrawTrainCard {
                    player_index: pi,
                    source: DrawSource::FaceUp { index },
                });
            }
            if legal.can_draw_deck {
                actions.push(Action::DrawTrainCard {
                    player_index: pi,
                    source: DrawSource::Deck,
                });
            }
            if legal.can_draw_tickets {
                actions.push(Action::DrawTickets { player_index: pi });
            }
            actions
        }
    }
}

/// Sceglie un'azione favorendo le rotte, così le partite arrivano alla fine.
fn pick(actions: Vec<Action>, choice: u16) -> Option<Action> {
    if actions.is_empty() {
        return None;
    }
    let (claims, others): (Vec<Action>, Vec<Action>) = actions
        .into_iter()
        .partition(|a| matches!(a, Action::ClaimRoute { .. }));
    let from = if !claims.is_empty() && !choice.is_multiple_of(3) {
        claims
    } else if !others.is_empty() {
        others
    } else {
        claims
    };
    let index = usize::from(choice / 3) % from.len();
    from.into_iter().nth(index)
}

/// Azione arbitraria, spesso non valida: giocatore, fonte, rotta e carte casuali.
fn arbitrary_action(eng: &Engine, picks: [u16; 4]) -> Action {
    let (_, map) = data();
    let player_index = usize::from(picks[0]) % (eng.num_players() + 1);
    match picks[1] % 4 {
        0 => Action::DrawTrainCard {
            player_index,
            source: if picks[2].is_multiple_of(3) {
                DrawSource::Deck
            } else {
                DrawSource::FaceUp {
                    index: usize::from(picks[2]) % 7,
                }
            },
        },
        1 => Action::DrawTickets { player_index },
        2 => {
            let mut pool: Vec<String> = eng.pending_tickets().to_vec();
            if let Some(initial) = eng.initial_tickets_for(player_index) {
                pool.extend(initial.iter().cloned());
            }
            pool.push("biglietto_inesistente".into());
            let keep: Vec<String> = pool
                .iter()
                .enumerate()
                .filter(|(i, _)| (usize::from(picks[2]) >> (i % 16)) & 1 == 1)
                .map(|(_, t)| t.clone())
                .collect();
            Action::ChooseTickets { player_index, keep }
        }
        _ => {
            let route_index = usize::from(picks[2]) % (map.routes.len() + 1);
            let route = map.routes.get(route_index);
            let route_id = route.map_or("rotta_inesistente".to_string(), |r| r.id.clone());
            let len = route.map_or(1, |r| r.len as usize);
            let hand = eng.player_hand(player_index).unwrap_or_default();
            let mut cards: Vec<CardColor> = if picks[3].is_multiple_of(2) {
                route_payment(eng, player_index, &route_id).unwrap_or_default()
            } else {
                hand.iter()
                    .cycle()
                    .skip(usize::from(picks[3]) % hand.len().max(1))
                    .take(len)
                    .copied()
                    .collect()
            };
            if picks[3].is_multiple_of(5) {
                cards.pop();
            }
            Action::ClaimRoute {
                player_index,
                route_id,
                cards_discarded: cards,
            }
        }
    }
}

/// Tutto lo stato osservabile del motore che un'azione fallita non deve toccare.
#[derive(Debug, PartialEq)]
struct Snapshot {
    state: String,
    rng_word: u64,
    draw_pile: Vec<CardColor>,
    face_up: Vec<Option<CardColor>>,
    ticket_pile: Vec<String>,
    pending: Vec<String>,
    hands: Vec<Vec<CardColor>>,
    tickets: Vec<Vec<String>>,
    initial_offers: Vec<Vec<String>>,
    log_len: usize,
}

fn snapshot(eng: &Engine) -> Snapshot {
    let players = 0..eng.num_players();
    Snapshot {
        state: eng.serialize_state(),
        rng_word: eng.peek_rng_word(),
        draw_pile: eng.draw_pile_cards().to_vec(),
        face_up: eng.face_up_cards().to_vec(),
        ticket_pile: eng.ticket_pile().to_vec(),
        pending: eng.pending_tickets().to_vec(),
        hands: players
            .clone()
            .map(|p| eng.player_hand(p).unwrap().to_vec())
            .collect(),
        tickets: players
            .clone()
            .map(|p| eng.player(p).unwrap().tickets.keys().cloned().collect())
            .collect(),
        initial_offers: players
            .map(|p| eng.initial_tickets_for(p).unwrap().to_vec())
            .collect(),
        log_len: eng.action_log().len(),
    }
}

fn total_train_cards(eng: &Engine) -> usize {
    eng.draw_pile_len()
        + eng.face_up_cards().iter().flatten().count()
        + eng.discards_len()
        + (0..eng.num_players())
            .map(|p| eng.player_hand(p).unwrap().len())
            .sum::<usize>()
}

/// Gioca una partita casuale chiamando `hook` prima di ogni mossa.
fn play(
    seed: u64,
    players: usize,
    choices: &[u16],
    mut hook: impl FnMut(&mut Engine, &[u16]) -> Result<(), TestCaseError>,
) -> Result<Engine, TestCaseError> {
    let mut eng = new_engine(seed, players);
    for window in choices.windows(4).step_by(4).take(MAX_STEPS) {
        hook(&mut eng, window)?;
        let Some(action) = pick(progress_actions(&eng), window[0]) else {
            break;
        };
        prop_assert!(
            eng.apply(action.clone()).is_ok(),
            "azione sensata rifiutata: {action:?}"
        );
    }
    Ok(eng)
}

fn game_inputs() -> impl Strategy<Value = (u64, usize, Vec<u16>)> {
    (
        any::<u64>(),
        2usize..=5,
        prop::collection::vec(any::<u16>(), 800..(MAX_STEPS * 4)),
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    /// Carte treno e biglietti si conservano: solo D27 aggiunge mazzi interi.
    #[test]
    fn cards_and_tickets_are_conserved((seed, players, choices) in game_inputs()) {
        let (_, map) = data();
        let mut previous_total = FULL_DECK;
        play(seed, players, &choices, |eng, _| {
            let total = total_train_cards(eng);
            prop_assert!(total >= previous_total && total.is_multiple_of(FULL_DECK), "carte {total}");
            previous_total = total;
            prop_assert!(eng.face_up_cards().iter().all(Option::is_some));

            let mut ids: Vec<&String> = eng.ticket_pile().iter().collect();
            ids.extend(eng.pending_tickets());
            for p in 0..eng.num_players() {
                ids.extend(eng.player(p).unwrap().tickets.keys());
                ids.extend(eng.initial_tickets_for(p).unwrap());
            }
            let unique: BTreeSet<&String> = ids.iter().copied().collect();
            prop_assert_eq!(ids.len(), map.tickets.len(), "biglietti persi o creati");
            prop_assert_eq!(unique.len(), ids.len(), "biglietto duplicato");
            Ok(())
        })?;
    }

    /// Un'azione rifiutata non modifica lo stato né la prossima estrazione dell'RNG.
    #[test]
    fn failed_action_leaves_state_identical((seed, players, choices) in game_inputs()) {
        play(seed, players, &choices, |eng, window| {
            let picks = [window[0], window[1], window[2], window[3]];
            let action = arbitrary_action(eng, picks);
            let before = snapshot(eng);
            if eng.apply(action.clone()).is_err() {
                prop_assert_eq!(snapshot(eng), before, "azione fallita {:?}", action);
            }
            Ok(())
        })?;
    }

    /// Ogni mossa dichiarata legale riesce e ogni mossa non dichiarata fallisce.
    #[test]
    fn legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        let (_, map) = data();
        play(seed, players, &choices, |eng, window| {
            let pi = eng.current_player();
            for other in (0..eng.num_players()).filter(|p| *p != pi) {
                let lm = eng.legal_moves(other);
                prop_assert!(
                    lm.routes.is_empty() && lm.draw_face_up.is_empty()
                        && !lm.can_draw_deck && !lm.can_draw_tickets,
                    "mosse per il giocatore {other} non di turno"
                );
            }
            if eng.phase() == GamePhase::GameOver {
                return Ok(());
            }
            let lm = eng.legal_moves(pi);

            // Una mossa elencata deve riuscire (su una copia); una non elencata
            // deve fallire sul motore reale senza modificarlo.
            let check = |eng: &mut Engine, action: Action, listed: bool, what: &str| {
                let ok = if listed {
                    eng.clone().apply(action).is_ok()
                } else {
                    eng.apply(action).is_ok()
                };
                prop_assert_eq!(ok, listed, "{}", what);
                Ok(())
            };

            let offset = usize::from(window[0]) % map.routes.len();
            let mut listed_checked = 0;
            for route in map.routes.iter().cycle().skip(offset).take(map.routes.len()) {
                let listed = lm.routes.contains(&route.id);
                let Some(cards) = route_payment(eng, pi, &route.id) else {
                    prop_assert!(!listed, "rotta {} elencata ma non pagabile", route.id);
                    continue;
                };
                if listed {
                    if listed_checked == 3 {
                        continue;
                    }
                    listed_checked += 1;
                }
                let action = Action::ClaimRoute {
                    player_index: pi,
                    route_id: route.id.clone(),
                    cards_discarded: cards,
                };
                check(eng, action, listed, &format!("rotta {}", route.id))?;
            }
            for index in 0..eng.face_up_cards().len() {
                let action = Action::DrawTrainCard {
                    player_index: pi,
                    source: DrawSource::FaceUp { index },
                };
                check(eng, action, lm.draw_face_up.contains(&index), &format!("scoperta {index}"))?;
            }
            let deck = Action::DrawTrainCard {
                player_index: pi,
                source: DrawSource::Deck,
            };
            check(eng, deck, lm.can_draw_deck, "pesca dal mazzo")?;
            let tickets = Action::DrawTickets { player_index: pi };
            check(eng, tickets, lm.can_draw_tickets, "pesca biglietti")?;
            Ok(())
        })?;
    }

    /// Rigiocare `action_log` da seed e giocatori riproduce stato ed eventi.
    #[test]
    fn replay_reproduces_state((seed, players, choices) in game_inputs()) {
        let original = play(seed, players, &choices, |_, _| Ok(()))?;
        let mut replayed = new_engine(seed, players);
        for record in original.action_log() {
            let events = replayed.apply(record.action.clone());
            prop_assert!(events.is_ok(), "replay fallito: {:?}", record.action);
            prop_assert_eq!(
                format!("{:?}", events.unwrap()),
                format!("{:?}", record.events),
                "eventi diversi per {:?}", record.action
            );
        }
        prop_assert_eq!(snapshot(&replayed), snapshot(&original));
    }
}

/// Gli eventi `FaceUpReset` descrivono davvero le scoperte dopo il reset.
#[test]
fn face_up_reset_events_match_final_face_up() {
    for seed in 0..40u64 {
        let mut eng = new_engine(seed, 2);
        let mut steps = 0;
        while steps < 300 {
            let Some(action) = pick(progress_actions(&eng), u16::try_from(steps).unwrap()) else {
                break;
            };
            let events = eng.apply(action).unwrap();
            let last_reset = events.iter().rev().find_map(|e| match e {
                GameEvent::FaceUpReset { new_face_up, .. } => Some(new_face_up.clone()),
                _ => None,
            });
            if let Some(face_up) = last_reset {
                assert_eq!(face_up, eng.face_up_cards(), "seed {seed}");
            }
            steps += 1;
        }
    }
}
