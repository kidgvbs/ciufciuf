//! Errori del motore.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errore restituito da `Engine::apply`.
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
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

    #[error("scelta biglietti: tenuti {kept} del deck '{deck}', ma il massimo è {max}")]
    TicketChoiceAboveDeckMax { deck: String, kept: usize, max: u32 },

    #[error("scelta biglietti: tenuti {kept} del deck '{deck}', ma il minimo è {min}")]
    TicketChoiceBelowDeckMin { deck: String, kept: usize, min: u32 },

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

    #[error("rotta '{route_id}': giocatore {player} ha già l'altra doppia")]
    SamePlayerDouble { route_id: String, player: usize },

    #[error("payload non valido: {reason}")]
    InvalidPayload { reason: String },

    #[error("città '{city_id}' non trovata nella mappa")]
    CityNotFound { city_id: String },

    #[error("la città '{city_id}' ha già una stazione")]
    StationCityOccupied { city_id: String },

    #[error("nessuna stazione disponibile")]
    NoStationsLeft,

    #[error("carte insufficienti per la stazione: servono {needed}")]
    InsufficientStationCards { needed: u32 },

    #[error("carte insufficienti per il costo extra del tunnel '{route_id}': servono {needed}")]
    InsufficientTunnelCards { route_id: String, needed: u32 },

    #[error(
        "rotta '{route_id}': la città '{city_id}' ha passeggeri di più colori, serve sceglierne uno"
    )]
    PassengerChoiceRequired { route_id: String, city_id: String },

    #[error("rotta '{route_id}': nessun passeggero {color} a '{city_id}'")]
    PassengerNotAvailable {
        route_id: String,
        city_id: String,
        color: String,
    },
}

/// Errore di configurazione: ruleset, mappa o giocatori incoerenti.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum EngineInitError {
    #[error("liste giocatori di lunghezze diverse (id {ids}, nomi {names}, colori {colors})")]
    PlayerListsMismatch {
        ids: usize,
        names: usize,
        colors: usize,
    },

    #[error("numero di giocatori {count} fuori dall'intervallo {min}..={max}")]
    PlayerCountOutOfRange { count: usize, min: u32, max: u32 },

    #[error("intervallo giocatori del ruleset non valido: min {min}, max {max}")]
    InvalidPlayerRange { min: u32, max: u32 },

    #[error("nome del ruleset vuoto")]
    EmptyRulesetName,

    #[error("primo giocatore {first_player} fuori dai {players} giocatori")]
    FirstPlayerOutOfRange { first_player: usize, players: usize },

    #[error("finalRoundIncludesTrigger=false non è supportato")]
    UnsupportedFinalRound,

    #[error("mazzo treni troppo piccolo: {have} carte, ne servono almeno {needed}")]
    TrainDeckTooSmall { have: usize, needed: usize },

    #[error("colore non valido '{color}' nella rotta '{route_id}'")]
    InvalidRouteColor { route_id: String, color: String },

    #[error("tipo di bonus '{bonus_type}' sconosciuto")]
    UnknownBonusType { bonus_type: String },

    #[error("bonus '{bonus_type}' ripetuto nel ruleset")]
    DuplicateBonusType { bonus_type: String },

    #[error("modalità del ruleset vuota")]
    EmptyRulesetMode,

    #[error("nessun biglietto della mappa è nel set '{set}'")]
    EmptyTicketSet { set: String },

    #[error("biglietto '{ticket_id}': sets è vuoto")]
    TicketSetsEmpty { ticket_id: String },

    #[error("biglietto '{ticket_id}': il set '{set}' è ripetuto in sets")]
    DuplicateTicketSet { ticket_id: String, set: String },

    #[error("bigCities: la città '{city_id}' non esiste")]
    BigCityUnknown { city_id: String },

    #[error("bigCities: la città '{city_id}' è ripetuta")]
    DuplicateBigCity { city_id: String },

    #[error(
        "initialTickets.dealt: keepMax {keep_max} maggiore di count {count} per il deck '{deck}'"
    )]
    DealtKeepMaxExceedsCount {
        deck: String,
        keep_max: u32,
        count: u32,
    },

    #[error(
        "initialTickets.dealt: keepMin {keep_min} maggiore di count {count} per il deck '{deck}'"
    )]
    DealtKeepMinExceedsCount {
        deck: String,
        keep_min: u32,
        count: u32,
    },

    #[error(
        "initialTickets.dealt: keepMin {keep_min} maggiore di keepMax {keep_max} per il deck '{deck}'"
    )]
    DealtKeepMinExceedsKeepMax {
        deck: String,
        keep_min: u32,
        keep_max: u32,
    },

    #[error(
        "initialTickets.dealt: i keepMin dei deck sommano {deck_min}, più di initialTickets.keepMin {keep_min}"
    )]
    DealtKeepMinSumExceedsKeepMin { deck_min: u32, keep_min: u32 },

    #[error(
        "initialTickets.keepMin {keep_min} non raggiungibile: con i keepMax si possono tenere al massimo {max_keep} biglietti"
    )]
    InitialKeepMinUnreachable { keep_min: u32, max_keep: u32 },

    #[error("deck biglietti '{deck}' ripetuto in ticketDecks")]
    DuplicateTicketDeck { deck: String },

    #[error("initialTickets.dealt usa il deck '{deck}' che non è in ticketDecks")]
    DealtTicketDeckUnknown { deck: String },

    #[error("ticketDraw.deck '{deck}' non è in ticketDecks")]
    DrawTicketDeckUnknown { deck: String },

    #[error("biglietto '{ticket_id}': deck '{deck}' non è in ticketDecks")]
    TicketDeckNotListed { ticket_id: String, deck: String },

    #[error("deck biglietti '{deck}' troppo piccolo: {have} biglietti, ne servono {needed}")]
    TicketDeckTooSmall {
        deck: String,
        have: usize,
        needed: usize,
    },

    #[error("initialTickets.keepMin {keep_min} maggiore dei biglietti distribuiti {dealt}")]
    InitialKeepMinExceedsDealt { keep_min: u32, dealt: u32 },

    #[error("ticketDraw.keepMin {keep_min} maggiore di ticketDraw.count {count}")]
    DrawKeepMinExceedsCount { keep_min: u32, count: u32 },

    #[error("ticketDraw.count deve essere almeno 1")]
    TicketDrawCountZero,

    #[error("trainsPerPlayer deve essere almeno 1")]
    ZeroTrainsPerPlayer,

    #[error(
        "troppe locomotive nel mazzo: {locomotives} locomotive con {per_color} carte per colore (serve perColor × 8 ≥ 3 × locomotives)"
    )]
    TrainDeckLocomotiveHeavy { per_color: u32, locomotives: u32 },

    #[error("id città '{city_id}' duplicato")]
    DuplicateCityId { city_id: String },

    #[error("id rotta '{route_id}' duplicato")]
    DuplicateRouteId { route_id: String },

    #[error("id biglietto '{ticket_id}' duplicato")]
    DuplicateTicketId { ticket_id: String },

    #[error("rotta '{route_id}': la città '{city_id}' non esiste")]
    RouteCityUnknown { route_id: String, city_id: String },

    #[error("biglietto '{ticket_id}': la città '{city_id}' non esiste")]
    TicketCityUnknown { ticket_id: String, city_id: String },

    #[error("rotta '{route_id}': la lunghezza deve essere almeno 1")]
    RouteLengthZero { route_id: String },

    #[error("rotta '{route_id}': la lunghezza {len} non è in routePoints")]
    RouteLengthUnscored { route_id: String, len: u32 },

    #[error("biglietto '{ticket_id}': partenza e arrivo coincidono ('{city_id}')")]
    TicketSameCity { ticket_id: String, city_id: String },

    #[error("rotta '{route_id}': partenza e arrivo coincidono ('{city_id}')")]
    RouteSameCity { route_id: String, city_id: String },

    #[error("rotta doppia '{route_id}': l'id deve finire con _1, _2, _3 o _4")]
    DoubleRouteBadId { route_id: String },

    #[error(
        "rotta doppia '{route_id}': il gruppo parallelo non è completo: serve '{sister_id}' con stessi estremi, stessa lunghezza e double=true (ids {{base}}_1..{{base}}_k contigui)"
    )]
    DoubleRouteUnpaired { route_id: String, sister_id: String },

    #[error(
        "mappa troppo densa per il percorso più lungo: fino a {routes} rotte reclamabili da un giocatore e numero ciclomatico stimato fino a {cyclomatic} (massimo {max})"
    )]
    LongestPathTooDense {
        routes: usize,
        cyclomatic: u32,
        max: u32,
    },

    #[error("id giocatore '{player_id}' duplicato")]
    DuplicatePlayerId { player_id: String },

    #[error("colore giocatore {color} duplicato")]
    DuplicatePlayerColor { color: String },

    #[error("rotta '{route_id}': i traghetti ({ferries}) superano la lunghezza ({len})")]
    RouteFerriesExceedLength {
        route_id: String,
        ferries: u32,
        len: u32,
    },

    #[error("rotta '{route_id}': non può essere insieme tunnel e traghetto")]
    RouteTunnelAndFerry { route_id: String },

    #[error("rotta tunnel '{route_id}': il ruleset non ha tunnels.revealed >= 1")]
    TunnelsConfigMissing { route_id: String },

    #[error("stations.costs ha {costs} voci ma stations.count è {count}")]
    StationCostsMismatch { count: u32, costs: usize },

    #[error("stations.costs[{index}] deve essere almeno 1")]
    StationCostZero { index: usize },

    #[error("id città '{city_id}' non valido: '#' è riservato ai nodi privati dei Paesi")]
    CityIdInvalid { city_id: String },

    #[error("rotta doppia '{route_id}': il gruppo ha {lanes} binari, il massimo è 4")]
    DoubleGroupTooLarge { route_id: String, lanes: usize },

    #[error(
        "rotta doppia '{route_id}': alwaysOpen diverso da quello del binario '{other_id}' dello stesso gruppo"
    )]
    DoubleGroupAlwaysOpenMismatch { route_id: String, other_id: String },

    #[error("ticketDraw.mergeDecks: il deck '{deck}' non è in ticketDecks")]
    MergeDeckUnknown { deck: String },

    #[error("ticketDraw.mergeDecks: il deck '{deck}' è ripetuto o coincide con ticketDraw.deck")]
    MergeDeckDuplicate { deck: String },

    #[error(
        "setupDiscards 'reshuffleBottom' non è ammesso con ticketDraw.mergeDecks: usare 'reshuffleBottomOwnDeck' o 'removed'"
    )]
    SetupDiscardsNeedOwnDeck,

    #[error("tieBreakers: chiave '{key}' sconosciuta")]
    UnknownTieBreaker { key: String },

    #[error("tieBreakers: chiave '{key}' ripetuta")]
    DuplicateTieBreaker { key: String },

    #[error("countryCards: '{city_id}' non è un Paese della mappa")]
    CountryCardsUnknownCountry { city_id: String },

    #[error("passengers.colors non può essere vuoto")]
    PassengersColorsEmpty,

    #[error("passengers.colors: il colore '{color}' è ripetuto")]
    PassengersDuplicateColor { color: String },

    #[error("passengers.perColor deve essere almeno 1")]
    PassengersPerColorZero,

    #[error("passengers.placement: la città '{city_id}' non esiste")]
    PassengersUnknownCity { city_id: String },

    #[error("passengers.placement: la città '{city_id}' ha 0 passeggeri")]
    PassengersPlacementZero { city_id: String },

    #[error(
        "passengers.placement distribuisce {placed} passeggeri ma il sacchetto ne ha {bag} (colori × perColor)"
    )]
    PassengersPlacementMismatch { placed: u32, bag: u32 },

    #[error("passengers.majorityPoints deve avere 2 valori, ne ha {len}")]
    PassengersMajorityPoints { len: usize },
}

/// Fase di gioco (macchina a stati interna).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum GamePhase {
    InitialDraft,
    PlayerTurn,
    SecondCardDraw,
    TicketChoice,
    TunnelResolution,
    LastRound,
    GameOver,
}
