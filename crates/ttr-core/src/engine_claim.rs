//! Handler per ClaimRoute e avanzamento turno.

use std::collections::BTreeMap;

use ttr_proto::CardColor;

use crate::engine::Engine;
use crate::error::{EngineError, GamePhase};
use crate::event::FinalScore;
use crate::event::GameEvent;
use crate::scoring::compute_final_scores;
use crate::types::*;

impl Engine {
    pub(crate) fn handle_claim_route(
        &mut self,
        player_index: usize,
        route_id: &str,
        cards_discarded: &[CardColor],
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

        // Trova la rotta
        let route = self
            .routes_by_id
            .get(route_id)
            .ok_or_else(|| EngineError::RouteNotFound {
                route_id: route_id.to_string(),
            })?;

        // Già reclamata?
        if self.claimed_routes.contains_key(route_id) {
            return Err(EngineError::RouteAlreadyClaimed {
                route_id: route_id.to_string(),
            });
        }

        // Rotte doppie: controlla blocco
        if route.double {
            // Trova l'altra rotta della coppia
            let sister_id = find_sister_route(route_id, &self.routes_by_id);

            if let Some(sid) = &sister_id
                && let Some(sister_claim) = self.claimed_routes.get(sid)
            {
                if sister_claim.claimed_by == player_index {
                    return Err(EngineError::SamePlayerDouble {
                        route_id: route_id.to_string(),
                        player: player_index,
                    });
                }
                // A 2-3 giocatori, una blocca l'altra
                if self.players.len() < self.ruleset.double_route_min_players as usize {
                    return Err(EngineError::RouteBlockedDouble {
                        route_id: route_id.to_string(),
                    });
                }
            }
        }

        // Validazione carte
        let len = route.len as usize;
        if cards_discarded.len() != len {
            return Err(EngineError::InsufficientCards {
                route_id: route_id.to_string(),
                needed: len as u32,
            });
        }

        // Verifica che il giocatore abbia le carte
        let mut hand = self.players[player_index].hand.clone();
        let mut used_indices: Vec<usize> = Vec::with_capacity(len);
        for card in cards_discarded {
            let pos = hand.iter().position(|c| c == card);
            if let Some(idx) = pos {
                hand.remove(idx);
                used_indices.push(idx);
            } else {
                // Prova con locomotiva come jolly
                let loco_pos = hand.iter().position(|c| *c == CardColor::Locomotive);
                if let Some(idx) = loco_pos {
                    hand.remove(idx);
                } else {
                    return Err(EngineError::InsufficientCards {
                        route_id: route_id.to_string(),
                        needed: len as u32,
                    });
                }
            }
        }

        // Validazione colore della rotta
        let route_color = parse_route_color(&route.color)?;
        if route_color != RouteColor::Gray {
            let expected = match route_color {
                RouteColor::Colored(c) => c,
                RouteColor::Gray => unreachable!(),
            };
            for card in cards_discarded {
                if *card != expected && *card != CardColor::Locomotive {
                    return Err(EngineError::InvalidPayload {
                        reason: format!(
                            "carta {:?} non corrisponde al colore della rotta {:?}",
                            card, expected
                        ),
                    });
                }
            }
        } else {
            // Rotte grigie: tutte le carte non-loco devono essere dello stesso colore
            let non_loco_color = cards_discarded
                .iter()
                .find(|c| **c != CardColor::Locomotive)
                .copied();
            if let Some(color) = non_loco_color {
                for card in cards_discarded {
                    if *card != color && *card != CardColor::Locomotive {
                        return Err(EngineError::InvalidPayload {
                            reason: format!(
                                "rotta grigia: carte di colori diversi {:?} e {:?}",
                                color, card
                            ),
                        });
                    }
                }
            }
        }

        // Treno sufficienti?
        if self.players[player_index].trains_remaining < len as u32 {
            return Err(EngineError::InsufficientCards {
                route_id: route_id.to_string(),
                needed: len as u32,
            });
        }

        // --- Tutto valido: esegui il claim ---

        // Rimuovi carte dalla mano
        self.players[player_index].hand = hand;

        // Metti carte negli scarti
        self.train_discards.extend_from_slice(cards_discarded);

        // Calcola punti
        let len_key = len.to_string();
        let points = self
            .ruleset
            .route_points
            .get(&len_key)
            .copied()
            .unwrap_or(0);

        // Aggiorna giocatore
        self.players[player_index].trains_remaining -= len as u32;
        self.players[player_index].score += points as i32;
        self.players[player_index].claimed_routes.insert(
            route_id.to_string(),
            ClaimedRouteRecord {
                points,
                train_count: len as u32,
            },
        );

        // Registra rotta reclamata globalmente
        self.claimed_routes.insert(
            route_id.to_string(),
            ClaimedRouteInfo {
                route_id: route_id.to_string(),
                claimed_by: player_index,
            },
        );

        // Aggiorna connettività
        self.connectivity
            .add_route(player_index, &route.from, &route.to);

        // Aggiorna completamento biglietti
        self.update_ticket_completion(player_index);

        let mut events = vec![GameEvent::RouteClaimed {
            route_id: route_id.to_string(),
            player_index,
            cards_used: cards_discarded.to_vec(),
            points,
            trains_placed: len as u32,
        }];

        // Avanza turno
        self.advance_turn(&mut events);

        Ok(events)
    }

    /// Avanza il turno e gestisce le transizioni di stato.
    pub(crate) fn advance_turn(&mut self, events: &mut Vec<GameEvent>) {
        let prev_player = self.current_player_index;

        // Controlla fine partita: giocatore con ≤2 treni
        let trains_remaining = self.players[self.current_player_index].trains_remaining;
        let just_triggered = !self.last_round && trains_remaining <= LAST_ROUND_TRAIN_THRESHOLD;
        if just_triggered {
            self.last_round = true;
            self.last_round_trigger_player = Some(self.current_player_index);
            self.phase = GamePhase::LastRound;

            events.push(GameEvent::LastRoundAnnounced {
                trigger_player: self.current_player_index,
                trains_remaining,
            });
        }

        // Se siamo in LastRound e il giocatore corrente (trigger) ha appena
        // completato il PROPRIO ultimo turno (non l'azione che ha innescato
        // l'ultimo giro), la partita finisce.
        if self.last_round
            && let Some(trigger) = self.last_round_trigger_player
            && !just_triggered
            && prev_player == trigger
        {
            self.phase = GamePhase::GameOver;
            let final_events = self.compute_game_over();
            events.extend(final_events);
            return;
        }

        // Avanza al prossimo giocatore
        let next = (self.current_player_index + 1) % self.players.len();
        self.current_player_index = next;
        self.phase = if self.last_round {
            GamePhase::LastRound
        } else {
            GamePhase::PlayerTurn
        };

        events.push(GameEvent::TurnAdvanced {
            previous_player: prev_player,
            next_player: next,
        });
    }

    /// Calcola il punteggio finale e genera l'evento GameOver.
    fn compute_game_over(&mut self) -> Vec<GameEvent> {
        let bonus_points = self.ruleset.bonuses.first().map(|b| b.points).unwrap_or(10);

        // Raccogli le rotte reclamate per giocatore
        let routes_per_player: Vec<Vec<ClaimedRouteInfo>> = (0..self.players.len())
            .map(|pi| {
                self.claimed_routes
                    .values()
                    .filter(|cri| cri.claimed_by == pi)
                    .cloned()
                    .collect()
            })
            .collect();

        // Raccogli i biglietti per giocatore
        let player_tickets: Vec<BTreeMap<String, bool>> = self
            .players
            .iter()
            .map(|p| {
                p.tickets
                    .iter()
                    .map(|(tid, ts)| (tid.clone(), ts.completed.unwrap_or(false)))
                    .collect()
            })
            .collect();

        let (scores, winners, bonus) = compute_final_scores(
            &routes_per_player,
            &self.routes_by_id,
            &self.map.tickets,
            &player_tickets,
            &mut self.connectivity,
            bonus_points,
        );

        // Aggiorna punteggi giocatori
        let final_scores: Vec<FinalScore> = self
            .players
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let s = &scores[i];
                let bonus_val = if winners.contains(&i) {
                    bonus as i32
                } else {
                    0
                };
                let total = p.score + s.completed_tickets + s.failed_tickets + bonus_val;
                FinalScore {
                    player_index: i,
                    route_points: p.score,
                    ticket_completed: s.completed_tickets,
                    ticket_failed: s.failed_tickets,
                    longest_path_bonus: bonus_val,
                    total,
                }
            })
            .collect();

        vec![GameEvent::GameOver {
            final_scores,
            longest_path_players: winners,
            longest_path_bonus: bonus,
        }]
    }

    /// Aggiorna il completamento dei biglietti per un giocatore
    /// basandosi sulla connettività corrente.
    fn update_ticket_completion(&mut self, player_index: usize) {
        let ticket_ids: Vec<(String, String, String)> = self.players[player_index]
            .tickets
            .keys()
            .map(|tid| {
                let t = self.tickets_by_id.get(tid);
                let from = t.map(|t| t.from.clone()).unwrap_or_default();
                let to = t.map(|t| t.to.clone()).unwrap_or_default();
                (tid.clone(), from, to)
            })
            .collect();

        for (tid, from, to) in ticket_ids {
            let completed = self.connectivity.ticket_completed(player_index, &from, &to);
            if let Some(ts) = self.players[player_index].tickets.get_mut(&tid) {
                ts.completed = Some(completed);
            }
        }
    }
}

/// Colore di una rotta (parsed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteColor {
    Colored(CardColor),
    Gray,
}

fn parse_route_color(color_str: &str) -> Result<RouteColor, EngineError> {
    match color_str {
        "Gray" | "grey" | "gray" => Ok(RouteColor::Gray),
        "Red" | "red" => Ok(RouteColor::Colored(CardColor::Red)),
        "Orange" | "orange" => Ok(RouteColor::Colored(CardColor::Orange)),
        "Yellow" | "yellow" => Ok(RouteColor::Colored(CardColor::Yellow)),
        "Green" | "green" => Ok(RouteColor::Colored(CardColor::Green)),
        "Blue" | "blue" => Ok(RouteColor::Colored(CardColor::Blue)),
        "Purple" | "purple" => Ok(RouteColor::Colored(CardColor::Purple)),
        "Black" | "black" => Ok(RouteColor::Colored(CardColor::Black)),
        "White" | "white" => Ok(RouteColor::Colored(CardColor::White)),
        _ => Err(EngineError::InvalidRouteColor {
            color: color_str.to_string(),
        }),
    }
}

/// Trova la rotta gemella in una coppia doppia.
fn find_sister_route(route_id: &str, routes: &BTreeMap<String, Route>) -> Option<String> {
    let base = route_id
        .strip_suffix("_1")
        .or_else(|| route_id.strip_suffix("_2"))?;
    let sister_id = if route_id.ends_with("_1") {
        format!("{}_2", base)
    } else {
        format!("{}_1", base)
    };
    if routes.contains_key(&sister_id) {
        Some(sister_id)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_route_color_gray() {
        assert!(matches!(parse_route_color("Gray"), Ok(RouteColor::Gray)));
        assert!(matches!(parse_route_color("gray"), Ok(RouteColor::Gray)));
    }

    #[test]
    fn test_parse_route_color_red() {
        assert!(matches!(
            parse_route_color("Red"),
            Ok(RouteColor::Colored(CardColor::Red))
        ));
    }
}
