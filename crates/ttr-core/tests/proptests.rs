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

/// Azioni massime per una partita giocata fino a `GameOver`.
const MAX_GAME_STEPS: usize = 6000;

/// Esito di una partita completa: eventi di fine partita e conteggi raccolti mentre si gioca.
struct FinishedGame {
    final_scores: Vec<ttr_core::event::FinalScore>,
    longest_path_players: Vec<usize>,
    longest_path_bonus: u32,
    /// Punti delle rotte reclamate, sommati dagli eventi `RouteClaimed`.
    route_points: Vec<i32>,
    /// Fine turno dopo `LastRoundAnnounced`, compreso quello dell'azione che l'ha annunciato
    /// e quello che chiude la partita.
    turn_ends_after_announcement: usize,
}

/// Gioca con mosse legali fino a `GameOver` verificando a ogni passo vagoni e carte.
fn play_to_game_over(
    seed: u64,
    players: usize,
    choices: &[u16],
) -> Result<(Engine, FinishedGame), TestCaseError> {
    let trains = data().0.trains_per_player;
    let mut eng = new_engine(seed, players);
    let mut route_points = vec![0i32; players];
    let mut announced = false;
    let mut turn_ends = 0;
    let mut finished = None;
    for step in 0..MAX_GAME_STEPS {
        let choice = choices[step % choices.len()].wrapping_add((step / choices.len()) as u16);
        let Some(action) = pick(progress_actions(&eng), choice) else {
            break;
        };
        let events = eng.apply(action.clone());
        prop_assert!(events.is_ok(), "azione legale rifiutata: {action:?}");
        for event in events.unwrap() {
            match event {
                GameEvent::RouteClaimed {
                    player_index,
                    points,
                    ..
                } => route_points[player_index] += i32::try_from(points).unwrap(),
                GameEvent::LastRoundAnnounced { .. } => {
                    prop_assert!(!announced, "ultimo giro annunciato due volte");
                    announced = true;
                }
                GameEvent::TurnAdvanced { .. } if announced => turn_ends += 1,
                GameEvent::GameOver {
                    final_scores,
                    longest_path_players,
                    longest_path_bonus,
                } => {
                    prop_assert!(announced, "GameOver senza ultimo giro");
                    turn_ends += 1;
                    finished = Some((final_scores, longest_path_players, longest_path_bonus));
                }
                _ => {}
            }
        }
        for p in 0..players {
            let state = eng.player(p).unwrap();
            let placed: u32 = state.claimed_routes.values().map(|r| r.train_count).sum();
            prop_assert_eq!(
                state.trains_remaining + placed,
                trains,
                "vagoni del giocatore {}",
                p
            );
        }
    }
    let Some((final_scores, longest_path_players, longest_path_bonus)) = finished else {
        return Err(TestCaseError::fail("la partita non è arrivata a GameOver"));
    };
    let game = FinishedGame {
        final_scores,
        longest_path_players,
        longest_path_bonus,
        route_points,
        turn_ends_after_announcement: turn_ends,
    };
    Ok((eng, game))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Una partita completa chiude il giro finale, assegna i punti e ha un vincitore.
    #[test]
    fn full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        let (eng, game) = play_to_game_over(seed, players, &choices)?;

        prop_assert_eq!(
            game.turn_ends_after_announcement,
            players + 1,
            "fine turno dopo LastRoundAnnounced"
        );
        prop_assert_eq!(game.final_scores.len(), players);
        let best_total = game.final_scores.iter().map(|fs| fs.total).max().unwrap();
        prop_assert!(game.final_scores.iter().any(|fs| fs.winner), "nessun vincitore");
        for fs in &game.final_scores {
            let state = eng.player(fs.player_index).unwrap();
            let completed: i32 = state.tickets.values()
                .filter(|t| t.completed == Some(true))
                .map(|t| i32::try_from(t.points).unwrap())
                .sum();
            let failed: i32 = state.tickets.values()
                .filter(|t| t.completed != Some(true))
                .map(|t| i32::try_from(t.points).unwrap())
                .sum();
            let bonus = if game.longest_path_players.contains(&fs.player_index) {
                i32::try_from(game.longest_path_bonus).unwrap()
            } else {
                0
            };
            prop_assert_eq!(fs.route_points, game.route_points[fs.player_index]);
            prop_assert_eq!(fs.route_points, state.score);
            prop_assert_eq!(fs.ticket_completed, completed);
            prop_assert_eq!(fs.ticket_failed, -failed);
            prop_assert_eq!(fs.longest_path_bonus, bonus);
            prop_assert_eq!(
                fs.total,
                fs.route_points + fs.ticket_completed + fs.ticket_failed + fs.longest_path_bonus
            );
            if fs.winner {
                prop_assert_eq!(fs.total, best_total, "un vincitore non ha il totale massimo");
            }
        }
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

// ── Longest path: esattezza alla soglia di validazione ───────────────

mod longest_path_threshold {
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    use proptest::prelude::*;
    use ttr_core::scoring::{LongestPath, MAX_CYCLOMATIC_NUMBER, longest_path_report};
    use ttr_core::types::{ClaimedRouteInfo, Route};

    /// Connected multigraph without self-loops on `v` cities: a random spanning tree plus
    /// `cyclomatic` further edges between random distinct cities.
    fn graph(v: usize, cyclomatic: usize, noise: &[u32]) -> Vec<(usize, usize)> {
        let mut edges: Vec<(usize, usize)> = (1..v).map(|i| (i, noise[i] as usize % i)).collect();
        for extra in 0..cyclomatic {
            let a = noise[v + 2 * extra] as usize % v;
            let b = (a + 1 + noise[v + 2 * extra + 1] as usize % (v - 1)) % v;
            edges.push((a, b));
        }
        edges
    }

    /// Endless splitmix64 stream from `seed`.
    fn stream(seed: u64) -> impl FnMut() -> u64 {
        let mut state = seed;
        move || {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
    }

    /// Fisher-Yates shuffle of `0..n` driven by `seed`.
    fn shuffled(n: usize, seed: u64) -> Vec<usize> {
        let mut next = stream(seed);
        let mut items: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            items.swap(i, (next() % (i as u64 + 1)) as usize);
        }
        items
    }

    /// Longest-path report of `edges` with city `i` named after `city_names[i]` and edge
    /// `j` given the route id of slot `edge_slots[j]`.
    fn report(
        edges: &[(usize, usize)],
        lens: &[u32],
        city_names: &[usize],
        edge_slots: &[usize],
    ) -> LongestPath {
        let mut routes: BTreeMap<String, Route> = BTreeMap::new();
        for (j, &(a, b)) in edges.iter().enumerate() {
            let id = format!("r{:03}", edge_slots[j]);
            routes.insert(
                id.clone(),
                Route {
                    id,
                    from: format!("c{:02}", city_names[a]),
                    to: format!("c{:02}", city_names[b]),
                    len: lens[j],
                    color: "Gray".into(),
                    double: false,
                },
            );
        }
        let claimed: Vec<ClaimedRouteInfo> = routes
            .keys()
            .map(|id| ClaimedRouteInfo {
                route_id: id.clone(),
                claimed_by: 0,
            })
            .collect();
        let refs: Vec<&ClaimedRouteInfo> = claimed.iter().collect();
        longest_path_report(&refs, &routes)
    }

    /// Brute force: the longest trail is the heaviest connected edge subset with at most
    /// two odd cities, found by removing the cheapest edge subset first.
    fn brute_force(edges: &[(usize, usize)], lens: &[u32], v: usize) -> u32 {
        struct Search<'a> {
            edges: &'a [(usize, usize)],
            lens: &'a [u32],
            v: usize,
            best_removed: u32,
        }
        impl Search<'_> {
            fn connected(&self, removed: u64) -> bool {
                let mut parent: Vec<usize> = (0..self.v).collect();
                let find = |parent: &mut Vec<usize>, mut x: usize| {
                    while parent[x] != x {
                        x = parent[x];
                    }
                    x
                };
                let mut root = None;
                for (j, &(a, b)) in self.edges.iter().enumerate() {
                    if removed >> j & 1 == 1 {
                        continue;
                    }
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    parent[ra] = rb;
                }
                for (j, &(a, _)) in self.edges.iter().enumerate() {
                    if removed >> j & 1 == 1 {
                        continue;
                    }
                    let r = find(&mut parent, a);
                    if *root.get_or_insert(r) != r {
                        return false;
                    }
                }
                true
            }

            fn walk(&mut self, next: usize, removed: u64, parity: u32, cost: u32) {
                if cost >= self.best_removed {
                    return;
                }
                if parity.count_ones() <= 2 && self.connected(removed) {
                    self.best_removed = cost;
                    return;
                }
                for j in next..self.edges.len() {
                    let (a, b) = self.edges[j];
                    let parity = parity ^ (1 << a) ^ (1 << b);
                    self.walk(j + 1, removed | 1 << j, parity, cost + self.lens[j]);
                }
            }
        }
        let total: u32 = lens.iter().sum();
        let mut search = Search {
            edges,
            lens,
            v,
            best_removed: total,
        };
        search.walk(0, 0, parity_of(edges), 0);
        total - search.best_removed
    }

    fn parity_of(edges: &[(usize, usize)]) -> u32 {
        edges.iter().fold(0, |p, &(a, b)| p ^ (1 << a) ^ (1 << b))
    }

    /// Random route lengths 1..=4 for `count` routes with a total of at most 45 trains.
    fn buildable_lengths(count: usize, next: &mut impl FnMut() -> u64) -> Vec<u32> {
        let mut total = 0;
        (0..count)
            .map(|left| {
                let len = 1 + (next() % 4) as u32;
                let len = if total + len + (count - left - 1) as u32 > 45 {
                    1
                } else {
                    len
                };
                total += len;
                len
            })
            .collect()
    }

    fn identity(n: usize) -> Vec<usize> {
        (0..n).collect()
    }

    /// Search result of `edges` equals the brute force and every relabeling of the cities
    /// and routes, and is exact.
    fn check_exact(
        v: usize,
        edges: &[(usize, usize)],
        lens: &[u32],
        seeds: [u64; 4],
    ) -> Result<(), TestCaseError> {
        let reference = report(edges, lens, &identity(v), &identity(edges.len()));
        prop_assert!(reference.exact, "{edges:?} {lens:?}");
        prop_assert_eq!(
            reference.length,
            brute_force(edges, lens, v),
            "{:?} {:?}",
            edges,
            lens
        );
        for seed in seeds {
            let relabeled = report(
                edges,
                lens,
                &shuffled(v, seed),
                &shuffled(edges.len(), seed ^ 0xa5a5),
            );
            prop_assert_eq!(relabeled, reference, "{:?} {:?}", edges, lens);
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

        /// Connected graphs at exactly `MAX_CYCLOMATIC_NUMBER` with unit lengths: the search
        /// is exact, equals the brute force and does not depend on how the cities are named.
        #[test]
        fn longest_path_is_exact_at_max_cyclomatic_number(
            v in 7usize..=14,
            noise in proptest::collection::vec(any::<u32>(), 64),
            seeds in any::<[u64; 4]>(),
        ) {
            let edges = graph(v, MAX_CYCLOMATIC_NUMBER as usize, &noise);
            check_exact(v, &edges, &vec![1; edges.len()], seeds)?;
        }

        /// Same with mixed route lengths adding up to at most the 45 trains of a player.
        #[test]
        fn longest_path_is_exact_at_max_cyclomatic_number_with_mixed_lengths(
            v in 7usize..=14,
            noise in proptest::collection::vec(any::<u32>(), 64),
            seeds in any::<[u64; 5]>(),
        ) {
            let edges = graph(v, MAX_CYCLOMATIC_NUMBER as usize, &noise);
            let mut next = stream(seeds[4]);
            let lens = buildable_lengths(edges.len(), &mut next);
            check_exact(v, &edges, &lens, [seeds[0], seeds[1], seeds[2], seeds[3]])?;
        }
    }

    /// Random connected multigraph without self-loops in which every one of `v` cities has
    /// three routes (the pairing model, retried until connected).
    fn cubic_graph(v: usize, next: &mut impl FnMut() -> u64) -> Vec<(usize, usize)> {
        fn root(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                x = parent[x];
            }
            x
        }
        loop {
            let mut stubs: Vec<usize> = (0..v).flat_map(|c| [c, c, c]).collect();
            for i in (1..stubs.len()).rev() {
                stubs.swap(i, (next() % (i as u64 + 1)) as usize);
            }
            let edges: Vec<(usize, usize)> = stubs.chunks(2).map(|c| (c[0], c[1])).collect();
            let mut parent: Vec<usize> = (0..v).collect();
            for &(a, b) in &edges {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                parent[ra] = rb;
            }
            let first = root(&mut parent, 0);
            if edges.iter().all(|&(a, b)| a != b) && (0..v).all(|c| root(&mut parent, c) == first) {
                return edges;
            }
        }
    }

    /// Panics unless `edges` is a connected graph on `v` cities with cyclomatic number
    /// `MAX_CYCLOMATIC_NUMBER`.
    fn assert_threshold_shape(v: usize, edges: &[(usize, usize)]) {
        let mut parent: Vec<usize> = (0..v).collect();
        for &(a, b) in edges {
            let (mut ra, mut rb) = (a, b);
            while parent[ra] != ra {
                ra = parent[ra];
            }
            while parent[rb] != rb {
                rb = parent[rb];
            }
            parent[ra] = rb;
        }
        let roots = (0..v).filter(|&c| parent[c] == c).count();
        assert_eq!(roots, 1, "not connected: {edges:?}");
        assert_eq!(
            edges.len() + 1 - v,
            MAX_CYCLOMATIC_NUMBER as usize,
            "cyclomatic number: {edges:?}"
        );
    }

    /// Release timing: every graph at the threshold (random graphs with unit and mixed
    /// lengths, 26-city cubic graphs and 20-city unit-length graphs, the hardest shapes)
    /// is solved exactly in under a second.
    #[test]
    #[ignore = "timing: run with --release"]
    fn longest_path_at_max_cyclomatic_number_takes_under_a_second() {
        let time = |edges: &[(usize, usize)], lens: &[u32], v: usize| {
            assert_threshold_shape(v, edges);
            assert!(lens.iter().sum::<u32>() <= 45, "{edges:?} {lens:?}");
            let started = Instant::now();
            let result = report(edges, lens, &identity(v), &identity(edges.len()));
            let elapsed = started.elapsed();
            assert!(result.exact, "{edges:?} {lens:?}");
            assert!(
                elapsed < Duration::from_secs(1),
                "{elapsed:?} {edges:?} {lens:?}"
            );
            elapsed
        };
        let mut worst = Duration::ZERO;
        for case in 0..600u64 {
            let v = 7 + (case % 8) as usize;
            let mut next = stream(case);
            let noise: Vec<u32> = (0..64).map(|_| next() as u32).collect();
            let edges = graph(v, MAX_CYCLOMATIC_NUMBER as usize, &noise);
            let lens = if case % 2 == 0 {
                vec![1; edges.len()]
            } else {
                buildable_lengths(edges.len(), &mut next)
            };
            worst = worst.max(time(&edges, &lens, v));
        }
        eprintln!("random graphs: worst {worst:?}");

        let mut worst = Duration::ZERO;
        for case in 0..100u64 {
            let mut next = stream(1_000 + case);
            let edges = cubic_graph(26, &mut next);
            let lens = if case % 2 == 0 {
                vec![1; edges.len()]
            } else {
                buildable_lengths(edges.len(), &mut next)
            };
            worst = worst.max(time(&edges, &lens, 26));
        }
        eprintln!("cubic graphs, 26 cities: worst {worst:?}");

        let mut worst = Duration::ZERO;
        for case in 0..10u64 {
            let mut next = stream(2_000 + case);
            let noise: Vec<u32> = (0..64).map(|_| next() as u32).collect();
            let edges = graph(20, MAX_CYCLOMATIC_NUMBER as usize, &noise);
            assert_eq!(edges.len(), 33);
            worst = worst.max(time(&edges, &vec![1; edges.len()], 20));
        }
        eprintln!("unit-length graphs, 20 cities: worst {worst:?}");
    }
}
