//! Handler per DrawTickets e ChooseTickets.

use std::collections::BTreeSet;

use crate::engine::Engine;
use crate::error::{EngineError, GamePhase};
use crate::event::GameEvent;
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
    /// Pesca nuovi biglietti (mid-game). Transizione a TicketChoice.
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
        let deck_name = &self.ruleset.ticket_draw.deck;

        // Posizioni dei primi `count` biglietti del deck di pesca.
        let positions: Vec<usize> = self
            .ticket_draw_pile
            .iter()
            .enumerate()
            .filter(|(_, tid)| {
                self.tickets_by_id
                    .get(*tid)
                    .is_some_and(|t| &t.deck == deck_name)
            })
            .map(|(pos, _)| pos)
            .take(count)
            .collect();

        if positions.len() < self.min_tickets_to_draw() {
            return Err(EngineError::InvalidPayload {
                reason: "nessun biglietto disponibile".into(),
            });
        }

        // Rimozione dalla posizione più alta, così le altre non si spostano.
        let mut drawn: Vec<TicketId> = positions
            .iter()
            .rev()
            .map(|&pos| self.ticket_draw_pile.remove(pos))
            .collect();
        drawn.reverse();

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
                self.ticket_draw_pile.extend(disc);
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

        // Gli scartati vanno in fondo al mazzo, in ordine.
        self.ticket_draw_pile.extend(discarded.iter().cloned());
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
