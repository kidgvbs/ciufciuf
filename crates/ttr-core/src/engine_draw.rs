//! Handler per DrawTrainCard.

use crate::action::DrawSource;
use crate::engine::Engine;
use crate::engine_deck::{draw_from_deck, draw_from_face_up};
use crate::error::{EngineError, GamePhase};
use crate::event::GameEvent;

impl Engine {
    pub(crate) fn handle_draw_train_card(
        &mut self,
        player_index: usize,
        source: DrawSource,
    ) -> Result<Vec<GameEvent>, EngineError> {
        // Validazione stato
        match self.phase {
            GamePhase::PlayerTurn | GamePhase::LastRound => {}
            GamePhase::SecondCardDraw => {}
            _ => return Err(EngineError::WrongState { state: self.phase }),
        }

        // Validazione turno
        if player_index != self.current_player_index {
            return Err(EngineError::NotYourTurn {
                player_index,
                current_player: self.current_player_index,
            });
        }

        let mut events = Vec::new();

        match source {
            DrawSource::Deck => {
                let card = draw_from_deck(
                    &mut self.draw_pile,
                    &mut self.train_discards,
                    &mut self.rng,
                    &self.ruleset.deck,
                )
                .ok_or(EngineError::DeckEmpty)?;

                let is_loco = card == ttr_proto::CardColor::Locomotive;
                self.players[player_index].hand.push(card);
                events.push(GameEvent::CardDrawnFromDeck { player_index, card });

                if self.phase == GamePhase::SecondCardDraw {
                    // Seconda pescata: fine del turno di pesca
                    events.push(GameEvent::DrawTurnComplete { player_index });
                    self.advance_turn(&mut events);
                } else if is_loco {
                    // Loco dal mazzo: pescata normale, si può continuare
                    // ma se era la prima pescata di PlayerTurn, si va a SecondCardDraw?
                    // No! La regola dice: loco dal mazzo coperto = pescata normale.
                    // Si può prendere una seconda carta (va a SecondCardDraw).
                    self.phase = GamePhase::SecondCardDraw;
                } else {
                    self.phase = GamePhase::SecondCardDraw;
                }
            }
            DrawSource::FaceUp { index } => {
                if index >= self.face_up.len() {
                    return Err(EngineError::FaceUpIndexOutOfRange {
                        index,
                        max: self.face_up.len().saturating_sub(1),
                    });
                }
                if self.face_up[index].is_none() {
                    return Err(EngineError::CardNotAvailable { index });
                }

                let card_color = self.face_up[index].unwrap();
                let is_loco = card_color == ttr_proto::CardColor::Locomotive;

                if self.phase == GamePhase::SecondCardDraw && is_loco {
                    return Err(EngineError::LocomotiveAsSecondCard);
                }

                let card = draw_from_face_up(
                    index,
                    &mut self.face_up,
                    &mut self.draw_pile,
                    &mut self.train_discards,
                    &mut self.rng,
                    &self.ruleset.deck,
                )
                .ok_or(EngineError::CardNotAvailable { index })?;

                self.players[player_index].hand.push(card);
                events.push(GameEvent::CardDrawnFromFaceUp {
                    player_index,
                    index,
                    card,
                });

                // Registra cosa è apparso al posto della carta pescata
                if let Some(new_card) = self.face_up[index] {
                    events.push(GameEvent::FaceUpRevealed {
                        index,
                        card: new_card,
                    });
                }

                if self.phase == GamePhase::SecondCardDraw {
                    events.push(GameEvent::DrawTurnComplete { player_index });
                    self.advance_turn(&mut events);
                } else if is_loco {
                    // Loco scoperta: pescata intera, fine turno
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
