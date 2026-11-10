//! Controlli di coerenza tra ruleset, mappa e giocatori.
//!
//! Gli stessi controlli girano in `Engine::new`, in `ttr-validate` e
//! all'avvio del server, così dati incoerenti non producono partite bloccate.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::EngineInitError;
use crate::types::{
    COLOR_COUNT, FACE_UP_COUNT, INITIAL_HAND_SIZE, LONGEST_PATH_BONUS_TYPE, MapData, RouteColor,
    Ruleset,
};

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

        for route in &map.routes {
            if RouteColor::parse(&route.color).is_none() {
                return Err(EngineInitError::InvalidRouteColor {
                    route_id: route.id.clone(),
                    color: route.color.clone(),
                });
            }
        }

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
        self.check_ticket_decks(map, num_players)
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
        if self.ticket_draw.keep_min > self.ticket_draw.count {
            return Err(EngineInitError::DrawKeepMinExceedsCount {
                keep_min: self.ticket_draw.keep_min,
                count: self.ticket_draw.count,
            });
        }
        Ok(())
    }
}
