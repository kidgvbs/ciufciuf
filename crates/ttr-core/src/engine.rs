//! Motore di gioco: setup, applicazione azioni, macchina a stati.
//!
//! Il motore è puro: nessun I/O, nessuna rete.

use std::collections::BTreeMap;

use ttr_proto::CardColor;

use crate::action::Action;
use crate::engine_deck::{build_train_deck, check_and_reset_loco_threshold};
use crate::error::{EngineError, GamePhase};
use crate::event::GameEvent;
use crate::rng::SeededRng;
use crate::scoring::PlayerConnectivity;
use crate::types::*;

/// Il motore di CiufCiuf!
pub struct Engine {
    pub(crate) rng: SeededRng,
    pub(crate) ruleset: Ruleset,
    pub(crate) map: MapData,
    pub(crate) players: Vec<PlayerState>,
    pub(crate) phase: GamePhase,
    pub(crate) current_player_index: usize,

    // Train deck
    pub(crate) draw_pile: Vec<CardColor>,
    pub(crate) face_up: Vec<Option<CardColor>>,
    pub(crate) train_discards: Vec<CardColor>,

    // Ticket deck
    pub(crate) ticket_draw_pile: Vec<TicketId>,
    pub(crate) ticket_discards: Vec<TicketId>,

    // Global state
    pub(crate) claimed_routes: BTreeMap<RouteId, ClaimedRouteInfo>,
    pub(crate) connectivity: PlayerConnectivity,

    // Turn / round
    pub(crate) last_round: bool,
    pub(crate) last_round_trigger_player: Option<usize>,

    // Pending state
    pub(crate) pending_initial_tickets: Vec<Vec<TicketId>>,
    pub(crate) pending_tickets: Vec<TicketId>,

    // Indexes
    pub(crate) routes_by_id: BTreeMap<RouteId, Route>,
    pub(crate) tickets_by_id: BTreeMap<TicketId, TicketDef>,

    // Log
    pub(crate) action_log: Vec<ActionRecord>,
}

/// Record di un'azione nel log (per replay).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionRecord {
    pub action: Action,
    pub events: Vec<GameEvent>,
}

impl Engine {
    /// Crea una nuova partita.
    ///
    /// # Panics
    ///
    /// Se `ruleset.final_round_includes_trigger` è `false`: la semantica
    /// «l'ultimo giro esclude il trigger» non è implementata (nessun ruleset
    /// corrente la usa; se servirà, sarà un change deliberato, piano §6/D11).
    pub fn new(
        seed: u64,
        ruleset: Ruleset,
        map: MapData,
        player_ids: Vec<String>,
        player_names: Vec<String>,
        player_colors: Vec<ttr_proto::PlayerColor>,
        first_player: usize,
    ) -> Self {
        assert!(
            ruleset.final_round_includes_trigger,
            "final_round_includes_trigger deve essere true; la semantica false non è implementata"
        );
        let num_players = player_ids.len();
        let mut rng = SeededRng::new(seed);

        // --- Train deck ---
        let mut train_cards = build_train_deck(&ruleset.deck);
        rng.shuffle(&mut train_cards);

        let mut face_up: Vec<Option<CardColor>> = Vec::with_capacity(FACE_UP_COUNT);
        for _ in 0..FACE_UP_COUNT {
            face_up.push(train_cards.pop());
        }
        // Check loco threshold after initial deal
        let mut train_discards: Vec<CardColor> = Vec::new();
        check_and_reset_loco_threshold(
            &mut face_up,
            &mut train_cards,
            &mut train_discards,
            &mut rng,
            &ruleset.deck,
        );

        // --- Ticket deck ---
        let mut ticket_ids: Vec<TicketId> = map
            .tickets
            .iter()
            .filter(|t| t.deck == "regular")
            .map(|t| t.id.clone())
            .collect();
        rng.shuffle(&mut ticket_ids);

        // --- Deal initial tickets ---
        let mut pending_initial: Vec<Vec<TicketId>> = Vec::with_capacity(num_players);
        for _ in 0..num_players {
            let mut dealt: Vec<TicketId> = Vec::new();
            for entry in &ruleset.initial_tickets.dealt {
                for _ in 0..entry.count {
                    if let Some(tid) = ticket_ids.pop() {
                        dealt.push(tid);
                    }
                }
            }
            pending_initial.push(dealt);
        }

        // --- Players ---
        let mut players: Vec<PlayerState> = Vec::with_capacity(num_players);
        for i in 0..num_players {
            let mut hand: Vec<CardColor> = Vec::new();
            for _ in 0..4 {
                if let Some(c) = train_cards.pop() {
                    hand.push(c);
                }
            }
            players.push(PlayerState {
                id: player_ids[i].clone(),
                name: player_names[i].clone(),
                color: player_colors[i],
                hand,
                tickets: BTreeMap::new(),
                trains_remaining: ruleset.trains_per_player,
                claimed_routes: BTreeMap::new(),
                score: 0,
                initial_draft_done: false,
            });
        }

        // --- Indexes ---
        let routes_by_id: BTreeMap<RouteId, Route> = map
            .routes
            .iter()
            .map(|r| (r.id.clone(), r.clone()))
            .collect();
        let tickets_by_id: BTreeMap<TicketId, TicketDef> = map
            .tickets
            .iter()
            .map(|t| (t.id.clone(), t.clone()))
            .collect();

        Engine {
            rng,
            ruleset,
            map,
            players,
            phase: GamePhase::InitialDraft,
            current_player_index: first_player,
            draw_pile: train_cards,
            face_up,
            train_discards,
            ticket_draw_pile: ticket_ids,
            ticket_discards: Vec::new(),
            claimed_routes: BTreeMap::new(),
            connectivity: PlayerConnectivity::new(num_players),
            last_round: false,
            last_round_trigger_player: None,
            pending_initial_tickets: pending_initial,
            pending_tickets: Vec::new(),
            routes_by_id,
            tickets_by_id,
            action_log: Vec::new(),
        }
    }

    /// Applica un'azione e restituisce gli eventi generati.
    pub fn apply(&mut self, action: Action) -> Result<Vec<GameEvent>, EngineError> {
        let events = match &action {
            Action::DrawTrainCard {
                player_index,
                source,
            } => self.handle_draw_train_card(*player_index, *source)?,
            Action::DrawTickets { player_index } => self.handle_draw_tickets(*player_index)?,
            Action::ChooseTickets { player_index, keep } => {
                self.handle_choose_tickets(*player_index, keep)?
            }
            Action::ClaimRoute {
                player_index,
                route_id,
                cards_discarded,
            } => self.handle_claim_route(*player_index, route_id, cards_discarded)?,
        };

        self.action_log.push(ActionRecord {
            action: action.clone(),
            events: events.clone(),
        });

        Ok(events)
    }

    // ── Getters ──────────────────────────────────────────────────────

    pub fn phase(&self) -> GamePhase {
        self.phase
    }
    pub fn current_player(&self) -> usize {
        self.current_player_index
    }
    pub fn num_players(&self) -> usize {
        self.players.len()
    }
    pub fn last_round(&self) -> bool {
        self.last_round
    }
    pub fn last_round_trigger_player(&self) -> Option<usize> {
        self.last_round_trigger_player
    }

    pub fn player(&self, index: usize) -> Option<&PlayerState> {
        self.players.get(index)
    }
    pub fn player_hand(&self, index: usize) -> Option<&[CardColor]> {
        self.players.get(index).map(|p| p.hand.as_slice())
    }
    pub fn face_up_cards(&self) -> &[Option<CardColor>] {
        &self.face_up
    }
    pub fn claimed_route_ids(&self) -> Vec<&RouteId> {
        self.claimed_routes.keys().collect()
    }
    pub fn draw_pile_len(&self) -> usize {
        self.draw_pile.len()
    }
    /// Numero di carte nel mazzo coperto (solo la pila pesca, NON gli scarti).
    pub fn deck_size(&self) -> u32 {
        self.draw_pile.len() as u32
    }
    pub fn discards_len(&self) -> usize {
        self.train_discards.len()
    }

    pub fn seed(&self) -> u64 {
        self.rng.seed()
    }
    pub fn action_log(&self) -> &[ActionRecord] {
        &self.action_log
    }

    /// Biglietti offerti a un giocatore nel draft iniziale.
    pub fn initial_tickets_for(&self, index: usize) -> Option<&[TicketId]> {
        self.pending_initial_tickets
            .get(index)
            .map(|v| v.as_slice())
    }

    /// Biglietti pendenti (offerti durante TicketChoice mid-game).
    pub fn pending_tickets(&self) -> &[TicketId] {
        &self.pending_tickets
    }

    /// `keep_min` per la pesca mid-game.
    pub fn ticket_draw_keep_min(&self) -> u32 {
        self.ruleset.ticket_draw.keep_min
    }

    /// Accede a una rotta per id (per test/fuzzing).
    pub fn get_route(&self, route_id: &str) -> Option<&Route> {
        self.routes_by_id.get(route_id)
    }

    // ── Test helpers (modifica stato interno per setup di test) ──────

    /// Imposta una carta scoperta (per test).
    pub fn set_face_up_card(&mut self, index: usize, card: Option<CardColor>) {
        if index < self.face_up.len() {
            self.face_up[index] = card;
        }
    }

    /// Svuota il mazzo e restituisce le carte (per test).
    pub fn drain_draw_pile(&mut self) -> Vec<CardColor> {
        std::mem::take(&mut self.draw_pile)
    }

    /// Rimpiazza gli scarti (per test).
    pub fn set_discards(&mut self, cards: Vec<CardColor>) {
        self.train_discards = cards;
    }

    /// Svuota mazzo e scarti (per test esaurimento totale).
    pub fn clear_deck_and_discards(&mut self) {
        self.draw_pile.clear();
        self.train_discards.clear();
    }

    /// Imposta i treni rimanenti di un giocatore (per test).
    pub fn set_player_trains(&mut self, index: usize, n: u32) {
        if let Some(p) = self.players.get_mut(index) {
            p.trains_remaining = n;
        }
    }

    /// Imposta la mano di un giocatore (per test).
    pub fn set_player_hand(&mut self, index: usize, hand: Vec<CardColor>) {
        if let Some(p) = self.players.get_mut(index) {
            p.hand = hand;
        }
    }

    /// Accede al mazzo biglietti (per test dell'ordine degli scarti).
    pub fn ticket_pile(&self) -> &[TicketId] {
        &self.ticket_draw_pile
    }

    // ── Serializzazione per replay ───────────────────────────────────

    /// Esporta lo stato completo per verifica replay.
    pub fn serialize_state(&self) -> String {
        serde_json::to_string(&SerializableState::from(self)).unwrap()
    }
}

/// Stato serializzabile per confronto replay.
#[derive(serde::Serialize)]
struct SerializableState {
    phase: GamePhase,
    current_player_index: usize,
    players: Vec<SerializablePlayer>,
    face_up: Vec<Option<CardColor>>,
    claimed_routes: Vec<ClaimedRouteInfo>,
    last_round: bool,
    last_round_trigger_player: Option<usize>,
    draw_pile_len: usize,
    discards_len: usize,
    ticket_draw_pile_len: usize,
    ticket_discards_len: usize,
}

#[derive(serde::Serialize)]
struct SerializablePlayer {
    hand_len: usize,
    hand_sorted: Vec<CardColor>,
    tickets_count: usize,
    trains_remaining: u32,
    score: i32,
}

impl From<&Engine> for SerializableState {
    fn from(e: &Engine) -> Self {
        SerializableState {
            phase: e.phase,
            current_player_index: e.current_player_index,
            players: e
                .players
                .iter()
                .map(|p| {
                    let mut h = p.hand.clone();
                    h.sort();
                    SerializablePlayer {
                        hand_len: h.len(),
                        hand_sorted: h,
                        tickets_count: p.tickets.len(),
                        trains_remaining: p.trains_remaining,
                        score: p.score,
                    }
                })
                .collect(),
            face_up: e.face_up.clone(),
            claimed_routes: e.claimed_routes.values().cloned().collect(),
            last_round: e.last_round,
            last_round_trigger_player: e.last_round_trigger_player,
            draw_pile_len: e.draw_pile.len(),
            discards_len: e.train_discards.len(),
            ticket_draw_pile_len: e.ticket_draw_pile.len(),
            ticket_discards_len: e.ticket_discards.len(),
        }
    }
}
