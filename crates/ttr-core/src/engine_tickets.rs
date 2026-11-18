//! Handler per DrawTickets e ChooseTickets.

use std::collections::BTreeSet;

use crate::engine::Engine;
use crate::error::{EngineError, GamePhase};
use crate::event::GameEvent;
use crate::types::{SetupDiscards, TicketDef, TicketDiscards, TicketId, TicketStatus};

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
    /// Pesca nuovi biglietti a metà partita (transizione a TicketChoice).
    pub(crate) fn handle_draw_tickets(
        &mut self,
        player_index: usize,
    ) -> Result<Vec<GameEvent>, EngineError> {
        match self.phase {
            GamePhase::PlayerTurn | GamePhase::LastRound => {}
            _ => return Err(EngineError::WrongState { state: self.phase }),
        }

        if player_index != self.current_player_index {
            return Err(EngineError::NotYourTurn {
                player_index,
                current_player: self.current_player_index,
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
        let drawn: Vec<TicketId> = pile.drain(..take).collect();

        self.pending_tickets = drawn.clone();
        self.phase = GamePhase::TicketChoice;

        Ok(vec![GameEvent::TicketsOffered {
            player_index,
            ticket_ids: drawn,
        }])
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
            let points = self.tickets_by_id.get(tid).map_or(0, TicketDef::max_points);
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

    /// Mazzo in cui un biglietto rientra: il proprio, o la pila unita di `ticketDraw`
    /// se il suo mazzo è in `ticketDraw.mergeDecks`.
    fn return_deck_of(&self, deck: &str) -> String {
        let draw = &self.ruleset.ticket_draw;
        if draw.merge_decks.iter().any(|d| d == deck) {
            draw.deck.clone()
        } else {
            deck.to_string()
        }
    }

    /// Rimette un biglietto in fondo al proprio mazzo (o alla pila unita).
    fn return_to_own_deck(&mut self, ticket_id: &TicketId) {
        if let Some(deck) = self.tickets_by_id.get(ticket_id).map(|t| t.deck.clone()) {
            let target = self.return_deck_of(&deck);
            self.ticket_piles
                .entry(target)
                .or_default()
                .push(ticket_id.clone());
        }
    }

    /// Finito il draft iniziale, i mazzi di `ticketDraw.mergeDecks` confluiscono in
    /// `ticketDraw.deck`: una sola pila mescolata con l'RNG del motore (prima il mazzo di
    /// pesca, poi i mazzi uniti nell'ordine del ruleset). Senza `mergeDecks` non fa nulla.
    fn merge_ticket_decks(&mut self) {
        if self.ruleset.ticket_draw.merge_decks.is_empty() {
            return;
        }
        let draw_deck = self.ruleset.ticket_draw.deck.clone();
        let mut merged = self.ticket_piles.remove(&draw_deck).unwrap_or_default();
        for deck in self.ruleset.ticket_draw.merge_decks.clone() {
            if let Some(pile) = self.ticket_piles.get_mut(&deck) {
                merged.append(pile);
            }
        }
        self.rng.shuffle(&mut merged);
        self.ticket_piles.insert(draw_deck, merged);
    }

    /// Biglietti ancora pescabili con `DRAW_TICKETS`: la pila di `ticketDraw` e, finché
    /// non sono stati uniti, i mazzi di `mergeDecks`.
    #[must_use]
    pub fn ticket_deck_size(&self) -> usize {
        let draw = &self.ruleset.ticket_draw;
        std::iter::once(&draw.deck)
            .chain(&draw.merge_decks)
            .filter_map(|deck| self.ticket_piles.get(deck))
            .map(Vec::len)
            .sum()
    }

    /// Per ogni deck con `keepMin` / `keepMax`, i biglietti tenuti (già verificati tra gli
    /// offerti) devono rispettarli.
    fn check_initial_deck_limits(&self, keep: &[TicketId]) -> Result<(), EngineError> {
        for rule in self.ruleset.initial_deck_rules() {
            let kept = keep
                .iter()
                .filter(|tid| {
                    self.tickets_by_id
                        .get(*tid)
                        .is_some_and(|t| t.deck == rule.deck)
                })
                .count();
            if kept < rule.min as usize {
                return Err(EngineError::TicketChoiceBelowDeckMin {
                    deck: rule.deck,
                    kept,
                    min: rule.min,
                });
            }
            if let Some(max) = rule.max
                && kept > max as usize
            {
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
        let discarded = validate_keep(keep, offered, self.ruleset.initial_tickets.keep_min)?;
        self.check_initial_deck_limits(keep)?;

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
            self.merge_ticket_decks();
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

        // Con `discards: removed` gli scartati escono dalla partita; altrimenti vanno in
        // fondo al proprio mazzo, in ordine.
        if self.ruleset.ticket_draw.discards == TicketDiscards::Bottom {
            for ticket_id in &discarded {
                self.return_to_own_deck(ticket_id);
            }
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
