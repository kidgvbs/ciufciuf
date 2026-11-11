//! Errori della lobby: ogni variante ha un codice stabile, uno status HTTP e
//! un messaggio italiano per l'utente.

use axum::http::StatusCode;

use crate::names::NameError;

/// Errore di un'operazione di lobby (join, reassign, start, creazione).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LobbyError {
    #[error("Partita non trovata.")]
    GameNotFound,
    #[error("Ruleset non trovato.")]
    RulesetNotFound,
    #[error("L'attore della partita è terminato.")]
    GameActorGone,
    #[error("La partita è al completo.")]
    GameFull,
    #[error("La partita è già avviata.")]
    GameStarted,
    #[error("Non tutti i giocatori della partita salvata sono rientrati.")]
    RosterIncomplete,
    #[error("Questo colore è già stato scelto da un altro giocatore.")]
    ColorTaken,
    #[error("Questo nome è già in uso nella partita.")]
    NameTaken,
    #[error("Questo nome non è nella partita salvata.")]
    NameNotInSave,
    #[error("Token host non valido.")]
    Unauthorized,
    #[error(
        "Il token non corrisponde a quello del giocatore salvato: serve la riassegnazione dalla TV."
    )]
    ResumeTokenMismatch,
    #[error("{0}")]
    InvalidName(NameError),
    #[error("{0}")]
    BadRequest(String),
    #[error("Servono almeno {0} giocatori.")]
    MinPlayers(u32),
    #[error("Troppe partite attive: attendi che ne termini una.")]
    TooManyGames,
    #[error("Il seed scelto dal client non è ammesso su questo server.")]
    SeedNotAllowed,
    #[error("Il server si sta spegnendo.")]
    ShuttingDown,
    #[error("Errore interno del server.")]
    Internal(String),
}

impl LobbyError {
    /// Codice stabile dell'errore, letto dal client.
    pub fn code(&self) -> &'static str {
        match self {
            Self::GameNotFound | Self::GameActorGone => "GAME_NOT_FOUND",
            Self::RulesetNotFound => "RULESET_NOT_FOUND",
            Self::GameFull => "GAME_FULL",
            Self::GameStarted => "GAME_STARTED",
            Self::RosterIncomplete => "ROSTER_INCOMPLETE",
            Self::ColorTaken => "COLOR_TAKEN",
            Self::NameTaken => "NAME_TAKEN",
            Self::NameNotInSave => "NAME_NOT_IN_SAVE",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::ResumeTokenMismatch => "RESUME_TOKEN_MISMATCH",
            Self::InvalidName(_) => "INVALID_NAME",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::MinPlayers(_) => "MIN_PLAYERS",
            Self::TooManyGames => "TOO_MANY_GAMES",
            Self::SeedNotAllowed => "SEED_NOT_ALLOWED",
            Self::ShuttingDown => "SHUTTING_DOWN",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    /// Status HTTP associato.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::GameNotFound | Self::GameActorGone | Self::RulesetNotFound => {
                StatusCode::NOT_FOUND
            }
            Self::GameFull
            | Self::GameStarted
            | Self::RosterIncomplete
            | Self::ColorTaken
            | Self::NameTaken
            | Self::NameNotInSave => StatusCode::CONFLICT,
            Self::Unauthorized | Self::ResumeTokenMismatch => StatusCode::UNAUTHORIZED,
            Self::InvalidName(_)
            | Self::BadRequest(_)
            | Self::MinPlayers(_)
            | Self::SeedNotAllowed => StatusCode::BAD_REQUEST,
            Self::ShuttingDown => StatusCode::SERVICE_UNAVAILABLE,
            Self::TooManyGames => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<NameError> for LobbyError {
    fn from(e: NameError) -> Self {
        Self::InvalidName(e)
    }
}
