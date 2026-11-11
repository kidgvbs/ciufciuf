//! Handler per ClaimRoute e avanzamento turno.

use std::collections::BTreeMap;

use ttr_proto::CardColor;

use crate::engine::Engine;
use crate::error::{EngineError, GamePhase};
use crate::event::{FinalScore, GameEvent};
use crate::scoring::compute_final_scores;
use crate::types::{
    ClaimedRouteInfo, ClaimedRouteRecord, LAST_ROUND_TRAIN_THRESHOLD, RouteColor,
    find_sister_route, points_to_score,
};

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

        let route = self
            .routes_by_id
            .get(route_id)
            .ok_or_else(|| EngineError::RouteNotFound {
                route_id: route_id.to_string(),
            })?;

        if self.claimed_routes.contains_key(route_id) {
            return Err(EngineError::RouteAlreadyClaimed {
                route_id: route_id.to_string(),
            });
        }

        // Rotte doppie: la gemella già reclamata dallo stesso giocatore, o da
        // chiunque con pochi giocatori, blocca la rotta.
        if route.double
            && let Some(sister_id) = find_sister_route(route_id, &self.routes_by_id)
            && let Some(sister_claim) = self.claimed_routes.get(&sister_id)
        {
            if sister_claim.claimed_by == player_index {
                return Err(EngineError::SamePlayerDouble {
                    route_id: route_id.to_string(),
                    player: player_index,
                });
            }
            if self.players.len() < self.ruleset.double_route_min_players as usize {
                return Err(EngineError::RouteBlockedDouble {
                    route_id: route_id.to_string(),
                });
            }
        }

        let len = route.len;
        let insufficient_cards = || EngineError::InsufficientCards {
            route_id: route_id.to_string(),
            needed: len,
        };
        if cards_discarded.len() != len as usize {
            return Err(insufficient_cards());
        }

        // Le carte dichiarate escono dalla mano; se mancano, al loro posto
        // una locomotiva.
        let mut hand = self.players[player_index].hand.clone();
        let mut removed: Vec<CardColor> = Vec::with_capacity(cards_discarded.len());
        for card in cards_discarded {
            let pos = hand
                .iter()
                .position(|c| c == card)
                .or_else(|| hand.iter().position(|c| *c == CardColor::Locomotive))
                .ok_or_else(insufficient_cards)?;
            removed.push(hand.remove(pos));
        }

        let route_color =
            RouteColor::parse(&route.color).ok_or_else(|| EngineError::InvalidRouteColor {
                color: route.color.clone(),
            })?;
        match route_color {
            RouteColor::Colored(expected) => {
                for card in cards_discarded {
                    if *card != expected && *card != CardColor::Locomotive {
                        return Err(EngineError::InvalidPayload {
                            reason: format!(
                                "carta {card:?} non corrisponde al colore della rotta {expected:?}"
                            ),
                        });
                    }
                }
            }
            RouteColor::Gray => {
                // Tutte le carte non-loco devono essere dello stesso colore.
                let non_loco_color = cards_discarded
                    .iter()
                    .find(|c| **c != CardColor::Locomotive)
                    .copied();
                if let Some(color) = non_loco_color {
                    for card in cards_discarded {
                        if *card != color && *card != CardColor::Locomotive {
                            return Err(EngineError::InvalidPayload {
                                reason: format!(
                                    "rotta grigia: carte di colori diversi {color:?} e {card:?}"
                                ),
                            });
                        }
                    }
                }
            }
        }

        if self.players[player_index].trains_remaining < len {
            return Err(insufficient_cards());
        }

        // --- Tutto valido: esegui il claim ---

        self.players[player_index].hand = hand;
        self.train_discards.extend_from_slice(&removed);

        let points = self
            .ruleset
            .route_points
            .get(&len.to_string())
            .copied()
            .unwrap_or(0);

        let player = &mut self.players[player_index];
        player.trains_remaining -= len;
        player.score = player.score.saturating_add(points_to_score(points));
        player.claimed_routes.insert(
            route_id.to_string(),
            ClaimedRouteRecord {
                points,
                train_count: len,
            },
        );

        self.claimed_routes.insert(
            route_id.to_string(),
            ClaimedRouteInfo {
                route_id: route_id.to_string(),
                claimed_by: player_index,
            },
        );

        self.connectivity
            .add_route(player_index, &route.from, &route.to);
        self.update_ticket_completion(player_index);

        let mut events = vec![GameEvent::RouteClaimed {
            route_id: route_id.to_string(),
            player_index,
            cards_used: removed,
            points,
            trains_placed: len,
        }];

        self.advance_turn(&mut events);

        Ok(events)
    }

    /// Avanza il turno e gestisce le transizioni di stato.
    pub(crate) fn advance_turn(&mut self, events: &mut Vec<GameEvent>) {
        let prev_player = self.current_player_index;

        // L'ultimo giro scatta con ≤2 treni rimasti al giocatore corrente
        // oppure, per D28, quando nessun giocatore può più reclamare rotte.
        let trains_remaining = self.players[self.current_player_index].trains_remaining;
        let just_triggered = !self.last_round
            && (trains_remaining <= LAST_ROUND_TRAIN_THRESHOLD
                || !self.any_route_claimable_by_anyone());
        if just_triggered {
            self.last_round = true;
            self.last_round_trigger_player = Some(self.current_player_index);
            self.phase = GamePhase::LastRound;

            events.push(GameEvent::LastRoundAnnounced {
                trigger_player: self.current_player_index,
                trains_remaining,
            });
        }

        // La partita finisce quando il giocatore che ha innescato l'ultimo giro
        // completa il proprio turno successivo, non l'azione che l'ha innescato.
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
        let routes_per_player: Vec<Vec<ClaimedRouteInfo>> = (0..self.players.len())
            .map(|pi| {
                self.claimed_routes
                    .values()
                    .filter(|cri| cri.claimed_by == pi)
                    .cloned()
                    .collect()
            })
            .collect();

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
            self.longest_path_bonus_points,
        );

        self.longest_path_inexact = scores
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.longest_path_exact)
            .map(|(i, _)| i)
            .collect();

        let mut final_scores: Vec<FinalScore> = self
            .players
            .iter()
            .zip(&scores)
            .enumerate()
            .map(|(i, (p, s))| {
                let bonus_val = if winners.contains(&i) {
                    points_to_score(bonus)
                } else {
                    0
                };
                let total = p
                    .score
                    .saturating_add(s.completed_tickets)
                    .saturating_add(s.failed_tickets)
                    .saturating_add(bonus_val);
                FinalScore {
                    player_index: i,
                    route_points: p.score,
                    ticket_completed: s.completed_tickets,
                    ticket_failed: s.failed_tickets,
                    longest_path_bonus: bonus_val,
                    total,
                    winner: false,
                }
            })
            .collect();

        // Vincitore: totale più alto; spareggio sul numero di biglietti
        // completati, poi sul bonus del percorso più lungo. Pari merito
        // residui restano tutti vincitori.
        let rank_key = |fs: &FinalScore| {
            (
                fs.total,
                scores[fs.player_index].completed_count,
                fs.longest_path_bonus,
            )
        };
        if let Some(best) = final_scores.iter().map(rank_key).max() {
            for fs in &mut final_scores {
                fs.winner = rank_key(fs) == best;
            }
        }

        vec![GameEvent::GameOver {
            final_scores,
            longest_path_players: winners,
            longest_path_bonus: bonus,
        }]
    }

    /// Aggiorna il completamento dei biglietti per un giocatore
    /// basandosi sulla connettività corrente.
    pub(crate) fn update_ticket_completion(&mut self, player_index: usize) {
        let endpoints: Vec<(String, String, String)> = self.players[player_index]
            .tickets
            .keys()
            .filter_map(|tid| {
                let t = self.tickets_by_id.get(tid)?;
                Some((tid.clone(), t.from.clone(), t.to.clone()))
            })
            .collect();

        for (tid, from, to) in endpoints {
            let completed = self.connectivity.ticket_completed(player_index, &from, &to);
            if let Some(ts) = self.players[player_index].tickets.get_mut(&tid) {
                ts.completed = Some(completed);
            }
        }
    }
}
