//! Handler per DrawTickets e ChooseTickets.

use std::collections::{BTreeMap, BTreeSet};

use crate::engine::Engine;
use crate::error::{EngineError, GamePhase};
use crate::event::GameEvent;
use crate::legal::{TicketDeckAvailability, TicketDrawMoves};
use crate::types::{SetupDiscards, TicketId, TicketStatus};

/// Valida una scelta di biglietti da tenere tra quelli offerti.
///
/// Restituisce i biglietti scartati, nell'ordine in cui erano offerti.
fn validate_keep(
    keep: &[TicketId],
    offered: &[TicketId],
    keep_min: u32,
) -> Result<Vec<TicketId>, EngineError> {
    let keep_set: BTreeSet<&str> = keep.iter().map(String::as_str).collect();
    if keep_set.len() != keep.len() {
        return Err(EngineError::InvalidPayload {
            reason: "biglietti duplicati nella scelta".into(),
        });
    }
    if keep.len() < keep_min as usize {
        return Err(EngineError::TicketChoiceBelowMin {
            kept: keep.len(),
            min: keep_min,
        });
    }
    if keep.len() > offered.len() {
        return Err(EngineError::TicketChoiceTooMany {
            kept: keep.len(),
            available: offered.len(),
        });
    }
    let offered_set: BTreeSet<&str> = offered.iter().map(String::as_str).collect();
    if let Some(bad) = keep.iter().find(|tid| !offered_set.contains(tid.as_str())) {
        return Err(EngineError::TicketNotOffered {
            ticket_id: bad.clone(),
        });
    }
    Ok(offered
        .iter()
        .filter(|tid| !keep_set.contains(tid.as_str()))
        .cloned()
        .collect())
}

impl Engine {
    /// Pesca nuovi biglietti: a metà partita (transizione a TicketChoice) oppure, nel
    /// draft iniziale con `initialTickets.chooseFrom`, i biglietti da scegliere.
    ///
    /// Con `chooseFrom` `counts` annuncia le quote per mazzo; altrimenti deve essere vuoto.
    pub(crate) fn handle_draw_tickets(
        &mut self,
        player_index: usize,
        counts: &BTreeMap<String, u32>,
    ) -> Result<Vec<GameEvent>, EngineError> {
        match self.phase {
            GamePhase::PlayerTurn | GamePhase::LastRound => {}
            GamePhase::InitialDraft if self.ruleset.initial_tickets.choose_from.is_some() => {
                return self.handle_initial_draw(player_index, counts);
            }
            _ => return Err(EngineError::WrongState { state: self.phase }),
        }

        if player_index != self.current_player_index {
            return Err(EngineError::NotYourTurn {
                player_index,
                current_player: self.current_player_index,
            });
        }

        let drawn = if self.ruleset.ticket_draw.choose_from.is_some() {
            let moves = self.ticket_draw_moves().ok_or_else(no_tickets_available)?;
            self.draw_with_counts(&moves, counts)?
        } else {
            if !counts.is_empty() {
                return Err(EngineError::InvalidPayload {
                    reason: "quote per mazzo non ammesse: si pesca da un solo mazzo".into(),
                });
            }
            let count = self.ruleset.ticket_draw.count as usize;
            let min = self.min_tickets_to_draw();
            let Some(pile) = self.ticket_piles.get_mut(&self.ruleset.ticket_draw.deck) else {
                return Err(no_tickets_available());
            };
            let take = count.min(pile.len());
            if take < min {
                return Err(no_tickets_available());
            }
            pile.drain(..take).collect()
        };

        self.pending_tickets = drawn.clone();
        self.phase = GamePhase::TicketChoice;

        Ok(vec![GameEvent::TicketsOffered {
            player_index,
            ticket_ids: drawn,
        }])
    }

    /// Pesca del draft iniziale con `chooseFrom`: ammessa a chi non ha ancora biglietti
    /// offerti né ha già scelto. L'ordine delle pescate tra i giocatori è quello delle azioni.
    fn handle_initial_draw(
        &mut self,
        player_index: usize,
        counts: &BTreeMap<String, u32>,
    ) -> Result<Vec<GameEvent>, EngineError> {
        let Some(player) = self.players.get(player_index) else {
            return Err(EngineError::PlayerNotFound {
                index: player_index,
            });
        };
        let already_offered = self
            .pending_initial_tickets
            .get(player_index)
            .is_none_or(|offered| !offered.is_empty());
        if player.initial_draft_done || already_offered {
            return Err(EngineError::InvalidPayload {
                reason: "biglietti del draft iniziale già pescati o scelta già fatta".into(),
            });
        }
        let moves = self.initial_draw_moves().ok_or_else(no_tickets_available)?;
        let drawn = self.draw_with_counts(&moves, counts)?;
        self.pending_initial_tickets[player_index] = drawn.clone();
        Ok(vec![GameEvent::TicketsOffered {
            player_index,
            ticket_ids: drawn,
        }])
    }

    /// Biglietti che si possono pescare con le quote (mazzi di `ticketDraw.chooseFrom`),
    /// `None` senza `chooseFrom` o se ne restano meno di `keepMin` (e di uno).
    pub(crate) fn ticket_draw_moves(&self) -> Option<TicketDrawMoves> {
        let draw = &self.ruleset.ticket_draw;
        self.draw_moves(draw.choose_from.as_deref()?, draw.count, draw.keep_min)
    }

    /// Come `ticket_draw_moves` per il draft iniziale (`initialTickets.chooseFrom`).
    pub(crate) fn initial_draw_moves(&self) -> Option<TicketDrawMoves> {
        let initial = &self.ruleset.initial_tickets;
        self.draw_moves(
            initial.choose_from.as_deref()?,
            initial.count,
            initial.keep_min,
        )
    }

    fn draw_moves(&self, decks: &[String], count: u32, keep_min: u32) -> Option<TicketDrawMoves> {
        let available: Vec<TicketDeckAvailability> = decks
            .iter()
            .map(|deck| TicketDeckAvailability {
                deck: deck.clone(),
                count: self
                    .ticket_piles
                    .get(deck)
                    .map_or(0, |pile| u32::try_from(pile.len()).unwrap_or(u32::MAX)),
            })
            .collect();
        let remaining = available
            .iter()
            .fold(0u32, |acc, deck| acc.saturating_add(deck.count));
        let total = count.min(remaining);
        (total >= keep_min.max(1)).then_some(TicketDrawMoves { total, available })
    }

    /// Valida le quote e pesca dalla cima di ogni mazzo, nell'ordine dei mazzi del ruleset;
    /// non tocca lo stato se le quote non sono valide.
    fn draw_with_counts(
        &mut self,
        moves: &TicketDrawMoves,
        counts: &BTreeMap<String, u32>,
    ) -> Result<Vec<TicketId>, EngineError> {
        let mut sum: u64 = 0;
        for (deck, &requested) in counts {
            let Some(available) = moves.available.iter().find(|a| &a.deck == deck) else {
                return Err(EngineError::InvalidPayload {
                    reason: format!("mazzo '{deck}' non ammesso per la pesca"),
                });
            };
            if requested > available.count {
                return Err(EngineError::InvalidPayload {
                    reason: format!(
                        "quota {requested} del mazzo '{deck}' superiore ai biglietti rimasti ({})",
                        available.count
                    ),
                });
            }
            sum += u64::from(requested);
        }
        if sum != u64::from(moves.total) {
            return Err(EngineError::InvalidPayload {
                reason: format!(
                    "la somma delle quote è {sum}, i biglietti da pescare sono {}",
                    moves.total
                ),
            });
        }

        let mut drawn: Vec<TicketId> = Vec::new();
        for available in &moves.available {
            let take = counts.get(&available.deck).copied().unwrap_or(0) as usize;
            if take == 0 {
                continue;
            }
            if let Some(pile) = self.ticket_piles.get_mut(&available.deck) {
                drawn.extend(pile.drain(..take));
            }
        }
        Ok(drawn)
    }

    /// Biglietti che devono essere disponibili per poter pescare: almeno uno
    /// e almeno `keep_min`, altrimenti la scelta non potrebbe mai completarsi.
    pub(crate) fn min_tickets_to_draw(&self) -> usize {
        (self.ruleset.ticket_draw.keep_min as usize).max(1)
    }

    /// Sceglie quali biglietti tenere (initial draft o mid-game).
    pub(crate) fn handle_choose_tickets(
        &mut self,
        player_index: usize,
        keep: &[String],
    ) -> Result<Vec<GameEvent>, EngineError> {
        match self.phase {
            GamePhase::InitialDraft => self.handle_initial_draft_choice(player_index, keep),
            GamePhase::TicketChoice => self.handle_mid_game_choice(player_index, keep),
            _ => Err(EngineError::WrongState { state: self.phase }),
        }
    }

    /// Aggiunge ai biglietti del giocatore quelli tenuti, ancora da valutare.
    fn add_kept_tickets(&mut self, player_index: usize, keep: &[TicketId]) {
        for tid in keep {
            let points = self.tickets_by_id.get(tid).map_or(0, |t| t.points);
            self.players[player_index].tickets.insert(
                tid.clone(),
                TicketStatus {
                    ticket_id: tid.clone(),
                    points,
                    completed: None,
                },
            );
        }
    }

    /// Rimette un biglietto in fondo al proprio mazzo.
    fn return_to_own_deck(&mut self, ticket_id: &TicketId) {
        if let Some(deck) = self.tickets_by_id.get(ticket_id).map(|t| t.deck.clone()) {
            self.ticket_piles
                .entry(deck)
                .or_default()
                .push(ticket_id.clone());
        }
    }

    /// Per ogni deck con `keepMax`, i biglietti tenuti (già verificati tra gli offerti)
    /// non possono superarlo.
    fn check_initial_deck_max(&self, keep: &[TicketId]) -> Result<(), EngineError> {
        for rule in self.ruleset.initial_deck_rules() {
            let Some(max) = rule.max else {
                continue;
            };
            let kept = keep
                .iter()
                .filter(|tid| {
                    self.tickets_by_id
                        .get(*tid)
                        .is_some_and(|t| t.deck == rule.deck)
                })
                .count();
            if kept > max as usize {
                return Err(EngineError::TicketChoiceAboveDeckMax {
                    deck: rule.deck,
                    kept,
                    max,
                });
            }
        }
        Ok(())
    }

    fn handle_initial_draft_choice(
        &mut self,
        player_index: usize,
        keep: &[String],
    ) -> Result<Vec<GameEvent>, EngineError> {
        let Some(player) = self.players.get(player_index) else {
            return Err(EngineError::PlayerNotFound {
                index: player_index,
            });
        };
        if player.initial_draft_done {
            return Err(EngineError::InvalidPayload {
                reason: "draft iniziale già completato per questo giocatore".into(),
            });
        }
        let Some(offered) = self.pending_initial_tickets.get(player_index) else {
            return Err(EngineError::PlayerNotFound {
                index: player_index,
            });
        };
        if offered.is_empty() && self.ruleset.initial_tickets.choose_from.is_some() {
            return Err(EngineError::InvalidPayload {
                reason: "pesca prima i biglietti del draft iniziale".into(),
            });
        }

        let discarded = validate_keep(keep, offered, self.ruleset.initial_tickets.keep_min)?;
        self.check_initial_deck_max(keep)?;

        self.add_kept_tickets(player_index, keep);

        match self.ruleset.setup_discards {
            SetupDiscards::ReshuffleBottom => {
                // Only tickets of the draw deck go back to the pile; other decks' discards are dropped.
                let draw_deck = &self.ruleset.ticket_draw.deck;
                let mut disc: Vec<TicketId> = discarded
                    .iter()
                    .filter(|tid| {
                        self.tickets_by_id
                            .get(*tid)
                            .is_some_and(|t| &t.deck == draw_deck)
                    })
                    .cloned()
                    .collect();
                self.rng.shuffle(&mut disc);
                self.ticket_piles
                    .entry(self.ruleset.ticket_draw.deck.clone())
                    .or_default()
                    .extend(disc);
            }
            SetupDiscards::ReshuffleBottomOwnDeck => {
                // Per mazzo, nell'ordine di `ticketDecks`: mescolati e in fondo al proprio mazzo.
                for deck in self.ruleset.ticket_decks.clone() {
                    let mut disc: Vec<TicketId> = discarded
                        .iter()
                        .filter(|tid| self.tickets_by_id.get(*tid).is_some_and(|t| t.deck == deck))
                        .cloned()
                        .collect();
                    if disc.is_empty() {
                        continue;
                    }
                    self.rng.shuffle(&mut disc);
                    self.ticket_piles.entry(deck).or_default().extend(disc);
                }
            }
            SetupDiscards::Removed => {}
        }

        self.pending_initial_tickets[player_index].clear();
        self.players[player_index].initial_draft_done = true;

        let mut events = vec![
            GameEvent::TicketsChosen {
                player_index,
                kept: keep.to_vec(),
                discarded,
            },
            GameEvent::InitialDraftDone { player_index },
        ];

        if self.players.iter().all(|p| p.initial_draft_done) {
            self.phase = GamePhase::PlayerTurn;
            events.push(GameEvent::InitialDraftComplete);
        }

        Ok(events)
    }

    fn handle_mid_game_choice(
        &mut self,
        player_index: usize,
        keep: &[String],
    ) -> Result<Vec<GameEvent>, EngineError> {
        if player_index != self.current_player_index {
            return Err(EngineError::NotYourTurn {
                player_index,
                current_player: self.current_player_index,
            });
        }

        let discarded = validate_keep(
            keep,
            &self.pending_tickets,
            self.ruleset.ticket_draw.keep_min,
        )?;

        self.add_kept_tickets(player_index, keep);

        // Gli scartati vanno in fondo al proprio mazzo, in ordine.
        for ticket_id in &discarded {
            self.return_to_own_deck(ticket_id);
        }
        self.pending_tickets.clear();

        // Un biglietto pescato potrebbe essere già completato dalle rotte esistenti.
        self.update_ticket_completion(player_index);

        let mut events = vec![GameEvent::TicketsChosen {
            player_index,
            kept: keep.to_vec(),
            discarded,
        }];

        self.advance_turn(&mut events);

        Ok(events)
    }
}

fn no_tickets_available() -> EngineError {
    EngineError::InvalidPayload {
        reason: "nessun biglietto disponibile".into(),
    }
}
