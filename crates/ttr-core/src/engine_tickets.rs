//! Handler per DrawTickets e ChooseTickets.

use crate::engine::Engine;
use crate::error::{EngineError, GamePhase};
use crate::event::GameEvent;
use crate::types::{SetupDiscards, TicketStatus};

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

        // Pesca biglietti dal mazzo indicato
        let mut drawn: Vec<String> = Vec::with_capacity(count);
        let mut i = 0;
        while i < count && !self.ticket_draw_pile.is_empty() {
            // Pesca solo dal deck giusto
            let idx = self.ticket_draw_pile.iter().position(|tid| {
                self.tickets_by_id
                    .get(tid)
                    .map(|t| &t.deck == deck_name)
                    .unwrap_or(false)
            });

            if let Some(pos) = idx {
                let tid = self.ticket_draw_pile.remove(pos);
                drawn.push(tid);
                i += 1;
            } else {
                // Nessun biglietto del deck richiesto rimasto
                break;
            }
        }

        if drawn.is_empty() {
            return Err(EngineError::InvalidPayload {
                reason: "nessun biglietto disponibile".into(),
            });
        }

        self.pending_tickets = drawn.clone();
        self.phase = GamePhase::TicketChoice;

        Ok(vec![GameEvent::TicketsOffered {
            player_index,
            ticket_ids: drawn,
        }])
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

    fn handle_initial_draft_choice(
        &mut self,
        player_index: usize,
        keep: &[String],
    ) -> Result<Vec<GameEvent>, EngineError> {
        if player_index >= self.players.len() {
            return Err(EngineError::PlayerNotFound {
                index: player_index,
            });
        }
        if self.players[player_index].initial_draft_done {
            return Err(EngineError::InvalidPayload {
                reason: "draft iniziale già completato per questo giocatore".into(),
            });
        }

        let offered = &self.pending_initial_tickets[player_index];
        let keep_min = self.ruleset.initial_tickets.keep_min as usize;

        if keep.len() < keep_min {
            return Err(EngineError::TicketChoiceBelowMin {
                kept: keep.len(),
                min: keep_min as u32,
            });
        }
        if keep.len() > offered.len() {
            return Err(EngineError::TicketChoiceTooMany {
                kept: keep.len(),
                available: offered.len(),
            });
        }

        // Validazione: tutti i keep devono essere tra gli offerti
        let offered_set: std::collections::BTreeSet<&str> =
            offered.iter().map(|s| s.as_str()).collect();
        let keep_set: std::collections::BTreeSet<&str> = keep.iter().map(|s| s.as_str()).collect();
        if !keep_set.is_subset(&offered_set) {
            let bad: Vec<_> = keep_set.difference(&offered_set).collect();
            return Err(EngineError::TicketNotOffered {
                ticket_id: bad.first().unwrap_or(&&"?").to_string(),
            });
        }

        // Determina scartati
        let discarded: Vec<String> = offered
            .iter()
            .filter(|tid| !keep_set.contains(tid.as_str()))
            .cloned()
            .collect();

        // Aggiungi i tenuti al giocatore
        for tid in keep {
            let ticket = self.tickets_by_id.get(tid).cloned();
            let points = ticket.map(|t| t.points).unwrap_or(0);
            self.players[player_index].tickets.insert(
                tid.clone(),
                TicketStatus {
                    ticket_id: tid.clone(),
                    points,
                    completed: None, // non ancora valutato
                },
            );
        }

        // Gestisci scarti secondo setupDiscards
        match self.ruleset.setup_discards {
            SetupDiscards::ReshuffleBottom => {
                let mut disc = discarded.clone();
                self.rng.shuffle(&mut disc);
                // Metti in fondo al mazzo
                self.ticket_draw_pile.extend(disc);
            }
            SetupDiscards::Removed => {
                // Scarti fuori dalla partita
            }
        }

        self.players[player_index].initial_draft_done = true;

        let mut events = vec![
            GameEvent::TicketsChosen {
                player_index,
                kept: keep.to_vec(),
                discarded,
            },
            GameEvent::InitialDraftDone { player_index },
        ];

        // Controlla se tutti hanno completato il draft
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

        let offered = &self.pending_tickets;
        let keep_min = self.ruleset.ticket_draw.keep_min as usize;

        if keep.len() < keep_min {
            return Err(EngineError::TicketChoiceBelowMin {
                kept: keep.len(),
                min: keep_min as u32,
            });
        }
        if keep.len() > offered.len() {
            return Err(EngineError::TicketChoiceTooMany {
                kept: keep.len(),
                available: offered.len(),
            });
        }

        let offered_set: std::collections::BTreeSet<&str> =
            offered.iter().map(|s| s.as_str()).collect();
        let keep_set: std::collections::BTreeSet<&str> = keep.iter().map(|s| s.as_str()).collect();
        if !keep_set.is_subset(&offered_set) {
            let bad: Vec<_> = keep_set.difference(&offered_set).collect();
            return Err(EngineError::TicketNotOffered {
                ticket_id: bad.first().unwrap_or(&&"?").to_string(),
            });
        }

        // Determina scartati (in ordine: quelli non tenuti)
        let discarded: Vec<String> = offered
            .iter()
            .filter(|tid| !keep_set.contains(tid.as_str()))
            .cloned()
            .collect();

        // Aggiungi tenuti al giocatore
        for tid in keep {
            let ticket = self.tickets_by_id.get(tid).cloned();
            let points = ticket.map(|t| t.points).unwrap_or(0);
            self.players[player_index].tickets.insert(
                tid.clone(),
                TicketStatus {
                    ticket_id: tid.clone(),
                    points,
                    completed: None,
                },
            );
        }

        // Scarti in fondo al mazzo in ordine
        self.ticket_draw_pile.extend(discarded.clone());

        // Aggiorna completamento biglietti: un biglietto pescato mid-game
        // potrebbe essere già completato dalle rotte esistenti (E10).
        self.update_ticket_completion(player_index);

        let mut events = vec![GameEvent::TicketsChosen {
            player_index,
            kept: keep.to_vec(),
            discarded,
        }];

        // Fine turno
        self.advance_turn(&mut events);

        Ok(events)
    }
}
