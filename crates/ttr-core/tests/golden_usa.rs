//! Replay golden: partite scriptate sulla mappa USA reale con un hash degli eventi di ogni
//! azione. Se cambia, il replay dei save esistenti cambia e va incrementato `RULES_VERSION`.

use ttr_core::error::GamePhase;
use ttr_core::types::{MapData, Ruleset, TRAIN_COLORS};
use ttr_core::{Action, DrawSource, Engine, RouteColor, default_player_colors};
use ttr_proto::CardColor;

/// Azioni massime per partita scriptata.
const GOLDEN_STEPS: usize = 1500;

/// `(seed, giocatori, hash atteso)`, catturati con `RULES_VERSION` 2 prima del fix multi-mazzo.
const GOLDEN_USA_REPLAY: [(u64, usize, u64); 4] = [
    (42, 2, 14_148_095_192_312_613_418),
    (7, 3, 9_157_768_721_607_806_273),
    (2026, 4, 13_570_062_906_367_741_418),
    (99, 5, 2_185_271_578_200_710_427),
];

fn usa() -> (Ruleset, MapData) {
    let ruleset = std::fs::read_to_string("../../data/rulesets/usa_v2.json").unwrap();
    let map = std::fs::read_to_string("../../data/maps/usa.json").unwrap();
    (
        serde_json::from_str(&ruleset).unwrap(),
        serde_json::from_str(&map).unwrap(),
    )
}

/// Pagamento valido per la rotta con le carte in mano, se esiste.
fn payment(hand: &[CardColor], color: RouteColor, len: usize) -> Option<Vec<CardColor>> {
    let count = |c: CardColor| hand.iter().filter(|h| **h == c).count();
    let locos = count(CardColor::Locomotive);
    let colors: Vec<CardColor> = match color {
        RouteColor::Colored(c) => vec![c],
        RouteColor::Gray => TRAIN_COLORS.to_vec(),
    };
    for c in colors {
        if count(c) + locos >= len {
            let mut cards = vec![c; count(c).min(len)];
            cards.resize(len, CardColor::Locomotive);
            return Some(cards);
        }
    }
    (locos >= len).then(|| vec![CardColor::Locomotive; len])
}

fn next_action(eng: &Engine, step: usize, keep_initial: usize, keep_draw: usize) -> Action {
    let pi = eng.current_player();
    match eng.phase() {
        GamePhase::InitialDraft => {
            let player = (0..eng.num_players())
                .find(|p| !eng.initial_tickets_for(*p).unwrap().is_empty())
                .unwrap();
            let offered = eng.initial_tickets_for(player).unwrap();
            Action::ChooseTickets {
                player_index: player,
                keep: offered.iter().take(keep_initial).cloned().collect(),
            }
        }
        GamePhase::TicketChoice => Action::ChooseTickets {
            player_index: pi,
            keep: eng
                .pending_tickets()
                .iter()
                .take(keep_draw)
                .cloned()
                .collect(),
        },
        _ => {
            let legal = eng.legal_moves(pi);
            if !legal.routes.is_empty() && !step.is_multiple_of(3) {
                let route = eng
                    .get_route(&legal.routes[step % legal.routes.len()])
                    .unwrap();
                let color = RouteColor::parse(&route.color).unwrap();
                if let Some(cards) =
                    payment(eng.player_hand(pi).unwrap(), color, route.len as usize)
                {
                    return Action::ClaimRoute {
                        player_index: pi,
                        route_id: route.id.clone(),
                        cards_discarded: cards,
                        passengers: Default::default(),
                    };
                }
            }
            if step.is_multiple_of(11) && legal.can_draw_tickets {
                Action::DrawTickets {
                    player_index: pi,
                    counts: Default::default(),
                }
            } else if legal.can_draw_deck {
                Action::DrawTrainCard {
                    player_index: pi,
                    source: DrawSource::Deck,
                }
            } else if let Some(&index) = legal.draw_face_up.first() {
                Action::DrawTrainCard {
                    player_index: pi,
                    source: DrawSource::FaceUp { index },
                }
            } else {
                Action::DrawTickets {
                    player_index: pi,
                    counts: Default::default(),
                }
            }
        }
    }
}

/// FNV-1a sul testo degli eventi di ogni azione, più il numero di azioni.
fn golden_hash(seed: u64, players: usize) -> u64 {
    let (ruleset, map) = usa();
    let keep_initial = ruleset.initial_tickets.keep_min as usize;
    let keep_draw = ruleset.ticket_draw.keep_min as usize;
    let mut eng = Engine::new(
        seed,
        ruleset,
        map,
        (0..players).map(|i| format!("p{i}")).collect(),
        (0..players).map(|i| format!("P{i}")).collect(),
        default_player_colors(players),
        0,
    )
    .unwrap();

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |text: &str| {
        for byte in text.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for step in 0..GOLDEN_STEPS {
        if eng.phase() == GamePhase::GameOver {
            break;
        }
        let action = next_action(&eng, step, keep_initial, keep_draw);
        let events = eng.apply(action).expect("azione scriptata rifiutata");
        // I campi Europe di `FinalScore` sono sempre vuoti sulla USA e `globetrotter_bonus`
        // è 0 senza il bonus: non entrano nell'hash, che resta quello catturato prima della
        // loro introduzione.
        feed(
            &format!("{events:?}")
                .replace(", globetrotter_bonus: 0", "")
                .replace(", station_points: 0, borrowed_routes: []", "")
                .replace(", country_points: 0, passenger_points: 0", ""),
        );
    }
    feed(&eng.action_log().len().to_string());
    hash
}

#[test]
fn golden_usa_replay_is_unchanged() {
    let actual: Vec<u64> = GOLDEN_USA_REPLAY
        .iter()
        .map(|&(seed, players, _)| golden_hash(seed, players))
        .collect();
    let expected: Vec<u64> = GOLDEN_USA_REPLAY.iter().map(|&(_, _, hash)| hash).collect();
    assert_eq!(actual, expected);
}
