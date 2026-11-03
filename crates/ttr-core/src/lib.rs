//! CiufCiuf! — Motore di gioco puro, deterministico, map-agnostic.
//!
//! ## Uso
//!
//! ```ignore
//! use ttr_core::Engine;
//!
//! let engine = Engine::new(
//!     42,                    // seed
//!     ruleset,
//!     map_data,
//!     player_ids,
//!     player_names,
//!     player_colors,
//!     0,                     // first_player
//! );
//!
//! let events = engine.apply(Action::DrawTickets { player_index: 0 })?;
//! ```

mod action;
mod engine;
mod engine_claim;
mod engine_deck;
mod engine_draw;
mod engine_tickets;
pub mod error;
pub mod event;
pub mod legal;
mod rng;
pub mod scoring;
pub mod types;

pub use action::{Action, DrawSource};
pub use engine::Engine;
pub use error::{EngineError, GamePhase};
pub use event::GameEvent;
pub use legal::LegalMoves;
pub use types::{MapData, Ruleset};

/// Carica mappa e ruleset da stringhe JSON (funzione di convenienza).
pub fn load_map_from_json(json: &str) -> Result<MapData, serde_json::Error> {
    serde_json::from_str(json)
}

/// Carica ruleset da stringa JSON.
pub fn load_ruleset_from_json(json: &str) -> Result<Ruleset, serde_json::Error> {
    serde_json::from_str(json)
}

/// Crea un mazzo di colori giocatore predefiniti.
pub fn default_player_colors(count: usize) -> Vec<ttr_proto::PlayerColor> {
    use ttr_proto::PlayerColor;
    let all = [
        PlayerColor::Red,
        PlayerColor::Blue,
        PlayerColor::Green,
        PlayerColor::Yellow,
        PlayerColor::Black,
    ];
    all.iter().take(count).copied().collect()
}
