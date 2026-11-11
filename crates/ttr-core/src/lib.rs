//! CiufCiuf! — Motore di gioco puro, deterministico, map-agnostic.
//!
//! ## Uso
//!
//! ```ignore
//! use ttr_core::{Action, Engine};
//!
//! let mut engine = Engine::new(
//!     42,                    // seed
//!     ruleset,
//!     map_data,
//!     player_ids,
//!     player_names,
//!     player_colors,
//!     0,                     // first_player
//! )?;
//!
//! let events = engine.apply(Action::DrawTickets { player_index: 0 })?;
//! ```

mod action;
mod density;
mod engine;
mod engine_claim;
mod engine_deck;
mod engine_draw;
mod engine_station;
mod engine_tickets;
pub mod error;
pub mod event;
pub mod legal;
mod rng;
pub mod scoring;
mod setup;
pub mod types;

pub use action::{Action, DrawSource};
pub use engine::{ActionRecord, Engine};
pub use error::{EngineError, EngineInitError, GamePhase};
pub use event::GameEvent;
pub use legal::{LegalMoves, TunnelMoves};
pub use types::{MapData, RouteColor, Ruleset};

/// Versione delle regole del motore, registrata nei save.
///
/// Va incrementata quando una modifica alle regole o alla pesca cambia l'esito
/// del replay di una partita esistente. Rispetto alla 1 la 2 cambia D27 (mazzo
/// nuovo a mazzo e scarti esauriti), D28 (ultimo giro senza rotte reclamabili)
/// e gli scarti (le carte realmente rimosse dalla mano, non quelle dichiarate).
///
/// Riordinare o aggiungere voci in `ticket_decks` di un ruleset esistente
/// cambia l'ordine degli shuffle dell'RNG e quindi il replay dei suoi save.
pub const RULES_VERSION: u32 = 2;

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
