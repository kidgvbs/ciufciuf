//! Controlli di coerenza tra ruleset, mappa e giocatori.
//!
//! Gli stessi controlli girano in `Engine::new`, in `ttr-validate` e
//! all'avvio del server, così dati incoerenti non producono partite bloccate.

use std::collections::{BTreeMap, BTreeSet};

use crate::density::{RouteSpan, max_cyclomatic_number};
use crate::error::EngineInitError;
use crate::scoring::MAX_CYCLOMATIC_NUMBER;
use crate::types::{
    Bonus, COLOR_COUNT, FACE_UP_COUNT, GLOBETROTTER_BONUS_TYPE, INITIAL_HAND_SIZE,
    LARGEST_NETWORK_BONUS_TYPE, LOCO_RESET_THRESHOLD, LONGEST_PATH_BONUS_TYPE, MANDALA_BONUS_TYPE,
    MAX_PARALLEL_LANES, MapData, REGIONS_BONUS_TYPE, Route, RouteColor, Ruleset, SetupDiscards,
    TicketDef, TieBreaker, route_lane,
};

/// Un biglietto ha `to` + `points` (una destinazione) oppure `destinations` (almeno 2),
/// mai entrambe le forme.
fn check_ticket_form(ticket: &TicketDef) -> Result<(), EngineInitError> {
    let ticket_id = || ticket.id.clone();
    match &ticket.destinations {
        Some(_) if ticket.to.is_some() || ticket.points.is_some() => {
            Err(EngineInitError::TicketFormsBoth {
                ticket_id: ticket_id(),
            })
        }
        Some(list) if list.len() < 2 => Err(EngineInitError::TicketDestinationsTooFew {
            ticket_id: ticket_id(),
            count: list.len(),
        }),
        None if ticket.to.is_none() || ticket.points.is_none() => {
            Err(EngineInitError::TicketFormMissing {
                ticket_id: ticket_id(),
            })
        }
        _ => Ok(()),
    }
}

/// `table` sui bonus `mandala` e `regions`, dove è obbligatoria: non vuota, non decrescente
/// e con `points` uguale all'ultimo valore. `minRegions` e `doubleRegions` solo su `regions`.
fn check_bonus_table(bonus: &Bonus) -> Result<(), EngineInitError> {
    if bonus.bonus_type != REGIONS_BONUS_TYPE
        && (bonus.min_regions.is_some() || !bonus.double_regions.is_empty())
    {
        return Err(EngineInitError::RegionsFieldsNotAllowed {
            bonus_type: bonus.bonus_type.clone(),
        });
    }
    let is_mandala = bonus.bonus_type == MANDALA_BONUS_TYPE;
    let is_regions = bonus.bonus_type == REGIONS_BONUS_TYPE;
    if !is_mandala && !is_regions {
        return if bonus.table.is_empty() {
            Ok(())
        } else {
            Err(EngineInitError::BonusTableNotAllowed {
                bonus_type: bonus.bonus_type.clone(),
            })
        };
    }
    let Some(&last) = bonus.table.last() else {
        return Err(if is_mandala {
            EngineInitError::MandalaTableEmpty
        } else {
            EngineInitError::RegionsTableEmpty
        });
    };
    if let Some(index) = bonus.table.windows(2).position(|pair| pair[1] < pair[0]) {
        return Err(if is_mandala {
            EngineInitError::MandalaTableDecreasing { index: index + 1 }
        } else {
            EngineInitError::RegionsTableDecreasing { index: index + 1 }
        });
    }
    if bonus.points != last {
        return Err(if is_mandala {
            EngineInitError::MandalaPointsMismatch {
                points: bonus.points,
                last,
            }
        } else {
            EngineInitError::RegionsPointsMismatch {
                points: bonus.points,
                last,
            }
        });
    }
    Ok(())
}

/// Rotte `(da, a, lunghezza)` come le può costruire un giocatore: un giocatore prende al
/// massimo un binario per gruppo parallelo, quindi ogni gruppo `double` (stessi estremi e
/// base dell'id `{base}_{n}`) vale una sola rotta, di lunghezza pari al suo binario più
/// corto: un limite superiore valido anche con binari di lunghezza diversa. Ordine del file.
fn claimable_routes(map: &MapData) -> Vec<(&str, &str, u32)> {
    let mut out: Vec<(&str, &str, u32)> = Vec::with_capacity(map.routes.len());
    let mut groups: BTreeMap<(&str, &str, &str), usize> = BTreeMap::new();
    for route in &map.routes {
        let lane_base = route_lane(&route.id).map(|(base, _)| base);
        let Some(base) = lane_base.filter(|_| route.double) else {
            out.push((&route.from, &route.to, route.len));
            continue;
        };
        let (a, b) = pair_key(route);
        match groups.get(&(a, b, base)) {
            Some(&at) => out[at].2 = out[at].2.min(route.len),
            None => {
                groups.insert((a, b, base), out.len());
                out.push((&route.from, &route.to, route.len));
            }
        }
    }
    out
}

/// Binari dichiarati come gruppo parallelo, per coppia non ordinata di città.
pub(crate) fn parallel_groups(routes: &[Route]) -> BTreeMap<(&str, &str), Vec<&Route>> {
    let mut groups: BTreeMap<(&str, &str), Vec<&Route>> = BTreeMap::new();
    for route in routes.iter().filter(|r| r.double) {
        groups.entry(pair_key(route)).or_default().push(route);
    }
    groups
}

/// Chiave della coppia non ordinata di città di una rotta, come in `parallel_groups`.
pub(crate) fn pair_key(route: &Route) -> (&str, &str) {
    if route.from <= route.to {
        (route.from.as_str(), route.to.as_str())
    } else {
        (route.to.as_str(), route.from.as_str())
    }
}

/// Un binario `double` ha un id `{base}_{n}` e appartiene a un gruppo di 2–4 binari con
/// gli stessi estremi e lo stesso `alwaysOpen`, ids `{base}_1..{base}_k`. I binari possono
/// avere lunghezze diverse (Nordic: traghetto e tunnel sulla stessa coppia di città).
fn check_double_route(
    route: &Route,
    groups: &BTreeMap<(&str, &str), Vec<&Route>>,
) -> Result<(), EngineInitError> {
    let Some((base, lane)) = route_lane(&route.id) else {
        return Err(EngineInitError::DoubleRouteBadId {
            route_id: route.id.clone(),
        });
    };
    let group = groups.get(&pair_key(route)).map_or(&[][..], Vec::as_slice);
    let unpaired = |sister_id: String| EngineInitError::DoubleRouteUnpaired {
        route_id: route.id.clone(),
        sister_id,
    };
    if group.len() < 2 {
        let other = if lane == 1 { 2 } else { 1 };
        return Err(unpaired(format!("{base}_{other}")));
    }
    if group.len() > MAX_PARALLEL_LANES {
        return Err(EngineInitError::DoubleGroupTooLarge {
            route_id: route.id.clone(),
            lanes: group.len(),
        });
    }
    for n in 1..=group.len() {
        let id = format!("{base}_{n}");
        if !group.iter().any(|r| r.id == id) {
            return Err(unpaired(id));
        }
    }
    for other in group {
        if other.always_open != route.always_open {
            return Err(EngineInitError::DoubleGroupAlwaysOpenMismatch {
                route_id: route.id.clone(),
                other_id: other.id.clone(),
            });
        }
    }
    Ok(())
}

impl Ruleset {
    /// Verifica che ruleset e mappa siano coerenti per ogni numero di giocatori
    /// ammesso (i controlli sulle dimensioni dei mazzi usano `players.max`).
    pub fn validate(&self, map: &MapData) -> Result<(), EngineInitError> {
        if self.name.trim().is_empty() {
            return Err(EngineInitError::EmptyRulesetName);
        }
        if self.mode.trim().is_empty() {
            return Err(EngineInitError::EmptyRulesetMode);
        }
        if self.players.min == 0 || self.players.min > self.players.max {
            return Err(EngineInitError::InvalidPlayerRange {
                min: self.players.min,
                max: self.players.max,
            });
        }
        self.check_setup(map, self.players.max as usize)
    }

    /// Controlli di ruleset e mappa per una partita con `num_players` giocatori.
    pub(crate) fn check_setup(
        &self,
        map: &MapData,
        num_players: usize,
    ) -> Result<(), EngineInitError> {
        if !self.final_round_includes_trigger {
            return Err(EngineInitError::UnsupportedFinalRound);
        }
        if self.trains_per_player == 0 {
            return Err(EngineInitError::ZeroTrainsPerPlayer);
        }

        self.check_map(map)?;

        self.check_bonuses()?;
        self.check_big_cities(map)?;
        self.check_special_rules()?;
        self.check_payment_rules(map)?;
        self.check_ferry_cards(map)?;
        self.check_regions(map)?;
        self.check_train_deck(num_players)?;
        self.check_ticket_decks(map, num_players)?;
        self.check_tie_breakers()?;
        self.check_country_cards(map)?;
        self.check_passengers(map)?;
        // Il percorso più lungo si calcola con il bonus o come spareggio `longestPath`.
        if self.bonus_points(LONGEST_PATH_BONUS_TYPE).is_some()
            || self.tie_breaker_keys().contains(&TieBreaker::LongestPath)
        {
            self.check_longest_path_budget(map)?;
        }
        Ok(())
    }

    /// Solo tipi di bonus noti, ciascuno al massimo una volta.
    fn check_bonuses(&self) -> Result<(), EngineInitError> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for bonus in &self.bonuses {
            let known = [
                LONGEST_PATH_BONUS_TYPE,
                GLOBETROTTER_BONUS_TYPE,
                MANDALA_BONUS_TYPE,
                LARGEST_NETWORK_BONUS_TYPE,
                REGIONS_BONUS_TYPE,
            ];
            if !known.contains(&bonus.bonus_type.as_str()) {
                return Err(EngineInitError::UnknownBonusType {
                    bonus_type: bonus.bonus_type.clone(),
                });
            }
            if !seen.insert(bonus.bonus_type.as_str()) {
                return Err(EngineInitError::DuplicateBonusType {
                    bonus_type: bonus.bonus_type.clone(),
                });
            }
            check_bonus_table(bonus)?;
        }
        Ok(())
    }

    /// `ferryCards`: `count`, `maxInHand` e `wavesPerCard` almeno 1, mazzetto che basta a
    /// `maxInHand` carte per ogni giocatore (di `players.max`), mappa con rotte con onde.
    fn check_ferry_cards(&self, map: &MapData) -> Result<(), EngineInitError> {
        let Some(config) = &self.ferry_cards else {
            return Ok(());
        };
        for (field, value) in [
            ("count", config.count),
            ("maxInHand", config.max_in_hand),
            ("wavesPerCard", config.waves_per_card),
        ] {
            if value == 0 {
                return Err(EngineInitError::FerryCardsFieldZero { field });
            }
        }
        let needed = config.max_in_hand.saturating_mul(self.players.max);
        if config.count < needed {
            return Err(EngineInitError::FerryCardsPoolTooSmall {
                count: config.count,
                needed,
            });
        }
        if map.routes.iter().all(|r| r.ferries == 0) {
            return Err(EngineInitError::FerryCardsUnused);
        }
        Ok(())
    }

    /// Regioni: con il bonus `regions` ogni città (non Paese) ha una Regione, i Paesi non ne
    /// hanno, `minRegions` è almeno 1 e `doubleRegions` cita Regioni esistenti, senza
    /// ripetizioni. Senza il bonus nessuna città ha una Regione.
    fn check_regions(&self, map: &MapData) -> Result<(), EngineInitError> {
        let Some(bonus) = self.regions_bonus() else {
            return match map.cities.iter().find(|c| c.region.is_some()) {
                Some(city) => Err(EngineInitError::CityRegionUnused {
                    city_id: city.id.clone(),
                }),
                None => Ok(()),
            };
        };
        if bonus.min_regions.is_none_or(|min| min == 0) {
            return Err(EngineInitError::RegionsMinRegionsInvalid);
        }
        for city in &map.cities {
            match (city.country, &city.region) {
                (true, Some(_)) => {
                    return Err(EngineInitError::CountryHasRegion {
                        city_id: city.id.clone(),
                    });
                }
                (false, None) => {
                    return Err(EngineInitError::CityRegionMissing {
                        city_id: city.id.clone(),
                    });
                }
                _ => {}
            }
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for region in &bonus.double_regions {
            if !seen.insert(region.as_str()) {
                return Err(EngineInitError::RegionsDoubleDuplicate {
                    region: region.clone(),
                });
            }
            if !map
                .cities
                .iter()
                .any(|c| c.region.as_deref() == Some(region.as_str()))
            {
                return Err(EngineInitError::RegionsDoubleUnknown {
                    region: region.clone(),
                });
            }
        }
        Ok(())
    }

    /// `specialRules`: nessuna voce vuota.
    fn check_special_rules(&self) -> Result<(), EngineInitError> {
        match self.special_rules.iter().position(|r| r.trim().is_empty()) {
            Some(index) => Err(EngineInitError::EmptySpecialRule { index }),
            None => Ok(()),
        }
    }

    /// `bigCities`: città esistenti e senza ripetizioni.
    fn check_big_cities(&self, map: &MapData) -> Result<(), EngineInitError> {
        let cities: BTreeSet<&str> = map.cities.iter().map(|c| c.id.as_str()).collect();
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for city in &self.big_cities {
            if !cities.contains(city.as_str()) {
                return Err(EngineInitError::BigCityUnknown {
                    city_id: city.clone(),
                });
            }
            if !seen.insert(city.as_str()) {
                return Err(EngineInitError::DuplicateBigCity {
                    city_id: city.clone(),
                });
            }
        }
        Ok(())
    }

    /// Ids, cities, route lengths, ticket endpoints and double-route pairs of the map.
    fn check_map(&self, map: &MapData) -> Result<(), EngineInitError> {
        let mut cities: BTreeSet<&str> = BTreeSet::new();
        for city in &map.cities {
            if city.id.contains('#') {
                return Err(EngineInitError::CityIdInvalid {
                    city_id: city.id.clone(),
                });
            }
            if !cities.insert(city.id.as_str()) {
                return Err(EngineInitError::DuplicateCityId {
                    city_id: city.id.clone(),
                });
            }
        }
        let mut routes: BTreeMap<&str, &Route> = BTreeMap::new();
        let groups = parallel_groups(&map.routes);
        for route in &map.routes {
            if routes.insert(route.id.as_str(), route).is_some() {
                return Err(EngineInitError::DuplicateRouteId {
                    route_id: route.id.clone(),
                });
            }
        }
        let mut ticket_ids: BTreeSet<&str> = BTreeSet::new();
        for ticket in &map.tickets {
            if !ticket_ids.insert(ticket.id.as_str()) {
                return Err(EngineInitError::DuplicateTicketId {
                    ticket_id: ticket.id.clone(),
                });
            }
        }

        for route in &map.routes {
            if RouteColor::parse(&route.color).is_none() {
                return Err(EngineInitError::InvalidRouteColor {
                    route_id: route.id.clone(),
                    color: route.color.clone(),
                });
            }
            for city in [&route.from, &route.to] {
                if !cities.contains(city.as_str()) {
                    return Err(EngineInitError::RouteCityUnknown {
                        route_id: route.id.clone(),
                        city_id: city.clone(),
                    });
                }
            }
            if route.from == route.to {
                return Err(EngineInitError::RouteSameCity {
                    route_id: route.id.clone(),
                    city_id: route.from.clone(),
                });
            }
            if route.len == 0 {
                return Err(EngineInitError::RouteLengthZero {
                    route_id: route.id.clone(),
                });
            }
            if !self.route_points.contains_key(&route.len.to_string()) {
                return Err(EngineInitError::RouteLengthUnscored {
                    route_id: route.id.clone(),
                    len: route.len,
                });
            }
            if route.double {
                check_double_route(route, &groups)?;
            }
            if route.ferries > route.len {
                return Err(EngineInitError::RouteFerriesExceedLength {
                    route_id: route.id.clone(),
                    ferries: route.ferries,
                    len: route.len,
                });
            }
            if route.tunnel && route.ferries > 0 {
                return Err(EngineInitError::RouteTunnelAndFerry {
                    route_id: route.id.clone(),
                });
            }
            if route.tunnel && route.any_cards_per_space.is_some() {
                return Err(EngineInitError::RouteTunnelAnyCardsPerSpace {
                    route_id: route.id.clone(),
                });
            }
            if route.tunnel && self.tunnels.as_ref().is_none_or(|t| t.revealed == 0) {
                return Err(EngineInitError::TunnelsConfigMissing {
                    route_id: route.id.clone(),
                });
            }
            if route.mountains > 0 && route.tunnel {
                return Err(EngineInitError::RouteMountainsOnTunnel {
                    route_id: route.id.clone(),
                });
            }
            if route.mountains > route.len {
                return Err(EngineInitError::RouteMountainsExceedLength {
                    route_id: route.id.clone(),
                    mountains: route.mountains,
                    len: route.len,
                });
            }
            if route.mountains > 0
                && self
                    .mountains
                    .as_ref()
                    .is_none_or(|m| m.points_per_train == 0)
            {
                return Err(EngineInitError::MountainsConfigMissing {
                    route_id: route.id.clone(),
                });
            }
        }

        if self.mountains.is_some() && map.routes.iter().all(|r| r.mountains == 0) {
            return Err(EngineInitError::MountainsConfigUnused);
        }

        for ticket in &map.tickets {
            check_ticket_form(ticket)?;
            let ends = std::iter::once(ticket.from.as_str())
                .chain(ticket.destinations().map(|(to, _)| to));
            for city in ends {
                if !cities.contains(city) {
                    return Err(EngineInitError::TicketCityUnknown {
                        ticket_id: ticket.id.clone(),
                        city_id: city.to_string(),
                    });
                }
            }
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for (to, _) in ticket.destinations() {
                if to == ticket.from {
                    return Err(EngineInitError::TicketSameCity {
                        ticket_id: ticket.id.clone(),
                        city_id: ticket.from.clone(),
                    });
                }
                if !seen.insert(to) {
                    return Err(EngineInitError::TicketDestinationDuplicate {
                        ticket_id: ticket.id.clone(),
                        city_id: to.to_string(),
                    });
                }
            }
            if ticket.sets.is_empty() {
                return Err(EngineInitError::TicketSetsEmpty {
                    ticket_id: ticket.id.clone(),
                });
            }
            let mut sets: BTreeSet<&str> = BTreeSet::new();
            for set in &ticket.sets {
                if !sets.insert(set.as_str()) {
                    return Err(EngineInitError::DuplicateTicketSet {
                        ticket_id: ticket.id.clone(),
                        set: set.clone(),
                    });
                }
            }
        }

        if let Some(stations) = &self.stations {
            if stations.costs.len() != stations.count as usize {
                return Err(EngineInitError::StationCostsMismatch {
                    count: stations.count,
                    costs: stations.costs.len(),
                });
            }
            if let Some(index) = stations.costs.iter().position(|&c| c == 0) {
                return Err(EngineInitError::StationCostZero { index });
            }
        }
        Ok(())
    }

    /// Rejects maps where a player's claimable routes could form a component the longest
    /// path search cannot solve exactly.
    ///
    /// A player claims at most `k` routes, the shortest ones that fit in `trains_per_player`
    /// (at most one lane per parallel group, see `claimable_routes`), and the estimate is
    /// the density bound of `max_cyclomatic_number` over each connected group of routes of
    /// the map.
    fn check_longest_path_budget(&self, map: &MapData) -> Result<(), EngineInitError> {
        let cyclomatic = self.max_cyclomatic_estimate(map);
        if cyclomatic > MAX_CYCLOMATIC_NUMBER {
            let mut lens: Vec<u32> = claimable_routes(map).iter().map(|r| r.2).collect();
            lens.sort_unstable();
            let mut trains = u64::from(self.trains_per_player);
            let mut claimable = 0usize;
            for len in lens {
                if u64::from(len) > trains {
                    break;
                }
                trains -= u64::from(len);
                claimable += 1;
            }
            return Err(EngineInitError::LongestPathTooDense {
                routes: claimable,
                cyclomatic,
                max: MAX_CYCLOMATIC_NUMBER,
            });
        }
        Ok(())
    }

    /// Upper bound on the cyclomatic number of a component any player can build on `map`;
    /// routes with unknown cities are ignored.
    pub fn max_cyclomatic_estimate(&self, map: &MapData) -> u32 {
        let index: BTreeMap<&str, usize> = map
            .cities
            .iter()
            .enumerate()
            .map(|(i, city)| (city.id.as_str(), i))
            .collect();
        let spans: Vec<RouteSpan> = claimable_routes(map)
            .into_iter()
            .filter_map(|(from, to, len)| {
                Some((*index.get(from)?, *index.get(to)?, u64::from(len)))
            })
            .collect();
        let bound = max_cyclomatic_number(&spans, index.len(), u64::from(self.trains_per_player));
        u32::try_from(bound).unwrap_or(u32::MAX)
    }

    fn check_train_deck(&self, num_players: usize) -> Result<(), EngineInitError> {
        let have = (self.deck.per_color as usize)
            .saturating_mul(COLOR_COUNT)
            .saturating_add(self.deck.locomotives as usize);
        let needed = num_players
            .saturating_mul(INITIAL_HAND_SIZE)
            .saturating_add(FACE_UP_COUNT);
        if have < needed {
            return Err(EngineInitError::TrainDeckTooSmall { have, needed });
        }
        let colored = (self.deck.per_color as usize).saturating_mul(COLOR_COUNT);
        if colored < (self.deck.locomotives as usize).saturating_mul(LOCO_RESET_THRESHOLD) {
            return Err(EngineInitError::TrainDeckLocomotiveHeavy {
                per_color: self.deck.per_color,
                locomotives: self.deck.locomotives,
            });
        }
        Ok(())
    }

    fn check_ticket_decks(&self, map: &MapData, num_players: usize) -> Result<(), EngineInitError> {
        if map.tickets_in_play(self).next().is_none() {
            return Err(EngineInitError::EmptyTicketSet {
                set: self.ticket_set.clone(),
            });
        }

        let mut listed: BTreeSet<&str> = BTreeSet::new();
        for deck in &self.ticket_decks {
            if !listed.insert(deck.as_str()) {
                return Err(EngineInitError::DuplicateTicketDeck { deck: deck.clone() });
            }
        }

        let mut dealt_per_player: BTreeMap<&str, usize> = BTreeMap::new();
        let mut dealt_total: u32 = 0;
        for entry in &self.initial_tickets.dealt {
            if !listed.contains(entry.deck.as_str()) {
                return Err(EngineInitError::DealtTicketDeckUnknown {
                    deck: entry.deck.clone(),
                });
            }
            let slot = dealt_per_player.entry(entry.deck.as_str()).or_default();
            *slot = slot.saturating_add(entry.count as usize);
            dealt_total = dealt_total.saturating_add(entry.count);
        }
        if !listed.contains(self.ticket_draw.deck.as_str()) {
            return Err(EngineInitError::DrawTicketDeckUnknown {
                deck: self.ticket_draw.deck.clone(),
            });
        }
        let mut merged: BTreeSet<&str> = BTreeSet::from([self.ticket_draw.deck.as_str()]);
        for deck in &self.ticket_draw.merge_decks {
            if !listed.contains(deck.as_str()) {
                return Err(EngineInitError::MergeDeckUnknown { deck: deck.clone() });
            }
            if !merged.insert(deck.as_str()) {
                return Err(EngineInitError::MergeDeckDuplicate { deck: deck.clone() });
            }
        }
        if !self.ticket_draw.merge_decks.is_empty()
            && self.setup_discards == SetupDiscards::ReshuffleBottom
        {
            return Err(EngineInitError::SetupDiscardsNeedOwnDeck);
        }

        let mut deck_sizes: BTreeMap<&str, usize> = BTreeMap::new();
        for ticket in map.tickets_in_play(self) {
            if !listed.contains(ticket.deck.as_str()) {
                return Err(EngineInitError::TicketDeckNotListed {
                    ticket_id: ticket.id.clone(),
                    deck: ticket.deck.clone(),
                });
            }
            *deck_sizes.entry(ticket.deck.as_str()).or_default() += 1;
        }
        for (deck, per_player) in &dealt_per_player {
            let have = deck_sizes.get(deck).copied().unwrap_or(0);
            let needed = per_player.saturating_mul(num_players);
            if have < needed {
                return Err(EngineInitError::TicketDeckTooSmall {
                    deck: (*deck).to_string(),
                    have,
                    needed,
                });
            }
        }
        if self.initial_tickets.keep_min > dealt_total {
            return Err(EngineInitError::InitialKeepMinExceedsDealt {
                keep_min: self.initial_tickets.keep_min,
                dealt: dealt_total,
            });
        }
        for entry in &self.initial_tickets.dealt {
            if let Some(keep_max) = entry.keep_max
                && keep_max > entry.count
            {
                return Err(EngineInitError::DealtKeepMaxExceedsCount {
                    deck: entry.deck.clone(),
                    keep_max,
                    count: entry.count,
                });
            }
            if entry.keep_min > entry.count {
                return Err(EngineInitError::DealtKeepMinExceedsCount {
                    deck: entry.deck.clone(),
                    keep_min: entry.keep_min,
                    count: entry.count,
                });
            }
            if let Some(keep_max) = entry.keep_max
                && entry.keep_min > keep_max
            {
                return Err(EngineInitError::DealtKeepMinExceedsKeepMax {
                    deck: entry.deck.clone(),
                    keep_min: entry.keep_min,
                    keep_max,
                });
            }
        }
        let rules = self.initial_deck_rules();
        let deck_min = rules
            .iter()
            .fold(0u32, |acc, rule| acc.saturating_add(rule.min));
        if deck_min > self.initial_tickets.keep_min {
            return Err(EngineInitError::DealtKeepMinSumExceedsKeepMin {
                deck_min,
                keep_min: self.initial_tickets.keep_min,
            });
        }
        let max_keep = rules.iter().fold(0u32, |acc, rule| {
            acc.saturating_add(rule.max.map_or(rule.count, |max| max.min(rule.count)))
        });
        if self.initial_tickets.keep_min > max_keep {
            return Err(EngineInitError::InitialKeepMinUnreachable {
                keep_min: self.initial_tickets.keep_min,
                max_keep,
            });
        }
        if self.ticket_draw.count == 0 {
            return Err(EngineInitError::TicketDrawCountZero);
        }
        if self.ticket_draw.keep_min > self.ticket_draw.count {
            return Err(EngineInitError::DrawKeepMinExceedsCount {
                keep_min: self.ticket_draw.keep_min,
                count: self.ticket_draw.count,
            });
        }
        Ok(())
    }

    /// `tieBreakers`: solo chiavi note, senza ripetizioni.
    fn check_tie_breakers(&self) -> Result<(), EngineInitError> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for key in self.tie_breakers.iter().flatten() {
            if TieBreaker::parse(key).is_none() {
                return Err(EngineInitError::UnknownTieBreaker { key: key.clone() });
            }
            if !seen.insert(key.as_str()) {
                return Err(EngineInitError::DuplicateTieBreaker { key: key.clone() });
            }
        }
        Ok(())
    }

    /// `countryCards`: le chiavi sono città `country` della mappa.
    fn check_country_cards(&self, map: &MapData) -> Result<(), EngineInitError> {
        for city_id in self.country_cards.keys() {
            if !map.cities.iter().any(|c| c.country && &c.id == city_id) {
                return Err(EngineInitError::CountryCardsUnknownCountry {
                    city_id: city_id.clone(),
                });
            }
        }
        Ok(())
    }

    /// `passengers`: colori non vuoti e unici, `perColor` ≥ 1, `placement` su città esistenti
    /// con conteggi ≥ 1 e somma pari al sacchetto, due `majorityPoints`.
    fn check_passengers(&self, map: &MapData) -> Result<(), EngineInitError> {
        let Some(config) = &self.passengers else {
            return Ok(());
        };
        if config.colors.is_empty() {
            return Err(EngineInitError::PassengersColorsEmpty);
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for color in &config.colors {
            if !seen.insert(color.as_str()) {
                return Err(EngineInitError::PassengersDuplicateColor {
                    color: color.clone(),
                });
            }
        }
        if config.per_color == 0 {
            return Err(EngineInitError::PassengersPerColorZero);
        }
        let mut placed: u32 = 0;
        for (city_id, &count) in &config.placement {
            if !map.cities.iter().any(|c| &c.id == city_id) {
                return Err(EngineInitError::PassengersUnknownCity {
                    city_id: city_id.clone(),
                });
            }
            if count == 0 {
                return Err(EngineInitError::PassengersPlacementZero {
                    city_id: city_id.clone(),
                });
            }
            placed = placed.saturating_add(count);
        }
        let bag = u32::try_from(config.colors.len())
            .unwrap_or(u32::MAX)
            .saturating_mul(config.per_color);
        if placed != bag {
            return Err(EngineInitError::PassengersPlacementMismatch { placed, bag });
        }
        if config.majority_points.len() != 2 {
            return Err(EngineInitError::PassengersMajorityPoints {
                len: config.majority_points.len(),
            });
        }
        Ok(())
    }
}
