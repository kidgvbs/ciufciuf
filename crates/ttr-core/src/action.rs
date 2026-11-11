//! Azioni del motore (enum interno, diverso dal proto InboundAction).
//!
//! Il server traduce i messaggi inbound del proto in queste azioni.
//! Il `player_index` è l'indice interno (0..num_players).

use serde::{Deserialize, Serialize};
use ttr_proto::CardColor;

use crate::types::{CityId, RouteId, TicketId};

/// Sorgente di pesca di una carta treno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DrawSource {
    Deck,
    FaceUp { index: usize },
}

/// Un'azione di gioco inviata al motore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Pescare una carta treno.
    DrawTrainCard {
        player_index: usize,
        source: DrawSource,
    },
    /// Pescare nuovi biglietti (mid-game).
    DrawTickets { player_index: usize },
    /// Scegliere quali biglietti tenere (draft iniziale o mid-game).
    ChooseTickets {
        player_index: usize,
        keep: Vec<TicketId>,
    },
    /// Reclamare una rotta.
    ClaimRoute {
        player_index: usize,
        route_id: RouteId,
        /// Suggerimento del client; il motore valida e risolve.
        cards_discarded: Vec<CardColor>,
    },
    /// Costruire una stazione in una città (Europe).
    BuildStation {
        player_index: usize,
        city_id: CityId,
        /// Carte pagate; una carta assente in mano è sostituita da una locomotiva.
        cards_discarded: Vec<CardColor>,
    },
    /// Decisione su un tunnel rivelato (Europe): pagare il costo extra o rinunciare.
    TunnelDecision {
        player_index: usize,
        pay: bool,
        /// Con `pay`: le carte extra; ignorato con la rinuncia.
        cards_discarded: Vec<CardColor>,
    },
}

impl Action {
    /// Restituisce l'indice del giocatore che compie l'azione.
    pub fn player_index(&self) -> usize {
        match self {
            Action::DrawTrainCard { player_index, .. }
            | Action::DrawTickets { player_index }
            | Action::ChooseTickets { player_index, .. }
            | Action::ClaimRoute { player_index, .. }
            | Action::BuildStation { player_index, .. }
            | Action::TunnelDecision { player_index, .. } => *player_index,
        }
    }
}
