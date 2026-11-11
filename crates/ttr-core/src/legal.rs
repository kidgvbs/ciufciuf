//! Calcolo delle mosse legali per un giocatore (piano §9.4).

use std::collections::BTreeMap;

use ttr_proto::CardColor;

use crate::engine::Engine;
use crate::error::GamePhase;
use crate::types::{CityId, Route, RouteColor, RouteId, find_sister_route};

/// Mosse legali per un giocatore.
#[derive(Debug, Clone)]
pub struct LegalMoves {
    pub routes: Vec<RouteId>,
    pub draw_face_up: Vec<usize>,
    pub can_draw_deck: bool,
    pub can_draw_tickets: bool,
    /// Città dove si può costruire ora la prossima stazione, per id crescente.
    pub station_cities: Vec<CityId>,
    /// Costo in carte della prossima stazione; 0 se non ce ne sono più.
    pub station_cost: u32,
    /// Decisione sul tunnel pendente (solo in `TunnelResolution`).
    pub tunnel: Option<TunnelMoves>,
}

/// Decisione sul tunnel rivelato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelMoves {
    pub route_id: RouteId,
    pub extra_cost: u32,
    /// Colore delle carte extra oltre alle locomotive (`Locomotive` = solo locomotive).
    pub color: CardColor,
    /// La mano corrente copre il costo extra.
    pub can_pay: bool,
}

impl LegalMoves {
    /// Nessuna mossa valida (fasi dove non si può agire).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            routes: vec![],
            draw_face_up: vec![],
            can_draw_deck: false,
            can_draw_tickets: false,
            station_cities: vec![],
            station_cost: 0,
            tunnel: None,
        }
    }
}

impl Engine {
    /// Mosse legali filtrate per fase: mai una mossa che il motore
    /// rifiuterebbe con `WrongState` o `NotYourTurn`.
    #[must_use]
    pub fn legal_moves(&self, player_index: usize) -> LegalMoves {
        if player_index >= self.players.len() || player_index != self.current_player_index {
            return LegalMoves::empty();
        }

        match self.phase {
            GamePhase::PlayerTurn | GamePhase::LastRound => {
                self.legal_moves_player_turn(player_index)
            }
            GamePhase::SecondCardDraw => self.legal_moves_second_draw(),
            GamePhase::TunnelResolution => self.legal_moves_tunnel(player_index),
            GamePhase::InitialDraft | GamePhase::TicketChoice | GamePhase::GameOver => {
                LegalMoves::empty()
            }
        }
    }

    fn legal_moves_player_turn(&self, player_index: usize) -> LegalMoves {
        let hand = &self.players[player_index].hand;
        let claimable = self.claimable_routes(player_index, hand);

        let draw_face_up: Vec<usize> = self
            .face_up
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_some())
            .map(|(i, _)| i)
            .collect();

        LegalMoves {
            routes: claimable,
            draw_face_up,
            can_draw_deck: self.can_draw_from_deck(),
            can_draw_tickets: self.can_draw_tickets(),
            station_cities: self.station_cities(player_index),
            station_cost: self.next_station_cost(player_index).unwrap_or(0),
            tunnel: None,
        }
    }

    fn legal_moves_second_draw(&self) -> LegalMoves {
        let draw_face_up: Vec<usize> = self
            .face_up
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_some())
            .filter(|(_, c)| !matches!(c, Some(CardColor::Locomotive)))
            .map(|(i, _)| i)
            .collect();

        LegalMoves {
            routes: vec![],
            draw_face_up,
            can_draw_deck: self.can_draw_from_deck(),
            can_draw_tickets: false,
            station_cities: vec![],
            station_cost: 0,
            tunnel: None,
        }
    }

    fn legal_moves_tunnel(&self, player_index: usize) -> LegalMoves {
        let tunnel = self.pending_tunnel.as_ref().map(|pending| {
            let hand = &self.players[player_index].hand;
            let usable = hand
                .iter()
                .filter(|c| **c == CardColor::Locomotive || **c == pending.color)
                .count();
            TunnelMoves {
                route_id: pending.route_id.clone(),
                extra_cost: pending.extra,
                color: pending.color,
                can_pay: usable >= pending.extra as usize,
            }
        });
        LegalMoves {
            tunnel,
            ..LegalMoves::empty()
        }
    }

    /// Città dove il giocatore può costruire ora la prossima stazione.
    fn station_cities(&self, player_index: usize) -> Vec<CityId> {
        let Some(cost) = self.next_station_cost(player_index) else {
            return Vec::new();
        };
        let hand = &self.players[player_index].hand;
        let loco_count = hand.iter().filter(|c| **c == CardColor::Locomotive).count();
        let cost = cost as usize;
        let payable = loco_count >= cost
            || count_colors(hand).iter().any(|(color, count)| {
                *color != CardColor::Locomotive && count + loco_count >= cost
            });
        if !payable {
            return Vec::new();
        }
        let mut cities: Vec<CityId> = self
            .map
            .cities
            .iter()
            .filter(|c| !self.station_owners.contains_key(&c.id))
            .map(|c| c.id.clone())
            .collect();
        cities.sort();
        cities
    }

    fn claimable_routes(&self, player_index: usize, hand: &[CardColor]) -> Vec<RouteId> {
        let mut result = Vec::new();
        let loco_count = hand.iter().filter(|c| **c == CardColor::Locomotive).count();
        let color_counts = count_colors(hand);

        for route in &self.map.routes {
            if !self.route_open_for(player_index, route) {
                continue;
            }
            let rid = &route.id;

            let can_pay = Self::can_pay_route(
                &route.color,
                route.len as usize,
                route.ferries as usize,
                hand.len(),
                loco_count,
                &color_counts,
            );
            if can_pay {
                result.push(rid.clone());
            }
        }
        result
    }

    /// Vero se il giocatore può reclamare la rotta, a prescindere dalle carte.
    fn route_open_for(&self, player_index: usize, route: &Route) -> bool {
        let rid = &route.id;

        if self.claimed_routes.contains_key(rid) {
            return false;
        }
        if self.players[player_index].trains_remaining < route.len {
            return false;
        }

        if route.double {
            let sister_id = find_sister_route(rid, &self.routes_by_id);
            if let Some(sid) = &sister_id
                && let Some(cri) = self.claimed_routes.get(sid)
            {
                if cri.claimed_by == player_index {
                    return false;
                }
                if self.players.len() < self.ruleset.double_route_min_players as usize {
                    return false;
                }
            }
        }
        true
    }

    /// Vero se almeno un giocatore ha ancora una rotta aperta, a prescindere
    /// dalle carte (D28).
    pub(crate) fn any_route_claimable_by_anyone(&self) -> bool {
        (0..self.players.len()).any(|player_index| {
            self.map
                .routes
                .iter()
                .any(|route| self.route_open_for(player_index, route))
        })
    }

    fn can_pay_route(
        route_color: &str,
        len: usize,
        ferries: usize,
        hand_len: usize,
        loco_count: usize,
        color_counts: &BTreeMap<CardColor, usize>,
    ) -> bool {
        if hand_len < len || loco_count < ferries {
            return false;
        }
        match RouteColor::parse(route_color) {
            Some(RouteColor::Gray) => {
                color_counts.iter().any(|(color, count)| {
                    *color != CardColor::Locomotive && count + loco_count >= len
                }) || loco_count >= len
            }
            Some(RouteColor::Colored(card_color)) => {
                color_counts.get(&card_color).copied().unwrap_or(0) + loco_count >= len
            }
            None => false,
        }
    }

    fn can_draw_from_deck(&self) -> bool {
        // Con mazzo e scarti vuoti la pesca crea un mazzo nuovo (D27).
        !self.draw_pile.is_empty()
            || !self.train_discards.is_empty()
            || self.ruleset.deck.per_color > 0
            || self.ruleset.deck.locomotives > 0
    }

    fn can_draw_tickets(&self) -> bool {
        let deck_name = &self.ruleset.ticket_draw.deck;
        let available = self
            .ticket_draw_pile
            .iter()
            .filter(|tid| {
                self.tickets_by_id
                    .get(*tid)
                    .is_some_and(|t| &t.deck == deck_name)
            })
            .count();
        available >= self.min_tickets_to_draw()
    }
}

fn count_colors(hand: &[CardColor]) -> BTreeMap<CardColor, usize> {
    let mut counts = BTreeMap::new();
    for card in hand {
        *counts.entry(*card).or_default() += 1;
    }
    counts
}
