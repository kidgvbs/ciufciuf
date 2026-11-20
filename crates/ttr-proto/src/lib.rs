//! CiufCiuf! — Tipi condivisi del contratto WebSocket.
//!
//! Questo crate definisce i tipi serializzabili (serde) che costituiscono
//! il contratto tra server Rust e client TypeScript. L'export TypeScript
//! via `ts-rs` garantisce che i tipi non vadano in drift.
//!
//! # Convenzioni di identificazione
//!
//! - **CityId**: lowercase snake case, es. `"new_york"`, `"salt_lake_city"`.
//! - **RouteId**: `{cityA}_{cityB}_{n}` con città in ordine lessicografico,
//!   suffisso `_1`..`_4` per i binari di un gruppo parallelo (2–4), es. `"denver_phoenix_1"`.
//!   (Nuovo: `_1` anche per le rotte singole, così il formato è uniforme.)
//! - **TicketId**: `{cityA}_{cityB}` con città in ordine lessicografico,
//!   es. `"los_angeles_new_york"`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ── Colori ──────────────────────────────────────────────────────────

/// Colore di una carta treno. `Locomotive` è il jolly / rainbow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum CardColor {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Black,
    White,
    /// Jolly — sostituisce qualunque colore, conta come locomotiva per le regole di pesca.
    Locomotive,
}

/// Colore giocatore (plancia + trenini).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum PlayerColor {
    Pink,
    Cyan,
    Lime,
    Brown,
    Lavender,
    Indigo,
}

// ── Stati di gioco ──────────────────────────────────────────────────

/// Macchina a stati del server.
///
/// L'enum è marcato `#[non_exhaustive]` per consentire l'aggiunta di stati
/// futuri (es. `WAITING_FOR_PLAYERS` e stati Europe) senza breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
#[non_exhaustive]
pub enum GameState {
    /// Lobby: in attesa di giocatori. I client collegati vedono i join in tempo reale.
    WaitingForPlayers,
    /// Draft iniziale biglietti — tutti in parallelo.
    InitialDraft,
    /// Turno normale: il giocatore sceglie una delle 3 (o 4 in Europe) azioni.
    PlayerTurn,
    /// Seconda pescata carta (solo se la prima NON era locomotiva scoperta).
    SecondCardDraw,
    /// Scelta biglietti dopo pesca mid-game.
    TicketChoice,
    /// Ultimo giro: banner attivo, contatore turni rimasti.
    LastRound,
    /// Europe: risoluzione tunnel dopo `CLAIM_ROUTE` su rotta tunnel.
    TunnelResolution,
    /// Europe: assegnazione stazioni a fine partita (server-side).
    StationScoring,
    /// Partita terminata, schermata punteggi.
    GameOver,
}

// ── Messaggi inbound (client → server) ──────────────────────────────

/// Azione inbound (client → server). Messaggio inviato su `game/{gameId}/action`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "actionType")]
#[ts(export)]
pub enum InboundAction {
    /// Pescare una carta treno dal mazzo coperto.
    #[serde(rename = "DRAW_TRAIN_CARD")]
    DrawTrainCard(DrawCardPayload),
    /// Pescare nuovi biglietti mid-game (azione nº 3 del turno).
    #[serde(rename = "DRAW_TICKETS")]
    DrawTickets(DrawTicketsPayload),
    /// Italy: pescare una carta Traghetto dal mazzetto (azione intera del turno).
    #[serde(rename = "DRAW_FERRY_CARD")]
    DrawFerryCard(DrawFerryCardPayload),
    /// Scegliere quali biglietti tenere (initial draft o mid-game).
    #[serde(rename = "CHOOSE_TICKETS")]
    ChooseTickets(ChooseTicketsPayload),
    /// Reclamare una rotta.
    #[serde(rename = "CLAIM_ROUTE")]
    ClaimRoute(ClaimRoutePayload),
    /// Richiedere lo snapshot completo dopo reconnect.
    #[serde(rename = "REQUEST_SNAPSHOT")]
    RequestSnapshot(RequestSnapshotPayload),
    /// Europe: costruire una stazione (azione del turno, D12).
    #[serde(rename = "BUILD_STATION")]
    BuildStation(BuildStationPayload),
    /// Europe: in `TUNNEL_RESOLUTION`, pagare le carte extra (`pay: true`) o rinunciare
    /// (`pay: false`: le carte giocate tornano in mano e il turno finisce, D13).
    #[serde(rename = "TUNNEL_DECISION")]
    TunnelDecision(TunnelDecisionPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrawCardPayload {
    pub player_id: String,
    pub source: DrawSource,
    /// Numero di sequenza client per idempotenza: un valore minore o uguale
    /// all'ultimo ricevuto è un duplicato scartato. Assente = nessuna deduplica.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DrawSource {
    Deck,
    FaceUp { index: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrawTicketsPayload {
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrawFerryCardPayload {
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChooseTicketsPayload {
    pub player_id: String,
    /// Id dei biglietti da tenere (in ordine lessicografico).
    pub keep: Vec<TicketId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClaimRoutePayload {
    pub player_id: String,
    pub route_id: RouteId,
    /// Suggerimento: il server valida e risolve il pagamento (locomotive automatiche).
    pub cards_discarded: Vec<CardColor>,
    /// Germany 2017: colore del passeggero scelto negli estremi con passeggeri di più colori
    /// (negli altri è automatico). Assente senza passeggeri.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub passengers: Option<Vec<PassengerPick>>,
    /// Italy: carte Traghetto giocate; ognuna copre 1 o `wavesPerCard` spazi onda della
    /// rotta, le altre onde si pagano con locomotive in `cards_discarded`. Assente = 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ferry_cards: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RequestSnapshotPayload {
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BuildStationPayload {
    pub player_id: String,
    pub city_id: CityId,
    /// Carte pagate: tante quante il costo della prossima stazione (`legalMoves.stationCost`),
    /// tutte dello stesso colore; le locomotive fanno da jolly.
    pub cards_discarded: Vec<CardColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TunnelDecisionPayload {
    pub player_id: String,
    /// `true` paga le carte extra, `false` rinuncia al tunnel.
    pub pay: bool,
    /// Con `pay: true`: esattamente `extraCost` carte, ognuna del colore del tunnel
    /// (`TunnelReveal.color`) o locomotiva; con `color = LOCOMOTIVE` solo locomotive.
    /// Ignorato con `pay: false`.
    pub cards_discarded: Vec<CardColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

// ── Envelope outbound ───────────────────────────────────────────────

/// Messaggio outbound inviato sul WebSocket.
///
/// L'envelope usa adjacent tagging: `{"type":"PUBLIC_STATE","payload":{…}}`.
/// Ogni canale di notifica è una variante di questo enum.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "payload")]
#[ts(export)]
// Gli snapshot sono rari rispetto agli stati pubblici: il box cambierebbe il tipo senza benefici.
#[allow(clippy::large_enum_variant)]
pub enum ServerMessage {
    /// Broadcast pubblico a tutti i client (giocatori e spettatori).
    #[serde(rename = "PUBLIC_STATE")]
    PublicState(PublicState),
    /// Unicast privato a un singolo giocatore.
    #[serde(rename = "PRIVATE_STATE")]
    PrivateState(PrivateState),
    /// Errore su un'azione, inviato solo al giocatore interessato.
    #[serde(rename = "ERROR")]
    Error(ErrorPayload),
    /// Snapshot completo dopo (re)connect o su richiesta.
    #[serde(rename = "SNAPSHOT")]
    Snapshot(Snapshot),
}

// ── Messaggi outbound: payload ──────────────────────────────────────

/// Broadcast pubblico: stato della partita visibile a tutti.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PublicState {
    pub game_id: String,
    pub protocol_version: u32,
    pub state: GameState,
    pub current_turn_player_id: Option<String>,
    pub last_round: bool,
    pub last_round_trigger_player_id: Option<String>,
    /// Carte scoperte: `None` = slot vuoto. Gli indici di `drawFaceUp` restano allineati.
    pub visible_deck: Vec<Option<CardColor>>,
    /// Numero di carte nel mazzo coperto (solo la pila pesca, NON gli scarti).
    pub deck_size: u32,
    pub claimed_routes: Vec<ClaimedRoute>,
    pub players_summary: Vec<PlayerSummary>,
    /// Europe: in `TUNNEL_RESOLUTION`, le carte rivelate e costo extra.
    pub tunnel_reveal: Option<TunnelReveal>,
    /// Europe: stazioni costruite.
    pub stations: Vec<StationPlacement>,
    /// Punteggi finali: `None` finché la partita non è terminata.
    pub final_scores: Option<Vec<FinalScoreInfo>>,
    /// Vincitori del longest path: id giocatori, non indici.
    pub longest_path_player_ids: Option<Vec<String>>,
    /// Id delle rotte di un percorso più lungo di ciascun vincitore del bonus, ordinati e
    /// senza duplicati; `None` finché la partita non è terminata. Solo rotte possedute dal
    /// giocatore, non quelle prese in prestito dalle stazioni.
    pub longest_path_route_ids: Option<Vec<String>>,
    /// Ultime mosse della partita, dalla più vecchia alla più recente (al più
    /// `RECENT_MOVES_MAX`), con sole informazioni pubbliche: niente carte pescate
    /// dal mazzo né biglietti. Il draft iniziale non compare.
    #[serde(default)]
    pub recent_moves: Vec<RecentMove>,
    /// Poland: carte Paese rimaste in ogni pila, dall'alto in basso (pubbliche); vuoto
    /// senza carte Paese.
    #[serde(default)]
    pub country_stacks: Vec<CountryCards>,
    /// Germany 2017: passeggeri ancora sulle città, in ordine di id città; vuoto senza
    /// passeggeri.
    #[serde(default)]
    pub passengers: Vec<CityPassengers>,
    /// Id dei binari liberi che nessun giocatore può più reclamare: un altro binario del
    /// loro gruppo parallelo è preso e la partita ha meno di `doubleRouteMinPlayers`
    /// giocatori (gruppo non `alwaysOpen`). Ordinati, senza duplicati; mai binari già presi.
    #[serde(default)]
    pub blocked_route_ids: Vec<RouteId>,
    /// Biglietti ancora pescabili con `DRAW_TICKETS` a partita in corso (pila di
    /// `ticketDraw`; per Germany la pila unita di lunghi e corti).
    #[serde(default)]
    pub ticket_deck_size: u32,
}

/// Numero massimo di voci in `PublicState.recent_moves`.
pub const RECENT_MOVES_MAX: usize = 8;

/// Una mossa recente (un turno di un giocatore), visibile a tutti.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum RecentMove {
    /// Pesca carte treno: `deck` carte coperte (colore nascosto) e le scoperte prese.
    DrawCards {
        player_id: String,
        deck: u32,
        face_up: Vec<CardColor>,
    },
    /// Pesca biglietti a metà partita: quanti ne ha tenuti (`None` finché sceglie).
    DrawTickets {
        player_id: String,
        kept: Option<u32>,
    },
    /// Italy: carta Traghetto pescata dal mazzetto.
    DrawFerryCard { player_id: String },
    /// Rotta reclamata (non tunnel).
    ClaimRoute {
        player_id: String,
        route_id: RouteId,
        points: u32,
    },
    /// Europe: tentativo di tunnel con le carte rivelate e l'esito.
    Tunnel {
        player_id: String,
        route_id: RouteId,
        revealed: Vec<CardColor>,
        color: CardColor,
        extra_cost: u32,
        outcome: TunnelOutcome,
    },
    /// Europe: stazione costruita.
    BuildStation { player_id: String, city_id: CityId },
}

/// Esito di un tentativo di tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum TunnelOutcome {
    /// Costo extra 0: rotta presa subito (D31).
    Claimed,
    /// In attesa della decisione (`TUNNEL_RESOLUTION`).
    Pending,
    /// Carte extra pagate, rotta presa.
    Paid,
    /// Rinuncia: carte giocate tornate in mano.
    Declined,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClaimedRoute {
    pub route_id: RouteId,
    pub claimed_by_player_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlayerSummary {
    pub player_id: String,
    pub name: String,
    pub color: PlayerColor,
    pub trains_remaining: u32,
    pub stations_remaining: u32,
    pub score: i32,
    pub cards_hand_count: u32,
    pub tickets_count: u32,
    /// `true` se il giocatore ha ≥1 connessione WS attiva (presence runtime).
    pub connected: bool,
    /// Poland: carte Paese prese dal giocatore, per Paese, in ordine di presa (pubbliche).
    #[serde(default)]
    pub country_cards: Vec<CountryCards>,
    /// Germany 2017: passeggeri raccolti per colore (pubblici); vuoto senza passeggeri.
    #[serde(default)]
    pub passengers: Vec<PassengerCount>,
    /// Italy: carte Traghetto in mano (pubbliche); 0 senza carte Traghetto.
    #[serde(default)]
    pub ferry_cards: u32,
    /// Italy: Regioni contate in ogni rete di rotte del giocatore, in ordine decrescente;
    /// vuoto senza il bonus `regions`.
    #[serde(default)]
    pub region_networks: Vec<u32>,
    /// Italy: punti attuali del bonus Regioni (somma sulle reti); 0 senza il bonus.
    #[serde(default)]
    pub regions_bonus: i32,
}

/// Europe: carte rivelate da un tentativo di tunnel in attesa della decisione
/// (`TUNNEL_RESOLUTION`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TunnelReveal {
    pub route_id: RouteId,
    /// Carte rivelate dal mazzo (fino a 3; meno se mazzo e scarti non bastano).
    pub cards: Vec<CardColor>,
    /// Colore che fa scattare il costo extra: quello delle carte giocate, oppure
    /// `LOCOMOTIVE` se il giocatore ha pagato solo con locomotive.
    pub color: CardColor,
    /// Carte extra da pagare (rivelate del colore `color` più locomotive rivelate).
    pub extra_cost: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationPlacement {
    pub city_id: CityId,
    pub player_id: String,
}

/// Punteggio finale per giocatore.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FinalScoreInfo {
    pub player_id: String,
    pub route_points: i32,
    pub ticket_completed: i32,
    pub ticket_failed: i32,
    pub longest_path_bonus: i32,
    /// Modalità con `globetrotter`: bonus di chi ha completato più biglietti.
    #[serde(default)]
    pub globetrotter_bonus: i32,
    /// Modalità con `mandala`: bonus dei biglietti con due percorsi distinti (Grand Tour).
    #[serde(default)]
    pub mandala_bonus: i32,
    /// Modalità con `mountains`: punti dei treni scartati sulle rotte di montagna (già in
    /// `total`, non in `route_points`).
    #[serde(default)]
    pub mountain_points: i32,
    /// Modalità con `largest_network`: bonus della rete di città più grande (Asian Explorer).
    #[serde(default)]
    pub largest_network_bonus: i32,
    /// Modalità con `largest_network`: città della rete più grande del giocatore.
    #[serde(default)]
    pub largest_network: u32,
    /// Modalità con `regions`: bonus delle Regioni collegate (Italy).
    #[serde(default)]
    pub regions_bonus: i32,
    /// Modalità con `regions`: Regioni contate in ogni rete del giocatore, in ordine
    /// decrescente.
    #[serde(default)]
    pub region_networks: Vec<u32>,
    /// Europe: punti delle stazioni non costruite (0 sulle mappe senza stazioni).
    #[serde(default)]
    pub station_points: i32,
    pub total: i32,
    /// Vincitore (pari merito: più vincitori). Calcolato dal motore con gli
    /// spareggi: totale, biglietti completati, bonus percorso più lungo.
    #[serde(default)]
    pub winner: bool,
    /// Europe: rotte avversarie prese in prestito dalle stazioni nell'assegnazione
    /// che massimizza i biglietti (una per stazione al massimo).
    #[serde(default)]
    pub borrowed_routes: Vec<BorrowedRoute>,
    /// Poland: punti delle carte Paese prese (0 senza carte Paese).
    #[serde(default)]
    pub country_points: i32,
    /// Germany 2017: punti delle maggioranze di passeggeri (0 senza passeggeri).
    #[serde(default)]
    pub passenger_points: i32,
    /// Esito di ogni biglietto del giocatore (per id): destinazione realizzata e punti.
    #[serde(default)]
    pub tickets: Vec<TicketResultInfo>,
}

/// Esito finale di un biglietto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TicketResultInfo {
    pub ticket_id: TicketId,
    pub completed: bool,
    /// Destinazione realizzata (la collegata di punti più alti); `None` se fallito.
    pub city_id: Option<CityId>,
    /// Punti guadagnati o, se fallito, persi (negativi).
    pub points: i32,
    /// Conta per il bonus `mandala`: due percorsi distinti tra le città del biglietto.
    #[serde(default)]
    pub mandala: bool,
}

/// Rotta avversaria usata da una stazione nel punteggio finale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BorrowedRoute {
    pub city_id: CityId,
    pub route_id: RouteId,
}

/// Broadcast unicast privato: stato visibile solo al giocatore interessato.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrivateState {
    /// Numero di sequenza outbound: cresce a ogni messaggio privato per il giocatore.
    pub seq: u64,
    pub hand: Vec<CardColor>,
    pub tickets: Vec<TicketInfo>,
    /// Biglietti attualmente offerti per la scelta.
    pub offered_tickets: Vec<TicketInfo>,
    pub legal_moves: LegalMoves,
}

/// Biglietto in mano o offerto. Le destinazioni sono sempre almeno una (i biglietti classici
/// ne hanno una sola); la partenza e i nomi delle città vengono dalla mappa.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TicketInfo {
    pub id: TicketId,
    /// Punti del biglietto; con più destinazioni il valore più alto.
    pub points: u32,
    pub completed: bool,
    /// Destinazioni con i loro punti, nell'ordine della mappa. Con più destinazioni vale
    /// il punteggio più alto tra quelle collegate; se nessuna lo è si perde il più basso.
    #[serde(default)]
    pub destinations: Vec<TicketDestinationInfo>,
}

/// Destinazione di un biglietto con i punti che vale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TicketDestinationInfo {
    pub city_id: CityId,
    pub points: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LegalMoves {
    pub routes: Vec<RouteId>,
    pub draw_face_up: Vec<usize>,
    pub can_draw_deck: bool,
    pub can_draw_tickets: bool,
    /// Italy: si può pescare una carta Traghetto (a inizio turno, con meno di `maxInHand`).
    #[serde(default)]
    pub can_draw_ferry_card: bool,
    /// Europe: città dove il giocatore può costruire ora la prossima stazione
    /// (vuoto se non ne ha più, se non può pagarla o fuori dal suo turno principale).
    pub station_cities: Vec<CityId>,
    /// Europe: costo in carte della prossima stazione; 0 senza stazioni disponibili.
    pub station_cost: u32,
    /// Europe: decisione richiesta in `TUNNEL_RESOLUTION`, solo al giocatore di turno.
    pub tunnel: Option<TunnelDecisionMoves>,
    /// Scelta dei biglietti offerti: nel draft iniziale (ogni giocatore che non ha
    /// ancora scelto) e in `TICKET_CHOICE` per il giocatore di turno.
    #[serde(default)]
    pub ticket_choice: Option<TicketChoiceMoves>,
}

/// Valori delle carte Paese di un Paese (Poland), ordine dall'alto in basso per le pile,
/// ordine di presa per le carte dei giocatori.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CountryCards {
    pub country_id: CityId,
    pub values: Vec<u32>,
}

/// Passeggeri di un colore (Germany 2017): `color` è il nome del ruleset (es. `"Red"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PassengerCount {
    pub color: String,
    pub count: u32,
}

/// Passeggeri ancora su una città (Germany 2017), per colore in ordine alfabetico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CityPassengers {
    pub city_id: CityId,
    pub passengers: Vec<PassengerCount>,
}

/// Colore del passeggero scelto in un estremo della rotta (Germany 2017).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PassengerPick {
    pub city_id: CityId,
    pub color: String,
}

/// Vincoli della scelta dei biglietti offerti.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TicketChoiceMoves {
    /// Minimo di biglietti da tenere.
    pub keep_min: u32,
    /// Limiti per gruppo di biglietti offerti (oggi: lunghi in Mega Europe).
    pub limits: Vec<TicketKeepLimit>,
}

/// Tra `ticket_ids` (nell'ordine di offerta) se ne tengono almeno `min` e al massimo `max`.
/// Un gruppo compare solo se il ruleset fissa un minimo o un massimo per quel mazzo;
/// senza massimo, `max` è il numero di biglietti del gruppo.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TicketKeepLimit {
    pub ticket_ids: Vec<TicketId>,
    pub min: u32,
    pub max: u32,
}

/// Europe: cosa può fare il giocatore di turno davanti a un tunnel rivelato.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TunnelDecisionMoves {
    pub route_id: RouteId,
    pub extra_cost: u32,
    /// Colore delle carte extra ammesse oltre alle locomotive (`LOCOMOTIVE` = solo locomotive).
    pub color: CardColor,
    /// `true` se la mano copre il costo extra; altrimenti resta solo la rinuncia.
    pub can_pay: bool,
}

/// Payload di errore (dentro `ServerMessage::Error`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ErrorPayload {
    pub seq: u64,
    pub reason: ErrorReason,
    pub action_ref: Option<String>,
    /// Messaggio leggibile (italiano) per l'utente, se disponibile.
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum ErrorReason {
    // Errori di gioco (da EngineError)
    InsufficientCards,
    RouteAlreadyClaimed,
    RouteBlockedDoubleVariant,
    InvalidAction,
    NotYourTurn,
    WrongState,
    TicketChoiceOutOfRange,
    // Errori server-level
    GameNotFound,
    Unauthorized,
    GameFull,
    NameTaken,
}

/// Snapshot completo dopo (re)connect.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Snapshot {
    pub protocol_version: u32,
    pub public: PublicState,
    /// `None` per gli spettatori (table view senza `playerId`), `Some` per i giocatori.
    pub private: Option<PrivateState>,
}

// ── Newtype id ──────────────────────────────────────────────────────

/// Identificatore città. Forma: lowercase snake (`"new_york"`, `"salt_lake_city"`).
pub type CityId = String;

/// Identificatore rotta. Forma: `{cityA}_{cityB}_{n}` con città in ordine lessicografico.
/// Le rotte singole usano `_1`; le doppie usano `_1` e `_2`.
pub type RouteId = String;

/// Identificatore biglietto. Forma: `{cityA}_{cityB}` in ordine lessicografico.
pub type TicketId = String;

// ── Test round-trip serde ───────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_card_color() {
        let vals = [
            CardColor::Red,
            CardColor::Orange,
            CardColor::Yellow,
            CardColor::Green,
            CardColor::Blue,
            CardColor::Purple,
            CardColor::Black,
            CardColor::White,
            CardColor::Locomotive,
        ];
        for v in vals {
            let json = serde_json::to_string(&v).unwrap();
            let back: CardColor = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn roundtrip_player_color() {
        let vals = [
            PlayerColor::Pink,
            PlayerColor::Cyan,
            PlayerColor::Lime,
            PlayerColor::Brown,
            PlayerColor::Lavender,
            PlayerColor::Indigo,
        ];
        for v in vals {
            let json = serde_json::to_string(&v).unwrap();
            let back: PlayerColor = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn roundtrip_game_state() {
        let json = serde_json::to_string(&GameState::PlayerTurn).unwrap();
        assert_eq!(json, "\"PLAYER_TURN\"");
        let back: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, GameState::PlayerTurn);
    }

    #[test]
    fn roundtrip_game_state_waiting() {
        let json = serde_json::to_string(&GameState::WaitingForPlayers).unwrap();
        assert_eq!(json, "\"WAITING_FOR_PLAYERS\"");
        let back: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, GameState::WaitingForPlayers);
    }

    #[test]
    fn roundtrip_inbound_draw() {
        let msg = InboundAction::DrawTrainCard(DrawCardPayload {
            player_id: "p1".into(),
            source: DrawSource::FaceUp { index: 2 },
            client_seq: Some(42),
        });
        let json = serde_json::to_string(&msg).unwrap();
        let back: InboundAction = serde_json::from_str(&json).unwrap();
        match back {
            InboundAction::DrawTrainCard(p) => {
                assert_eq!(p.player_id, "p1");
                assert!(matches!(p.source, DrawSource::FaceUp { index: 2 }));
                assert_eq!(p.client_seq, Some(42));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_inbound_draw_no_seq() {
        let msg = InboundAction::DrawTrainCard(DrawCardPayload {
            player_id: "p1".into(),
            source: DrawSource::Deck,
            client_seq: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        // client_seq=None non deve apparire nel JSON
        assert!(!json.contains("clientSeq"));
        assert!(json.contains("DRAW_TRAIN_CARD"));
        let _back: InboundAction = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn client_seq_none_is_omitted() {
        let player_id = "p".to_string();
        let msgs = [
            InboundAction::DrawTrainCard(DrawCardPayload {
                player_id: player_id.clone(),
                source: DrawSource::Deck,
                client_seq: None,
            }),
            InboundAction::DrawFerryCard(DrawFerryCardPayload {
                player_id: player_id.clone(),
                client_seq: None,
            }),
            InboundAction::DrawTickets(DrawTicketsPayload {
                player_id: player_id.clone(),
                client_seq: None,
            }),
            InboundAction::ChooseTickets(ChooseTicketsPayload {
                player_id: player_id.clone(),
                keep: vec![],
                client_seq: None,
            }),
            InboundAction::ClaimRoute(ClaimRoutePayload {
                player_id: player_id.clone(),
                route_id: "r".into(),
                cards_discarded: vec![],
                passengers: None,
                ferry_cards: None,
                client_seq: None,
            }),
            InboundAction::BuildStation(BuildStationPayload {
                player_id: player_id.clone(),
                city_id: "c".into(),
                cards_discarded: vec![],
                client_seq: None,
            }),
            InboundAction::TunnelDecision(TunnelDecisionPayload {
                player_id: player_id.clone(),
                pay: false,
                cards_discarded: vec![],
                client_seq: None,
            }),
            InboundAction::RequestSnapshot(RequestSnapshotPayload {
                player_id,
                client_seq: None,
            }),
        ];
        for msg in msgs {
            let json = serde_json::to_string(&msg).unwrap();
            assert!(!json.contains("client_seq"), "{json}");
            let back: InboundAction = serde_json::from_str(&json).unwrap();
            assert_eq!(serde_json::to_string(&back).unwrap(), json);
        }
    }
    #[test]
    fn roundtrip_inbound_choose() {
        let msg = InboundAction::ChooseTickets(ChooseTicketsPayload {
            player_id: "p2".into(),
            keep: vec!["denver_phoenix".into(), "seattle_new_york".into()],
            client_seq: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let back: InboundAction = serde_json::from_str(&json).unwrap();
        match back {
            InboundAction::ChooseTickets(p) => assert_eq!(p.keep.len(), 2),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_inbound_draw_tickets() {
        let msg = InboundAction::DrawTickets(DrawTicketsPayload {
            player_id: "p2".into(),
            client_seq: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("DRAW_TICKETS"));
        let back: InboundAction = serde_json::from_str(&json).unwrap();
        match back {
            InboundAction::DrawTickets(p) => assert_eq!(p.player_id, "p2"),
            _ => panic!("wrong variant"),
        }
    }

    /// Forma sul filo delle azioni Europe, come la invia il client.
    #[test]
    fn inbound_europe_actions_wire_shape() {
        let station: InboundAction = serde_json::from_str(
            r#"{"actionType":"BUILD_STATION","player_id":"p1","city_id":"paris","cards_discarded":["RED","LOCOMOTIVE"],"client_seq":7}"#,
        )
        .unwrap();
        match station {
            InboundAction::BuildStation(p) => {
                assert_eq!(p.city_id, "paris");
                assert_eq!(p.cards_discarded, [CardColor::Red, CardColor::Locomotive]);
                assert_eq!(p.client_seq, Some(7));
            }
            other => panic!("wrong variant {other:?}"),
        }
        let tunnel: InboundAction = serde_json::from_str(
            r#"{"actionType":"TUNNEL_DECISION","player_id":"p1","pay":true,"cards_discarded":["BLACK"]}"#,
        )
        .unwrap();
        match tunnel {
            InboundAction::TunnelDecision(p) => {
                assert!(p.pay);
                assert_eq!(p.cards_discarded, [CardColor::Black]);
                assert_eq!(p.client_seq, None);
            }
            other => panic!("wrong variant {other:?}"),
        }
    }

    #[test]
    fn roundtrip_inbound_request_snapshot() {
        let msg = InboundAction::RequestSnapshot(RequestSnapshotPayload {
            player_id: "p3".into(),
            client_seq: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let _back: InboundAction = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn roundtrip_public_state() {
        let state = PublicState {
            game_id: "g1".into(),
            protocol_version: 1,
            state: GameState::PlayerTurn,
            current_turn_player_id: Some("p1".into()),
            last_round: false,
            last_round_trigger_player_id: None,
            visible_deck: vec![
                Some(CardColor::Red),
                Some(CardColor::Blue),
                Some(CardColor::Locomotive),
            ],
            deck_size: 0,
            claimed_routes: vec![],
            players_summary: vec![],
            tunnel_reveal: None,
            stations: vec![],
            final_scores: None,
            longest_path_player_ids: None,
            longest_path_route_ids: None,
            recent_moves: vec![
                RecentMove::DrawCards {
                    player_id: "p1".into(),
                    deck: 1,
                    face_up: vec![CardColor::Red],
                },
                RecentMove::Tunnel {
                    player_id: "p2".into(),
                    route_id: "madrid_pamplona_1".into(),
                    revealed: vec![CardColor::Black, CardColor::Locomotive],
                    color: CardColor::Black,
                    extra_cost: 2,
                    outcome: TunnelOutcome::Pending,
                },
            ],
            country_stacks: vec![],
            passengers: vec![],
            blocked_route_ids: vec![],
            ticket_deck_size: 0,
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: PublicState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.game_id, "g1");
        assert_eq!(back.visible_deck.len(), 3);
        assert!(back.final_scores.is_none());
        assert_eq!(back.recent_moves, state.recent_moves);
        assert!(
            json.contains(r#""kind":"DRAW_CARDS","player_id":"p1","deck":1"#),
            "{json}"
        );
        assert!(json.contains(r#""outcome":"PENDING""#), "{json}");
    }

    #[test]
    fn roundtrip_public_state_final_scores() {
        let state = PublicState {
            game_id: "g1".into(),
            protocol_version: 1,
            state: GameState::GameOver,
            current_turn_player_id: None,
            last_round: false,
            last_round_trigger_player_id: None,
            visible_deck: vec![],
            deck_size: 0,
            claimed_routes: vec![],
            players_summary: vec![],
            tunnel_reveal: None,
            stations: vec![],
            final_scores: Some(vec![FinalScoreInfo {
                player_id: "p1".into(),
                route_points: 42,
                ticket_completed: 30,
                ticket_failed: -10,
                longest_path_bonus: 10,
                globetrotter_bonus: 15,
                mandala_bonus: 20,
                mountain_points: 12,
                largest_network_bonus: 10,
                largest_network: 9,
                regions_bonus: 29,
                region_networks: vec![9, 6],
                station_points: 4,
                country_points: 7,
                passenger_points: 0,
                tickets: vec![TicketResultInfo {
                    ticket_id: "a_b_c".into(),
                    completed: true,
                    city_id: Some("b".into()),
                    points: 13,
                    mandala: true,
                }],
                total: 91,
                winner: true,
                borrowed_routes: vec![BorrowedRoute {
                    city_id: "paris".into(),
                    route_id: "dieppe_paris_1".into(),
                }],
            }]),
            longest_path_player_ids: Some(vec!["p1".into()]),
            longest_path_route_ids: Some(vec!["a_b_1".into(), "b_c_1".into()]),
            recent_moves: vec![],
            country_stacks: vec![],
            passengers: vec![],
            blocked_route_ids: vec![],
            ticket_deck_size: 0,
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: PublicState = serde_json::from_str(&json).unwrap();
        let scores = back.final_scores.unwrap();
        assert_eq!(scores[0].total, 91);
        assert_eq!(scores[0].globetrotter_bonus, 15);
        assert_eq!(scores[0].mandala_bonus, 20);
        assert_eq!(scores[0].mountain_points, 12);
        assert_eq!(scores[0].largest_network_bonus, 10);
        assert_eq!(scores[0].largest_network, 9);
        assert_eq!(scores[0].regions_bonus, 29);
        assert_eq!(scores[0].region_networks, [9, 6]);
        assert!(scores[0].tickets[0].mandala);
        assert_eq!(scores[0].station_points, 4);
        assert_eq!(scores[0].borrowed_routes[0].route_id, "dieppe_paris_1");
        assert_eq!(back.longest_path_player_ids.unwrap(), vec!["p1"]);
        assert_eq!(back.longest_path_route_ids.unwrap(), ["a_b_1", "b_c_1"]);
    }

    #[test]
    fn roundtrip_public_state_visible_deck_none() {
        let state = PublicState {
            game_id: "g1".into(),
            protocol_version: 1,
            state: GameState::PlayerTurn,
            current_turn_player_id: Some("p1".into()),
            last_round: false,
            last_round_trigger_player_id: None,
            visible_deck: vec![Some(CardColor::Red), None, Some(CardColor::Blue)],
            deck_size: 0,
            claimed_routes: vec![],
            players_summary: vec![],
            tunnel_reveal: None,
            stations: vec![],
            final_scores: None,
            longest_path_player_ids: None,
            longest_path_route_ids: None,
            recent_moves: vec![],
            country_stacks: vec![],
            passengers: vec![],
            blocked_route_ids: vec![],
            ticket_deck_size: 0,
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: PublicState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.visible_deck.len(), 3);
        assert!(back.visible_deck[1].is_none());
    }

    #[test]
    fn roundtrip_private_state() {
        let state = PrivateState {
            seq: 42,
            hand: vec![CardColor::Red, CardColor::Blue, CardColor::Locomotive],
            tickets: vec![TicketInfo {
                id: "denver_phoenix".into(),
                points: 11,
                completed: false,
                destinations: vec![TicketDestinationInfo {
                    city_id: "phoenix".into(),
                    points: 11,
                }],
            }],
            offered_tickets: vec![],
            legal_moves: LegalMoves {
                routes: vec!["denver_phoenix_1".into()],
                draw_face_up: vec![0, 2],
                can_draw_deck: true,
                can_draw_tickets: true,
                can_draw_ferry_card: true,
                station_cities: vec!["paris".into()],
                station_cost: 2,
                tunnel: Some(TunnelDecisionMoves {
                    route_id: "madrid_pamplona_1".into(),
                    extra_cost: 1,
                    color: CardColor::Black,
                    can_pay: true,
                }),
                ticket_choice: Some(TicketChoiceMoves {
                    keep_min: 3,
                    limits: vec![TicketKeepLimit {
                        ticket_ids: vec!["a_b".into()],
                        min: 0,
                        max: 1,
                    }],
                }),
            },
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains(
                r#""ticket_choice":{"keep_min":3,"limits":[{"ticket_ids":["a_b"],"min":0,"max":1}]}"#
            ),
            "{json}"
        );
        let back: PrivateState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.legal_moves.ticket_choice.unwrap().limits[0].max, 1);
        assert_eq!(back.seq, 42);
        assert_eq!(back.hand.len(), 3);
        assert!(back.legal_moves.can_draw_deck);
        assert!(back.offered_tickets.is_empty());
    }

    #[test]
    fn error_reason_deser() {
        let json = "\"INSUFFICIENT_CARDS\"";
        let reason: ErrorReason = serde_json::from_str(json).unwrap();
        assert!(matches!(reason, ErrorReason::InsufficientCards));
    }

    #[test]
    fn error_reason_new_variants() {
        for json in [
            "\"GAME_NOT_FOUND\"",
            "\"UNAUTHORIZED\"",
            "\"GAME_FULL\"",
            "\"NAME_TAKEN\"",
        ] {
            let reason: ErrorReason = serde_json::from_str(json).unwrap();
            let back_str = serde_json::to_string(&reason).unwrap();
            assert_eq!(back_str, json, "roundtrip failed for {}", json);
        }
    }

    #[test]
    fn roundtrip_server_message_public() {
        let msg = ServerMessage::PublicState(PublicState {
            game_id: "g1".into(),
            protocol_version: 1,
            state: GameState::WaitingForPlayers,
            current_turn_player_id: None,
            last_round: false,
            last_round_trigger_player_id: None,
            visible_deck: vec![],
            deck_size: 0,
            claimed_routes: vec![],
            players_summary: vec![],
            tunnel_reveal: None,
            stations: vec![],
            final_scores: None,
            longest_path_player_ids: None,
            longest_path_route_ids: None,
            recent_moves: vec![],
            country_stacks: vec![],
            passengers: vec![],
            blocked_route_ids: vec![],
            ticket_deck_size: 0,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PUBLIC_STATE\""));
        assert!(json.contains("\"payload\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::PublicState(ps) => {
                assert_eq!(ps.game_id, "g1");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_server_message_error() {
        let msg = ServerMessage::Error(ErrorPayload {
            seq: 7,
            reason: ErrorReason::InsufficientCards,
            action_ref: Some("CLAIM_ROUTE denver_phoenix_1".into()),
            detail: Some("carte insufficienti".into()),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ERROR\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Error(ep) => {
                assert_eq!(ep.seq, 7);
                assert_eq!(ep.action_ref.unwrap(), "CLAIM_ROUTE denver_phoenix_1");
                assert_eq!(ep.detail.as_deref(), Some("carte insufficienti"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_server_message_snapshot() {
        let msg = ServerMessage::Snapshot(Snapshot {
            protocol_version: 1,
            public: PublicState {
                game_id: "g1".into(),
                protocol_version: 1,
                state: GameState::PlayerTurn,
                current_turn_player_id: Some("p1".into()),
                last_round: false,
                last_round_trigger_player_id: None,
                visible_deck: vec![],
                deck_size: 0,
                claimed_routes: vec![],
                players_summary: vec![],
                tunnel_reveal: None,
                stations: vec![],
                final_scores: None,
                longest_path_player_ids: None,
                longest_path_route_ids: None,
                recent_moves: vec![],
                country_stacks: vec![],
                passengers: vec![],
                blocked_route_ids: vec![],
                ticket_deck_size: 0,
            },
            private: Some(PrivateState {
                seq: 5,
                hand: vec![],
                tickets: vec![],
                offered_tickets: vec![],
                legal_moves: LegalMoves {
                    routes: vec![],
                    draw_face_up: vec![],
                    can_draw_deck: false,
                    can_draw_tickets: false,
                    can_draw_ferry_card: false,
                    station_cities: vec![],
                    station_cost: 0,
                    tunnel: None,
                    ticket_choice: None,
                },
            }),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"SNAPSHOT\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Snapshot(snap) => {
                assert!(snap.private.is_some());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn snapshot_spectator_private_none() {
        let msg = ServerMessage::Snapshot(Snapshot {
            protocol_version: 1,
            public: PublicState {
                game_id: "g1".into(),
                protocol_version: 1,
                state: GameState::PlayerTurn,
                current_turn_player_id: Some("p1".into()),
                last_round: false,
                last_round_trigger_player_id: None,
                visible_deck: vec![],
                deck_size: 0,
                claimed_routes: vec![],
                players_summary: vec![],
                tunnel_reveal: None,
                stations: vec![],
                final_scores: None,
                longest_path_player_ids: None,
                longest_path_route_ids: None,
                recent_moves: vec![],
                country_stacks: vec![],
                passengers: vec![],
                blocked_route_ids: vec![],
                ticket_deck_size: 0,
            },
            private: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Snapshot(snap) => {
                assert!(snap.private.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_visible_deck_empty_slot() {
        // Slot vuoto = null nel JSON
        let deck: Vec<Option<CardColor>> = vec![Some(CardColor::Red), None, None];
        let json = serde_json::to_string(&deck).unwrap();
        assert_eq!(json, "[\"RED\",null,null]");
        let back: Vec<Option<CardColor>> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back[0], Some(CardColor::Red));
        assert_eq!(back[1], None);
    }
}
