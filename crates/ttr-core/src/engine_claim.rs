//! Handler per ClaimRoute e avanzamento turno.

use std::collections::{BTreeMap, BTreeSet};

use ttr_proto::CardColor;

use crate::engine::{Engine, LaneBlock};
use crate::engine_deck::reveal_from_deck;
use crate::error::{EngineError, GamePhase};
use crate::event::{FinalScore, GameEvent};
use crate::payment::{PaymentError, PaymentSpec, check_declared_colors};
use crate::scoring::{
    LongestPathRules, MandalaGraph, RegionStanding, TicketSpec, UnionFind, compute_final_scores,
    evaluate_tickets, globetrotter_winners, largest_network_size, largest_network_winners,
    mandala_points, passenger_majority_points,
};
use crate::types::{
    CityId, ClaimedRouteInfo, ClaimedRouteRecord, LARGEST_NETWORK_BONUS_TYPE,
    LAST_ROUND_TRAIN_THRESHOLD, PendingTunnel, Route, TicketDef, TieBreaker, points_to_score,
};

impl Engine {
    pub(crate) fn handle_claim_route(
        &mut self,
        player_index: usize,
        route_id: &str,
        cards_discarded: &[CardColor],
        passengers: &BTreeMap<CityId, String>,
        ferry_cards: u32,
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

        let (route, info) = self
            .routes_by_id
            .get(route_id)
            .zip(self.route_info.get(route_id))
            .ok_or_else(|| EngineError::RouteNotFound {
                route_id: route_id.to_string(),
            })?;

        if self.claimed_routes.contains_key(route_id) {
            return Err(EngineError::RouteAlreadyClaimed {
                route_id: route_id.to_string(),
            });
        }

        // Binari paralleli: un solo binario per giocatore e, con pochi giocatori, un
        // binario preso blocca gli altri del gruppo (salvo `alwaysOpen`).
        if let Some(block) = self.lane_block(player_index, info) {
            return Err(match block {
                LaneBlock::SamePlayer => EngineError::SamePlayerDouble {
                    route_id: route_id.to_string(),
                    player: player_index,
                },
                LaneBlock::Blocked => EngineError::RouteBlockedDouble {
                    route_id: route_id.to_string(),
                },
            });
        }
        let route_color = info.color;
        let route = route.clone();

        let len = route.len;
        let insufficient_cards = || EngineError::InsufficientCards {
            route_id: route_id.to_string(),
            needed: len,
        };
        if ferry_cards > 0 {
            if self.ruleset.ferry_cards.is_none() {
                return Err(EngineError::FerryCardsDisabled);
            }
            if ferry_cards > self.players[player_index].ferry_cards {
                return Err(insufficient_cards());
            }
        }
        let spec = PaymentSpec::new(&route, route_color, &self.ruleset)
            .with_ferry_cards(ferry_cards as usize);
        // Senza gruppi di carte qualsiasi le carte dichiarate sono `len` meno le onde coperte
        // dalle carte Traghetto; con i gruppi la lista può essere più lunga e decide
        // `spec.verify`.
        if !spec.has_groups() {
            let Some(range) = spec.regular_card_range() else {
                return Err(EngineError::InvalidPayload {
                    reason: "carte Traghetto non ammesse su questa rotta o più delle onde da \
                             coprire"
                        .to_string(),
                });
            };
            if !range.contains(&cards_discarded.len()) {
                return Err(insufficient_cards());
            }
        }

        // Le carte dichiarate escono dalla mano; se mancano, al loro posto una
        // locomotiva (solo dove una locomotiva da sola è ammessa).
        let mut hand = self.players[player_index].hand.clone();
        let mut removed: Vec<CardColor> = Vec::with_capacity(cards_discarded.len());
        for card in cards_discarded {
            let pos = hand
                .iter()
                .position(|c| c == card)
                .or_else(|| {
                    spec.single_loco()
                        .then(|| hand.iter().position(|c| *c == CardColor::Locomotive))
                        .flatten()
                })
                .ok_or_else(insufficient_cards)?;
            removed.push(hand.remove(pos));
        }

        // Senza gruppi le carte dichiarate devono rispettare il colore della rotta.
        if !spec.has_groups() {
            check_declared_colors(route_color, cards_discarded)
                .map_err(|reason| EngineError::InvalidPayload { reason })?;
        }

        // Tutte le carte tolte si assegnano esattamente agli spazi della rotta.
        spec.verify(&removed).map_err(|err| match err {
            PaymentError::NotEnough => insufficient_cards(),
            PaymentError::Invalid(reason) => EngineError::InvalidPayload { reason },
        })?;

        if self.players[player_index].trains_remaining < route.train_cost() {
            return Err(insufficient_cards());
        }
        let taken = self.resolve_passengers(&route, passengers)?;

        // --- Tutto valido: esegui il claim ---

        if route.tunnel {
            return Ok(self.start_tunnel(player_index, &route, hand, removed, taken));
        }

        let player = &mut self.players[player_index];
        player.hand = hand;
        // Le carte Traghetto usate tornano nel mazzetto.
        player.ferry_cards -= ferry_cards;
        self.train_discards.extend_from_slice(&removed);

        let mut events = Vec::new();
        self.complete_claim(player_index, &route, removed, &taken, &mut events);
        Ok(events)
    }

    /// Completa una rotta pagata: punti, treni, connettività, biglietti,
    /// `RouteClaimed` e avanzamento del turno. Le carte sono già state scartate.
    fn complete_claim(
        &mut self,
        player_index: usize,
        route: &Route,
        cards_used: Vec<CardColor>,
        taken: &BTreeMap<CityId, String>,
        events: &mut Vec<GameEvent>,
    ) {
        let len = route.len;
        let points = self
            .ruleset
            .route_points
            .get(&len.to_string())
            .copied()
            .unwrap_or(0);
        // Le X di montagna: treni scartati dalla riserva, punti subito e a parte.
        let mountain_points = points_to_score(self.ruleset.mountain_points(route));

        let player = &mut self.players[player_index];
        player.trains_remaining -= route.train_cost();
        player.score = player
            .score
            .saturating_add(points_to_score(points))
            .saturating_add(mountain_points);
        player.mountain_points = player.mountain_points.saturating_add(mountain_points);
        player.claimed_routes.insert(
            route.id.clone(),
            ClaimedRouteRecord {
                points,
                train_count: len,
            },
        );

        self.claimed_routes.insert(
            route.id.clone(),
            ClaimedRouteInfo {
                route_id: route.id.clone(),
                claimed_by: player_index,
            },
        );

        // Prima della presa: le reti già premiate che la rotta unisce o estende.
        let rewarded = self.rewarded_countries(player_index, route);
        self.connectivity
            .add_route(player_index, &route.id, &route.from, &route.to);
        self.update_ticket_completion(player_index);
        // Una rotta avversaria nuova può completare i biglietti di chi ha stazioni.
        for other in 0..self.players.len() {
            if other != player_index && !self.players[other].stations.is_empty() {
                self.update_ticket_completion(other);
            }
        }

        events.push(GameEvent::RouteClaimed {
            route_id: route.id.clone(),
            player_index,
            cards_used,
            // Punti totali del claim: rotta più treni di montagna.
            points: points.saturating_add(self.ruleset.mountain_points(route)),
            trains_placed: len,
        });

        self.take_passengers(player_index, route, taken, events);
        self.take_country_cards(player_index, route, &rewarded, events);
        self.advance_turn(events);
    }

    /// Passeggeri da prendere con la rotta: da ogni estremo con passeggeri uno solo (il
    /// colore è scelto con `picks` quando ce n'è più di uno). Non modifica lo stato.
    fn resolve_passengers(
        &self,
        route: &Route,
        picks: &BTreeMap<CityId, String>,
    ) -> Result<BTreeMap<CityId, String>, EngineError> {
        let mut taken = BTreeMap::new();
        if self.ruleset.passengers.is_none() {
            return Ok(taken);
        }
        if let Some(city) = picks.keys().find(|c| **c != route.from && **c != route.to) {
            return Err(EngineError::InvalidPayload {
                reason: format!("passeggero scelto in '{city}', che non è un estremo della rotta"),
            });
        }
        for city in [&route.from, &route.to] {
            let present = self.passengers_on_city.get(city);
            let color = match (present, picks.get(city)) {
                (Some(colors), Some(color)) if colors.contains_key(color) => color.clone(),
                (_, Some(color)) => {
                    return Err(EngineError::PassengerNotAvailable {
                        route_id: route.id.clone(),
                        city_id: city.clone(),
                        color: color.clone(),
                    });
                }
                (Some(colors), None) if colors.len() == 1 => {
                    colors.keys().next().cloned().unwrap_or_default()
                }
                (Some(colors), None) if colors.len() > 1 => {
                    return Err(EngineError::PassengerChoiceRequired {
                        route_id: route.id.clone(),
                        city_id: city.clone(),
                    });
                }
                _ => continue,
            };
            taken.insert(city.clone(), color);
        }
        Ok(taken)
    }

    /// Il giocatore prende i passeggeri già risolti con `resolve_passengers`.
    fn take_passengers(
        &mut self,
        player_index: usize,
        route: &Route,
        taken: &BTreeMap<CityId, String>,
        events: &mut Vec<GameEvent>,
    ) {
        let mut moved: Vec<(CityId, String)> = Vec::new();
        for (city, color) in taken {
            let Some(colors) = self.passengers_on_city.get_mut(city) else {
                continue;
            };
            let Some(count) = colors.get_mut(color) else {
                continue;
            };
            *count -= 1;
            if *count == 0 {
                colors.remove(color);
            }
            if colors.is_empty() {
                self.passengers_on_city.remove(city);
            }
            *self.players[player_index]
                .passengers
                .entry(color.clone())
                .or_default() += 1;
            moved.push((city.clone(), color.clone()));
        }
        if !moved.is_empty() {
            events.push(GameEvent::PassengersTaken {
                player_index,
                route_id: route.id.clone(),
                taken: moved,
            });
        }
    }

    /// Paesi delle reti già premiate (almeno due Paesi con una pila) a cui appartengono gli
    /// estremi della rotta, calcolati prima della presa. Vuoto senza `countryCards`.
    fn rewarded_countries(&mut self, player_index: usize, route: &Route) -> BTreeSet<CityId> {
        let mut rewarded = BTreeSet::new();
        if self.country_stacks.is_empty() {
            return rewarded;
        }
        for city in [&route.from, &route.to] {
            let network: Vec<CityId> = self
                .connectivity
                .countries_in_city_network(player_index, city)
                .into_iter()
                .filter(|country| self.country_stacks.contains_key(country))
                .collect();
            if network.len() >= 2 {
                rewarded.extend(network);
            }
        }
        rewarded
    }

    /// Carte Paese (Poland, D38): se la rete del giocatore che contiene la rotta raggiunge
    /// almeno due Paesi e ne include uno che non era in una rete già premiata (`rewarded`),
    /// prende la carta in cima alla pila di ogni Paese della rete; le pile vuote non danno
    /// nulla. L'esito non dipende dall'ordine delle prese.
    fn take_country_cards(
        &mut self,
        player_index: usize,
        route: &Route,
        rewarded: &BTreeSet<CityId>,
        events: &mut Vec<GameEvent>,
    ) {
        if self.country_stacks.is_empty() {
            return;
        }
        // Contano solo i Paesi con una pila (`countryCards`): un Paese senza carte non
        // completa la coppia richiesta dal regolamento.
        let network: Vec<CityId> = self
            .connectivity
            .countries_in_network(player_index, &route.id, &route.from)
            .into_iter()
            .filter(|country| self.country_stacks.contains_key(country))
            .collect();
        if network.len() < 2 || network.iter().all(|country| rewarded.contains(country)) {
            return;
        }
        let mut taken: Vec<(CityId, u32)> = Vec::new();
        for country in network {
            let Some(stack) = self.country_stacks.get_mut(&country) else {
                continue;
            };
            if stack.is_empty() {
                continue;
            }
            let value = stack.remove(0);
            self.players[player_index]
                .country_cards
                .entry(country.clone())
                .or_default()
                .push(value);
            taken.push((country, value));
        }
        if !taken.is_empty() {
            events.push(GameEvent::CountryCardsTaken {
                player_index,
                cards: taken,
            });
        }
    }

    /// Rivela le carte di un tunnel (D30) dopo che il pagamento base è stato validato.
    /// Con costo extra 0 la rotta è reclamata subito (D31), altrimenti si attende la
    /// decisione del giocatore.
    fn start_tunnel(
        &mut self,
        player_index: usize,
        route: &Route,
        hand: Vec<CardColor>,
        played: Vec<CardColor>,
        taken: BTreeMap<CityId, String>,
    ) -> Vec<GameEvent> {
        self.players[player_index].hand = hand;
        let color = played
            .iter()
            .copied()
            .find(|c| *c != CardColor::Locomotive)
            .unwrap_or(CardColor::Locomotive);

        let mut events = Vec::new();
        let count = self.ruleset.tunnels.as_ref().map_or(0, |t| t.revealed);
        let revealed = reveal_from_deck(
            &mut self.draw_pile,
            &mut self.train_discards,
            &mut self.rng,
            count,
            &mut events,
        );
        let extra = u32::try_from(
            revealed
                .iter()
                .filter(|c| **c == CardColor::Locomotive || **c == color)
                .count(),
        )
        .unwrap_or(u32::MAX);
        events.push(GameEvent::TunnelRevealed {
            player_index,
            route_id: route.id.clone(),
            cards: revealed.clone(),
            color,
            extra_cost: extra,
        });

        if extra == 0 {
            self.train_discards.extend_from_slice(&played);
            self.train_discards.extend(revealed);
            self.complete_claim(player_index, route, played, &taken, &mut events);
        } else {
            self.pending_tunnel = Some(PendingTunnel {
                route_id: route.id.clone(),
                played,
                revealed,
                color,
                extra,
                passengers: taken,
            });
            self.phase = GamePhase::TunnelResolution;
        }
        events
    }

    /// Decisione sul tunnel pendente: pagare le carte extra o rinunciare.
    pub(crate) fn handle_tunnel_decision(
        &mut self,
        player_index: usize,
        pay: bool,
        cards_discarded: &[CardColor],
    ) -> Result<Vec<GameEvent>, EngineError> {
        if self.phase != GamePhase::TunnelResolution {
            return Err(EngineError::WrongState { state: self.phase });
        }
        if player_index != self.current_player_index {
            return Err(EngineError::NotYourTurn {
                player_index,
                current_player: self.current_player_index,
            });
        }
        let Some(pending) = self.pending_tunnel.as_ref() else {
            return Err(EngineError::WrongState { state: self.phase });
        };

        if !pay {
            let Some(pending) = self.pending_tunnel.take() else {
                return Err(EngineError::WrongState { state: self.phase });
            };
            self.players[player_index].hand.extend(pending.played);
            self.train_discards.extend(pending.revealed);
            let mut events = vec![GameEvent::TunnelDeclined {
                player_index,
                route_id: pending.route_id,
            }];
            self.advance_turn(&mut events);
            return Ok(events);
        }

        let needed = pending.extra;
        let insufficient = || EngineError::InsufficientTunnelCards {
            route_id: pending.route_id.clone(),
            needed,
        };
        if cards_discarded.len() != needed as usize {
            return Err(insufficient());
        }
        let route = self
            .routes_by_id
            .get(&pending.route_id)
            .ok_or_else(|| EngineError::RouteNotFound {
                route_id: pending.route_id.clone(),
            })?
            .clone();

        let locos_usable = self.ruleset.locomotive_usable_on(&route);
        let mut hand = self.players[player_index].hand.clone();
        let mut extra_cards: Vec<CardColor> = Vec::with_capacity(cards_discarded.len());
        for card in cards_discarded {
            let pos = hand
                .iter()
                .position(|c| c == card)
                .or_else(|| {
                    locos_usable
                        .then(|| hand.iter().position(|c| *c == CardColor::Locomotive))
                        .flatten()
                })
                .ok_or_else(insufficient)?;
            extra_cards.push(hand.remove(pos));
        }
        if let Some(bad) = extra_cards
            .iter()
            .find(|c| **c != pending.color && (**c != CardColor::Locomotive || !locos_usable))
        {
            return Err(EngineError::InvalidPayload {
                reason: format!(
                    "carta {bad:?} non ammessa per il costo extra del tunnel ({:?})",
                    pending.color
                ),
            });
        }

        // --- Tutto valido: paga e completa il claim ---

        let Some(pending) = self.pending_tunnel.take() else {
            return Err(EngineError::WrongState { state: self.phase });
        };
        self.players[player_index].hand = hand;
        let taken = pending.passengers;
        // Ordine di scarto deterministico: giocate, extra, rivelate.
        self.train_discards.extend_from_slice(&pending.played);
        self.train_discards.extend_from_slice(&extra_cards);
        self.train_discards.extend(pending.revealed);
        let mut cards_used = pending.played;
        cards_used.extend(extra_cards);

        let mut events = Vec::new();
        self.complete_claim(player_index, &route, cards_used, &taken, &mut events);
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

    /// Bonus `mandala` per giocatore (tutti 0 senza il bonus nel ruleset): marca in
    /// `final_ticket_results` i biglietti con una destinazione collegata alla partenza da
    /// almeno due percorsi senza rotte in comune tra le sole rotte del giocatore (non
    /// quelle prese in prestito dalle stazioni), poi legge la tabella con il loro numero.
    fn mark_mandala_tickets(&mut self, routes_per_player: &[Vec<ClaimedRouteInfo>]) -> Vec<i32> {
        let Some(table) = self.ruleset.mandala_table() else {
            return vec![0; self.players.len()];
        };
        let countries = self.connectivity.countries();
        self.final_ticket_results
            .iter_mut()
            .zip(routes_per_player)
            .map(|(results, claimed)| {
                let graph = MandalaGraph::new(claimed, &self.routes_by_id, countries);
                for result in results.iter_mut() {
                    result.mandala = self
                        .tickets_by_id
                        .get(&result.ticket_id)
                        .is_some_and(|def| {
                            TicketSpec::of(def)
                                .destinations
                                .iter()
                                .any(|&(to, _)| graph.two_disjoint_paths(&def.from, to))
                        });
                }
                let count = results.iter().filter(|r| r.mandala).count();
                points_to_score(mandala_points(table, count))
            })
            .collect()
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

        let stations: Vec<_> = (0..self.players.len())
            .map(|pi| self.station_candidates(pi))
            .collect();

        let (mut scores, winners, bonus) = compute_final_scores(
            &routes_per_player,
            &self.routes_by_id,
            &self.map.tickets,
            &player_tickets,
            &stations,
            &mut self.connectivity,
            LongestPathRules {
                bonus_points: self.longest_path_bonus_points,
                tie_break: self
                    .ruleset
                    .tie_breaker_keys()
                    .contains(&TieBreaker::LongestPath),
            },
        );

        let globetrotter = match self.globetrotter_bonus_points {
            Some(points) => (globetrotter_winners(&scores), points_to_score(points)),
            None => (Vec::new(), 0),
        };

        self.longest_path_inexact = scores
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.longest_path_exact)
            .map(|(i, _)| i)
            .collect();
        self.longest_path_routes = scores
            .iter_mut()
            .map(|s| std::mem::take(&mut s.longest_path_routes))
            .collect();
        self.final_ticket_results = scores
            .iter_mut()
            .map(|s| std::mem::take(&mut s.ticket_results))
            .collect();
        let mandala_points = self.mark_mandala_tickets(&routes_per_player);

        let passenger_points: Vec<i32> = match &self.ruleset.passengers {
            Some(config) => {
                let held: Vec<&BTreeMap<String, u32>> =
                    self.players.iter().map(|p| &p.passengers).collect();
                let first = config.majority_points.first().copied().unwrap_or(0);
                let second = config.majority_points.get(1).copied().unwrap_or(0);
                passenger_majority_points(&held, &config.colors, first, second)
                    .into_iter()
                    .map(points_to_score)
                    .collect()
            }
            None => vec![0; self.players.len()],
        };

        // Asian Explorer: città della rete più grande di rotte proprie (0 senza il bonus).
        let network_sizes: Vec<u32> = match self.ruleset.bonus_points(LARGEST_NETWORK_BONUS_TYPE) {
            Some(_) => routes_per_player
                .iter()
                .map(|claimed| {
                    largest_network_size(claimed, &self.routes_by_id, self.connectivity.countries())
                })
                .collect(),
            None => vec![0; self.players.len()],
        };
        let network_winners = largest_network_winners(&network_sizes);
        let network_points = self
            .ruleset
            .bonus_points(LARGEST_NETWORK_BONUS_TYPE)
            .map_or(0, points_to_score);
        // Bonus Regioni (Italy): reti e punti per giocatore (vuoti e 0 senza il bonus).
        let region_standings: Vec<RegionStanding> = match &self.region_rules {
            Some(rules) => routes_per_player
                .iter()
                .map(|claimed| {
                    rules.standing(claimed, &self.routes_by_id, self.connectivity.countries())
                })
                .collect(),
            None => vec![RegionStanding::default(); self.players.len()],
        };
        // Spareggio `mountainRoutes`: rotte con simboli di montagna reclamate.
        let mountain_routes: Vec<i64> = routes_per_player
            .iter()
            .map(|claimed| {
                let count = claimed
                    .iter()
                    .filter(|c| {
                        self.routes_by_id
                            .get(&c.route_id)
                            .is_some_and(|r| r.mountains > 0)
                    })
                    .count();
                i64::try_from(count).unwrap_or(i64::MAX)
            })
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
                let globetrotter_bonus = if globetrotter.0.contains(&i) {
                    globetrotter.1
                } else {
                    0
                };
                let mandala_bonus = mandala_points[i];
                let largest_network_bonus = if network_winners.contains(&i) {
                    network_points
                } else {
                    0
                };
                let regions_bonus = region_standings[i].bonus;
                let station_points = self.unbuilt_station_points(i);
                let country_points = points_to_score(
                    p.country_cards
                        .values()
                        .flatten()
                        .fold(0u32, |acc, &value| acc.saturating_add(value)),
                );
                let passenger_points = passenger_points[i];
                let total = p
                    .score
                    .saturating_add(s.completed_tickets)
                    .saturating_add(s.failed_tickets)
                    .saturating_add(bonus_val)
                    .saturating_add(globetrotter_bonus)
                    .saturating_add(mandala_bonus)
                    .saturating_add(largest_network_bonus)
                    .saturating_add(regions_bonus)
                    .saturating_add(station_points)
                    .saturating_add(country_points)
                    .saturating_add(passenger_points);
                FinalScore {
                    player_index: i,
                    route_points: p.score.saturating_sub(p.mountain_points),
                    ticket_completed: s.completed_tickets,
                    ticket_failed: s.failed_tickets,
                    longest_path_bonus: bonus_val,
                    globetrotter_bonus,
                    mandala_bonus,
                    mountain_points: p.mountain_points,
                    largest_network_bonus,
                    largest_network: network_sizes[i],
                    regions_bonus,
                    region_networks: region_standings[i].networks.clone(),
                    total,
                    station_points,
                    borrowed_routes: s.borrowed_routes.clone(),
                    country_points,
                    passenger_points,
                    winner: false,
                }
            })
            .collect();

        // Vincitore: massimo lessicografico di (totale, spareggi del ruleset in ordine);
        // di default biglietti completati, poi bonus del percorso più lungo. Pari merito
        // residui restano tutti vincitori.
        let tie_breakers = self.ruleset.tie_breaker_keys();
        let rank_key = |fs: &FinalScore| -> Vec<i64> {
            let score = &scores[fs.player_index];
            let mut key = vec![i64::from(fs.total)];
            key.extend(tie_breakers.iter().map(|tie| {
                match tie {
                    TieBreaker::CompletedTickets => i64::from(score.completed_count),
                    TieBreaker::CompletedTicketPoints => i64::from(score.completed_tickets),
                    TieBreaker::LongestPathBonus => i64::from(fs.longest_path_bonus),
                    TieBreaker::LongestPath => i64::from(score.longest_path),
                    TieBreaker::Passengers => self.players[fs.player_index]
                        .passengers
                        .values()
                        .map(|&n| i64::from(n))
                        .sum(),
                    TieBreaker::MountainRoutes => mountain_routes[fs.player_index],
                }
            }));
            key
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

    /// Aggiorna il completamento dei biglietti per un giocatore basandosi sulla
    /// connettività corrente e sulla migliore assegnazione delle stazioni (D32).
    pub(crate) fn update_ticket_completion(&mut self, player_index: usize) {
        let stations = self.station_candidates(player_index);
        let (ids, eval) = {
            let tickets_by_id = &self.tickets_by_id;
            let defs: Vec<&TicketDef> = self.players[player_index]
                .tickets
                .keys()
                .filter_map(|tid| tickets_by_id.get(tid))
                .collect();
            let list: Vec<TicketSpec> = defs.iter().map(|t| TicketSpec::of(t)).collect();
            let eval = match self.connectivity.uf_mut(player_index) {
                Some(uf) => evaluate_tickets(uf, &list, &stations),
                None => evaluate_tickets(&mut UnionFind::new(), &list, &stations),
            };
            let ids: Vec<String> = defs.iter().map(|t| t.id.clone()).collect();
            (ids, eval)
        };

        for (tid, completed) in ids.iter().zip(eval.completed) {
            if let Some(ts) = self.players[player_index].tickets.get_mut(tid) {
                ts.completed = Some(completed);
            }
        }
    }
}
