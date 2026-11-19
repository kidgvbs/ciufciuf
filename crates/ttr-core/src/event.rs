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
        /// Punti del claim: tabella per la lunghezza più i punti dei treni scartati sulle
        /// rotte di montagna (`mountains × pointsPerTrain`).
        points: u32,
        /// Treni posati sulla rotta (la lunghezza); i treni scartati per le X non sono contati.
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
    /// Carte Paese prese (Poland): per Paese, il valore della carta in cima alla pila.
    CountryCardsTaken {
        player_index: usize,
        cards: Vec<(CityId, u32)>,
    },
    /// Passeggeri presi con una rotta (Germany 2017): (città, colore) per estremo.
    PassengersTaken {
        player_index: usize,
        route_id: RouteId,
        taken: Vec<(CityId, String)>,
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
    /// Bonus dei biglietti con due percorsi distinti (modalità con `mandala`).
    #[serde(default)]
    pub mandala_bonus: i32,
    /// Punti dei treni scartati sulle rotte di montagna, già in `total` e non in
    /// `route_points` (modalità con `mountains`).
    #[serde(default)]
    pub mountain_points: i32,
    /// Bonus della rete di città più grande (modalità con `largest_network`).
    #[serde(default)]
    pub largest_network_bonus: i32,
    /// Città della rete più grande del giocatore (0 senza il bonus `largest_network`).
    #[serde(default)]
    pub largest_network: u32,
    pub total: i32,
    /// Punti delle stazioni non costruite (Europe).
    #[serde(default)]
    pub station_points: i32,
    /// Rotte avversarie usate dalle stazioni (città, rotta) nell'assegnazione migliore.
    #[serde(default)]
    pub borrowed_routes: Vec<(CityId, RouteId)>,
    /// Punti delle carte Paese prese (Poland).
    #[serde(default)]
    pub country_points: i32,
    /// Punti delle maggioranze di passeggeri (Germany 2017).
    #[serde(default)]
    pub passenger_points: i32,
    /// Vincitore (o pari merito dopo gli spareggi): totale più alto, poi gli spareggi del
    /// ruleset (`tieBreakers`; di default biglietti completati, poi bonus percorso più lungo).
    #[serde(default)]
    pub winner: bool,
}
