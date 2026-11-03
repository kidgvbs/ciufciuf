//! Calcolo delle mosse legali per un giocatore (piano §9.4).

use std::collections::{BTreeMap, HashMap};

use ttr_proto::CardColor;

use crate::engine::Engine;
use crate::error::GamePhase;
use crate::types::*;

/// Mosse legali per un giocatore.
#[derive(Debug, Clone)]
pub struct LegalMoves {
    pub routes: Vec<RouteId>,
    pub draw_face_up: Vec<usize>,
    pub can_draw_deck: bool,
    pub can_draw_tickets: bool,
}

impl Engine {
    pub fn legal_moves(&self, player_index: usize) -> LegalMoves {
        if player_index >= self.players.len() {
            return LegalMoves {
                routes: vec![],
                draw_face_up: vec![],
                can_draw_deck: false,
                can_draw_tickets: false,
            };
        }

        let player = &self.players[player_index];
        let hand = &player.hand;

        let claimable = self.claimable_routes(player_index, hand);

        let drawable_indices: Vec<usize> = self
            .face_up
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_some())
            .map(|(i, _)| i)
            .collect();

        let (draw_face_up, can_draw_deck) = match self.phase {
            GamePhase::SecondCardDraw => {
                let filtered: Vec<usize> = drawable_indices
                    .into_iter()
                    .filter(|&i| !matches!(self.face_up[i], Some(CardColor::Locomotive)))
                    .collect();
                (filtered, self.can_draw_from_deck())
            }
            _ => (drawable_indices, self.can_draw_from_deck()),
        };

        let can_draw_tickets = self.can_draw_tickets();

        LegalMoves {
            routes: claimable,
            draw_face_up,
            can_draw_deck,
            can_draw_tickets,
        }
    }

    fn claimable_routes(&self, player_index: usize, hand: &[CardColor]) -> Vec<RouteId> {
        let mut result = Vec::new();
        let loco_count = hand.iter().filter(|c| **c == CardColor::Locomotive).count();
        let color_counts = count_colors(hand);

        for route in &self.map.routes {
            let rid = &route.id;

            if self.claimed_routes.contains_key(rid) {
                continue;
            }
            if self.players[player_index].trains_remaining < route.len {
                continue;
            }

            if route.double {
                let sister_id = find_sister_for_legal(rid, &self.routes_by_id);
                if let Some(sid) = &sister_id
                    && let Some(cri) = self.claimed_routes.get(sid)
                {
                    if cri.claimed_by == player_index {
                        continue;
                    }
                    if self.players.len() < self.ruleset.double_route_min_players as usize {
                        continue;
                    }
                }
            }

            let can_pay = self.can_pay_route(
                hand,
                &route.color,
                route.len as usize,
                loco_count,
                &color_counts,
            );
            if can_pay {
                result.push(rid.clone());
            }
        }
        result
    }

    fn can_pay_route(
        &self,
        hand: &[CardColor],
        route_color: &str,
        len: usize,
        loco_count: usize,
        color_counts: &HashMap<CardColor, usize>,
    ) -> bool {
        if hand.len() < len {
            return false;
        }
        if route_color.eq_ignore_ascii_case("Gray") {
            for color in color_counts.keys() {
                if *color == CardColor::Locomotive {
                    continue;
                }
                let colored = color_counts.get(color).copied().unwrap_or(0);
                if colored + loco_count >= len {
                    return true;
                }
            }
            loco_count >= len
        } else if let Some(card_color) = parse_route_color_internal(route_color) {
            let colored = color_counts.get(&card_color).copied().unwrap_or(0);
            colored + loco_count >= len
        } else {
            false
        }
    }

    fn can_draw_from_deck(&self) -> bool {
        !self.draw_pile.is_empty() || !self.train_discards.is_empty()
    }

    fn can_draw_tickets(&self) -> bool {
        let deck_name = &self.ruleset.ticket_draw.deck;
        self.ticket_draw_pile.iter().any(|tid| {
            self.tickets_by_id
                .get(tid)
                .map(|t| &t.deck == deck_name)
                .unwrap_or(false)
        })
    }
}

fn count_colors(hand: &[CardColor]) -> HashMap<CardColor, usize> {
    let mut counts = HashMap::new();
    for card in hand {
        *counts.entry(*card).or_default() += 1;
    }
    counts
}

fn find_sister_for_legal(route_id: &str, routes: &BTreeMap<String, Route>) -> Option<String> {
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

fn parse_route_color_internal(color_str: &str) -> Option<CardColor> {
    match color_str {
        "Gray" | "grey" | "gray" => None,
        "Red" | "red" => Some(CardColor::Red),
        "Orange" | "orange" => Some(CardColor::Orange),
        "Yellow" | "yellow" => Some(CardColor::Yellow),
        "Green" | "green" => Some(CardColor::Green),
        "Blue" | "blue" => Some(CardColor::Blue),
        "Purple" | "purple" => Some(CardColor::Purple),
        "Black" | "black" => Some(CardColor::Black),
        "White" | "white" => Some(CardColor::White),
        _ => None,
    }
}
