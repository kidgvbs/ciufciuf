//! Errori del motore.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum EngineError {
    #[error("azione non valida nello stato {state:?}")]
    WrongState { state: GamePhase },

    #[error("non è il tuo turno (giocatore {player_index}, turno di {current_player})")]
    NotYourTurn {
        player_index: usize,
        current_player: usize,
    },

    #[error("rotta '{route_id}' già reclamata")]
    RouteAlreadyClaimed { route_id: String },

    #[error("rotta '{route_id}' bloccata (doppia, 2-3 giocatori)")]
    RouteBlockedDouble { route_id: String },

    #[error("carte insufficienti per reclamare '{route_id}': servono {needed}")]
    InsufficientCards { route_id: String, needed: u32 },

    #[error("indice face-up {index} non valido (max {max})")]
    FaceUpIndexOutOfRange { index: usize, max: usize },

    #[error("locomotiva scoperta non può essere presa come seconda carta")]
    LocomotiveAsSecondCard,

    #[error("carta non disponibile all'indice {index}")]
    CardNotAvailable { index: usize },

    #[error("scelta biglietti: tenuti {kept} < minimo {min}")]
    TicketChoiceBelowMin { kept: usize, min: u32 },

    #[error("scelta biglietti: tenuti {kept} > disponibili {available}")]
    TicketChoiceTooMany { kept: usize, available: usize },

    #[error("biglietto '{ticket_id}' non tra quelli offerti")]
    TicketNotOffered { ticket_id: String },

    #[error("tutti i giocatori devono completare il draft iniziale")]
    DraftNotComplete,

    #[error("nessuna carta disponibile nel mazzo")]
    DeckEmpty,

    #[error("giocatore {index} non trovato")]
    PlayerNotFound { index: usize },

    #[error("rotta '{route_id}' non trovata nella mappa")]
    RouteNotFound { route_id: String },

    #[error("colore della rotta non valido: '{color}'")]
    InvalidRouteColor { color: String },

    #[error("rotta '{route_id}': giocatore {player} ha già l'altra doppia")]
    SamePlayerDouble { route_id: String, player: usize },

    #[error("payload non valido: {reason}")]
    InvalidPayload { reason: String },
}

/// Fase di gioco (macchina a stati interna).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GamePhase {
    InitialDraft,
    PlayerTurn,
    SecondCardDraw,
    TicketChoice,
    LastRound,
    GameOver,
}
