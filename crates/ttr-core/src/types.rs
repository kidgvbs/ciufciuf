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
    /// Paese confinante: vicolo cieco. Due rotte che entrano nello stesso Paese non sono
    /// collegate tra loro; un biglietto con estremo Paese è completato da una qualunque
    /// rotta del giocatore che vi entra.
    #[serde(default)]
    pub country: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub from: String,
    pub to: String,
    pub len: u32,
    pub color: String, // "Gray" or a card color in PascalCase ("Red", "Blue", ...)
    /// Binario di un gruppo parallelo di 2–4 binari: le rotte `double` con la stessa
    /// coppia (non ordinata) di città, con id `{base}_{n}`, n = 1..k contigui.
    pub double: bool,
    /// Tutti i binari del gruppo sono aperti con qualunque numero di giocatori; uguale su
    /// tutti i binari del gruppo. Su una rotta singola non cambia nulla.
    #[serde(rename = "alwaysOpen", default)]
    pub always_open: bool,
    /// Tunnel (Europe): dopo il pagamento si rivelano carte che possono costare extra.
    #[serde(default)]
    pub tunnel: bool,
    /// Traghetto (Europe): locomotive obbligatorie nel pagamento.
    #[serde(default)]
    pub ferries: u32,
    /// Ogni spazio della rotta è pagabile anche con questo numero di carte qualsiasi
    /// (locomotive comprese) al posto della carta del colore (Nordic: Murmansk–Lieksa);
    /// almeno 2.
    #[serde(
        rename = "anyCardsPerSpace",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub any_cards_per_space: Option<u32>,
}

/// Biglietto: `from` più UNA tra due forme (la validazione le impone esclusive):
/// - classica: `to` + `points`, una sola destinazione;
/// - multi-destinazione (Switzerland): `destinations`, almeno 2 voci. Vale il punteggio
///   più alto tra le destinazioni collegate; se nessuna lo è si perde il valore più basso.
///
/// Il motore legge le destinazioni solo con [`TicketDef::destinations`], che tratta la
/// forma classica come una lista di una destinazione.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketDef {
    pub id: String,
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destinations: Option<Vec<TicketDestination>>,
    pub deck: String, // "regular" | "long" (future)
    /// Modalità (`Ruleset::ticket_set`) in cui il biglietto è in gioco; non vuoto,
    /// senza duplicati.
    pub sets: Vec<String>,
}

/// Una destinazione di un biglietto multi-destinazione, con i suoi punti.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketDestination {
    pub to: CityId,
    pub points: u32,
}

impl TicketDef {
    /// Destinazioni `(città, punti)` nell'ordine del file: una per il biglietto classico.
    pub fn destinations(&self) -> impl Iterator<Item = (&str, u32)> {
        self.to.as_deref().zip(self.points).into_iter().chain(
            self.destinations
                .iter()
                .flatten()
                .map(|d| (d.to.as_str(), d.points)),
        )
    }

    /// Punti più alti tra le destinazioni (quelli del biglietto classico).
    #[must_use]
    pub fn max_points(&self) -> u32 {
        self.destinations().map(|(_, p)| p).max().unwrap_or(0)
    }
}

impl MapData {
    /// Biglietti in gioco con il ruleset (`ticket_set`), nell'ordine del file.
    pub fn tickets_in_play<'a>(
        &'a self,
        ruleset: &'a Ruleset,
    ) -> impl Iterator<Item = &'a TicketDef> {
        self.tickets
            .iter()
            .filter(|t| t.sets.contains(&ruleset.ticket_set))
    }
}

// ── Ruleset descriptor ───────────────────────────────────────────────

/// Regole per-mappa (caricate da JSON, piano §4.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ruleset {
    /// Nome mostrato nell'elenco dei ruleset; obbligatorio e non vuoto.
    pub name: String,
    /// Modalità di gioco mostrata nell'elenco dei ruleset (es. "Standard", "Mega");
    /// obbligatoria e non vuota.
    pub mode: String,
    /// Posizione della modalità tra quelle della stessa mappa.
    #[serde(rename = "modeOrder", default)]
    pub mode_order: u32,
    /// Insieme di biglietti in gioco: i `MapData::tickets` il cui `sets` lo contiene.
    #[serde(rename = "ticketSet")]
    pub ticket_set: String,
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
    /// Città «grandi» della modalità Grandi Città: solo visualizzazione.
    #[serde(rename = "bigCities", default)]
    pub big_cities: Vec<CityId>,
    #[serde(rename = "doubleRouteMinPlayers")]
    pub double_route_min_players: u32,
    #[serde(rename = "finalRoundIncludesTrigger")]
    pub final_round_includes_trigger: bool,
    /// Stazioni (Europe); assenti sulle mappe senza stazioni.
    #[serde(default)]
    pub stations: Option<StationsConfig>,
    /// Tunnel (Europe); obbligatorio se la mappa ha rotte tunnel.
    #[serde(default)]
    pub tunnels: Option<TunnelsConfig>,
    /// Spareggi tra pari punti, in ordine di importanza (`completedTickets`,
    /// `completedTicketPoints`, `longestPathBonus`, `longestPath`, `passengers`); assente =
    /// `DEFAULT_TIE_BREAKERS`.
    #[serde(rename = "tieBreakers", default)]
    pub tie_breakers: Option<Vec<String>>,
    /// Carte Paese (Poland): per Paese (città `country`), i valori dall'alto in basso.
    /// Le pile non si mescolano. Vuoto = nessuna carta Paese.
    #[serde(rename = "countryCards", default)]
    pub country_cards: BTreeMap<CityId, Vec<u32>>,
    /// Passeggeri (Germany 2017); assenti sulle altre mappe.
    #[serde(default)]
    pub passengers: Option<PassengersConfig>,
    /// Regole speciali della modalità, mostrate ai giocatori a inizio partita (solo
    /// testo: non influiscono sul motore né sul replay); voci non vuote.
    #[serde(rename = "specialRules", default)]
    pub special_rules: Vec<String>,
    /// Regole delle locomotive (uso sulle rotte, pesca, mercato); assente = default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locomotives: Option<LocomotivesConfig>,
    /// Regole dei traghetti; assente = nessuna sostituzione delle locomotive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ferries: Option<FerriesConfig>,
}

/// Stazioni per giocatore: `costs[i]` è il costo in carte della stazione `i`-esima.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationsConfig {
    pub count: u32,
    pub costs: Vec<u32>,
    #[serde(rename = "unbuiltPoints")]
    pub unbuilt_points: u32,
}

/// Regole dei tunnel: carte rivelate dal mazzo a ogni tentativo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelsConfig {
    pub revealed: u32,
}

/// Regole delle locomotive. Il default (campo assente) è il comportamento classico:
/// usabili su ogni rotta, mai come seconda carta scoperta, reset del mercato con 3 scoperte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocomotivesConfig {
    /// Tipi di rotta su cui una locomotiva può pagare da sola uno spazio (`tunnel`,
    /// `ferry`); assente = tutte le rotte. Dentro un gruppo di carte qualsiasi
    /// (`anyCardsPerSpace`, `ferries.anyCardsPerLocomotive`) la locomotiva è sempre ammessa.
    #[serde(rename = "usableOn", default, skip_serializing_if = "Option::is_none")]
    pub usable_on: Option<Vec<String>>,
    /// Una locomotiva scoperta conta come una carta normale: chiude il turno solo come
    /// seconda pescata e si può prendere come seconda carta.
    #[serde(rename = "freeDraw", default)]
    pub free_draw: bool,
    /// Con 3 locomotive scoperte il mercato si scarta e rimpiazza (default `true`).
    #[serde(rename = "faceUpReset", default = "default_face_up_reset")]
    pub face_up_reset: bool,
}

fn default_face_up_reset() -> bool {
    true
}

/// Regole dei traghetti.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FerriesConfig {
    /// Sui traghetti questo numero di carte qualsiasi vale una locomotiva (almeno 2).
    #[serde(rename = "anyCardsPerLocomotive")]
    pub any_cards_per_locomotive: u32,
}

/// Passeggeri (Germany 2017): un sacchetto di `colors × perColor` passeggeri mescolato a
/// inizio partita e distribuito sulle città in ordine di id (`placement`); a ogni claim il
/// giocatore ne prende uno da ciascun estremo; a fine partita premi di maggioranza per colore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassengersConfig {
    /// Colori dei passeggeri (nomi liberi, es. `"Red"`), senza ripetizioni.
    pub colors: Vec<String>,
    /// Passeggeri per colore.
    #[serde(rename = "perColor")]
    pub per_color: u32,
    /// Passeggeri iniziali per città (anche i Paesi); la somma è `colors × perColor`.
    pub placement: BTreeMap<CityId, u32>,
    /// Punti per colore: `[maggioranza, secondo posto]`.
    #[serde(rename = "majorityPoints")]
    pub majority_points: Vec<u32>,
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
    /// Biglietti distribuiti per mazzo a ogni giocatore, da cui scegliere cosa tenere.
    #[serde(default)]
    pub dealt: Vec<TicketDealtEntry>,
    #[serde(rename = "keepMin")]
    pub keep_min: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketDealtEntry {
    pub deck: String,
    pub count: u32,
    /// Minimo di biglietti di questo deck da tenere nel draft iniziale (0 = nessun minimo).
    #[serde(rename = "keepMin", default)]
    pub keep_min: u32,
    /// Massimo di biglietti di questo deck tenibili nel draft iniziale.
    #[serde(rename = "keepMax", default)]
    pub keep_max: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketDraw {
    /// Mazzo da cui si pesca a partita in corso.
    pub deck: String,
    /// Altri mazzi che, finito il draft iniziale, si fondono con `deck` in un'unica pila
    /// mescolata (Germany: lunghi + corti). Gli scartati vanno in fondo a quella pila.
    #[serde(rename = "mergeDecks", default)]
    pub merge_decks: Vec<String>,
    pub count: u32,
    #[serde(rename = "keepMin")]
    pub keep_min: u32,
    /// Cosa succede ai biglietti scartati a metà partita (default `bottom`).
    #[serde(default)]
    pub discards: TicketDiscards,
}

/// Destino dei biglietti scartati dopo una pesca a metà partita (`ticketDraw.discards`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TicketDiscards {
    /// In fondo al proprio mazzo (o alla pila unita di `mergeDecks`), nell'ordine di offerta.
    #[default]
    Bottom,
    /// Rimossi dalla partita (Nordic, Switzerland): il mazzo può esaurirsi.
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bonus {
    #[serde(rename = "type")]
    pub bonus_type: String,
    pub label: String,
    pub points: u32,
}

/// Regola di scelta del draft iniziale per un deck: biglietti distribuiti a ogni
/// giocatore, minimo da tenere e massimo tenibile (`None` = nessun limite oltre a quelli
/// distribuiti).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckKeepRule {
    pub deck: String,
    pub count: u32,
    pub min: u32,
    pub max: Option<u32>,
}

impl Ruleset {
    /// Regole del draft iniziale per deck, nell'ordine di prima comparsa in
    /// `initialTickets.dealt`; le voci dello stesso deck si sommano.
    #[must_use]
    pub fn initial_deck_rules(&self) -> Vec<DeckKeepRule> {
        let mut rules: Vec<DeckKeepRule> = Vec::new();
        for entry in &self.initial_tickets.dealt {
            let Some(rule) = rules.iter_mut().find(|r| r.deck == entry.deck) else {
                rules.push(DeckKeepRule {
                    deck: entry.deck.clone(),
                    count: entry.count,
                    min: entry.keep_min,
                    max: entry.keep_max,
                });
                continue;
            };
            // Voce senza `keepMax`: tutti i suoi biglietti sono tenibili.
            let unlimited = rule.max.is_none() && entry.keep_max.is_none();
            let kept = rule
                .max
                .unwrap_or(rule.count)
                .saturating_add(entry.keep_max.unwrap_or(entry.count));
            rule.count = rule.count.saturating_add(entry.count);
            rule.min = rule.min.saturating_add(entry.keep_min);
            rule.max = (!unlimited).then_some(kept);
        }
        rules
    }

    /// Punti del bonus del tipo dato, se il ruleset lo prevede.
    #[must_use]
    pub fn bonus_points(&self, bonus_type: &str) -> Option<u32> {
        self.bonuses
            .iter()
            .find(|b| b.bonus_type == bonus_type)
            .map(|b| b.points)
    }

    /// Spareggi in ordine di importanza: quelli del ruleset (le chiavi sconosciute, che la
    /// validazione rifiuta, sono ignorate) oppure `DEFAULT_TIE_BREAKERS`.
    #[must_use]
    pub fn tie_breaker_keys(&self) -> Vec<TieBreaker> {
        match &self.tie_breakers {
            Some(keys) => keys.iter().filter_map(|k| TieBreaker::parse(k)).collect(),
            None => DEFAULT_TIE_BREAKERS.to_vec(),
        }
    }
}

/// Cosa fare degli scarti di setup (piano §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SetupDiscards {
    /// Rimescolati tra loro e messi in fondo al mazzo (USA V2 2025).
    ReshuffleBottom,
    /// Rimossi dalla partita (Europe).
    Removed,
    /// Come `ReshuffleBottom`, ma per ogni mazzo di `ticketDecks`: gli scarti di un mazzo
    /// sono rimescolati (RNG del motore, mazzi nell'ordine di `ticketDecks`) e messi in
    /// fondo al mazzo stesso (Germany).
    ReshuffleBottomOwnDeck,
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
    /// Città con una stazione del giocatore, in ordine di costruzione.
    pub stations: Vec<CityId>,
    /// Carte Paese prese (Poland): per Paese, i valori nell'ordine in cui sono state prese.
    #[serde(default)]
    pub country_cards: BTreeMap<CityId, Vec<u32>>,
    /// Passeggeri presi (Germany 2017): per colore, quanti.
    #[serde(default)]
    pub passengers: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketStatus {
    pub ticket_id: String,
    pub points: u32,
    /// null = non ancora rivelato / in scelta; Some(bool) = completato o meno
    pub completed: Option<bool>,
}

/// Esito finale di un biglietto: completato (con la destinazione realizzata, quella di
/// punti più alti tra le collegate) o fallito (nessuna destinazione).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketResult {
    pub ticket_id: TicketId,
    pub completed: bool,
    /// Destinazione realizzata; `None` se il biglietto è fallito.
    pub destination: Option<CityId>,
    /// Punti guadagnati (> 0) o persi (≤ 0: il valore più basso del biglietto).
    pub points: i32,
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

/// Tentativo di tunnel in attesa della decisione del giocatore di turno.
///
/// Le carte giocate e quelle rivelate restano fuori da mano e scarti finché
/// il tunnel è pendente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTunnel {
    pub route_id: RouteId,
    /// Carte giocate per la rotta, nell'ordine in cui sono uscite dalla mano.
    pub played: Vec<CardColor>,
    pub revealed: Vec<CardColor>,
    /// Colore delle carte non locomotiva giocate; `Locomotive` se sono tutte locomotive.
    pub color: CardColor,
    /// Rivelate del colore del tunnel o locomotive.
    pub extra: u32,
    /// Passeggeri scelti con il `ClaimRoute` (città → colore), presi a claim completato.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub passengers: BTreeMap<CityId, String>,
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

/// Carte treno distribuite a ogni giocatore all'inizio della partita.
pub const INITIAL_HAND_SIZE: usize = 4;

/// Colori carta (esclusa Locomotive) nell'ordine di costruzione del mazzo.
pub const TRAIN_COLORS: [CardColor; COLOR_COUNT] = [
    CardColor::Red,
    CardColor::Orange,
    CardColor::Yellow,
    CardColor::Green,
    CardColor::Blue,
    CardColor::Purple,
    CardColor::Black,
    CardColor::White,
];

/// Valore di `Bonus::bonus_type` per il bonus del percorso più lungo.
pub const LONGEST_PATH_BONUS_TYPE: &str = "longest_path";

/// Valore di `Bonus::bonus_type` per il bonus di chi completa più biglietti.
pub const GLOBETROTTER_BONUS_TYPE: &str = "globetrotter";

/// Valore canonico di `Route::color` per le rotte grigie.
pub const GRAY_ROUTE_COLOR: &str = "Gray";

/// Massimo di binari di un gruppo parallelo.
pub const MAX_PARALLEL_LANES: usize = 4;

/// Chiave di spareggio del ruleset (`tieBreakers`), in ordine di importanza.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TieBreaker {
    /// Numero di biglietti completati.
    CompletedTickets,
    /// Punti dei biglietti completati.
    CompletedTicketPoints,
    /// Bonus del percorso più lungo ricevuto.
    LongestPathBonus,
    /// Lunghezza del percorso più lungo del giocatore, calcolata a fine partita anche senza
    /// il bonus `longest_path`.
    LongestPath,
    /// Numero totale di passeggeri raccolti.
    Passengers,
}

impl TieBreaker {
    /// Interpreta il nome nel ruleset; `None` se sconosciuto.
    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "completedTickets" => Some(Self::CompletedTickets),
            "completedTicketPoints" => Some(Self::CompletedTicketPoints),
            "longestPathBonus" => Some(Self::LongestPathBonus),
            "longestPath" => Some(Self::LongestPath),
            "passengers" => Some(Self::Passengers),
            _ => None,
        }
    }
}

/// Spareggi di default: biglietti completati, poi bonus del percorso più lungo.
pub const DEFAULT_TIE_BREAKERS: [TieBreaker; 2] =
    [TieBreaker::CompletedTickets, TieBreaker::LongestPathBonus];

/// Colore di una rotta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteColor {
    /// Qualunque colore, purché uniforme.
    Gray,
    /// Colore specifico (le locomotive fanno da jolly).
    Colored(CardColor),
}

impl RouteColor {
    /// Interpreta il colore di una rotta; accetta solo i valori canonici
    /// (`"Gray"` e i nomi in PascalCase dei colori carta).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let card = match value {
            GRAY_ROUTE_COLOR => return Some(RouteColor::Gray),
            "Red" => CardColor::Red,
            "Orange" => CardColor::Orange,
            "Yellow" => CardColor::Yellow,
            "Green" => CardColor::Green,
            "Blue" => CardColor::Blue,
            "Purple" => CardColor::Purple,
            "Black" => CardColor::Black,
            "White" => CardColor::White,
            _ => return None,
        };
        Some(RouteColor::Colored(card))
    }
}

/// Base e numero del binario (`{base}_{n}`, n in 1..=`MAX_PARALLEL_LANES`) di una rotta
/// di un gruppo parallelo, dal solo suffisso dell'id.
pub(crate) fn route_lane(route_id: &str) -> Option<(&str, usize)> {
    let (base, suffix) = route_id.rsplit_once('_')?;
    if suffix.len() != 1 {
        return None;
    }
    let lane = usize::from(suffix.as_bytes()[0].checked_sub(b'0')?);
    (1..=MAX_PARALLEL_LANES)
        .contains(&lane)
        .then_some((base, lane))
}

/// Converte punti (`u32`) nel tipo con segno dei punteggi, saturando.
pub(crate) fn points_to_score(points: u32) -> i32 {
    i32::try_from(points).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_color_parses_only_canonical_values() {
        assert_eq!(RouteColor::parse("Gray"), Some(RouteColor::Gray));
        assert_eq!(
            RouteColor::parse("Red"),
            Some(RouteColor::Colored(CardColor::Red))
        );
        for color in ["gray", "grey", "GRAY", "red", "Locomotive", ""] {
            assert_eq!(RouteColor::parse(color), None, "{color:?}");
        }
    }

    #[test]
    fn route_lane_parses_base_and_lane_number() {
        assert_eq!(route_lane("x_y_1"), Some(("x_y", 1)));
        assert_eq!(route_lane("x_y_4"), Some(("x_y", 4)));
        for id in ["other", "x_y_0", "x_y_5", "x_y_a", "x_y_", "_1x", "x_y_01"] {
            assert_eq!(route_lane(id), None, "{id:?}");
        }
        assert_eq!(route_lane("_1"), Some(("", 1)));
    }

    #[test]
    fn tie_breaker_parses_known_keys_only() {
        assert_eq!(
            TieBreaker::parse("completedTicketPoints"),
            Some(TieBreaker::CompletedTicketPoints)
        );
        assert_eq!(
            TieBreaker::parse("passengers"),
            Some(TieBreaker::Passengers)
        );
        assert_eq!(
            TieBreaker::parse("longestPath"),
            Some(TieBreaker::LongestPath)
        );
        assert_eq!(TieBreaker::parse("longest_path"), None);
    }
}
