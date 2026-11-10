//! Motore di gioco: setup, applicazione azioni, macchina a stati.
//!
//! Il motore è puro: nessun I/O, nessuna rete.

use std::collections::{BTreeMap, BTreeSet};

use ttr_proto::CardColor;

use crate::action::Action;
use crate::engine_deck::{build_train_deck, check_and_reset_loco_threshold};
use crate::error::{EngineError, EngineInitError, GamePhase};
use crate::event::GameEvent;
use crate::rng::SeededRng;
use crate::scoring::PlayerConnectivity;
use crate::types::{
    ClaimedRouteInfo, FACE_UP_COUNT, INITIAL_HAND_SIZE, LONGEST_PATH_BONUS_TYPE, MapData,
    PlayerState, Route, RouteId, Ruleset, TicketDef, TicketId,
};

/// Il motore di CiufCiuf!
#[derive(Clone)]
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

    /// Punti del bonus del percorso più lungo, letti dal ruleset in `new`.
    pub(crate) longest_path_bonus_points: u32,

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
    /// # Errors
    ///
    /// Restituisce `EngineInitError` se le liste dei giocatori sono
    /// incoerenti tra loro, il numero di giocatori o il primo giocatore sono
    /// fuori intervallo, oppure ruleset e mappa non superano `Ruleset::validate`
    /// per questo numero di giocatori. `finalRoundIncludesTrigger = false` non
    /// è implementato e viene rifiutato.
    pub fn new(
        seed: u64,
        ruleset: Ruleset,
        map: MapData,
        player_ids: Vec<String>,
        player_names: Vec<String>,
        player_colors: Vec<ttr_proto::PlayerColor>,
        first_player: usize,
    ) -> Result<Self, EngineInitError> {
        let num_players = player_ids.len();
        if player_names.len() != num_players || player_colors.len() != num_players {
            return Err(EngineInitError::PlayerListsMismatch {
                ids: num_players,
                names: player_names.len(),
                colors: player_colors.len(),
            });
        }
        let (min, max) = (ruleset.players.min, ruleset.players.max);
        if num_players == 0 || !(min as usize..=max as usize).contains(&num_players) {
            return Err(EngineInitError::PlayerCountOutOfRange {
                count: num_players,
                min,
                max,
            });
        }
        if first_player >= num_players {
            return Err(EngineInitError::FirstPlayerOutOfRange {
                first_player,
                players: num_players,
            });
        }
        let mut seen_ids: BTreeSet<&str> = BTreeSet::new();
        for id in &player_ids {
            if !seen_ids.insert(id.as_str()) {
                return Err(EngineInitError::DuplicatePlayerId {
                    player_id: id.clone(),
                });
            }
        }
        for (i, color) in player_colors.iter().enumerate() {
            if player_colors[..i].contains(color) {
                return Err(EngineInitError::DuplicatePlayerColor {
                    color: format!("{color:?}"),
                });
            }
        }
        ruleset.check_setup(&map, num_players)?;
        let Some(longest_path_bonus_points) = ruleset
            .bonuses
            .iter()
            .find(|b| b.bonus_type == LONGEST_PATH_BONUS_TYPE)
            .map(|b| b.points)
        else {
            return Err(EngineInitError::MissingLongestPathBonus {
                bonus_type: LONGEST_PATH_BONUS_TYPE,
            });
        };

        let mut rng = SeededRng::new(seed);

        // --- Train deck ---
        let mut train_cards = build_train_deck(&ruleset.deck);
        rng.shuffle(&mut train_cards);

        let mut face_up: Vec<Option<CardColor>> = Vec::with_capacity(FACE_UP_COUNT);
        for _ in 0..FACE_UP_COUNT {
            face_up.push(train_cards.pop());
        }
        let mut train_discards: Vec<CardColor> = Vec::new();
        // Gli eventi del setup non vengono pubblicati: nessun client li ha visti.
        let mut setup_events: Vec<GameEvent> = Vec::new();
        check_and_reset_loco_threshold(
            &mut face_up,
            &mut train_cards,
            &mut train_discards,
            &mut rng,
            &ruleset.deck,
            &mut setup_events,
        );

        // --- Ticket decks, mescolati nell'ordine di `ruleset.ticket_decks` ---
        let mut ticket_piles: BTreeMap<String, Vec<TicketId>> = BTreeMap::new();
        for deck_name in &ruleset.ticket_decks {
            let mut ids: Vec<TicketId> = map
                .tickets
                .iter()
                .filter(|t| &t.deck == deck_name)
                .map(|t| t.id.clone())
                .collect();
            rng.shuffle(&mut ids);
            ticket_piles.insert(deck_name.clone(), ids);
        }

        // --- Deal initial tickets ---
        let mut pending_initial: Vec<Vec<TicketId>> = Vec::with_capacity(num_players);
        for _ in 0..num_players {
            let mut dealt: Vec<TicketId> = Vec::new();
            for entry in &ruleset.initial_tickets.dealt {
                let Some(pile) = ticket_piles.get_mut(&entry.deck) else {
                    return Err(EngineInitError::DealtTicketDeckUnknown {
                        deck: entry.deck.clone(),
                    });
                };
                for _ in 0..entry.count {
                    let Some(tid) = pile.pop() else {
                        return Err(EngineInitError::TicketDeckTooSmall {
                            deck: entry.deck.clone(),
                            have: map.tickets.iter().filter(|t| t.deck == entry.deck).count(),
                            needed: num_players.saturating_mul(entry.count as usize),
                        });
                    };
                    dealt.push(tid);
                }
            }
            pending_initial.push(dealt);
        }
        let Some(ticket_ids) = ticket_piles.remove(&ruleset.ticket_draw.deck) else {
            return Err(EngineInitError::DrawTicketDeckUnknown {
                deck: ruleset.ticket_draw.deck.clone(),
            });
        };

        // --- Players ---
        let trains_per_player = ruleset.trains_per_player;
        let players: Vec<PlayerState> = player_ids
            .into_iter()
            .zip(player_names)
            .zip(player_colors)
            .map(|((id, name), color)| PlayerState {
                id,
                name,
                color,
                hand: (0..INITIAL_HAND_SIZE)
                    .filter_map(|_| train_cards.pop())
                    .collect(),
                tickets: BTreeMap::new(),
                trains_remaining: trains_per_player,
                claimed_routes: BTreeMap::new(),
                score: 0,
                initial_draft_done: false,
            })
            .collect();

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

        Ok(Engine {
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
            longest_path_bonus_points,
            action_log: Vec::new(),
        })
    }

    /// Applica un'azione e restituisce gli eventi generati.
    ///
    /// # Errors
    ///
    /// Se l'azione non è valida nello stato corrente restituisce
    /// `EngineError` e lo stato resta invariato.
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

    #[must_use]
    pub fn phase(&self) -> GamePhase {
        self.phase
    }
    #[must_use]
    pub fn current_player(&self) -> usize {
        self.current_player_index
    }
    #[must_use]
    pub fn num_players(&self) -> usize {
        self.players.len()
    }
    #[must_use]
    pub fn last_round(&self) -> bool {
        self.last_round
    }
    #[must_use]
    pub fn last_round_trigger_player(&self) -> Option<usize> {
        self.last_round_trigger_player
    }

    #[must_use]
    pub fn player(&self, index: usize) -> Option<&PlayerState> {
        self.players.get(index)
    }
    #[must_use]
    pub fn player_hand(&self, index: usize) -> Option<&[CardColor]> {
        self.players.get(index).map(|p| p.hand.as_slice())
    }
    #[must_use]
    pub fn face_up_cards(&self) -> &[Option<CardColor>] {
        &self.face_up
    }
    #[must_use]
    pub fn claimed_route_ids(&self) -> Vec<&RouteId> {
        self.claimed_routes.keys().collect()
    }
    #[must_use]
    pub fn draw_pile_len(&self) -> usize {
        self.draw_pile.len()
    }
    /// Numero di carte nel mazzo coperto (solo la pila pesca, NON gli scarti).
    #[must_use]
    pub fn deck_size(&self) -> u32 {
        u32::try_from(self.draw_pile.len()).unwrap_or(u32::MAX)
    }
    #[must_use]
    pub fn discards_len(&self) -> usize {
        self.train_discards.len()
    }

    #[must_use]
    pub fn seed(&self) -> u64 {
        self.rng.seed()
    }
    #[must_use]
    pub fn action_log(&self) -> &[ActionRecord] {
        &self.action_log
    }

    /// Biglietti offerti a un giocatore nel draft iniziale; vuoto dopo la sua scelta.
    #[must_use]
    pub fn initial_tickets_for(&self, index: usize) -> Option<&[TicketId]> {
        self.pending_initial_tickets.get(index).map(Vec::as_slice)
    }

    /// Biglietti pendenti (offerti durante TicketChoice mid-game).
    #[must_use]
    pub fn pending_tickets(&self) -> &[TicketId] {
        &self.pending_tickets
    }

    /// `keep_min` per la pesca mid-game.
    #[must_use]
    pub fn ticket_draw_keep_min(&self) -> u32 {
        self.ruleset.ticket_draw.keep_min
    }

    /// Accede a una rotta per id (per test/fuzzing).
    #[must_use]
    pub fn get_route(&self, route_id: &str) -> Option<&Route> {
        self.routes_by_id.get(route_id)
    }

    // ── Test helpers (modificano o espongono lo stato interno) ───────

    /// Imposta una carta scoperta.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_face_up_card(&mut self, index: usize, card: Option<CardColor>) {
        if let Some(slot) = self.face_up.get_mut(index) {
            *slot = card;
        }
    }

    /// Svuota il mazzo e restituisce le carte.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn drain_draw_pile(&mut self) -> Vec<CardColor> {
        std::mem::take(&mut self.draw_pile)
    }

    /// Rimpiazza gli scarti.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_discards(&mut self, cards: Vec<CardColor>) {
        self.train_discards = cards;
    }

    /// Svuota mazzo e scarti.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn clear_deck_and_discards(&mut self) {
        self.draw_pile.clear();
        self.train_discards.clear();
    }

    /// Imposta i treni rimanenti di un giocatore.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_player_trains(&mut self, index: usize, n: u32) {
        if let Some(p) = self.players.get_mut(index) {
            p.trains_remaining = n;
        }
    }

    /// Imposta la mano di un giocatore.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_player_hand(&mut self, index: usize, hand: Vec<CardColor>) {
        if let Some(p) = self.players.get_mut(index) {
            p.hand = hand;
        }
    }

    /// Mazzo biglietti di pesca, dalla cima in fondo.
    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn ticket_pile(&self) -> &[TicketId] {
        &self.ticket_draw_pile
    }

    /// Mazzo treni coperto, dal fondo alla cima.
    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn draw_pile_cards(&self) -> &[CardColor] {
        &self.draw_pile
    }

    /// Prossima parola dell'RNG, senza consumarla.
    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn peek_rng_word(&self) -> u64 {
        self.rng.clone().next_word()
    }

    // ── Serializzazione per replay ───────────────────────────────────

    /// Esporta lo stato completo per verifica replay.
    #[must_use]
    pub fn serialize_state(&self) -> String {
        serde_json::to_string(&SerializableState::from(self))
            .expect("SerializableState contiene solo tipi serializzabili")
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
