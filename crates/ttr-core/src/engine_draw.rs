//! Handler per DrawTrainCard.

use ttr_proto::CardColor;

use crate::action::DrawSource;
use crate::engine::Engine;
use crate::engine_deck::{
    DeckDraw, check_and_reset_loco_threshold, draw_from_deck, draw_from_face_up,
};
use crate::error::{EngineError, GamePhase};
use crate::event::GameEvent;

impl Engine {
    pub(crate) fn handle_draw_train_card(
        &mut self,
        player_index: usize,
        source: DrawSource,
    ) -> Result<Vec<GameEvent>, EngineError> {
        match self.phase {
            GamePhase::PlayerTurn | GamePhase::LastRound | GamePhase::SecondCardDraw => {}
            _ => return Err(EngineError::WrongState { state: self.phase }),
        }

        if player_index != self.current_player_index {
            return Err(EngineError::NotYourTurn {
                player_index,
                current_player: self.current_player_index,
            });
        }

        let mut events = Vec::new();

        match source {
            DrawSource::Deck => {
                let DeckDraw { card, rebuilt } = draw_from_deck(
                    &mut self.draw_pile,
                    &mut self.train_discards,
                    &mut self.rng,
                    &self.ruleset.deck,
                    &mut events,
                )
                .ok_or(EngineError::DeckEmpty)?;

                self.players[player_index].hand.push(card);
                events.push(GameEvent::CardDrawnFromDeck { player_index, card });

                // Il mazzo nuovo D27 può sbloccare un mercato rimasto con ≥3
                // locomotive dopo un reset senza uscita (D26): un solo controllo,
                // mai a ogni pesca, che altrimenti rimescolerebbe le scoperte.
                if rebuilt && self.ruleset.face_up_reset_enabled() {
                    check_and_reset_loco_threshold(
                        &mut self.face_up,
                        &mut self.draw_pile,
                        &mut self.train_discards,
                        &mut self.rng,
                        &self.ruleset.deck,
                        &mut events,
                    );
                }

                if self.phase == GamePhase::SecondCardDraw {
                    events.push(GameEvent::DrawTurnComplete { player_index });
                    self.advance_turn(&mut events);
                } else {
                    // Anche una locomotiva dal mazzo coperto è una pescata normale.
                    self.phase = GamePhase::SecondCardDraw;
                }
            }
            DrawSource::FaceUp { index } => {
                let Some(slot) = self.face_up.get(index) else {
                    return Err(EngineError::FaceUpIndexOutOfRange {
                        index,
                        max: self.face_up.len().saturating_sub(1),
                    });
                };
                let Some(slot_card) = *slot else {
                    return Err(EngineError::CardNotAvailable { index });
                };
                let is_loco = slot_card == CardColor::Locomotive;

                let free_draw = self.ruleset.free_locomotive_draw();
                if self.phase == GamePhase::SecondCardDraw && is_loco && !free_draw {
                    return Err(EngineError::LocomotiveAsSecondCard);
                }

                let mut replenish_events = Vec::new();
                let card = draw_from_face_up(
                    index,
                    &mut self.face_up,
                    &mut self.draw_pile,
                    &mut self.train_discards,
                    &mut self.rng,
                    &self.ruleset.deck,
                    &mut replenish_events,
                )
                .ok_or(EngineError::CardNotAvailable { index })?;
                if self.ruleset.face_up_reset_enabled() {
                    check_and_reset_loco_threshold(
                        &mut self.face_up,
                        &mut self.draw_pile,
                        &mut self.train_discards,
                        &mut self.rng,
                        &self.ruleset.deck,
                        &mut replenish_events,
                    );
                }

                self.players[player_index].hand.push(card);
                events.push(GameEvent::CardDrawnFromFaceUp {
                    player_index,
                    index,
                    card,
                });
                events.extend(replenish_events);

                if self.phase == GamePhase::SecondCardDraw || (is_loco && !free_draw) {
                    // Seconda carta, oppure locomotiva scoperta (se non `freeDraw`):
                    // pescata intera.
                    events.push(GameEvent::DrawTurnComplete { player_index });
                    self.advance_turn(&mut events);
                } else {
                    self.phase = GamePhase::SecondCardDraw;
                }
            }
        }

        Ok(events)
    }
}
