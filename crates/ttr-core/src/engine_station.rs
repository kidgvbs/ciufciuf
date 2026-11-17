//! Stazioni (Europe): costruzione e rotte avversarie prese in prestito.

use ttr_proto::CardColor;

use crate::engine::Engine;
use crate::error::{EngineError, GamePhase};
use crate::event::GameEvent;
use crate::scoring::{StationCandidates, StationOption, route_node};
use crate::types::points_to_score;

impl Engine {
    /// Costo in carte della prossima stazione del giocatore; `None` se il ruleset non
    /// ha stazioni o le ha già costruite tutte.
    pub(crate) fn next_station_cost(&self, player_index: usize) -> Option<u32> {
        let config = self.ruleset.stations.as_ref()?;
        let built = self.players.get(player_index)?.stations.len();
        if built >= config.count as usize {
            return None;
        }
        config.costs.get(built).copied()
    }

    /// Punti delle stazioni non costruite del giocatore.
    pub(crate) fn unbuilt_station_points(&self, player_index: usize) -> i32 {
        let per_station = self
            .ruleset
            .stations
            .as_ref()
            .map_or(0, |s| s.unbuilt_points);
        points_to_score(
            self.stations_remaining(player_index)
                .saturating_mul(per_station),
        )
    }

    /// Stazioni del giocatore in ordine di costruzione, ciascuna con le rotte
    /// reclamate da avversari incidenti alla sua città, per id crescente (D32).
    pub(crate) fn station_candidates(&self, player_index: usize) -> Vec<StationCandidates> {
        self.players[player_index]
            .stations
            .iter()
            .map(|city| StationCandidates {
                city: city.clone(),
                options: self
                    .claimed_routes
                    .values()
                    .filter(|claim| claim.claimed_by != player_index)
                    .filter_map(|claim| {
                        let route = self.routes_by_id.get(&claim.route_id)?;
                        let other_city = if route.from == *city {
                            &route.to
                        } else if route.to == *city {
                            &route.from
                        } else {
                            return None;
                        };
                        Some(StationOption {
                            route_id: route.id.clone(),
                            other_city: route_node(&self.country_ids, &route.id, other_city),
                        })
                    })
                    .collect(),
            })
            .collect()
    }

    pub(crate) fn handle_build_station(
        &mut self,
        player_index: usize,
        city_id: &str,
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
        let Some(cost) = self.next_station_cost(player_index) else {
            return Err(EngineError::NoStationsLeft);
        };
        if !self.map.cities.iter().any(|c| c.id == city_id) {
            return Err(EngineError::CityNotFound {
                city_id: city_id.to_string(),
            });
        }
        if self.station_owners.contains_key(city_id) {
            return Err(EngineError::StationCityOccupied {
                city_id: city_id.to_string(),
            });
        }

        let insufficient = || EngineError::InsufficientStationCards { needed: cost };
        if cards_discarded.len() != cost as usize {
            return Err(insufficient());
        }

        // Le carte dichiarate escono dalla mano; se mancano, al loro posto una
        // locomotiva. I colori si validano sulle carte realmente tolte.
        let mut hand = self.players[player_index].hand.clone();
        let mut removed: Vec<CardColor> = Vec::with_capacity(cards_discarded.len());
        for card in cards_discarded {
            let pos = hand
                .iter()
                .position(|c| c == card)
                .or_else(|| hand.iter().position(|c| *c == CardColor::Locomotive))
                .ok_or_else(insufficient)?;
            removed.push(hand.remove(pos));
        }
        let mut colors = removed.iter().filter(|c| **c != CardColor::Locomotive);
        if let Some(first) = colors.next()
            && let Some(other) = colors.find(|c| *c != first)
        {
            return Err(EngineError::InvalidPayload {
                reason: format!("stazione: carte di colori diversi {first:?} e {other:?}"),
            });
        }

        // --- Tutto valido: costruisci ---

        self.players[player_index].hand = hand;
        self.train_discards.extend_from_slice(&removed);
        self.players[player_index]
            .stations
            .push(city_id.to_string());
        self.station_owners
            .insert(city_id.to_string(), player_index);
        self.update_ticket_completion(player_index);

        let mut events = vec![GameEvent::StationBuilt {
            player_index,
            city_id: city_id.to_string(),
            cards_used: removed,
        }];
        self.advance_turn(&mut events);
        Ok(events)
    }
}
