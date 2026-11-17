//! Test property-based sul motore: conservazione, azioni fallite,
//! legal-vs-apply e replay, su partite casuali sulle mappe USA, Europe, Germany e Poland reali.
//!
//! Sull'Europe i bot costruiscono anche stazioni e pagano o rifiutano i tunnel; sulla
//! Germany pescano i biglietti con le quote per mazzo.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use proptest::prelude::*;
use ttr_core::error::GamePhase;
use ttr_core::types::{
    GLOBETROTTER_BONUS_TYPE, LONGEST_PATH_BONUS_TYPE, MapData, Ruleset, SetupDiscards, TRAIN_COLORS,
};
use ttr_core::{
    Action, DrawSource, Engine, GameEvent, RouteColor, TicketDrawMoves, default_player_colors,
};
use ttr_proto::CardColor;

/// Carte di un mazzo completo (12 × 8 colori + 14 locomotive, come USA, Europe, Germany e Poland).
const FULL_DECK: usize = 110;

/// Massimo di mosse per partita casuale.
const MAX_STEPS: usize = 400;

/// Mappa e ruleset su cui giocano i bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Game {
    Usa,
    Europe,
    /// USA Mega: 5 biglietti tieni 3, pesca 4, due bonus.
    UsaMega,
    /// USA Grandi Città: nessun bonus.
    UsaBigCities,
    /// Europe Mega: 2 lunghi (al massimo 1) + 5 regolari, tieni 3.
    EuropeMega,
    /// Europe Grandi Città: solo regolari, pesca 4.
    EuropeBigCities,
    /// Germany: gruppi di 3 binari, Paesi, pesca con scelta del mazzo (corti/lunghi),
    /// Globetrotter senza percorso più lungo.
    Germany,
    /// Poland: gruppi di 3–4 binari sempre aperti, Paesi con carte Paese, spareggi sui punti
    /// dei biglietti.
    Poland,
    /// Germany 2017: 89 biglietti, passeggeri con scelta del colore al claim.
    Germany2017,
}

static USA: LazyLock<(Ruleset, MapData)> = LazyLock::new(|| load("usa_v2", "usa"));
static EUROPE: LazyLock<(Ruleset, MapData)> = LazyLock::new(|| load("europe", "europe"));
static USA_MEGA: LazyLock<(Ruleset, MapData)> = LazyLock::new(|| load("usa_mega", "usa"));
static USA_BIG_CITIES: LazyLock<(Ruleset, MapData)> =
    LazyLock::new(|| load("usa_big_cities", "usa"));
static EUROPE_MEGA: LazyLock<(Ruleset, MapData)> = LazyLock::new(|| load("europe_mega", "europe"));
static EUROPE_BIG_CITIES: LazyLock<(Ruleset, MapData)> =
    LazyLock::new(|| load("europe_big_cities", "europe"));
static GERMANY: LazyLock<(Ruleset, MapData)> = LazyLock::new(|| load("germany", "germany"));
static POLAND: LazyLock<(Ruleset, MapData)> = LazyLock::new(|| load("poland", "poland"));
static GERMANY_2017: LazyLock<(Ruleset, MapData)> =
    LazyLock::new(|| load("germany_2017", "germany"));

fn load(ruleset: &str, map: &str) -> (Ruleset, MapData) {
    let ruleset = std::fs::read_to_string(format!("../../data/rulesets/{ruleset}.json")).unwrap();
    let map = std::fs::read_to_string(format!("../../data/maps/{map}.json")).unwrap();
    (
        serde_json::from_str(&ruleset).unwrap(),
        serde_json::from_str(&map).unwrap(),
    )
}

fn data(game: Game) -> &'static (Ruleset, MapData) {
    match game {
        Game::Usa => &USA,
        Game::Europe => &EUROPE,
        Game::UsaMega => &USA_MEGA,
        Game::UsaBigCities => &USA_BIG_CITIES,
        Game::EuropeMega => &EUROPE_MEGA,
        Game::EuropeBigCities => &EUROPE_BIG_CITIES,
        Game::Germany => &GERMANY,
        Game::Poland => &POLAND,
        Game::Germany2017 => &GERMANY_2017,
    }
}

/// Modalità diverse dallo standard coperte dai test property-based.
const MODE_GAMES: [Game; 4] = [
    Game::UsaMega,
    Game::UsaBigCities,
    Game::EuropeMega,
    Game::EuropeBigCities,
];

/// Numero di giocatori ammesso dal ruleset più vicino a `players`.
fn clamp_players(game: Game, players: usize) -> usize {
    let range = &data(game).0.players;
    players.clamp(range.min as usize, range.max as usize)
}

fn new_engine(game: Game, seed: u64, players: usize) -> Engine {
    let (ruleset, map) = data(game);
    let players = clamp_players(game, players);
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

/// Pagamento valido per la rotta con le carte in mano, se esiste: i traghetti
/// richiedono almeno `ferries` locomotive.
fn build_payment(
    hand: &[CardColor],
    color: RouteColor,
    len: usize,
    ferries: usize,
) -> Option<Vec<CardColor>> {
    let count = |c: CardColor| hand.iter().filter(|h| **h == c).count();
    let locos = count(CardColor::Locomotive);
    let colors: Vec<CardColor> = match color {
        RouteColor::Colored(c) => vec![c],
        RouteColor::Gray => TRAIN_COLORS.to_vec(),
    };
    for c in colors {
        let colored = count(c).min(len - ferries);
        if locos >= len - colored {
            let mut payment = vec![c; colored];
            payment.resize(len, CardColor::Locomotive);
            return Some(payment);
        }
    }
    (locos >= len).then(|| vec![CardColor::Locomotive; len])
}

fn route_payment(eng: &Engine, player: usize, route_id: &str) -> Option<Vec<CardColor>> {
    let route = eng.get_route(route_id)?;
    let color = RouteColor::parse(&route.color)?;
    build_payment(
        eng.player_hand(player)?,
        color,
        route.len as usize,
        route.ferries as usize,
    )
}

/// Passeggeri da prendere con la rotta: il primo colore (alfabetico) di ogni estremo che ne
/// ha più d'uno; vuoto senza passeggeri.
fn passenger_picks(eng: &Engine, route_id: &str) -> BTreeMap<String, String> {
    let Some(route) = eng.get_route(route_id) else {
        return BTreeMap::new();
    };
    [&route.from, &route.to]
        .into_iter()
        .filter_map(|city| {
            let colors = eng.passengers_on_cities().get(city)?;
            let first = colors.keys().next()?;
            (colors.len() > 1).then(|| (city.clone(), first.clone()))
        })
        .collect()
}

/// `cost` carte di un solo colore (più locomotive) per una stazione, se la mano le ha.
fn station_payment(eng: &Engine, player: usize, cost: usize) -> Option<Vec<CardColor>> {
    build_payment(eng.player_hand(player)?, RouteColor::Gray, cost, 0)
}

/// `extra` carte del colore del tunnel o locomotive (solo locomotive con `Locomotive`).
fn tunnel_payment(hand: &[CardColor], color: CardColor, extra: usize) -> Option<Vec<CardColor>> {
    let mut payment: Vec<CardColor> = hand.iter().copied().filter(|c| *c == color).collect();
    payment.truncate(extra);
    let locos = hand.iter().filter(|c| **c == CardColor::Locomotive);
    payment.extend(locos.take(extra - payment.len()));
    (payment.len() == extra).then_some(payment)
}

/// Fino a `want` biglietti tra gli offerti, nell'ordine di offerta, rispettando i
/// limiti di `ticket_choice` di `legal_moves`.
fn keep_within_limits(eng: &Engine, pi: usize, offered: &[String], want: usize) -> Vec<String> {
    let limits = eng
        .legal_moves(pi)
        .ticket_choice
        .map(|c| c.limits)
        .unwrap_or_default();
    let mut left: Vec<u32> = limits.iter().map(|l| l.max).collect();
    let mut keep = Vec::new();
    for id in offered {
        if keep.len() == want {
            break;
        }
        let group = limits.iter().position(|l| l.ticket_ids.contains(id));
        if let Some(g) = group {
            if left[g] == 0 {
                continue;
            }
            left[g] -= 1;
        }
        keep.push(id.clone());
    }
    keep
}

/// Quote per una pesca con scelta del mazzo: riempie i mazzi nell'ordine (o al contrario).
fn greedy_counts(draw: &TicketDrawMoves, reverse: bool) -> BTreeMap<String, u32> {
    let mut left = draw.total;
    let mut counts = BTreeMap::new();
    let mut decks: Vec<_> = draw.available.iter().collect();
    if reverse {
        decks.reverse();
    }
    for deck in decks {
        let take = deck.count.min(left);
        if take > 0 {
            counts.insert(deck.deck.clone(), take);
        }
        left -= take;
    }
    counts
}

/// Quote per una pesca con scelta del mazzo: un biglietto alla volta, un mazzo dopo l'altro.
fn round_robin_counts(draw: &TicketDrawMoves) -> BTreeMap<String, u32> {
    let mut left = draw.total;
    let mut taken: Vec<u32> = vec![0; draw.available.len()];
    while left > 0 {
        let mut progressed = false;
        for (slot, deck) in taken.iter_mut().zip(&draw.available) {
            if left > 0 && *slot < deck.count {
                *slot += 1;
                left -= 1;
                progressed = true;
            }
        }
        assert!(progressed, "quote impossibili da distribuire");
    }
    draw.available
        .iter()
        .zip(taken)
        .filter(|(_, n)| *n > 0)
        .map(|(deck, n)| (deck.deck.clone(), n))
        .collect()
}

/// Pescate di biglietti valide per il giocatore: con le quote (`chooseFrom`) tre ripartizioni
/// diverse, altrimenti la pesca classica senza quote se è ammessa.
fn draw_ticket_actions(
    player_index: usize,
    draw: Option<&TicketDrawMoves>,
    can_draw: bool,
) -> Vec<Action> {
    let draw_with = |counts| Action::DrawTickets {
        player_index,
        counts,
    };
    match draw {
        Some(moves) => vec![
            draw_with(greedy_counts(moves, false)),
            draw_with(greedy_counts(moves, true)),
            draw_with(round_robin_counts(moves)),
        ],
        None if can_draw => vec![draw_with(BTreeMap::new())],
        None => Vec::new(),
    }
}

/// Azioni sensate per la fase corrente, usate per far avanzare la partita.
fn progress_actions(game: Game, eng: &Engine) -> Vec<Action> {
    let (ruleset, _) = data(game);
    match eng.phase() {
        GamePhase::InitialDraft => {
            let mut actions = Vec::new();
            for pi in 0..eng.num_players() {
                let offered = eng.initial_tickets_for(pi).unwrap_or_default();
                if offered.is_empty() {
                    // Con `chooseFrom` il draft inizia pescando con le quote.
                    let legal = eng.legal_moves(pi);
                    actions.extend(draw_ticket_actions(
                        pi,
                        legal.ticket_draw.as_ref(),
                        legal.can_draw_tickets,
                    ));
                    continue;
                }
                let keep_min = ruleset.initial_tickets.keep_min as usize;
                for k in [keep_min, offered.len()] {
                    actions.push(Action::ChooseTickets {
                        player_index: pi,
                        keep: keep_within_limits(eng, pi, offered, k),
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
                    keep: keep_within_limits(eng, eng.current_player(), pending, k),
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
                    passengers: passenger_picks(eng, route_id),
                });
            }
            if let Some(tunnel) = &legal.tunnel {
                actions.push(Action::TunnelDecision {
                    player_index: pi,
                    pay: false,
                    cards_discarded: Vec::new(),
                });
                if tunnel.can_pay {
                    let cards = tunnel_payment(
                        eng.player_hand(pi).unwrap(),
                        tunnel.color,
                        tunnel.extra_cost as usize,
                    )
                    .expect("tunnel pagabile senza carte");
                    actions.push(Action::TunnelDecision {
                        player_index: pi,
                        pay: true,
                        cards_discarded: cards,
                    });
                }
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
            actions.extend(draw_ticket_actions(
                pi,
                legal.ticket_draw.as_ref(),
                legal.can_draw_tickets,
            ));
            for city in &legal.station_cities {
                let cards = station_payment(eng, pi, legal.station_cost as usize)
                    .expect("stazione elencata ma non pagabile");
                actions.push(Action::BuildStation {
                    player_index: pi,
                    city_id: city.clone(),
                    cards_discarded: cards,
                });
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
fn arbitrary_action(game: Game, eng: &Engine, picks: [u16; 4]) -> Action {
    let (_, map) = data(game);
    let player_index = usize::from(picks[0]) % (eng.num_players() + 1);
    match picks[1] % 6 {
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
        1 => {
            // Quote per mazzo a caso (anche vuote, sbagliate o di un mazzo ignoto).
            let counts: BTreeMap<String, u32> = match picks[3] % 4 {
                0 => BTreeMap::new(),
                1 => BTreeMap::from([("regular".to_string(), u32::from(picks[2] % 6))]),
                2 => BTreeMap::from([("nessuno".to_string(), u32::from(picks[2] % 3))]),
                _ => BTreeMap::from([
                    ("regular".to_string(), u32::from(picks[2] % 5)),
                    ("long".to_string(), u32::from(picks[2] % 3)),
                ]),
            };
            Action::DrawTickets {
                player_index,
                counts,
            }
        }
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
        3 => {
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
                passengers: Default::default(),
            }
        }
        4 => {
            let city_index = usize::from(picks[2]) % (map.cities.len() + 1);
            let city_id = map
                .cities
                .get(city_index)
                .map_or("citta_inesistente".to_string(), |c| c.id.clone());
            let hand = eng.player_hand(player_index).unwrap_or_default();
            let cost = 1 + usize::from(picks[3]) % 3;
            let cards: Vec<CardColor> = hand
                .iter()
                .cycle()
                .skip(usize::from(picks[3]) % hand.len().max(1))
                .take(cost.min(hand.len()))
                .copied()
                .collect();
            Action::BuildStation {
                player_index,
                city_id,
                cards_discarded: cards,
            }
        }
        _ => {
            let hand = eng.player_hand(player_index).unwrap_or_default();
            let cards: Vec<CardColor> = hand
                .iter()
                .cycle()
                .skip(usize::from(picks[2]) % hand.len().max(1))
                .take(usize::from(picks[3]) % 4)
                .copied()
                .collect();
            Action::TunnelDecision {
                player_index,
                pay: picks[3].is_multiple_of(2),
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
    ticket_piles: BTreeMap<String, Vec<String>>,
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
        ticket_piles: eng.ticket_piles().clone(),
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

/// Carte treno in gioco, comprese quelle sul tavolo durante un tunnel pendente.
fn total_train_cards(eng: &Engine) -> usize {
    eng.draw_pile_len()
        + eng.face_up_cards().iter().flatten().count()
        + eng.discards_len()
        + eng
            .pending_tunnel()
            .map_or(0, |p| p.played.len() + p.revealed.len())
        + (0..eng.num_players())
            .map(|p| eng.player_hand(p).unwrap().len())
            .sum::<usize>()
}

/// Invarianti di stazioni e tunnel validi a ogni passo.
fn check_station_and_tunnel_state(game: Game, eng: &Engine) -> Result<(), TestCaseError> {
    let (ruleset, map) = data(game);
    let total = ruleset.stations.as_ref().map_or(0, |s| s.count);
    let placements = eng.station_placements();
    let mut cities = BTreeSet::new();
    for (city, owner) in &placements {
        prop_assert!(cities.insert(*city), "due stazioni a {city}");
        prop_assert!(map.cities.iter().any(|c| &c.id == *city));
        prop_assert!(eng.player(*owner).unwrap().stations.contains(*city));
    }
    let mut built = 0;
    for p in 0..eng.num_players() {
        let state = eng.player(p).unwrap();
        built += state.stations.len();
        prop_assert!(state.stations.len() as u32 <= total, "troppe stazioni");
        prop_assert_eq!(
            eng.stations_remaining(p) as usize + state.stations.len(),
            total as usize
        );
    }
    prop_assert_eq!(built, placements.len());

    // Un tunnel è pendente se e solo se la fase è TUNNEL_RESOLUTION, con costo extra > 0.
    prop_assert_eq!(
        eng.pending_tunnel().is_some(),
        eng.phase() == GamePhase::TunnelResolution
    );
    if let Some(pending) = eng.pending_tunnel() {
        prop_assert!(pending.extra > 0 && pending.extra as usize <= pending.revealed.len());
        let route = eng.get_route(&pending.route_id).unwrap();
        prop_assert!(route.tunnel);
        prop_assert_eq!(pending.played.len(), route.len as usize);
        prop_assert!(!eng.claimed_route_ids().contains(&&pending.route_id));
        let colors = pending
            .played
            .iter()
            .filter(|c| **c != CardColor::Locomotive);
        prop_assert!(colors.clone().all(|c| *c == pending.color));
        if colors.count() == 0 {
            prop_assert_eq!(pending.color, CardColor::Locomotive);
        }
    }
    Ok(())
}

/// Gioca una partita casuale chiamando `hook` prima di ogni mossa.
fn play(
    game: Game,
    seed: u64,
    players: usize,
    choices: &[u16],
    mut hook: impl FnMut(&mut Engine, &[u16]) -> Result<(), TestCaseError>,
) -> Result<Engine, TestCaseError> {
    let mut eng = new_engine(game, seed, players);
    for window in choices.windows(4).step_by(4).take(MAX_STEPS) {
        hook(&mut eng, window)?;
        let Some(action) = pick(progress_actions(game, &eng), window[0]) else {
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

/// Carte treno e biglietti si conservano: solo D27 aggiunge mazzi interi.
fn check_conservation(
    game: Game,
    seed: u64,
    players: usize,
    choices: &[u16],
) -> Result<(), TestCaseError> {
    let (ruleset, map) = data(game);
    let in_play: BTreeSet<&String> = map.tickets_in_play(ruleset).map(|t| &t.id).collect();
    let mut previous_total = FULL_DECK;
    play(game, seed, players, choices, |eng, _| {
        let total = total_train_cards(eng);
        prop_assert!(
            total >= previous_total && total.is_multiple_of(FULL_DECK),
            "carte {total}"
        );
        previous_total = total;
        prop_assert!(eng.face_up_cards().iter().all(Option::is_some));
        check_station_and_tunnel_state(game, eng)?;

        let mut ids: Vec<&String> = eng.ticket_piles().values().flatten().collect();
        ids.extend(eng.pending_tickets());
        for p in 0..eng.num_players() {
            ids.extend(eng.player(p).unwrap().tickets.keys());
            ids.extend(eng.initial_tickets_for(p).unwrap());
        }
        let unique: BTreeSet<&String> = ids.iter().copied().collect();
        prop_assert!(unique.is_subset(&in_play), "biglietto fuori dalla modalità");
        match ruleset.setup_discards {
            SetupDiscards::ReshuffleBottom if ruleset.ticket_decks.len() == 1 => {
                prop_assert_eq!(ids.len(), in_play.len(), "biglietti persi o creati");
            }
            SetupDiscards::ReshuffleBottomOwnDeck => {
                prop_assert_eq!(ids.len(), in_play.len(), "biglietti persi o creati");
            }
            // Scarti di setup fuori dalla partita (Europe) o mazzi non pescabili (lunghi).
            _ => prop_assert!(ids.len() <= in_play.len(), "biglietti creati"),
        }
        prop_assert_eq!(unique.len(), ids.len(), "biglietto duplicato");

        // Le carte Paese si conservano: le prese sono le prime della pila iniziale e la
        // pila ha le altre, nello stesso ordine.
        for (country, original) in &ruleset.country_cards {
            let stack = &eng.country_stacks()[country];
            let taken: usize = (0..eng.num_players())
                .map(|p| {
                    eng.player(p)
                        .unwrap()
                        .country_cards
                        .get(country)
                        .map_or(0, Vec::len)
                })
                .sum();
            prop_assert_eq!(taken + stack.len(), original.len(), "carte {}", country);
            prop_assert_eq!(&original[taken..], stack.as_slice());
        }

        // I passeggeri si conservano: quelli sulle città più quelli dei giocatori.
        if let Some(config) = &ruleset.passengers {
            let on_cities: u32 = eng
                .passengers_on_cities()
                .values()
                .flat_map(|colors| colors.values())
                .sum();
            let held: u32 = (0..eng.num_players())
                .map(|p| eng.player(p).unwrap().passengers.values().sum::<u32>())
                .sum();
            let bag = config.per_color * config.colors.len() as u32;
            prop_assert_eq!(on_cities + held, bag, "passeggeri persi o creati");
        }
        Ok(())
    })?;
    Ok(())
}

/// Un'azione rifiutata non modifica lo stato né la prossima estrazione dell'RNG.
fn check_failed_action_leaves_state_identical(
    game: Game,
    seed: u64,
    players: usize,
    choices: &[u16],
) -> Result<(), TestCaseError> {
    play(game, seed, players, choices, |eng, window| {
        let picks = [window[0], window[1], window[2], window[3]];
        let action = arbitrary_action(game, eng, picks);
        let before = snapshot(eng);
        if eng.apply(action.clone()).is_err() {
            prop_assert_eq!(snapshot(eng), before, "azione fallita {:?}", action);
        }
        Ok(())
    })?;
    Ok(())
}

/// Ogni mossa dichiarata legale riesce e ogni mossa non dichiarata fallisce.
fn check_legal_moves_agree_with_apply(
    game: Game,
    seed: u64,
    players: usize,
    choices: &[u16],
) -> Result<(), TestCaseError> {
    let (ruleset, map) = data(game);
    play(game, seed, players, choices, |eng, window| {
        let pi = eng.current_player();
        for other in (0..eng.num_players()).filter(|p| *p != pi) {
            let lm = eng.legal_moves(other);
            prop_assert!(
                lm.routes.is_empty()
                    && lm.draw_face_up.is_empty()
                    && !lm.can_draw_deck
                    && lm.station_cities.is_empty()
                    && lm.station_cost == 0
                    && lm.tunnel.is_none(),
                "mosse per il giocatore {other} non di turno"
            );
            // Il draft iniziale è simultaneo: la scelta spetta a chi non l'ha ancora fatta,
            // dopo aver pescato i biglietti (con `chooseFrom` la pesca con le quote viene prima).
            let drafting = eng.phase() == GamePhase::InitialDraft
                && !eng.player(other).unwrap().initial_draft_done;
            let has_offers = !eng.initial_tickets_for(other).unwrap().is_empty();
            prop_assert_eq!(
                lm.ticket_choice.is_some(),
                drafting && has_offers,
                "ticket_choice di {}",
                other
            );
            prop_assert_eq!(
                lm.ticket_draw.is_some(),
                drafting && !has_offers && ruleset.initial_tickets.choose_from.is_some(),
                "ticket_draw di {}",
                other
            );
            prop_assert_eq!(lm.can_draw_tickets, lm.ticket_draw.is_some());
        }
        if eng.phase() == GamePhase::GameOver {
            return Ok(());
        }
        let lm = eng.legal_moves(pi);

        // La scelta dei biglietti è elencata nel draft iniziale e in `TicketChoice`; la
        // scelta che rispetta i limiti elencati deve riuscire.
        let offered: Vec<String> = match eng.phase() {
            GamePhase::InitialDraft => eng.initial_tickets_for(pi).unwrap().to_vec(),
            GamePhase::TicketChoice => eng.pending_tickets().to_vec(),
            _ => Vec::new(),
        };
        prop_assert_eq!(
            lm.ticket_choice.is_some(),
            !offered.is_empty(),
            "ticket_choice"
        );
        if let Some(choice) = &lm.ticket_choice {
            let keep = keep_within_limits(eng, pi, &offered, choice.keep_min as usize);
            prop_assert!(
                eng.clone()
                    .apply(Action::ChooseTickets {
                        player_index: pi,
                        keep
                    })
                    .is_ok(),
                "scelta biglietti elencata rifiutata"
            );
        }

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
        for route in map
            .routes
            .iter()
            .cycle()
            .skip(offset)
            .take(map.routes.len())
        {
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
                passengers: passenger_picks(eng, &route.id),
            };
            check(eng, action, listed, &format!("rotta {}", route.id))?;
        }
        for index in 0..eng.face_up_cards().len() {
            let action = Action::DrawTrainCard {
                player_index: pi,
                source: DrawSource::FaceUp { index },
            };
            check(
                eng,
                action,
                lm.draw_face_up.contains(&index),
                &format!("scoperta {index}"),
            )?;
        }
        let deck = Action::DrawTrainCard {
            player_index: pi,
            source: DrawSource::Deck,
        };
        check(eng, deck, lm.can_draw_deck, "pesca dal mazzo")?;
        let ticket_counts = lm
            .ticket_draw
            .as_ref()
            .map(|draw| greedy_counts(draw, false))
            .unwrap_or_default();
        let tickets = Action::DrawTickets {
            player_index: pi,
            counts: ticket_counts,
        };
        check(eng, tickets, lm.can_draw_tickets, "pesca biglietti")?;
        // Quote con la somma sbagliata non sono mai valide.
        if let Some(draw) = &lm.ticket_draw {
            let mut counts = greedy_counts(draw, false);
            *counts.entry(draw.available[0].deck.clone()).or_default() += 1;
            let wrong = Action::DrawTickets {
                player_index: pi,
                counts,
            };
            check(eng, wrong, false, "quote con somma errata")?;
        }

        // Stazioni: elencate solo in città libere e pagabili; il costo è quello della prossima.
        let in_turn = matches!(eng.phase(), GamePhase::PlayerTurn | GamePhase::LastRound);
        let built = eng.player(pi).unwrap().stations.len();
        if in_turn {
            prop_assert_eq!(lm.station_cost == 0, eng.stations_remaining(pi) == 0);
            if lm.station_cost > 0 {
                let costs = &data(game).0.stations.as_ref().unwrap().costs;
                prop_assert_eq!(lm.station_cost, costs[built]);
            }
        } else {
            prop_assert_eq!(lm.station_cost, 0);
        }
        let payment = (lm.station_cost > 0)
            .then(|| station_payment(eng, pi, lm.station_cost as usize))
            .flatten();
        let placed: Vec<String> = eng
            .station_placements()
            .iter()
            .map(|(city, _)| (*city).clone())
            .collect();
        for city in &placed {
            prop_assert!(!lm.station_cities.contains(city), "città occupata {city}");
        }
        prop_assert_eq!(
            !lm.station_cities.is_empty(),
            in_turn && payment.is_some(),
            "stazioni elencate"
        );
        if let Some(cards) = payment.filter(|_| in_turn) {
            prop_assert_eq!(lm.station_cities.len(), map.cities.len() - placed.len());
            for city in lm.station_cities.iter().step_by(7) {
                let action = Action::BuildStation {
                    player_index: pi,
                    city_id: city.clone(),
                    cards_discarded: cards.clone(),
                };
                check(eng, action, true, &format!("stazione {city}"))?;
            }
            if let Some(city) = placed.first() {
                let action = Action::BuildStation {
                    player_index: pi,
                    city_id: city.clone(),
                    cards_discarded: cards,
                };
                check(eng, action, false, &format!("stazione occupata {city}"))?;
            }
        }

        // Tunnel: la rinuncia è sempre ammessa; il pagamento solo con `can_pay`.
        prop_assert_eq!(
            lm.tunnel.is_some(),
            eng.phase() == GamePhase::TunnelResolution
        );
        if let Some(tunnel) = &lm.tunnel {
            let decline = Action::TunnelDecision {
                player_index: pi,
                pay: false,
                cards_discarded: Vec::new(),
            };
            check(eng, decline, true, "rinuncia al tunnel")?;
            let extra = tunnel.extra_cost as usize;
            let hand = eng.player_hand(pi).unwrap();
            let cards = tunnel_payment(hand, tunnel.color, extra).unwrap_or_else(|| {
                // Mano insufficiente: un pagamento qualunque deve comunque fallire.
                hand.iter().cycle().take(extra).copied().collect()
            });
            let pay = Action::TunnelDecision {
                player_index: pi,
                pay: true,
                cards_discarded: cards,
            };
            check(eng, pay, tunnel.can_pay, "pagamento del tunnel")?;
        }
        Ok(())
    })?;
    Ok(())
}

/// Rigiocare `action_log` da seed e giocatori riproduce stato ed eventi.
fn check_replay_reproduces_state(
    game: Game,
    seed: u64,
    players: usize,
    choices: &[u16],
) -> Result<(), TestCaseError> {
    let original = play(game, seed, players, choices, |_, _| Ok(()))?;
    let mut replayed = new_engine(game, seed, players);
    for record in original.action_log() {
        let events = replayed.apply(record.action.clone());
        prop_assert!(events.is_ok(), "replay fallito: {:?}", record.action);
        prop_assert_eq!(
            format!("{:?}", events.unwrap()),
            format!("{:?}", record.events),
            "eventi diversi per {:?}",
            record.action
        );
    }
    prop_assert_eq!(snapshot(&replayed), snapshot(&original));
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    #[test]
    fn cards_and_tickets_are_conserved((seed, players, choices) in game_inputs()) {
        check_conservation(Game::Usa, seed, players, &choices)?;
    }

    #[test]
    fn failed_action_leaves_state_identical((seed, players, choices) in game_inputs()) {
        check_failed_action_leaves_state_identical(Game::Usa, seed, players, &choices)?;
    }

    #[test]
    fn legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        check_legal_moves_agree_with_apply(Game::Usa, seed, players, &choices)?;
    }

    #[test]
    fn replay_reproduces_state((seed, players, choices) in game_inputs()) {
        check_replay_reproduces_state(Game::Usa, seed, players, &choices)?;
    }

    #[test]
    fn europe_cards_and_tickets_are_conserved((seed, players, choices) in game_inputs()) {
        check_conservation(Game::Europe, seed, players, &choices)?;
    }

    #[test]
    fn europe_failed_action_leaves_state_identical((seed, players, choices) in game_inputs()) {
        check_failed_action_leaves_state_identical(Game::Europe, seed, players, &choices)?;
    }

    #[test]
    fn europe_legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        check_legal_moves_agree_with_apply(Game::Europe, seed, players, &choices)?;
    }

    #[test]
    fn europe_replay_reproduces_state((seed, players, choices) in game_inputs()) {
        check_replay_reproduces_state(Game::Europe, seed, players, &choices)?;
    }

    #[test]
    fn germany_cards_and_tickets_are_conserved((seed, players, choices) in game_inputs()) {
        check_conservation(Game::Germany, seed, players, &choices)?;
    }

    #[test]
    fn germany_failed_action_leaves_state_identical((seed, players, choices) in game_inputs()) {
        check_failed_action_leaves_state_identical(Game::Germany, seed, players, &choices)?;
    }

    #[test]
    fn germany_legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        check_legal_moves_agree_with_apply(Game::Germany, seed, players, &choices)?;
    }

    #[test]
    fn germany_replay_reproduces_state((seed, players, choices) in game_inputs()) {
        check_replay_reproduces_state(Game::Germany, seed, players, &choices)?;
    }

    #[test]
    fn poland_cards_and_tickets_are_conserved((seed, players, choices) in game_inputs()) {
        check_conservation(Game::Poland, seed, players, &choices)?;
    }

    #[test]
    fn poland_failed_action_leaves_state_identical((seed, players, choices) in game_inputs()) {
        check_failed_action_leaves_state_identical(Game::Poland, seed, players, &choices)?;
    }

    #[test]
    fn poland_legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        check_legal_moves_agree_with_apply(Game::Poland, seed, players, &choices)?;
    }

    #[test]
    fn poland_replay_reproduces_state((seed, players, choices) in game_inputs()) {
        check_replay_reproduces_state(Game::Poland, seed, players, &choices)?;
    }

    #[test]
    fn germany_2017_cards_tickets_and_passengers_are_conserved(
        (seed, players, choices) in game_inputs()
    ) {
        check_conservation(Game::Germany2017, seed, players, &choices)?;
    }

    #[test]
    fn germany_2017_failed_action_leaves_state_identical(
        (seed, players, choices) in game_inputs()
    ) {
        check_failed_action_leaves_state_identical(Game::Germany2017, seed, players, &choices)?;
    }

    #[test]
    fn germany_2017_legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        check_legal_moves_agree_with_apply(Game::Germany2017, seed, players, &choices)?;
    }

    #[test]
    fn germany_2017_replay_reproduces_state((seed, players, choices) in game_inputs()) {
        check_replay_reproduces_state(Game::Germany2017, seed, players, &choices)?;
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
    game: Game,
    seed: u64,
    players: usize,
    choices: &[u16],
) -> Result<(Engine, FinishedGame), TestCaseError> {
    let players = clamp_players(game, players);
    let trains = data(game).0.trains_per_player;
    let mut eng = new_engine(game, seed, players);
    let mut route_points = vec![0i32; players];
    let mut announced = false;
    let mut turn_ends = 0;
    let mut finished = None;
    for step in 0..MAX_GAME_STEPS {
        let choice = choices[step % choices.len()].wrapping_add((step / choices.len()) as u16);
        let Some(action) = pick(progress_actions(game, &eng), choice) else {
            break;
        };
        let events = eng.apply(action.clone());
        prop_assert!(events.is_ok(), "azione legale rifiutata: {action:?}");
        let events = events.unwrap();
        let declined = events
            .iter()
            .any(|e| matches!(e, GameEvent::TunnelDeclined { .. }));
        let claimed = events
            .iter()
            .any(|e| matches!(e, GameEvent::RouteClaimed { .. }));
        prop_assert!(
            !(declined && claimed),
            "tunnel rifiutato ma rotta reclamata"
        );
        for event in events {
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
        check_station_and_tunnel_state(game, &eng)?;
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
    let finished = FinishedGame {
        final_scores,
        longest_path_players,
        longest_path_bonus,
        route_points,
        turn_ends_after_announcement: turn_ends,
    };
    Ok((eng, finished))
}

/// Una partita completa chiude il giro finale, assegna i punti e ha un vincitore.
fn check_full_game_invariants(
    game: Game,
    seed: u64,
    players: usize,
    choices: &[u16],
) -> Result<(), TestCaseError> {
    let players = clamp_players(game, players);
    let (eng, finished) = play_to_game_over(game, seed, players, choices)?;
    let unbuilt_points = data(game)
        .0
        .stations
        .as_ref()
        .map_or(0, |s| i32::try_from(s.unbuilt_points).unwrap());

    prop_assert_eq!(
        finished.turn_ends_after_announcement,
        players + 1,
        "fine turno dopo LastRoundAnnounced"
    );
    prop_assert_eq!(finished.final_scores.len(), players);
    let best_total = finished
        .final_scores
        .iter()
        .map(|fs| fs.total)
        .max()
        .unwrap();
    prop_assert!(
        finished.final_scores.iter().any(|fs| fs.winner),
        "nessun vincitore"
    );
    let ruleset = &data(game).0;
    let globetrotter_points = ruleset
        .bonuses
        .iter()
        .find(|b| b.bonus_type == GLOBETROTTER_BONUS_TYPE)
        .map(|b| i32::try_from(b.points).unwrap());
    let completed_counts: Vec<usize> = (0..players)
        .map(|p| {
            let tickets = &eng.player(p).unwrap().tickets;
            tickets
                .values()
                .filter(|t| t.completed == Some(true))
                .count()
        })
        .collect();
    let most_completed = completed_counts.iter().copied().max().unwrap_or(0);
    if !ruleset
        .bonuses
        .iter()
        .any(|b| b.bonus_type == LONGEST_PATH_BONUS_TYPE)
    {
        prop_assert!(finished.longest_path_players.is_empty());
    }
    for fs in &finished.final_scores {
        let state = eng.player(fs.player_index).unwrap();
        // L'esito live dei biglietti coincide con quello finale (stesse stazioni e rotte).
        let completed: i32 = state
            .tickets
            .values()
            .filter(|t| t.completed == Some(true))
            .map(|t| i32::try_from(t.points).unwrap())
            .sum();
        let failed: i32 = state
            .tickets
            .values()
            .filter(|t| t.completed != Some(true))
            .map(|t| i32::try_from(t.points).unwrap())
            .sum();
        let bonus = if finished.longest_path_players.contains(&fs.player_index) {
            i32::try_from(finished.longest_path_bonus).unwrap()
        } else {
            0
        };
        let unbuilt = i32::try_from(eng.stations_remaining(fs.player_index)).unwrap();
        prop_assert_eq!(fs.route_points, finished.route_points[fs.player_index]);
        prop_assert_eq!(fs.route_points, state.score);
        prop_assert_eq!(fs.ticket_completed, completed);
        prop_assert_eq!(fs.ticket_failed, -failed);
        prop_assert_eq!(fs.longest_path_bonus, bonus);
        prop_assert_eq!(fs.station_points, unbuilt * unbuilt_points);
        let country: u32 = state.country_cards.values().flatten().sum();
        prop_assert_eq!(fs.country_points, i32::try_from(country).unwrap());
        if ruleset.country_cards.is_empty() {
            prop_assert!(state.country_cards.is_empty());
        }
        let globetrotter = match globetrotter_points {
            Some(points)
                if most_completed > 0 && completed_counts[fs.player_index] == most_completed =>
            {
                points
            }
            _ => 0,
        };
        prop_assert_eq!(fs.globetrotter_bonus, globetrotter);
        prop_assert_eq!(
            fs.total,
            fs.route_points
                + fs.ticket_completed
                + fs.ticket_failed
                + fs.longest_path_bonus
                + fs.globetrotter_bonus
                + fs.station_points
                + fs.country_points
                + fs.passenger_points
        );
        if ruleset.passengers.is_none() {
            prop_assert_eq!(fs.passenger_points, 0);
            prop_assert!(state.passengers.is_empty());
        }
        if fs.winner {
            prop_assert_eq!(
                fs.total,
                best_total,
                "un vincitore non ha il totale massimo"
            );
        }

        // Rotte in prestito: al più una per stazione, avversarie e incidenti alla città.
        let mut borrowing_cities = BTreeSet::new();
        for (city, route_id) in &fs.borrowed_routes {
            prop_assert!(
                state.stations.contains(city),
                "prestito senza stazione a {city}"
            );
            prop_assert!(borrowing_cities.insert(city), "due prestiti per {city}");
            let route = eng.get_route(route_id).unwrap();
            prop_assert!(
                route.from == *city || route.to == *city,
                "{route_id} non tocca {city}"
            );
            let owner = (0..players).find(|p| {
                eng.player(*p)
                    .unwrap()
                    .claimed_routes
                    .contains_key(route_id)
            });
            prop_assert!(
                owner.is_some_and(|p| p != fs.player_index),
                "{route_id} non è di un avversario"
            );
        }
        if state.stations.is_empty() {
            prop_assert!(fs.borrowed_routes.is_empty());
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(Game::Usa, seed, players, &choices)?;
    }

    #[test]
    fn europe_full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(Game::Europe, seed, players, &choices)?;
    }

    #[test]
    fn germany_full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(Game::Germany, seed, players, &choices)?;
    }

    #[test]
    fn poland_full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(Game::Poland, seed, players, &choices)?;
    }

    #[test]
    fn germany_2017_full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(Game::Germany2017, seed, players, &choices)?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, ..ProptestConfig::default() })]

    /// Modalità con setup e bonus diversi dallo standard (biglietti per set, keep 3,
    /// pesca 4, Globetrotter, nessun bonus): stesse invarianti delle mappe base.
    #[test]
    fn modes_keep_engine_invariants(
        game in prop::sample::select(MODE_GAMES.to_vec()),
        (seed, players, choices) in game_inputs(),
    ) {
        check_conservation(game, seed, players, &choices)?;
        check_legal_moves_agree_with_apply(game, seed, players, &choices)?;
        check_replay_reproduces_state(game, seed, players, &choices)?;
    }

    #[test]
    fn modes_full_game_invariants(
        game in prop::sample::select(MODE_GAMES.to_vec()),
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(game, seed, players, &choices)?;
    }
}

/// Gli eventi `FaceUpReset` descrivono davvero le scoperte dopo il reset.
#[test]
fn face_up_reset_events_match_final_face_up() {
    for seed in 0..40u64 {
        let mut eng = new_engine(Game::Usa, seed, 2);
        let mut steps = 0;
        while steps < 300 {
            let Some(action) = pick(
                progress_actions(Game::Usa, &eng),
                u16::try_from(steps).unwrap(),
            ) else {
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
                    always_open: false,
                    tunnel: false,
                    ferries: 0,
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

/// I bot Europe usano davvero stazioni, tunnel (pagati e rifiutati) e traghetti.
#[test]
fn europe_bots_exercise_stations_tunnels_and_ferries() {
    let (mut stations, mut declined, mut paid, mut ferries, mut borrowed) = (0, 0, 0, 0, 0);
    for seed in 0..40u64 {
        let choices: Vec<u16> = (0..200u64)
            .map(|i| (seed.wrapping_mul(7919).wrapping_add(i * 31) % 65_536) as u16)
            .collect();
        let players = 2 + seed as usize % 4;
        let (eng, finished) = play_to_game_over(Game::Europe, seed, players, &choices).unwrap();
        for record in eng.action_log() {
            let claimed = record
                .events
                .iter()
                .any(|e| matches!(e, GameEvent::RouteClaimed { .. }));
            match &record.action {
                Action::BuildStation { .. } => stations += 1,
                Action::TunnelDecision { pay: false, .. } => declined += 1,
                Action::TunnelDecision { pay: true, .. } if claimed => paid += 1,
                Action::ClaimRoute { route_id, .. }
                    if claimed && eng.get_route(route_id).unwrap().ferries > 0 =>
                {
                    ferries += 1;
                }
                _ => {}
            }
        }
        borrowed += finished
            .final_scores
            .iter()
            .map(|fs| fs.borrowed_routes.len())
            .sum::<usize>();
    }
    eprintln!(
        "stazioni {stations}, rinunce {declined}, pagati {paid}, traghetti {ferries}, prestiti {borrowed}"
    );
    assert!(stations > 0 && declined > 0 && paid > 0 && ferries > 0);
}
