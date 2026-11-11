//! Controlli di coerenza tra ruleset, mappa e giocatori.
//!
//! Gli stessi controlli girano in `Engine::new`, in `ttr-validate` e
//! all'avvio del server, così dati incoerenti non producono partite bloccate.

use std::collections::{BTreeMap, BTreeSet};

use crate::density::{RouteSpan, max_cyclomatic_number};
use crate::error::EngineInitError;
use crate::scoring::MAX_CYCLOMATIC_NUMBER;
use crate::types::{
    COLOR_COUNT, FACE_UP_COUNT, INITIAL_HAND_SIZE, LOCO_RESET_THRESHOLD, LONGEST_PATH_BONUS_TYPE,
    MapData, Route, RouteColor, Ruleset, sister_route_id,
};

/// A double route needs a `_1`/`_2` id and a sister route that is also double, with the
/// same endpoints and length.
fn check_double_route(
    route: &Route,
    routes: &BTreeMap<&str, &Route>,
) -> Result<(), EngineInitError> {
    let Some(sister_id) = sister_route_id(&route.id) else {
        return Err(EngineInitError::DoubleRouteBadId {
            route_id: route.id.clone(),
        });
    };
    let paired = routes.get(sister_id.as_str()).is_some_and(|sister| {
        let same_ends = (sister.from == route.from && sister.to == route.to)
            || (sister.from == route.to && sister.to == route.from);
        sister.double && same_ends && sister.len == route.len
    });
    if paired {
        Ok(())
    } else {
        Err(EngineInitError::DoubleRouteUnpaired {
            route_id: route.id.clone(),
            sister_id,
        })
    }
}

impl Ruleset {
    /// Verifica che ruleset e mappa siano coerenti per ogni numero di giocatori
    /// ammesso (i controlli sulle dimensioni dei mazzi usano `players.max`).
    pub fn validate(&self, map: &MapData) -> Result<(), EngineInitError> {
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

        if !self
            .bonuses
            .iter()
            .any(|b| b.bonus_type == LONGEST_PATH_BONUS_TYPE)
        {
            return Err(EngineInitError::MissingLongestPathBonus {
                bonus_type: LONGEST_PATH_BONUS_TYPE,
            });
        }

        self.check_train_deck(num_players)?;
        self.check_ticket_decks(map, num_players)?;
        self.check_longest_path_budget(map)
    }

    /// Ids, cities, route lengths, ticket endpoints and double-route pairs of the map.
    fn check_map(&self, map: &MapData) -> Result<(), EngineInitError> {
        let mut cities: BTreeSet<&str> = BTreeSet::new();
        for city in &map.cities {
            if !cities.insert(city.id.as_str()) {
                return Err(EngineInitError::DuplicateCityId {
                    city_id: city.id.clone(),
                });
            }
        }
        let mut routes: BTreeMap<&str, &Route> = BTreeMap::new();
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
                check_double_route(route, &routes)?;
            }
        }

        for ticket in &map.tickets {
            for city in [&ticket.from, &ticket.to] {
                if !cities.contains(city.as_str()) {
                    return Err(EngineInitError::TicketCityUnknown {
                        ticket_id: ticket.id.clone(),
                        city_id: city.clone(),
                    });
                }
            }
            if ticket.from == ticket.to {
                return Err(EngineInitError::TicketSameCity {
                    ticket_id: ticket.id.clone(),
                    city_id: ticket.from.clone(),
                });
            }
        }
        Ok(())
    }

    /// Rejects maps where a player's claimable routes could form a component the longest
    /// path search cannot solve exactly.
    ///
    /// A player claims at most `k` routes, the shortest ones that fit in `trains_per_player`,
    /// and the estimate is the density bound of `max_cyclomatic_number` over each connected
    /// group of routes of the map.
    fn check_longest_path_budget(&self, map: &MapData) -> Result<(), EngineInitError> {
        let cyclomatic = self.max_cyclomatic_estimate(map);
        if cyclomatic > MAX_CYCLOMATIC_NUMBER {
            let mut lens: Vec<u32> = map.routes.iter().map(|r| r.len).collect();
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
        let spans: Vec<RouteSpan> = map
            .routes
            .iter()
            .filter_map(|r| {
                Some((
                    *index.get(r.from.as_str())?,
                    *index.get(r.to.as_str())?,
                    u64::from(r.len),
                ))
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

        let mut deck_sizes: BTreeMap<&str, usize> = BTreeMap::new();
        for ticket in &map.tickets {
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
}
