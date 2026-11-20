//! Test property-based sul motore: conservazione, azioni fallite,
//! legal-vs-apply e replay, su partite casuali sulle mappe USA, Europe, Germany, Poland,
//! Nordic, Switzerland, India, Legendary Asia e Italy reali (sull'Asia rotte di montagna e
//! Asian Explorer, sull'Italy carte Traghetto e bonus Regioni).
//!
//! Sull'Europe i bot costruiscono anche stazioni e pagano o rifiutano i tunnel; sulla
//! Germany scelgono i biglietti rispettando minimi e massimi per mazzo. I pagamenti delle
//! rotte sono generati da `payment_candidates` leggendo le regole dal ruleset (locomotive
//! ammesse, gruppi di carte qualsiasi): sul Nordic coprono i traghetti da 3 carte e
//! Murmansk–Lieksa da 4 per spazio, sulla Switzerland le locomotive solo sui tunnel e i
//! biglietti multi-destinazione, sull'India il bonus mandala (biglietti con due percorsi
//! senza rotte in comune).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use proptest::prelude::*;
use ttr_core::error::GamePhase;
use ttr_core::types::{
    GLOBETROTTER_BONUS_TYPE, LARGEST_NETWORK_BONUS_TYPE, LONGEST_PATH_BONUS_TYPE, MapData, Route,
    Ruleset, SetupDiscards, TRAIN_COLORS, TicketDef,
};
use ttr_core::{Action, DrawSource, Engine, GameEvent, RouteColor, default_player_colors};
use ttr_proto::CardColor;

/// Carte di un mazzo completo (12 × 8 colori + 14 locomotive, come tutte le mappe testate).
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
    /// Nordic: locomotive solo su tunnel e traghetti, traghetti pagabili con 3 carte qualsiasi
    /// per locomotiva, Murmansk–Lieksa con 4 per spazio, pesca libera delle locomotive
    /// scoperte senza reset del mercato, Globetrotter.
    Nordic,
    /// Switzerland: locomotive solo sui tunnel, pesca libera delle locomotive scoperte con
    /// reset del mercato, biglietti multi-destinazione, percorso più lungo.
    Switzerland,
    /// India: traghetti, binari doppi solo a 4 giocatori, percorso più lungo e bonus mandala.
    India,
    /// Legendary Asia: rotte di montagna (treni scartati, +2 punti ciascuno), traghetti,
    /// bonus Asian Explorer, spareggio per rotte di montagna.
    LegendaryAsia,
    /// Italy: carte Traghetto (pesca, 1 o 2 onde per carta), traghetti grigi, triple, Paesi e
    /// bonus Regioni.
    Italy,
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
static NORDIC: LazyLock<(Ruleset, MapData)> = LazyLock::new(|| load("nordic", "nordic"));
static SWITZERLAND: LazyLock<(Ruleset, MapData)> =
    LazyLock::new(|| load("switzerland", "switzerland"));
static INDIA: LazyLock<(Ruleset, MapData)> = LazyLock::new(|| load("india", "india"));
static LEGENDARY_ASIA: LazyLock<(Ruleset, MapData)> =
    LazyLock::new(|| load("legendary_asia", "legendary_asia"));
static ITALY: LazyLock<(Ruleset, MapData)> = LazyLock::new(|| load("italy", "italy"));

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
        Game::Nordic => &NORDIC,
        Game::Switzerland => &SWITZERLAND,
        Game::India => &INDIA,
        Game::LegendaryAsia => &LEGENDARY_ASIA,
        Game::Italy => &ITALY,
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

/// `len` carte di un colore (più locomotive) dalla mano, se esistono: costo di una stazione.
fn build_payment(hand: &[CardColor], color: RouteColor, len: usize) -> Option<Vec<CardColor>> {
    let count = |c: CardColor| hand.iter().filter(|h| **h == c).count();
    let locos = count(CardColor::Locomotive);
    let colors: Vec<CardColor> = match color {
        RouteColor::Colored(c) => vec![c],
        RouteColor::Gray => TRAIN_COLORS.to_vec(),
    };
    for c in colors {
        let colored = count(c).min(len);
        if locos >= len - colored {
            let mut payment = vec![c; colored];
            payment.resize(len, CardColor::Locomotive);
            return Some(payment);
        }
    }
    (locos >= len).then(|| vec![CardColor::Locomotive; len])
}

/// Regole di pagamento di una rotta, lette dai dati del ruleset e non dal motore.
struct PayRules {
    len: usize,
    /// Spazi locomotiva del traghetto.
    ferries: usize,
    /// Una locomotiva paga da sola uno spazio colore (`locomotives.usableOn`).
    single_loco: bool,
    /// Carte qualsiasi che valgono una locomotiva sugli spazi locomotiva del traghetto.
    loco_group: Option<usize>,
    /// Dimensioni dei gruppi di carte qualsiasi (locomotive comprese) che pagano uno spazio
    /// colore, ordinate e senza ripetizioni.
    color_groups: Vec<usize>,
}

impl PayRules {
    fn of(ruleset: &Ruleset, route: &Route) -> Self {
        let usable_on = ruleset
            .locomotives
            .as_ref()
            .and_then(|l| l.usable_on.as_ref());
        let single_loco = usable_on.is_none_or(|list| {
            let has = |value: &str| list.iter().any(|v| v == value);
            (route.tunnel && has("tunnel")) || (route.ferries > 0 && has("ferry"))
        });
        let loco_group = ruleset
            .ferries
            .as_ref()
            .filter(|_| route.ferries > 0)
            .map(|f| f.any_cards_per_locomotive as usize);
        let mut color_groups: Vec<usize> = loco_group
            .into_iter()
            .chain(route.any_cards_per_space.map(|v| v as usize))
            .collect();
        color_groups.sort_unstable();
        color_groups.dedup();
        Self {
            len: route.len as usize,
            ferries: route.ferries as usize,
            single_loco,
            loco_group,
            color_groups,
        }
    }

    fn has_groups(&self) -> bool {
        self.loco_group.is_some() || !self.color_groups.is_empty()
    }
    /// Le stesse regole con `waves` spazi onda già coperti da carte Traghetto.
    fn without_waves(&self, waves: usize) -> Self {
        Self {
            len: self.len - waves,
            ferries: self.ferries - waves,
            single_loco: self.single_loco,
            loco_group: self.loco_group,
            color_groups: self.color_groups.clone(),
        }
    }
}

/// Tutti i pagamenti strutturati di una rotta con la mano data. Gli spazi locomotiva del
/// traghetto si pagano con una locomotiva o un gruppo; gli spazi colore con una carta del
/// colore, una locomotiva (se ammessa) o un gruppo; i gruppi sono carte qualsiasi della
/// mano non usate come singole. Ordine: prima i pagamenti con più carte singole; i gruppi si
/// riempiono con le carte più rare, le locomotive per ultime (per prime con `loco_filler`).
fn payment_candidates(
    hand: &[CardColor],
    rules: &PayRules,
    color: RouteColor,
    loco_filler: bool,
) -> Vec<Vec<CardColor>> {
    let count = |cards: &[CardColor], c: CardColor| cards.iter().filter(|h| **h == c).count();
    let locos = count(hand, CardColor::Locomotive);
    let colors: Vec<CardColor> = match color {
        RouteColor::Colored(c) => vec![c],
        RouteColor::Gray => TRAIN_COLORS.to_vec(),
    };
    let spaces = rules.len - rules.ferries;
    let sizes: Vec<Option<usize>> = if rules.color_groups.is_empty() {
        vec![None]
    } else {
        rules.color_groups.iter().map(|g| Some(*g)).collect()
    };
    let min_loco_spaces = if rules.loco_group.is_some() {
        0
    } else {
        rules.ferries
    };
    let max_single_locos = if rules.single_loco { spaces } else { 0 };
    let mut out = Vec::new();
    for color in colors {
        let have = count(hand, color);
        for loco_spaces in (min_loco_spaces..=rules.ferries).rev() {
            let loco_groups = rules.ferries - loco_spaces;
            for single_locos in 0..=max_single_locos {
                if loco_spaces + single_locos > locos {
                    break;
                }
                for &size in &sizes {
                    let max_groups = if size.is_some() {
                        spaces - single_locos
                    } else {
                        0
                    };
                    for groups in 0..=max_groups {
                        let singles = spaces - single_locos - groups;
                        let filler = loco_groups * rules.loco_group.unwrap_or(0)
                            + groups * size.unwrap_or(0);
                        let used = singles + loco_spaces + single_locos;
                        if singles > have || hand.len() - used < filler {
                            continue;
                        }
                        let mut cards = vec![color; singles];
                        cards.resize(used, CardColor::Locomotive);
                        let mut rest = hand.to_vec();
                        for card in &cards {
                            let at = rest.iter().position(|h| h == card).unwrap();
                            rest.remove(at);
                        }
                        let snapshot = rest.clone();
                        rest.sort_by_key(|c| match (*c == CardColor::Locomotive, loco_filler) {
                            (true, true) => 0,
                            (true, false) => usize::MAX,
                            (false, _) => 1 + count(&snapshot, *c),
                        });
                        cards.extend(rest.into_iter().take(filler));
                        out.push(cards);
                    }
                }
            }
        }
    }
    out
}

/// Quante carte di ogni colore e locomotive: chiave per confrontare i pagamenti come insiemi.
fn card_counts(cards: &[CardColor]) -> [usize; 9] {
    let mut counts = [0; 9];
    for card in cards {
        let at = TRAIN_COLORS.iter().position(|c| c == card);
        counts[at.unwrap_or(8)] += 1;
    }
    counts
}

fn push_distinct(out: &mut Vec<Vec<CardColor>>, cards: &[CardColor]) {
    if !out.iter().any(|c| card_counts(c) == card_counts(cards)) {
        out.push(cards.to_vec());
    }
}

/// Un pagamento: carte treno (locomotive comprese) e carte Traghetto giocate.
type Payment = (Vec<CardColor>, u32);

/// Pagamenti validi per le regole date con la mano del giocatore (vuoto se non esiste):
/// quello con più carte singole, quello con più carte in tutto e, dove ci sono gruppi, uno
/// con le locomotive nei gruppi.
fn payments_for_rules(
    hand: &[CardColor],
    rules: &PayRules,
    color: RouteColor,
) -> Vec<Vec<CardColor>> {
    let plain = payment_candidates(hand, rules, color, false);
    let mut out = Vec::new();
    if let Some(first) = plain.first() {
        push_distinct(&mut out, first);
    }
    if let Some(longest) = plain.iter().max_by_key(|c| c.len()) {
        push_distinct(&mut out, longest);
    }
    if rules.has_groups() {
        let with_locos = payment_candidates(hand, rules, color, true);
        if let Some(longest) = with_locos.iter().max_by_key(|c| c.len()) {
            push_distinct(&mut out, longest);
        }
    }
    out
}

/// Pagamenti validi per la rotta con la mano e le carte Traghetto del giocatore (vuoto se non
/// ne esiste): prima quelli senza carte Traghetto, poi per ogni numero di carte Traghetto
/// giocabili e ogni numero di onde che coprono (da una per carta a `wavesPerCard`).
fn route_payments(game: Game, eng: &Engine, player: usize, route_id: &str) -> Vec<Payment> {
    let (ruleset, _) = data(game);
    let (Some(route), Some(hand)) = (eng.get_route(route_id), eng.player_hand(player)) else {
        return Vec::new();
    };
    let Some(color) = RouteColor::parse(&route.color) else {
        return Vec::new();
    };
    let rules = PayRules::of(ruleset, route);
    let owned = eng.player(player).map_or(0, |p| p.ferry_cards as usize);
    let waves_per_card = ruleset
        .ferry_cards
        .as_ref()
        .map_or(0, |c| c.waves_per_card as usize);
    let mut out: Vec<Payment> = Vec::new();
    for played in 0..=owned.min(rules.ferries) {
        let covered = if played == 0 {
            0..=0
        } else {
            played..=(played * waves_per_card).min(rules.ferries)
        };
        let played = u32::try_from(played).unwrap();
        for waves in covered {
            for cards in payments_for_rules(hand, &rules.without_waves(waves), color) {
                let counts = card_counts(&cards);
                if !out
                    .iter()
                    .any(|(c, f)| *f == played && card_counts(c) == counts)
                {
                    out.push((cards, played));
                }
            }
        }
    }
    out
}

fn route_payment(game: Game, eng: &Engine, player: usize, route_id: &str) -> Option<Payment> {
    route_payments(game, eng, player, route_id)
        .into_iter()
        .next()
}

/// Pagamenti che il ruleset deve rifiutare, derivati da uno valido su una rotta senza
/// gruppi: una carta in meno e, dove una locomotiva sola non è ammessa, una locomotiva al
/// posto di una carta colorata (solo se la mano ne ha una libera).
fn invalid_payments(
    rules: &PayRules,
    hand: &[CardColor],
    valid: &[CardColor],
) -> Vec<Vec<CardColor>> {
    if rules.has_groups() {
        return Vec::new();
    }
    let mut short = valid.to_vec();
    short.pop();
    let mut out = vec![short];
    let locos = |cards: &[CardColor]| {
        cards
            .iter()
            .filter(|c| **c == CardColor::Locomotive)
            .count()
    };
    if !rules.single_loco && rules.ferries == 0 && locos(hand) > locos(valid) {
        let mut swapped = valid.to_vec();
        if let Some(at) = swapped.iter().position(|c| *c != CardColor::Locomotive) {
            swapped[at] = CardColor::Locomotive;
            out.push(swapped);
        }
    }
    out
}

/// Biglietto della mappa per id.
fn ticket_def(game: Game, id: &str) -> &'static TicketDef {
    data(game).1.tickets.iter().find(|t| t.id == id).unwrap()
}

/// Destinazioni di `def` collegate dalle rotte reclamate dal giocatore, senza stazioni. Un
/// Paese è un vicolo cieco: ogni rotta che lo tocca ha un suo nodo privato.
fn connected_destinations(
    game: Game,
    eng: &Engine,
    player: usize,
    def: &TicketDef,
) -> Vec<(String, u32)> {
    let (_, map) = data(game);
    let is_country = |city: &str| map.cities.iter().any(|c| c.id == city && c.country);
    let node = |city: &str, route_id: &str| {
        if is_country(city) {
            format!("{city}#{route_id}")
        } else {
            city.to_string()
        }
    };
    let mut adjacent: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for route_id in eng.player(player).unwrap().claimed_routes.keys() {
        let route = eng.get_route(route_id).unwrap();
        let (a, b) = (node(&route.from, route_id), node(&route.to, route_id));
        adjacent.entry(a.clone()).or_default().push(b.clone());
        adjacent.entry(b).or_default().push(a);
    }
    let nodes_of = |city: &str| -> Vec<String> {
        if is_country(city) {
            let prefix = format!("{city}#");
            adjacent
                .keys()
                .filter(|n| n.starts_with(&prefix))
                .cloned()
                .collect()
        } else {
            vec![city.to_string()]
        }
    };
    let mut reached: BTreeSet<String> = nodes_of(&def.from).into_iter().collect();
    let mut frontier: Vec<String> = reached.iter().cloned().collect();
    while let Some(at) = frontier.pop() {
        for next in adjacent.get(&at).into_iter().flatten() {
            if reached.insert(next.clone()) {
                frontier.push(next.clone());
            }
        }
    }
    def.destinations()
        .filter(|(to, _)| nodes_of(to).iter().any(|n| reached.contains(n)))
        .map(|(to, points)| (to.to_string(), points))
        .collect()
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
    build_payment(eng.player_hand(player)?, RouteColor::Gray, cost)
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
/// limiti (minimo e massimo per gruppo) di `ticket_choice` di `legal_moves`: prima si
/// prendono i minimi dei gruppi, poi si riempie fino a `want` nell'ordine di offerta.
fn keep_within_limits(eng: &Engine, pi: usize, offered: &[String], want: usize) -> Vec<String> {
    let limits = eng
        .legal_moves(pi)
        .ticket_choice
        .map(|c| c.limits)
        .unwrap_or_default();
    let mut chosen: BTreeSet<&String> = BTreeSet::new();
    let mut left: Vec<u32> = limits.iter().map(|l| l.max).collect();
    for (g, limit) in limits.iter().enumerate() {
        for id in limit.ticket_ids.iter().take(limit.min as usize) {
            chosen.insert(id);
            left[g] -= 1;
        }
    }
    for id in offered {
        if chosen.len() >= want {
            break;
        }
        if chosen.contains(id) {
            continue;
        }
        let group = limits.iter().position(|l| l.ticket_ids.contains(id));
        if let Some(g) = group {
            if left[g] == 0 {
                continue;
            }
            left[g] -= 1;
        }
        chosen.insert(id);
    }
    offered
        .iter()
        .filter(|id| chosen.contains(id))
        .cloned()
        .collect()
}

/// Pescata di biglietti valida per il giocatore, se è ammessa.
fn draw_ticket_actions(player_index: usize, can_draw: bool) -> Vec<Action> {
    if can_draw {
        vec![Action::DrawTickets { player_index }]
    } else {
        Vec::new()
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
                let payments = route_payments(game, eng, pi, route_id);
                assert!(!payments.is_empty(), "rotta legale non pagabile");
                for (cards, ferry_cards) in payments {
                    actions.push(Action::ClaimRoute {
                        player_index: pi,
                        route_id: route_id.clone(),
                        cards_discarded: cards,
                        passengers: passenger_picks(eng, route_id),
                        ferry_cards,
                    });
                }
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
            actions.extend(draw_ticket_actions(pi, legal.can_draw_tickets));
            if legal.can_draw_ferry_card {
                actions.push(Action::DrawFerryCard { player_index: pi });
            }
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
    if data(game).0.ferry_cards.is_some() && picks[1] % 13 == 12 {
        return Action::DrawFerryCard { player_index };
    }
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
        3 => {
            let route_index = usize::from(picks[2]) % (map.routes.len() + 1);
            let route = map.routes.get(route_index);
            let route_id = route.map_or("rotta_inesistente".to_string(), |r| r.id.clone());
            let len = route.map_or(1, |r| r.len as usize);
            let hand = eng.player_hand(player_index).unwrap_or_default();
            let (mut cards, ferry_cards): (Vec<CardColor>, u32) = if picks[3].is_multiple_of(2) {
                route_payment(game, eng, player_index, &route_id).unwrap_or_default()
            } else {
                let cards = hand
                    .iter()
                    .cycle()
                    .skip(usize::from(picks[3]) % hand.len().max(1))
                    .take(len)
                    .copied()
                    .collect();
                (cards, 0)
            };
            if picks[3].is_multiple_of(5) {
                cards.pop();
            }
            Action::ClaimRoute {
                player_index,
                route_id,
                cards_discarded: cards,
                passengers: Default::default(),
                ferry_cards: ferry_cards + u32::from(picks[3].is_multiple_of(7)),
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

        // Le carte Traghetto: mai più di `maxInHand` in mano, mazzetto = carte − in mano;
        // senza `ferryCards` nessuno ne ha.
        let held: u32 = (0..eng.num_players())
            .map(|p| eng.player(p).unwrap().ferry_cards)
            .sum();
        match &ruleset.ferry_cards {
            Some(config) => {
                for p in 0..eng.num_players() {
                    prop_assert!(
                        eng.player(p).unwrap().ferry_cards <= config.max_in_hand,
                        "troppe carte Traghetto"
                    );
                }
                prop_assert_eq!(eng.ferry_pile_size(), config.count - held);
            }
            None => {
                prop_assert_eq!(held, 0);
                prop_assert_eq!(eng.ferry_pile_size(), 0);
            }
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
    let (_, map) = data(game);
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
            // Il draft iniziale è simultaneo: la scelta spetta a chi non l'ha ancora fatta;
            // i biglietti sono già offerti, nessuna pesca.
            let drafting = eng.phase() == GamePhase::InitialDraft
                && !eng.player(other).unwrap().initial_draft_done;
            let has_offers = !eng.initial_tickets_for(other).unwrap().is_empty();
            prop_assert_eq!(
                lm.ticket_choice.is_some(),
                drafting && has_offers,
                "ticket_choice di {}",
                other
            );
            prop_assert!(!lm.can_draw_tickets, "pesca biglietti di {}", other);
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
            let mut payments = route_payments(game, eng, pi, &route.id);
            if payments.is_empty() {
                prop_assert!(!listed, "rotta {} elencata ma non pagabile", route.id);
                continue;
            }
            let rules = PayRules::of(&data(game).0, route);
            let hand = eng.player_hand(pi).unwrap().to_vec();
            // Le varianti rifiutate si derivano da un pagamento senza carte Traghetto.
            let mut invalid: Vec<Payment> = if payments[0].1 == 0 {
                invalid_payments(&rules, &hand, &payments[0].0)
                    .into_iter()
                    .map(|cards| (cards, 0))
                    .collect()
            } else {
                Vec::new()
            };
            // Più carte Traghetto delle onde della rotta (nessuna su una rotta senza onde):
            // mai ammesse, anche se la mano le avesse.
            if data(game).0.ferry_cards.is_some() {
                let too_many = u32::try_from(rules.ferries).unwrap() + 1;
                invalid.push((payments[0].0.clone(), too_many));
            }
            // Le mosse elencate si provano su una copia: al più 3 pagamenti a passo, a
            // rotazione tra le varianti della rotta.
            if listed {
                let shift = usize::from(window[1]) % payments.len();
                payments.rotate_left(shift);
            }
            for (cards, ferry_cards) in payments {
                if listed {
                    if listed_checked == 3 {
                        break;
                    }
                    listed_checked += 1;
                }
                let action = Action::ClaimRoute {
                    player_index: pi,
                    route_id: route.id.clone(),
                    cards_discarded: cards,
                    passengers: passenger_picks(eng, &route.id),
                    ferry_cards,
                };
                check(eng, action, listed, &format!("rotta {}", route.id))?;
            }
            for (cards, ferry_cards) in invalid {
                let action = Action::ClaimRoute {
                    player_index: pi,
                    route_id: route.id.clone(),
                    cards_discarded: cards,
                    passengers: passenger_picks(eng, &route.id),
                    ferry_cards,
                };
                check(
                    eng,
                    action,
                    false,
                    &format!("pagamento vietato {}", route.id),
                )?;
            }
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
        let tickets = Action::DrawTickets { player_index: pi };
        check(eng, tickets, lm.can_draw_tickets, "pesca biglietti")?;
        let ferry_draw = Action::DrawFerryCard { player_index: pi };
        check(
            eng,
            ferry_draw,
            lm.can_draw_ferry_card,
            "pesca carta Traghetto",
        )?;

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
            let placed: u32 = state
                .claimed_routes
                .iter()
                .map(|(id, r)| r.train_count + eng.get_route(id).unwrap().mountains)
                .sum();
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

/// Città della rete più grande di rotte proprie del giocatore, da una visita in ampiezza
/// indipendente dal motore: i Paesi sono nodi privati per rotta e contano una volta per
/// componente. 0 se il giocatore non ha rotte.
fn naive_network_size(game: Game, eng: &Engine, player: usize) -> u32 {
    let countries: BTreeSet<&str> = data(game)
        .1
        .cities
        .iter()
        .filter(|c| c.country)
        .map(|c| c.id.as_str())
        .collect();
    let node = |route: &Route, city: &str| {
        if countries.contains(city) {
            format!("{city}#{}", route.id)
        } else {
            city.to_string()
        }
    };
    let routes: Vec<&Route> = eng
        .player(player)
        .unwrap()
        .claimed_routes
        .keys()
        .map(|id| eng.get_route(id).unwrap())
        .collect();
    let mut adjacent: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for route in &routes {
        let (a, b) = (node(route, &route.from), node(route, &route.to));
        adjacent.entry(a.clone()).or_default().push(b.clone());
        adjacent.entry(b).or_default().push(a);
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut best = 0;
    for start in adjacent.keys() {
        if seen.contains(start) {
            continue;
        }
        let mut cities: BTreeSet<String> = BTreeSet::new();
        let mut stack = vec![start.clone()];
        seen.insert(start.clone());
        while let Some(at) = stack.pop() {
            cities.insert(at.split('#').next().unwrap().to_string());
            for next in &adjacent[&at] {
                if seen.insert(next.clone()) {
                    stack.push(next.clone());
                }
            }
        }
        best = best.max(u32::try_from(cities.len()).unwrap());
    }
    best
}

/// Reti di rotte proprie del giocatore (Regioni per rete, in ordine decrescente) e punti del
/// bonus `regions`, da una visita in ampiezza indipendente dal motore: i Paesi sono nodi
/// privati per rotta e non hanno una Regione. Vuoto e 0 senza il bonus.
fn naive_regions(game: Game, eng: &Engine, player: usize) -> (Vec<u32>, i32) {
    let (ruleset, map) = data(game);
    let Some(bonus) = ruleset.regions_bonus() else {
        return (Vec::new(), 0);
    };
    let region_of = |city: &str| {
        map.cities
            .iter()
            .find(|c| c.id == city)
            .and_then(|c| c.region.as_deref())
    };
    let is_country = |city: &str| map.cities.iter().any(|c| c.id == city && c.country);
    let node = |route: &Route, city: &str| {
        if is_country(city) {
            format!("{city}#{}", route.id)
        } else {
            city.to_string()
        }
    };
    let mut adjacent: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in eng.player(player).unwrap().claimed_routes.keys() {
        let route = eng.get_route(id).unwrap();
        let (a, b) = (node(route, &route.from), node(route, &route.to));
        adjacent.entry(a.clone()).or_default().push(b.clone());
        adjacent.entry(b).or_default().push(a);
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut counts: Vec<u32> = Vec::new();
    for start in adjacent.keys() {
        if !seen.insert(start.clone()) {
            continue;
        }
        let mut cities: BTreeSet<String> = BTreeSet::new();
        let mut stack = vec![start.clone()];
        while let Some(at) = stack.pop() {
            cities.insert(at.split('#').next().unwrap().to_string());
            for next in &adjacent[&at] {
                if seen.insert(next.clone()) {
                    stack.push(next.clone());
                }
            }
        }
        let regions: BTreeSet<&str> = cities.iter().filter_map(|c| region_of(c)).collect();
        let complete = bonus
            .double_regions
            .iter()
            .filter(|region| {
                map.cities
                    .iter()
                    .filter(|c| c.region.as_deref() == Some(region.as_str()))
                    .all(|c| cities.contains(&c.id))
            })
            .count();
        counts.push(u32::try_from(regions.len() + complete).unwrap());
    }
    counts.sort_unstable_by(|a, b| b.cmp(a));
    let min = bonus.min_regions.unwrap();
    let points: u32 = counts
        .iter()
        .filter(|&&k| k >= min)
        .map(|&k| bonus.table[((k - min) as usize).min(bonus.table.len() - 1)])
        .sum();
    (counts, i32::try_from(points).unwrap())
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
        // Biglietti classici: punti fissi dallo stato. Multi-destinazione (Switzerland): il
        // massimo tra le destinazioni collegate dalle rotte del giocatore, altrimenti −minimo.
        let results = eng.final_ticket_results(fs.player_index);
        prop_assert_eq!(results.len(), state.tickets.len());
        let (mut completed, mut failed) = (0i32, 0i32);
        for result in results {
            let status = &state.tickets[&result.ticket_id];
            prop_assert_eq!(status.completed == Some(true), result.completed);
            let def = ticket_def(game, &result.ticket_id);
            if def.destinations.is_none() {
                let points = i32::try_from(status.points).unwrap();
                prop_assert_eq!(
                    result.points,
                    if result.completed { points } else { -points }
                );
            } else {
                prop_assert!(ruleset.stations.is_none(), "stazioni e biglietti multipli");
                let connected = connected_destinations(game, &eng, fs.player_index, def);
                let best = connected.iter().map(|(_, p)| *p).max();
                prop_assert_eq!(result.completed, best.is_some(), "{}", def.id);
                if let Some(best) = best {
                    prop_assert_eq!(result.points, i32::try_from(best).unwrap(), "{}", def.id);
                    let to = result.destination.as_deref();
                    prop_assert!(
                        connected
                            .iter()
                            .any(|(d, p)| Some(d.as_str()) == to && *p == best),
                        "{}: destinazione realizzata {:?}",
                        def.id,
                        to
                    );
                } else {
                    let lowest = def.destinations().map(|(_, p)| p).min().unwrap();
                    prop_assert_eq!(result.points, -i32::try_from(lowest).unwrap(), "{}", def.id);
                    prop_assert!(result.destination.is_none());
                }
            }
            if result.completed {
                completed += result.points;
            } else {
                failed += result.points;
            }
        }
        let bonus = if finished.longest_path_players.contains(&fs.player_index) {
            i32::try_from(finished.longest_path_bonus).unwrap()
        } else {
            0
        };
        let unbuilt = i32::try_from(eng.stations_remaining(fs.player_index)).unwrap();
        prop_assert_eq!(
            fs.route_points + fs.mountain_points,
            finished.route_points[fs.player_index]
        );
        prop_assert_eq!(fs.ticket_completed, completed);
        prop_assert_eq!(fs.ticket_failed, failed);
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
        // Mandala: i biglietti marcati sono completati; il bonus è `table[min(k, len) - 1]`
        // con k i biglietti marcati, 0 senza il bonus nel ruleset (e allora nessun marcato).
        let marked = results.iter().filter(|r| r.mandala).count();
        prop_assert!(
            results.iter().all(|r| !r.mandala || r.completed),
            "biglietto mandala non completato"
        );
        let mandala = match ruleset.mandala_table() {
            Some(table) if marked > 0 => table[marked.min(table.len()) - 1],
            _ => 0,
        };
        if ruleset.mandala_table().is_none() {
            prop_assert_eq!(marked, 0);
        }
        prop_assert_eq!(fs.mandala_bonus, i32::try_from(mandala).unwrap());
        // Rotte di montagna: i punti dei treni scartati sono a parte nel punteggio finale
        // ma dentro il punteggio corrente e nei punti degli eventi `RouteClaimed`.
        let per_train = ruleset.mountains.as_ref().map_or(0, |m| m.points_per_train);
        let symbols: u32 = state
            .claimed_routes
            .keys()
            .map(|id| eng.get_route(id).unwrap().mountains)
            .sum();
        prop_assert_eq!(
            fs.mountain_points,
            i32::try_from(per_train * symbols).unwrap()
        );
        prop_assert_eq!(fs.mountain_points, state.mountain_points);
        prop_assert_eq!(fs.route_points + fs.mountain_points, state.score);
        // Asian Explorer: stessa dimensione di una visita in ampiezza indipendente; il
        // bonus va a tutti i pari merito col massimo, solo se > 0.
        let explorer = ruleset
            .bonuses
            .iter()
            .find(|b| b.bonus_type == LARGEST_NETWORK_BONUS_TYPE)
            .map(|b| i32::try_from(b.points).unwrap());
        let size = if explorer.is_some() {
            naive_network_size(game, &eng, fs.player_index)
        } else {
            0
        };
        prop_assert_eq!(fs.largest_network, size);
        let best_size = (0..players)
            .map(|p| naive_network_size(game, &eng, p))
            .max()
            .unwrap_or(0);
        let explorer_bonus = match explorer {
            Some(points) if size > 0 && size == best_size => points,
            _ => 0,
        };
        prop_assert_eq!(fs.largest_network_bonus, explorer_bonus);
        // Regioni: reti e punti da una visita indipendente; vuoti senza il bonus.
        let (networks, region_points) = naive_regions(game, &eng, fs.player_index);
        prop_assert_eq!(&fs.region_networks, &networks);
        prop_assert_eq!(fs.regions_bonus, region_points);
        prop_assert_eq!(eng.region_standing(fs.player_index).bonus, region_points);
        prop_assert_eq!(eng.region_standing(fs.player_index).networks, networks);
        prop_assert_eq!(
            fs.total,
            fs.route_points
                + fs.mountain_points
                + fs.ticket_completed
                + fs.ticket_failed
                + fs.longest_path_bonus
                + fs.globetrotter_bonus
                + fs.mandala_bonus
                + fs.largest_network_bonus
                + fs.regions_bonus
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

    #[test]
    fn nordic_full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(Game::Nordic, seed, players, &choices)?;
    }

    #[test]
    fn switzerland_full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(Game::Switzerland, seed, players, &choices)?;
    }

    #[test]
    fn india_full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=4,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(Game::India, seed, players, &choices)?;
    }

    #[test]
    fn legendary_asia_full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(Game::LegendaryAsia, seed, players, &choices)?;
    }

    #[test]
    fn italy_full_game_invariants(
        seed in any::<u64>(),
        players in 2usize..=5,
        choices in prop::collection::vec(any::<u16>(), 64..256),
    ) {
        check_full_game_invariants(Game::Italy, seed, players, &choices)?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    #[test]
    fn nordic_cards_and_tickets_are_conserved((seed, players, choices) in game_inputs()) {
        check_conservation(Game::Nordic, seed, players, &choices)?;
    }

    #[test]
    fn nordic_failed_action_leaves_state_identical((seed, players, choices) in game_inputs()) {
        check_failed_action_leaves_state_identical(Game::Nordic, seed, players, &choices)?;
    }

    #[test]
    fn nordic_legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        check_legal_moves_agree_with_apply(Game::Nordic, seed, players, &choices)?;
    }

    #[test]
    fn nordic_replay_reproduces_state((seed, players, choices) in game_inputs()) {
        check_replay_reproduces_state(Game::Nordic, seed, players, &choices)?;
    }

    #[test]
    fn switzerland_cards_and_tickets_are_conserved((seed, players, choices) in game_inputs()) {
        check_conservation(Game::Switzerland, seed, players, &choices)?;
    }

    #[test]
    fn switzerland_failed_action_leaves_state_identical(
        (seed, players, choices) in game_inputs()
    ) {
        check_failed_action_leaves_state_identical(Game::Switzerland, seed, players, &choices)?;
    }

    #[test]
    fn switzerland_legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        check_legal_moves_agree_with_apply(Game::Switzerland, seed, players, &choices)?;
    }

    #[test]
    fn switzerland_replay_reproduces_state((seed, players, choices) in game_inputs()) {
        check_replay_reproduces_state(Game::Switzerland, seed, players, &choices)?;
    }

    #[test]
    fn india_cards_and_tickets_are_conserved((seed, players, choices) in game_inputs()) {
        check_conservation(Game::India, seed, players, &choices)?;
    }

    #[test]
    fn india_failed_action_leaves_state_identical((seed, players, choices) in game_inputs()) {
        check_failed_action_leaves_state_identical(Game::India, seed, players, &choices)?;
    }

    #[test]
    fn india_legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        check_legal_moves_agree_with_apply(Game::India, seed, players, &choices)?;
    }

    #[test]
    fn india_replay_reproduces_state((seed, players, choices) in game_inputs()) {
        check_replay_reproduces_state(Game::India, seed, players, &choices)?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    #[test]
    fn legendary_asia_cards_and_tickets_are_conserved(
        (seed, players, choices) in game_inputs()
    ) {
        check_conservation(Game::LegendaryAsia, seed, players, &choices)?;
    }

    #[test]
    fn legendary_asia_failed_action_leaves_state_identical(
        (seed, players, choices) in game_inputs()
    ) {
        check_failed_action_leaves_state_identical(Game::LegendaryAsia, seed, players, &choices)?;
    }

    #[test]
    fn legendary_asia_legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        check_legal_moves_agree_with_apply(Game::LegendaryAsia, seed, players, &choices)?;
    }

    #[test]
    fn legendary_asia_replay_reproduces_state((seed, players, choices) in game_inputs()) {
        check_replay_reproduces_state(Game::LegendaryAsia, seed, players, &choices)?;
    }

    #[test]
    fn italy_cards_and_tickets_are_conserved((seed, players, choices) in game_inputs()) {
        check_conservation(Game::Italy, seed, players, &choices)?;
    }

    #[test]
    fn italy_failed_action_leaves_state_identical((seed, players, choices) in game_inputs()) {
        check_failed_action_leaves_state_identical(Game::Italy, seed, players, &choices)?;
    }

    #[test]
    fn italy_legal_moves_agree_with_apply((seed, players, choices) in game_inputs()) {
        check_legal_moves_agree_with_apply(Game::Italy, seed, players, &choices)?;
    }

    #[test]
    fn italy_replay_reproduces_state((seed, players, choices) in game_inputs()) {
        check_replay_reproduces_state(Game::Italy, seed, players, &choices)?;
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
                    any_cards_per_space: None,
                    mountains: 0,
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

fn seeded_choices(seed: u64) -> Vec<u16> {
    (0..200u64)
        .map(|i| (seed.wrapping_mul(7919).wrapping_add(i * 31) % 65_536) as u16)
        .collect()
}

/// I bot Nordic pagano i traghetti con gruppi di carte qualsiasi; i bot Switzerland tengono,
/// completano e falliscono biglietti multi-destinazione (esito verificato sulle rotte da
/// `check_full_game_invariants`).
#[test]
fn bots_exercise_card_groups_and_multi_destination_tickets() {
    let mut ferry_groups = 0;
    for seed in 0..40u64 {
        let players = 2 + seed as usize % 2;
        let (eng, _) =
            play_to_game_over(Game::Nordic, seed, players, &seeded_choices(seed)).unwrap();
        for record in eng.action_log() {
            let claimed = record
                .events
                .iter()
                .any(|e| matches!(e, GameEvent::RouteClaimed { .. }));
            if let Action::ClaimRoute {
                route_id,
                cards_discarded,
                ..
            } = &record.action
            {
                let route = eng.get_route(route_id).unwrap();
                if claimed && route.ferries > 0 && cards_discarded.len() > route.len as usize {
                    ferry_groups += 1;
                }
            }
        }
    }
    let (mut held, mut completed, mut failed) = (0, 0, 0);
    for seed in 0..40u64 {
        let players = 2 + seed as usize % 2;
        let (eng, _) =
            play_to_game_over(Game::Switzerland, seed, players, &seeded_choices(seed)).unwrap();
        for p in 0..players {
            for result in eng.final_ticket_results(p) {
                if ticket_def(Game::Switzerland, &result.ticket_id)
                    .destinations
                    .is_some()
                {
                    held += 1;
                    if result.completed {
                        completed += 1;
                    } else {
                        failed += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "traghetti con gruppi {ferry_groups}, multi-destinazione tenuti {held} (completati {completed}, falliti {failed})"
    );
    assert!(ferry_groups > 0, "nessun traghetto pagato con gruppi");
    assert!(
        completed > 0 && failed > 0,
        "biglietti multi-destinazione non esercitati"
    );
}

/// Cammino più corto (per treni) da `from` a `to` senza le rotte in `banned`: ripete il
/// rilassamento di Bellman–Ford sulle rotte non orientate; restituisce gli indici delle rotte.
fn shortest_path(
    routes: &[&Route],
    banned: &BTreeSet<usize>,
    from: &str,
    to: &str,
) -> Option<Vec<usize>> {
    let mut best: BTreeMap<&str, (u32, Vec<usize>)> = BTreeMap::new();
    best.insert(from, (0, Vec::new()));
    for _ in 0..routes.len() {
        for (i, route) in routes
            .iter()
            .enumerate()
            .filter(|(i, _)| !banned.contains(i))
        {
            for (a, b) in [(&route.from, &route.to), (&route.to, &route.from)] {
                let Some((cost, path)) = best.get(a.as_str()).cloned() else {
                    continue;
                };
                let cost = cost + route.len;
                if best.get(b.as_str()).is_none_or(|(c, _)| cost < *c) {
                    let mut path = path;
                    path.push(i);
                    best.insert(b, (cost, path));
                }
            }
        }
    }
    best.remove(to).map(|(_, path)| path)
}

/// Su India reale, con un biglietto collegato da due percorsi senza rotte in comune (il
/// primo trovato dal più corto, poi dal più corto nelle rotte rimaste) il giocatore prende
/// il bonus mandala della prima voce della tabella, e solo quel biglietto è marcato. I bot
/// casuali non lo raggiungono mai: senza questo test l'invariante sarebbe vuota.
#[test]
fn india_two_disjoint_paths_earn_the_mandala_bonus() {
    let (ruleset, map) = data(Game::India);
    // Un giocatore non prende mai due binari dello stesso gruppo: solo il primo di ognuno.
    let routes: Vec<&Route> = map
        .routes
        .iter()
        .filter(|r| !r.double || r.id.ends_with("_1"))
        .collect();
    let none = BTreeSet::new();
    let (ticket, picked) = map
        .tickets
        .iter()
        .filter(|t| t.to.is_some())
        .filter_map(|t| {
            let to = t.to.as_deref()?;
            let first = shortest_path(&routes, &none, &t.from, to)?;
            let banned: BTreeSet<usize> = first.iter().copied().collect();
            let second = shortest_path(&routes, &banned, &t.from, to)?;
            let picked: Vec<usize> = first.into_iter().chain(second).collect();
            let trains: u32 = picked.iter().map(|&i| routes[i].len).sum();
            (trains <= 45).then_some((t, picked))
        })
        .min_by_key(|(_, picked)| picked.iter().map(|&i| routes[i].len).sum::<u32>())
        .expect("nessun biglietto India con due percorsi disgiunti entro 45 treni");

    let mut eng = new_engine(Game::India, 7, 2);
    eng.set_player_tickets(0, &[ticket.id.as_str()]);
    for &i in &picked {
        let route = routes[i];
        let cards = vec![CardColor::Locomotive; route.len as usize];
        eng.set_phase_for_test(GamePhase::PlayerTurn, 0);
        eng.set_player_hand(0, cards.clone());
        eng.apply(Action::ClaimRoute {
            player_index: 0,
            route_id: route.id.clone(),
            cards_discarded: cards,
            passengers: Default::default(),
            ferry_cards: 0,
        })
        .unwrap();
    }
    let current = eng.current_player();
    eng.set_phase_for_test(GamePhase::PlayerTurn, current);
    eng.set_player_trains(current, 2);
    let mut final_scores = None;
    for _ in 0..200 {
        let pi = eng.current_player();
        let events = eng
            .apply(Action::DrawTrainCard {
                player_index: pi,
                source: DrawSource::Deck,
            })
            .unwrap();
        for event in events {
            if let GameEvent::GameOver {
                final_scores: f, ..
            } = event
            {
                final_scores = Some(f);
            }
        }
        if final_scores.is_some() {
            break;
        }
    }
    let final_scores = final_scores.expect("la partita non è terminata");
    let marked: Vec<&str> = eng
        .final_ticket_results(0)
        .iter()
        .filter(|r| r.mandala)
        .map(|r| r.ticket_id.as_str())
        .collect();
    assert_eq!(marked, [ticket.id.as_str()]);
    let first = ruleset.mandala_table().unwrap()[0];
    assert_eq!(final_scores[0].mandala_bonus, i32::try_from(first).unwrap());
    assert_eq!(final_scores[1].mandala_bonus, 0);
}

/// Murmansk–Lieksa si paga con 7 carte verdi più 8 qualsiasi: il generatore le produce e il
/// motore le accetta; con le sole locomotive in mano la rotta non è pagabile.
#[test]
fn nordic_long_route_is_paid_with_groups_of_any_cards() {
    let mut eng = new_engine(Game::Nordic, 3, 2);
    while eng.phase() == GamePhase::InitialDraft {
        let action = progress_actions(Game::Nordic, &eng).remove(0);
        eng.apply(action).unwrap();
    }
    let pi = eng.current_player();
    let route_id = "lieksa_murmansk_1";
    let mut hand = vec![CardColor::Green; 7];
    hand.extend([CardColor::Red; 4]);
    hand.extend([CardColor::Blue; 3]);
    hand.push(CardColor::Locomotive);
    eng.set_player_hand(pi, hand);
    assert!(eng.legal_moves(pi).routes.contains(&route_id.to_string()));
    let payments = route_payments(Game::Nordic, &eng, pi, route_id);
    assert!(
        payments.iter().all(|(p, f)| p.len() == 15 && *f == 0),
        "{payments:?}"
    );
    for (cards, _) in payments {
        let action = Action::ClaimRoute {
            player_index: pi,
            route_id: route_id.into(),
            cards_discarded: cards,
            passengers: BTreeMap::new(),
            ferry_cards: 0,
        };
        eng.clone().apply(action).unwrap();
    }

    eng.set_player_hand(pi, vec![CardColor::Locomotive; 9]);
    assert!(route_payments(Game::Nordic, &eng, pi, route_id).is_empty());
    assert!(!eng.legal_moves(pi).routes.contains(&route_id.to_string()));
}

/// I bot Italy pescano carte Traghetto e le giocano sulle onde (anche una carta su più onde);
/// il bonus Regioni di fine partita coincide con quello di una visita indipendente (verificato
/// da `check_full_game_invariants`).
#[test]
fn italy_bots_draw_and_play_ferry_cards() {
    let (mut draws, mut played, mut multi_wave, mut with_bonus) = (0, 0, 0, 0);
    for seed in 0..40u64 {
        let players = 2 + seed as usize % 4;
        let (eng, finished) =
            play_to_game_over(Game::Italy, seed, players, &seeded_choices(seed)).unwrap();
        for record in eng.action_log() {
            match &record.action {
                Action::DrawFerryCard { .. } => draws += 1,
                Action::ClaimRoute {
                    route_id,
                    cards_discarded,
                    ferry_cards,
                    ..
                } if *ferry_cards > 0 => {
                    played += 1;
                    let len = eng.get_route(route_id).unwrap().len as usize;
                    if cards_discarded.len() + *ferry_cards as usize != len {
                        multi_wave += 1;
                    }
                }
                _ => {}
            }
        }
        if finished.final_scores.iter().any(|fs| fs.regions_bonus > 0) {
            with_bonus += 1;
        }
    }
    eprintln!(
        "carte Traghetto pescate {draws}, giocate {played}, su più onde {multi_wave}, partite con bonus Regioni {with_bonus}"
    );
    assert!(draws > 0 && played > 0);
}
