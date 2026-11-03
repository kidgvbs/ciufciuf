//! Tipi interni del motore: mappa, ruleset, stato giocatore.
//!
//! Usa `BTreeMap` ovunque l'ordine di iterazione influenzi lo stato
//! (piano §9.5, R3 — determinismo).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use ttr_proto::{CardColor, PlayerColor};

// ── Dati mappa (caricati da JSON) ────────────────────────────────────

/// Mappa di gioco: città, rotte, biglietti.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapData {
    pub cities: Vec<City>,
    pub routes: Vec<Route>,
    pub tickets: Vec<TicketDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct City {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub from: String,
    pub to: String,
    pub len: u32,
    pub color: String, // "Red", "Blue", ... "Gray", "Locomotive"
    pub double: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketDef {
    pub id: String,
    pub from: String,
    pub to: String,
    pub points: u32,
    pub deck: String, // "regular" | "long" (future)
}

// ── Ruleset descriptor ───────────────────────────────────────────────

/// Regole per-mappa (caricate da JSON, piano §4.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ruleset {
    pub map: String,
    pub players: PlayersConfig,
    #[serde(rename = "trainsPerPlayer")]
    pub trains_per_player: u32,
    pub deck: DeckConfig,
    #[serde(rename = "ticketDecks")]
    pub ticket_decks: Vec<String>,
    #[serde(rename = "initialTickets")]
    pub initial_tickets: InitialTickets,
    #[serde(rename = "ticketDraw")]
    pub ticket_draw: TicketDraw,
    #[serde(rename = "setupDiscards")]
    pub setup_discards: SetupDiscards,
    #[serde(rename = "routePoints")]
    pub route_points: BTreeMap<String, u32>,
    pub bonuses: Vec<Bonus>,
    #[serde(rename = "doubleRouteMinPlayers")]
    pub double_route_min_players: u32,
    #[serde(rename = "finalRoundIncludesTrigger")]
    pub final_round_includes_trigger: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayersConfig {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckConfig {
    #[serde(rename = "perColor")]
    pub per_color: u32,
    pub locomotives: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialTickets {
    pub dealt: Vec<TicketDealtEntry>,
    #[serde(rename = "keepMin")]
    pub keep_min: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketDealtEntry {
    pub deck: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketDraw {
    pub deck: String,
    pub count: u32,
    #[serde(rename = "keepMin")]
    pub keep_min: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bonus {
    #[serde(rename = "type")]
    pub bonus_type: String,
    pub label: String,
    pub points: u32,
}

/// Cosa fare degli scarti di setup (piano §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SetupDiscards {
    /// Rimescolati tra loro e messi in fondo al mazzo (USA V2 2025).
    ReshuffleBottom,
    /// Rimossi dalla partita (Europe).
    Removed,
}

// ── Stato giocatore ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: String,
    pub name: String,
    pub color: PlayerColor,
    pub hand: Vec<CardColor>,
    pub tickets: BTreeMap<String, TicketStatus>,
    pub trains_remaining: u32,
    /// Mappa route_id → (punti_immediati, treni_usati)
    pub claimed_routes: BTreeMap<String, ClaimedRouteRecord>,
    pub score: i32,
    /// Se il giocatore ha già scelto in INITIAL_DRAFT.
    pub initial_draft_done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketStatus {
    pub ticket_id: String,
    pub points: u32,
    /// null = non ancora rivelato / in scelta; Some(bool) = completato o meno
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedRouteRecord {
    pub points: u32,
    pub train_count: u32,
}

/// Route reclamata globalmente (visibile a tutti).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedRouteInfo {
    pub route_id: String,
    pub claimed_by: usize, // player index
}

// ── Identificatori ───────────────────────────────────────────────────

pub type CityId = String;
pub type RouteId = String;
pub type TicketId = String;

// ── Costanti ─────────────────────────────────────────────────────────

/// Numero di carte scoperte visibili.
pub const FACE_UP_COUNT: usize = 5;

/// Soglia locomotive scoperte per il reset del pool.
pub const LOCO_RESET_THRESHOLD: usize = 3;

/// Soglia treni rimasti per innescare l'ultimo giro.
pub const LAST_ROUND_TRAIN_THRESHOLD: u32 = 2;

/// Numero totale di colori carta (esclusa Locomotive).
pub const COLOR_COUNT: usize = 8;
