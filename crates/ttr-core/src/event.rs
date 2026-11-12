//! Eventi emessi dal motore dopo ogni azione.
//!
//! Ogni `apply()` restituisce un vettore di eventi che descrivono
//! cosa è successo. Il server li usa per aggiornare lo stato broadcast
//! e privato.

use serde::{Deserialize, Serialize};
use ttr_proto::CardColor;

use crate::types::{CityId, RouteId, TicketId};

/// Evento emesso da `Engine::apply`, in ordine cronologico nel vettore restituito.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GameEvent {
    /// Carta pescata dal mazzo coperto.
    CardDrawnFromDeck {
        player_index: usize,
        card: CardColor,
    },
    /// Carta pescata dalle scoperte.
    CardDrawnFromFaceUp {
        player_index: usize,
        index: usize,
        card: CardColor,
    },
    /// Nuova carta apparsa nello slot `index` delle scoperte.
    FaceUpRevealed { index: usize, card: CardColor },
    /// Pool scoperte resettato (≥3 locomotive): `discarded` sono le scoperte
    /// scartate, `new_face_up` le nuove cinque.
    FaceUpReset {
        discarded: Vec<CardColor>,
        new_face_up: Vec<Option<CardColor>>,
    },
    /// Mazzo ricostruito: scarti rimescolati oppure, con mazzo e scarti vuoti,
    /// mazzo nuovo (D27).
    DeckReshuffled,
    /// Turno di pesca completato.
    DrawTurnComplete { player_index: usize },
    /// Biglietti offerti a un giocatore.
    TicketsOffered {
        player_index: usize,
        ticket_ids: Vec<TicketId>,
    },
    /// Biglietti scelti (tenuti e scartati).
    TicketsChosen {
        player_index: usize,
        kept: Vec<TicketId>,
        discarded: Vec<TicketId>,
    },
    /// Draft iniziale completato per un giocatore.
    InitialDraftDone { player_index: usize },
    /// Tutti hanno completato il draft, si passa al primo turno.
    InitialDraftComplete,
    /// Rotta reclamata.
    RouteClaimed {
        route_id: RouteId,
        player_index: usize,
        cards_used: Vec<CardColor>,
        points: u32,
        trains_placed: u32,
    },
    /// Carte rivelate da un tunnel (Europe), prima di `RouteClaimed` o dell'attesa
    /// della decisione.
    TunnelRevealed {
        player_index: usize,
        route_id: RouteId,
        cards: Vec<CardColor>,
        color: CardColor,
        extra_cost: u32,
    },
    /// Tunnel rifiutato: le carte giocate tornano in mano, il turno finisce.
    TunnelDeclined {
        player_index: usize,
        route_id: RouteId,
    },
    /// Stazione costruita (Europe).
    StationBuilt {
        player_index: usize,
        city_id: CityId,
        cards_used: Vec<CardColor>,
    },
    /// Turno passato al giocatore successivo.
    TurnAdvanced {
        previous_player: usize,
        next_player: usize,
    },
    /// Ultimo giro annunciato.
    LastRoundAnnounced {
        trigger_player: usize,
        trains_remaining: u32,
    },
    /// Partita terminata.
    GameOver {
        final_scores: Vec<FinalScore>,
        longest_path_players: Vec<usize>,
        longest_path_bonus: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalScore {
    pub player_index: usize,
    pub route_points: i32,
    pub ticket_completed: i32,
    pub ticket_failed: i32,
    pub longest_path_bonus: i32,
    /// Bonus di chi ha completato più biglietti (modalità con `globetrotter`).
    #[serde(default)]
    pub globetrotter_bonus: i32,
    pub total: i32,
    /// Punti delle stazioni non costruite (Europe).
    #[serde(default)]
    pub station_points: i32,
    /// Rotte avversarie usate dalle stazioni (città, rotta) nell'assegnazione migliore.
    #[serde(default)]
    pub borrowed_routes: Vec<(CityId, RouteId)>,
    /// Vincitore (o pari merito dopo gli spareggi): totale più alto, poi
    /// più biglietti completati, poi bonus percorso più lungo.
    #[serde(default)]
    pub winner: bool,
}
