//! Sessione di gioco con attore tokio: possiede `Option<Engine>`, roster,
//! collegamenti WebSocket, contatori seq, e coordina il ciclo di vita.
//!
//! Ogni mutazione del motore passa attraverso l'attore — nessuna race. Le
//! scritture su disco passano dal `SaveWriter` della partita.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;
use ttr_proto::{PlayerColor, ServerMessage};

use crate::bridge;
use crate::error::LobbyError;
use crate::names::{name_key, normalize_name};
use crate::save::{build_save_data, now_rfc3339, token_matches, tokens_equal};
use crate::writer::SaveWriter;

/// Lobby non avviata senza comandi `Join`/`Start`/`Reassign` né connessioni di giocatori: oltre questo tempo l'attore termina.
pub(crate) const LOBBY_IDLE_TTL: Duration = Duration::from_secs(30 * 60);
/// Lobby senza giocatori: oltre questo tempo l'attore termina, anche se ci sono spettatori.
pub(crate) const EMPTY_LOBBY_TTL: Duration = Duration::from_secs(5 * 60);
/// Partita avviata senza connessioni attive: oltre questo tempo l'attore termina e il save resta su disco.
pub(crate) const ABANDONED_TTL: Duration = Duration::from_secs(6 * 60 * 60);
/// Quanto resta viva una partita conclusa (schermata finale su TV e telefoni).
pub(crate) const GAME_OVER_LINGER: Duration = Duration::from_secs(30 * 60);
/// Partite attive (lobby comprese) oltre le quali `POST /api/games` risponde 429.
pub(crate) const MAX_ACTIVE_GAMES: usize = 20;
/// Spettatori WebSocket per partita oltre i quali la connessione viene rifiutata.
pub(crate) const MAX_SPECTATORS_PER_GAME: usize = 8;
/// Connessioni WebSocket per giocatore oltre le quali la più vecchia viene chiusa.
pub(crate) const MAX_CONNECTIONS_PER_PLAYER: usize = 4;
/// Intervallo tra due `Ping` inviati a un client WebSocket.
pub(crate) const WS_PING_INTERVAL: Duration = Duration::from_secs(15);
/// Silenzio (nessun messaggio, `Pong` compresi) oltre il quale un client WebSocket è scollegato.
pub(crate) const WS_IDLE_TIMEOUT: Duration = Duration::from_secs(45);

/// Lunghezza massima del nome di un giocatore, in caratteri.
pub(crate) const MAX_PLAYER_NAME_CHARS: usize = 24;

/// Durate e limiti di una sessione; i test li riducono a millisecondi.
#[derive(Debug, Clone, Copy)]
pub struct SessionLimits {
    /// Vita massima di una lobby inattiva.
    pub lobby_idle_ttl: Duration,
    /// Vita massima di una lobby senza giocatori.
    pub empty_lobby_ttl: Duration,
    /// Vita massima di una partita avviata senza connessioni.
    pub abandoned_ttl: Duration,
    /// Vita di una partita conclusa.
    pub game_over_linger: Duration,
    /// Numero massimo di partite attive.
    pub max_active_games: usize,
    /// Numero massimo di spettatori per partita.
    pub max_spectators: usize,
    /// Numero massimo di connessioni per giocatore.
    pub max_connections_per_player: usize,
    /// Intervallo tra i `Ping` WebSocket.
    pub ws_ping_interval: Duration,
    /// Silenzio massimo di un client WebSocket prima di scollegarlo.
    pub ws_idle_timeout: Duration,
}

impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            lobby_idle_ttl: LOBBY_IDLE_TTL,
            empty_lobby_ttl: EMPTY_LOBBY_TTL,
            abandoned_ttl: ABANDONED_TTL,
            game_over_linger: GAME_OVER_LINGER,
            max_active_games: MAX_ACTIVE_GAMES,
            max_spectators: MAX_SPECTATORS_PER_GAME,
            max_connections_per_player: MAX_CONNECTIONS_PER_PLAYER,
            ws_ping_interval: WS_PING_INTERVAL,
            ws_idle_timeout: WS_IDLE_TIMEOUT,
        }
    }
}

/// Canale outbound di una connessione WebSocket (limitato: se è pieno la
/// connessione viene chiusa).
pub(crate) type OutboundTx = mpsc::Sender<ServerMessage>;

/// Handle pubblico per interagire con una GameSession.
#[derive(Clone)]
pub struct GameHandle {
    pub tx: mpsc::Sender<GameCommand>,
    pub game_id: String,
    pub game_name: String,
    pub ruleset_id: String,
    pub max_players: u32,
    pub players_joined: usize,
    pub started: bool,
    /// Impostato dall'attore al game over: la partita non conta più nel limite
    /// delle partite attive.
    pub finished: Arc<AtomicBool>,
}

/// Esito della connessione WebSocket restituito all'handler dopo l'auth.
#[derive(Debug)]
pub enum ConnectOutcome {
    /// Connessione autenticata con successo.
    Ok { conn_id: u64 },
    /// Token non valido o assente per giocatore autenticato.
    Unauthorized,
    /// Partita non trovata o attore terminato.
    GameNotFound,
    /// Raggiunto il limite di spettatori della partita.
    TooManySpectators,
}

/// Informazioni fresche sulla partita richieste dalla lobby REST.
#[derive(Debug, Clone)]
pub struct GameInfo {
    pub players: Vec<(String, String, PlayerColor)>, // (player_id, name, color)
    pub players_joined: usize,
    pub started: bool,
    /// Nomi del save non ancora rientrati (vuoto fuori dal resume).
    pub missing_players: Vec<String>,
}

#[derive(Debug)]
pub enum GameCommand {
    Join {
        player_id: String,
        name: String,
        /// `None` = colore automatico (primo libero in ordine).
        color: Option<PlayerColor>,
        /// Token nuovo, usato fuori dal resume.
        token: String,
        /// Token originale presentato dal client, richiesto in resume.
        resume_token: Option<String>,
        reply: tokio::sync::oneshot::Sender<Result<JoinResponse, LobbyError>>,
    },
    /// Assegna un token nuovo a un giocatore del save (solo host, solo resume).
    Reassign {
        host_token: String,
        name: String,
        new_token: String,
        reply: tokio::sync::oneshot::Sender<Result<JoinResponse, LobbyError>>,
    },
    Start {
        host_token: String,
        ruleset: ttr_core::Ruleset,
        map: ttr_core::MapData,
        reply: tokio::sync::oneshot::Sender<Result<(), LobbyError>>,
    },
    Action {
        player_id: String,
        inbound: ttr_proto::InboundAction,
        client_seq: Option<u64>,
    },
    Connect {
        player_id: Option<String>,
        token: Option<String>,
        tx: OutboundTx,
        /// Risposta con l'esito dell'auth; l'handler WS chiude il socket su errore.
        reply: tokio::sync::oneshot::Sender<ConnectOutcome>,
    },
    Disconnect {
        player_id: Option<String>,
        /// ID della singola connessione da rimuovere (multi-tab).
        conn_id: u64,
    },
    RequestSnapshot {
        player_id: Option<String>,
        reply: tokio::sync::oneshot::Sender<Option<ServerMessage>>,
    },
    /// Interrogazione dello stato corrente della lobby dall'esterno (REST).
    GetInfo {
        reply: tokio::sync::oneshot::Sender<GameInfo>,
    },
    /// Chiude la partita su richiesta dell'host (REST `DELETE`).
    Delete {
        host_token: String,
        reply: tokio::sync::oneshot::Sender<Result<(), LobbyError>>,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct JoinResponse {
    pub player_id: String,
    pub token: String,
    /// Nome come registrato nella partita (normalizzato, o quello del save).
    pub name: String,
}

/// Ordine di assegnazione automatica dei colori.
const COLOR_ORDER: [PlayerColor; 5] = [
    PlayerColor::Red,
    PlayerColor::Blue,
    PlayerColor::Green,
    PlayerColor::Yellow,
    PlayerColor::Black,
];

/// Il nome ricevuto da un join senza autenticazione non va normalizzato se supera ciò che può
/// contenere un nome del roster: `MAX_PLAYER_NAME_CHARS`, o più se un save esistente ha nomi
/// fuori limite (restano riprendibili). `name_key` costa per carattere.
fn name_too_long<'a>(name: &str, roster_names: impl Iterator<Item = &'a str>) -> bool {
    let limit = roster_names
        .map(|n| n.chars().count())
        .fold(MAX_PLAYER_NAME_CHARS, usize::max);
    name.chars().nth(limit).is_some()
}

/// Primo colore libero in ordine, dato il roster attuale.
fn first_free_color(players: &[SessionPlayer]) -> PlayerColor {
    COLOR_ORDER
        .into_iter()
        .find(|c| !players.iter().any(|p| p.color == *c))
        .unwrap_or(PlayerColor::Red)
}

#[derive(Debug, Clone)]
pub(crate) struct SessionPlayer {
    pub name: String,
    pub color: PlayerColor,
    pub token: String,
    /// Indice del giocatore nel motore (assegnato allo start o dal save).
    pub index: Option<usize>,
}

/// Giocatore di un save, come arriva a una sessione ripresa.
#[derive(Debug, Clone)]
pub(crate) struct SavedPlayer {
    pub id: String,
    pub name: String,
    pub color: PlayerColor,
    pub token_hash: Option<String>,
}

/// Voce del roster salvato, indicizzata per `name_key`.
#[derive(Debug, Clone)]
pub(crate) struct SavedEntry {
    pub id: String,
    pub engine_index: usize,
    pub color: PlayerColor,
    pub name: String,
    pub token_hash: Option<String>,
}

/// Parametri comuni a ogni sessione.
pub(crate) struct SessionConfig {
    pub game_id: String,
    pub game_name: String,
    pub ruleset_id: String,
    pub map_id: String,
    pub max_players: u32,
    pub host_token: String,
    pub save_path: Option<PathBuf>,
    pub results_lock: Arc<Mutex<()>>,
    pub limits: SessionLimits,
    pub finished: Arc<AtomicBool>,
}

/// Partita ricostruita da un save.
pub(crate) struct ResumedGame {
    pub engine: ttr_core::Engine,
    pub map_data: ttr_core::MapData,
    pub players: Vec<SavedPlayer>,
    pub created_at: String,
}

pub(crate) struct GameSession {
    pub game_id: String,
    pub game_name: String,
    pub ruleset_id: String,
    pub map_id: String,
    pub created_at: String,
    pub engine: Option<ttr_core::Engine>,
    /// Mappa di gioco (per lookup punti biglietti nelle conversioni bridge).
    pub map_data: Option<ttr_core::MapData>,
    pub players: Vec<SessionPlayer>,
    pub player_index: BTreeMap<String, usize>,
    pub host_token: String,
    pub max_players: u32,
    pub started: bool,
    /// Connessioni per giocatore: ogni entry è (conn_id, sender).
    pub connections: BTreeMap<String, Vec<(u64, OutboundTx)>>,
    /// Spettatori: (conn_id, sender).
    pub spectators: Vec<(u64, OutboundTx)>,
    pub outbound_seq: BTreeMap<String, u64>,
    /// Ultimo `client_seq` ricevuto per giocatore (`None` = mai visto).
    pub last_client_seq: BTreeMap<String, Option<u64>>,
    /// Contatore per assegnare connection ID univoci.
    pub next_conn_id: u64,
    pub final_scores: Option<Vec<ttr_proto::FinalScoreInfo>>,
    pub longest_path_player_ids: Option<Vec<String>>,
    pub save_path: Option<PathBuf>,
    writer: Option<SaveWriter>,
    results_lock: Arc<Mutex<()>>,
    limits: SessionLimits,
    finished: Arc<AtomicBool>,
    pub rx: mpsc::Receiver<GameCommand>,
    /// Seed RNG scelto alla creazione (solo se il server lo ammette); `None` =
    /// seed casuale da CSPRNG allo start.
    pub seed: Option<u64>,
    /// Motore pre-replay per resume.
    pub resumed_engine: Option<ttr_core::Engine>,
    /// Mappa pre-caricata per resume.
    pub resumed_map_data: Option<ttr_core::MapData>,
    /// Roster salvato, indicizzato per `name_key`; vuoto fuori dal resume.
    pub(crate) saved_roster: BTreeMap<String, SavedEntry>,
    /// Istante di fine partita: dopo `game_over_linger` l'attore termina.
    pub finished_at: Option<Instant>,
    /// Ultimo `Join`/`Start`/`Reassign` riuscito o `Connect` autenticato di un giocatore.
    last_activity: Instant,
    /// Da quando la partita avviata non ha connessioni attive.
    abandoned_since: Option<Instant>,
}

impl GameSession {
    pub fn new_lobby(
        config: SessionConfig,
        rx: mpsc::Receiver<GameCommand>,
        seed: Option<u64>,
    ) -> Self {
        Self::build(config, rx, seed, now_rfc3339(), None)
    }

    /// Crea una sessione in modalità resume da un save file.
    /// Il motore è già ricostruito via replay; la lobby parte col roster
    /// salvato e i giocatori si riagganciano col token originale.
    pub fn new_resumed(
        config: SessionConfig,
        rx: mpsc::Receiver<GameCommand>,
        resumed: ResumedGame,
    ) -> Self {
        let created_at = resumed.created_at.clone();
        Self::build(config, rx, None, created_at, Some(resumed))
    }

    fn build(
        config: SessionConfig,
        rx: mpsc::Receiver<GameCommand>,
        seed: Option<u64>,
        created_at: String,
        resumed: Option<ResumedGame>,
    ) -> Self {
        let mut saved_roster = BTreeMap::new();
        let (resumed_engine, resumed_map_data) = match resumed {
            Some(r) => {
                for (idx, p) in r.players.into_iter().enumerate() {
                    saved_roster.insert(
                        name_key(&p.name),
                        SavedEntry {
                            id: p.id,
                            engine_index: idx,
                            color: p.color,
                            name: p.name,
                            token_hash: p.token_hash,
                        },
                    );
                }
                (Some(r.engine), Some(r.map_data))
            }
            None => (None, None),
        };

        Self {
            game_id: config.game_id,
            game_name: config.game_name,
            ruleset_id: config.ruleset_id,
            map_id: config.map_id,
            created_at,
            engine: None,
            map_data: None,
            players: Vec::new(),
            player_index: BTreeMap::new(),
            host_token: config.host_token,
            max_players: config.max_players,
            started: false,
            connections: BTreeMap::new(),
            spectators: Vec::new(),
            outbound_seq: BTreeMap::new(),
            last_client_seq: BTreeMap::new(),
            next_conn_id: 0,
            final_scores: None,
            longest_path_player_ids: None,
            save_path: config.save_path,
            writer: None,
            results_lock: config.results_lock,
            limits: config.limits,
            finished: config.finished,
            rx,
            seed,
            resumed_engine,
            resumed_map_data,
            saved_roster,
            finished_at: None,
            last_activity: Instant::now(),
            abandoned_since: None,
        }
    }

    /// Prossima scadenza del ciclo di vita, se c'è: partita conclusa,
    /// lobby inattiva o partita senza connessioni.
    fn deadline(&self) -> Option<Instant> {
        if let Some(t) = self.finished_at {
            return Some(t + self.limits.game_over_linger);
        }
        if !self.started {
            let ttl = if self.players.is_empty() && self.saved_roster.is_empty() {
                self.limits.empty_lobby_ttl.min(self.limits.lobby_idle_ttl)
            } else {
                self.limits.lobby_idle_ttl
            };
            return Some(self.last_activity + ttl);
        }
        self.abandoned_since.map(|t| t + self.limits.abandoned_ttl)
    }

    fn has_active_connection(&self) -> bool {
        self.spectators.iter().any(|(_, tx)| !tx.is_closed())
            || self
                .connections
                .values()
                .flatten()
                .any(|(_, tx)| !tx.is_closed())
    }

    /// Rinnova il TTL della lobby: solo attività riuscita e autenticata.
    fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Aggiorna `abandoned_since` in base alle connessioni attive.
    fn refresh_abandoned(&mut self) {
        if !self.started || self.finished_at.is_some() || self.has_active_connection() {
            self.abandoned_since = None;
        } else if self.abandoned_since.is_none() {
            self.abandoned_since = Some(Instant::now());
        }
    }

    pub async fn run(mut self) {
        self.writer = self
            .save_path
            .clone()
            .map(|path| SaveWriter::spawn(path, self.results_lock.clone()));

        loop {
            let (cmd, timed_out) = match self.deadline() {
                Some(t) => tokio::select! {
                    cmd = self.rx.recv() => (cmd, false),
                    _ = tokio::time::sleep_until(t) => (None, true),
                },
                None => (self.rx.recv().await, false),
            };
            if timed_out {
                // Le connessioni chiuse senza `Disconnect` non contano come attive.
                self.refresh_abandoned();
                if self.deadline().is_some_and(|d| d <= Instant::now()) {
                    break;
                }
                continue;
            }
            let Some(cmd) = cmd else { break };

            match cmd {
                GameCommand::Join {
                    player_id,
                    name,
                    color,
                    token,
                    resume_token,
                    reply,
                } => {
                    let result = self.handle_join(player_id, name, color, token, resume_token);
                    if result.is_ok() {
                        self.touch();
                    }
                    let _ = reply.send(result);
                }
                GameCommand::Reassign {
                    host_token,
                    name,
                    new_token,
                    reply,
                } => {
                    let result = self.handle_reassign(&host_token, &name, new_token);
                    if result.is_ok() {
                        self.touch();
                    }
                    let _ = reply.send(result);
                }
                GameCommand::Start {
                    host_token,
                    ruleset,
                    map,
                    reply,
                } => {
                    let result = self.handle_start(host_token, ruleset, map);
                    if result.is_ok() {
                        self.touch();
                    }
                    let _ = reply.send(result);
                }
                GameCommand::Action {
                    player_id,
                    inbound,
                    client_seq,
                } => self.handle_action(player_id, inbound, client_seq),
                GameCommand::Connect {
                    player_id,
                    token,
                    tx,
                    reply,
                } => self.handle_connect(player_id, token, tx, reply),
                GameCommand::Disconnect { player_id, conn_id } => {
                    self.handle_disconnect(player_id, conn_id)
                }
                GameCommand::RequestSnapshot { player_id, reply } => {
                    let snap = self.build_snapshot(player_id.as_deref());
                    let _ = reply.send(snap);
                }
                GameCommand::GetInfo { reply } => {
                    let info = self.build_game_info();
                    let _ = reply.send(info);
                }
                GameCommand::Delete { host_token, reply } => {
                    if tokens_equal(&host_token, &self.host_token) {
                        // Il 204 arriva a scritture chiuse: un resume subito dopo
                        // non legge né sostituisce un save ancora in scrittura.
                        if let Some(writer) = self.writer.take() {
                            writer.finish().await;
                        }
                        let _ = reply.send(Ok(()));
                        break;
                    }
                    let _ = reply.send(Err(LobbyError::Unauthorized));
                }
                GameCommand::Shutdown => break,
            }
            self.refresh_abandoned();
        }

        // Prima di terminare i lavori pendenti devono essere su disco.
        if let Some(writer) = self.writer.take() {
            writer.finish().await;
        }
    }

    fn handle_join(
        &mut self,
        player_id: String,
        name: String,
        color: Option<PlayerColor>,
        token: String,
        resume_token: Option<String>,
    ) -> Result<JoinResponse, LobbyError> {
        if self.started {
            return self.rejoin_started(&name, resume_token.as_deref());
        }

        // Resume: il nome deve essere nel save e il token è quello originale.
        // I limiti sui nomi valgono solo per i nuovi join: i nomi di un save
        // esistente restano accettati così come sono.
        if !self.saved_roster.is_empty() {
            return self.join_resumed(&name, resume_token.as_deref());
        }

        let name = normalize_name(&name, MAX_PLAYER_NAME_CHARS)?;
        let key = name_key(&name);

        if self.players.len() >= self.max_players as usize {
            return Err(LobbyError::GameFull);
        }
        if self.players.iter().any(|p| name_key(&p.name) == key) {
            return Err(LobbyError::NameTaken);
        }
        if let Some(requested) = color
            && self.players.iter().any(|p| p.color == requested)
        {
            return Err(LobbyError::ColorTaken);
        }

        let idx = self.players.len();
        self.player_index.insert(player_id.clone(), idx);
        self.outbound_seq.insert(player_id.clone(), 0);
        self.last_client_seq.insert(player_id.clone(), None);
        self.connections.insert(player_id.clone(), Vec::new());

        // Colore esplicito se richiesto, altrimenti primo libero in ordine
        // RED, BLUE, GREEN, YELLOW, BLACK.
        let resolved_color = color.unwrap_or_else(|| first_free_color(&self.players));

        self.players.push(SessionPlayer {
            name: name.clone(),
            color: resolved_color,
            token: token.clone(),
            index: None,
        });

        self.broadcast_public_state();

        Ok(JoinResponse {
            player_id,
            token,
            name,
        })
    }

    /// Join in una partita avviata: nessun nuovo giocatore, rientra solo chi è
    /// già nel roster e conosce il proprio token. Idempotente, nessun broadcast.
    fn rejoin_started(
        &self,
        name: &str,
        resume_token: Option<&str>,
    ) -> Result<JoinResponse, LobbyError> {
        if name_too_long(name, self.players.iter().map(|p| p.name.as_str())) {
            return Err(LobbyError::GameStarted);
        }
        let key = name_key(name);
        let (player_id, player) = self
            .player_index
            .iter()
            .map(|(id, &pos)| (id, &self.players[pos]))
            .find(|(_, p)| name_key(&p.name) == key)
            .ok_or(LobbyError::GameStarted)?;
        match resume_token {
            Some(t) if tokens_equal(t, &player.token) => Ok(JoinResponse {
                player_id: player_id.clone(),
                token: player.token.clone(),
                name: player.name.clone(),
            }),
            _ => Err(LobbyError::ResumeTokenMismatch),
        }
    }

    /// Join in resume: riesce solo col token originale del giocatore salvato,
    /// che resta il suo token.
    fn join_resumed(
        &mut self,
        name: &str,
        resume_token: Option<&str>,
    ) -> Result<JoinResponse, LobbyError> {
        if name_too_long(name, self.saved_roster.values().map(|e| e.name.as_str())) {
            return Err(LobbyError::NameNotInSave);
        }
        let entry = self
            .saved_roster
            .get(&name_key(name))
            .cloned()
            .ok_or(LobbyError::NameNotInSave)?;

        if let Some(&pos) = self.player_index.get(&entry.id) {
            // Già rientrato: ripetere il join col proprio token è innocuo.
            let current = &self.players[pos].token;
            return match resume_token {
                Some(t) if tokens_equal(t, current) => Ok(JoinResponse {
                    player_id: entry.id,
                    token: current.clone(),
                    name: entry.name,
                }),
                _ => Err(LobbyError::NameTaken),
            };
        }

        let token = match (resume_token, entry.token_hash.as_deref()) {
            (Some(t), Some(hash)) if token_matches(t, hash) => t.to_string(),
            _ => return Err(LobbyError::ResumeTokenMismatch),
        };
        Ok(self.add_resumed_player(entry, token))
    }

    /// Inserisce nel roster un giocatore del save con il token dato.
    fn add_resumed_player(&mut self, entry: SavedEntry, token: String) -> JoinResponse {
        // `player_index` mappa player_id → posizione in `self.players`,
        // non l'indice del motore (che sta in `SessionPlayer.index`).
        let roster_idx = self.players.len();
        self.player_index.insert(entry.id.clone(), roster_idx);
        self.outbound_seq.insert(entry.id.clone(), 0);
        self.last_client_seq.insert(entry.id.clone(), None);
        self.connections.insert(entry.id.clone(), Vec::new());
        self.players.push(SessionPlayer {
            name: entry.name.clone(),
            color: entry.color,
            token: token.clone(),
            index: Some(entry.engine_index),
        });
        self.broadcast_public_state();
        JoinResponse {
            player_id: entry.id,
            token,
            name: entry.name,
        }
    }

    /// Assegna un token nuovo a un giocatore, su richiesta dell'host: in un
    /// resume per i nomi del save, in una partita avviata per i giocatori del
    /// roster. Le connessioni aperte col token precedente vengono scollegate.
    fn handle_reassign(
        &mut self,
        host_token: &str,
        name: &str,
        new_token: String,
    ) -> Result<JoinResponse, LobbyError> {
        if !tokens_equal(host_token, &self.host_token) {
            return Err(LobbyError::Unauthorized);
        }
        if self.finished_at.is_some() {
            return Err(LobbyError::BadRequest("La partita è conclusa.".to_string()));
        }
        if self.saved_roster.is_empty() {
            if !self.started {
                return Err(LobbyError::BadRequest(
                    "La partita non è stata ripresa da un save.".to_string(),
                ));
            }
            return self.reassign_started(name, new_token);
        }
        let entry = self
            .saved_roster
            .get(&name_key(name))
            .cloned()
            .ok_or(LobbyError::NameNotInSave)?;

        let Some(&pos) = self.player_index.get(&entry.id) else {
            let response = self.add_resumed_player(entry, new_token);
            self.write_save();
            return Ok(response);
        };
        self.players[pos].token = new_token.clone();
        if let Some(conns) = self.connections.get_mut(&entry.id) {
            conns.clear();
        }
        self.write_save();
        self.broadcast_public_state();
        Ok(JoinResponse {
            player_id: entry.id,
            token: new_token,
            name: entry.name,
        })
    }

    /// Reassign in una partita avviata non ripresa: il giocatore è già nel
    /// roster, quindi si sostituisce solo il token.
    fn reassign_started(
        &mut self,
        name: &str,
        new_token: String,
    ) -> Result<JoinResponse, LobbyError> {
        let key = name_key(name);
        let (player_id, pos) = self
            .player_index
            .iter()
            .map(|(id, &pos)| (id.clone(), pos))
            .find(|&(_, pos)| name_key(&self.players[pos].name) == key)
            .ok_or(LobbyError::NameNotInSave)?;
        self.players[pos].token = new_token.clone();
        if let Some(conns) = self.connections.get_mut(&player_id) {
            conns.clear();
        }
        self.write_save();
        self.broadcast_public_state();
        Ok(JoinResponse {
            player_id,
            token: new_token,
            name: self.players[pos].name.clone(),
        })
    }

    fn handle_start(
        &mut self,
        host_token: String,
        ruleset: ttr_core::Ruleset,
        map: ttr_core::MapData,
    ) -> Result<(), LobbyError> {
        if self.started {
            return Err(LobbyError::GameStarted);
        }
        if !tokens_equal(&host_token, &self.host_token) {
            return Err(LobbyError::Unauthorized);
        }

        // Il controllo precede `take()`: un rifiuto non deve consumare il motore.
        // Il confronto è col motore perché `saved_roster` ha chiavi `name_key`
        // e può contenere meno voci dei giocatori di un save manomesso.
        if let Some(engine) = &self.resumed_engine
            && self.player_index.len() != engine.num_players()
        {
            return Err(LobbyError::RosterIncomplete);
        }

        if let Some(engine) = self.resumed_engine.take() {
            let map_data = self.resumed_map_data.take().unwrap_or(map);
            // Gli indici del motore sono già assegnati dal join.
            self.engine = Some(engine);
            self.map_data = Some(map_data);
            self.started = true;
            self.write_save();
            self.broadcast_public_state();
            self.push_private_states();
            return Ok(());
        }

        let min_players = ruleset.players.min;
        if self.players.len() < min_players as usize {
            return Err(LobbyError::MinPlayers(min_players));
        }

        // Il seed scelto alla creazione vale solo se il server lo ammette;
        // altrimenti 64 bit da CSPRNG, che finiscono solo nel save.
        let seed = match self.seed {
            Some(seed) => seed,
            None => getrandom::u64().map_err(|e| {
                tracing::error!(game_id = %self.game_id, error = %e, "seed CSPRNG non disponibile");
                LobbyError::Internal(e.to_string())
            })?,
        };

        // Ordine di join = indice del motore.
        let mut ordered: Vec<(usize, String)> = self
            .player_index
            .iter()
            .map(|(id, &idx)| (idx, id.clone()))
            .collect();
        ordered.sort_by_key(|(idx, _)| *idx);
        let ordered_ids: Vec<String> = ordered.iter().map(|(_, id)| id.clone()).collect();
        let ordered_names: Vec<String> = ordered
            .iter()
            .map(|(idx, _)| self.players[*idx].name.clone())
            .collect();
        let ordered_colors: Vec<PlayerColor> = ordered
            .iter()
            .map(|(idx, _)| self.players[*idx].color)
            .collect();

        let engine = ttr_core::Engine::new(
            seed,
            ruleset,
            map.clone(),
            ordered_ids,
            ordered_names,
            ordered_colors,
            0, // first_player
        )
        .map_err(|e| {
            tracing::error!(game_id = %self.game_id, error = %e, "inizializzazione del motore fallita");
            LobbyError::Internal(e.to_string())
        })?;

        for (i, p) in self.players.iter_mut().enumerate() {
            p.index = Some(i);
        }
        self.engine = Some(engine);
        self.map_data = Some(map);
        self.started = true;

        self.write_save();
        self.broadcast_public_state();
        self.push_private_states();

        Ok(())
    }

    fn handle_action(
        &mut self,
        player_id: String,
        inbound: ttr_proto::InboundAction,
        client_seq: Option<u64>,
    ) {
        // Idempotenza: `None` = mai visto un client_seq da questo giocatore e
        // qualsiasi seq è accettato; con `Some(last)` un seq <= last è un
        // duplicato, scartato rinviando lo stato privato corrente.
        if let Some(seq) = client_seq {
            let last = self.last_client_seq.get(&player_id).and_then(|v| *v);
            if let Some(last_seq) = last
                && seq <= last_seq
            {
                let engine_idx = self
                    .player_index
                    .get(&player_id)
                    .and_then(|&session_pos| self.players.get(session_pos).and_then(|p| p.index));
                self.push_private_state_single(&player_id, engine_idx);
                return;
            }
            self.last_client_seq.insert(player_id.clone(), Some(seq));
        }

        // REQUEST_SNAPSHOT è gestita dal chiamante in ws.rs e non arriva qui.

        let session_pos = match self.player_index.get(&player_id).copied() {
            Some(idx) => idx,
            None => {
                self.send_error(
                    &player_id,
                    ttr_proto::ErrorReason::InvalidAction,
                    Some("player sconosciuto".into()),
                );
                return;
            }
        };
        let engine_idx = match self.players.get(session_pos).and_then(|p| p.index) {
            Some(ei) => ei,
            None => {
                self.send_error(
                    &player_id,
                    ttr_proto::ErrorReason::InvalidAction,
                    Some("giocatore non ancora inizializzato".into()),
                );
                return;
            }
        };

        let action = crate::lobby::inbound_to_core_action(&inbound, engine_idx);

        let engine = match &mut self.engine {
            Some(e) => e,
            None => {
                self.send_error(
                    &player_id,
                    ttr_proto::ErrorReason::WrongState,
                    Some("PARTITA_NON_AVVIATA".into()),
                );
                return;
            }
        };

        match engine.apply(action) {
            Ok(events) => {
                for event in &events {
                    if let ttr_core::event::GameEvent::GameOver {
                        final_scores,
                        longest_path_players,
                        ..
                    } = event
                    {
                        self.finished.store(true, Ordering::Release);
                        if let Some(inexact) = self
                            .engine
                            .as_ref()
                            .map(|e| e.longest_path_inexact_players())
                            .filter(|p| !p.is_empty())
                        {
                            tracing::warn!(
                                game_id = %self.game_id,
                                players = ?inexact,
                                "percorso più lungo non esatto: budget di ricerca esaurito"
                            );
                        }
                        self.final_scores = Some(bridge::build_final_scores(
                            final_scores,
                            longest_path_players,
                            &self.players,
                            &self.player_index,
                        ));
                        self.longest_path_player_ids = Some(
                            longest_path_players
                                .iter()
                                .map(|&ei| {
                                    // `ei` è un indice del motore: cerca il SessionPlayer con .index == ei.
                                    self.players
                                        .iter()
                                        .position(|p| p.index == Some(ei))
                                        .and_then(|session_pos| {
                                            self.player_index
                                                .iter()
                                                .find(|&(_, &sp)| sp == session_pos)
                                                .map(|(id, _)| id.clone())
                                        })
                                        .unwrap_or_default()
                                })
                                .collect(),
                        );
                    }
                }

                self.broadcast_public_state();
                self.push_private_states();
                if self.final_scores.is_some() {
                    // Partita conclusa: restano solo i punteggi per la
                    // classifica generale; il save della partita sparisce.
                    self.archive_result_and_delete_save();
                    self.finished_at = Some(Instant::now());
                } else {
                    self.write_save();
                }
            }
            Err(err) => {
                let reason = bridge::engine_error_to_reason(&err);
                let action_ref = bridge::error_action_ref(&err);
                let detail = Some(err.to_string());
                self.send_error_detail(&player_id, reason, action_ref, detail);
            }
        }
    }

    fn handle_connect(
        &mut self,
        player_id: Option<String>,
        token: Option<String>,
        tx: OutboundTx,
        reply: tokio::sync::oneshot::Sender<ConnectOutcome>,
    ) {
        let authenticated = match (&player_id, &token) {
            (Some(pid), Some(tok)) => self
                .player_index
                .get(pid)
                .and_then(|&idx| self.players.get(idx))
                .map(|p| tokens_equal(&p.token, tok))
                .unwrap_or(false),
            (None, _) => true, // spettatore: sempre ok
            _ => false,
        };

        if !authenticated {
            // L'handler WS chiude il socket su auth fallita.
            let _ = reply.send(ConnectOutcome::Unauthorized);
            return;
        }

        if player_id.is_none() {
            self.spectators.retain(|(_, tx)| !tx.is_closed());
            if self.spectators.len() >= self.limits.max_spectators {
                let _ = reply.send(ConnectOutcome::TooManySpectators);
                return;
            }
        }

        if player_id.is_some() {
            self.touch();
        }

        let conn_id = self.next_conn_id;
        self.next_conn_id += 1;

        // Presence: registra la soglia 0→1 (prima connessione attiva) per decidere
        // se broadcastare lo stato pubblico aggiornato. Gli spettatori non contano.
        let mut became_connected = false;
        if let Some(pid) = &player_id {
            // Una connessione è "attiva" solo se il canale è ancora aperto: un
            // task ws abortito senza Disconnect (iOS che uccide la tab in
            // background) non deve lasciare il giocatore online per sempre.
            let was_connected = self
                .connections
                .get(pid)
                .is_some_and(|v| v.iter().any(|(_, tx)| !tx.is_closed()));
            let conns = self.connections.entry(pid.clone()).or_default();
            conns.retain(|(_, tx)| !tx.is_closed());
            // Oltre il limite si chiude la connessione più vecchia: il suo
            // canale cade e il task di uscita chiude il socket.
            let keep = self.limits.max_connections_per_player.saturating_sub(1);
            if conns.len() > keep {
                let excess = conns.len() - keep;
                conns.drain(..excess);
            }
            conns.push((conn_id, tx.clone()));
            became_connected = !was_connected;
        } else {
            self.spectators.push((conn_id, tx.clone()));
        }

        let _ = reply.send(ConnectOutcome::Ok { conn_id });

        if let Some(msg) = self.build_snapshot(player_id.as_deref()) {
            let _ = tx.try_send(msg);
        }

        // Solo al passaggio offline→online: tutti vedono `connected` aggiornato.
        // Una seconda connessione dello stesso player (dedup/reconnect) non broadcasta.
        if became_connected {
            self.broadcast_public_state();
        }
    }

    fn handle_disconnect(&mut self, player_id: Option<String>, conn_id: u64) {
        // Presenza: true se il giocatore passa da N→0 connessioni attive (online→offline).
        let mut became_disconnected = false;

        // Rimuove la singola connessione per conn_id, non tutte del giocatore.
        if let Some(pid) = &player_id {
            if let Some(senders) = self.connections.get_mut(pid) {
                let before = !senders.is_empty();
                senders.retain(|(cid, _)| *cid != conn_id);
                // Se il giocatore non ha più connessioni, non rimuoviamo l'entry:
                // il giocatore esiste ancora e può riconnettersi.
                became_disconnected = before && senders.is_empty();
            }
            // `pid` non registrato (es. Connect finito in Unauthorized, socket chiuso
            // subito): nessuna connessione da rimuovere → nessun broadcast, nessun panic.
        } else {
            self.spectators.retain(|(cid, _)| *cid != conn_id);
        }
        // Pulizia connessioni chiuse (best-effort).
        self.spectators.retain(|(_, tx)| !tx.is_closed());
        for senders in self.connections.values_mut() {
            senders.retain(|(_, tx)| !tx.is_closed());
        }

        // Solo al passaggio online→offline: tutti vedono `connected` aggiornato.
        if became_disconnected {
            self.broadcast_public_state();
        }
    }

    // ── Broadcast e push ─────────────────────────────────────────

    /// Set dei `player_id` con ≥1 connessione WS attiva (presence runtime).
    /// Gli spettatori non hanno `player_id` e non compaiono qui.
    fn connected_player_ids(&self) -> std::collections::HashSet<String> {
        self.connections
            .iter()
            .filter(|(_, senders)| senders.iter().any(|(_, tx)| !tx.is_closed()))
            .map(|(pid, _)| pid.clone())
            .collect()
    }

    /// Invia a tutte le connessioni; quelle con il canale pieno o chiuso
    /// vengono rimosse, così il loro socket si chiude e il client si
    /// riconnette ricevendo lo SNAPSHOT.
    fn broadcast(&mut self, msg: &ServerMessage) {
        self.spectators
            .retain(|(_, tx)| tx.try_send(msg.clone()).is_ok());
        for senders in self.connections.values_mut() {
            senders.retain(|(_, tx)| tx.try_send(msg.clone()).is_ok());
        }
    }

    fn broadcast_public_state(&mut self) {
        let connected = self.connected_player_ids();
        let public = bridge::build_public_state(self, &connected);
        self.broadcast(&ServerMessage::PublicState(public));
    }

    fn push_private_states(&mut self) {
        // Passa l'indice del motore (SessionPlayer.index), non la posizione in self.players.
        let players: Vec<(String, usize)> = self
            .player_index
            .iter()
            .filter_map(|(pid, &idx)| {
                self.players
                    .get(idx)
                    .and_then(|p| p.index)
                    .map(|ei| (pid.clone(), ei))
            })
            .collect();
        for (pid, engine_idx) in players {
            self.push_private_state_single(&pid, Some(engine_idx));
        }
    }

    fn push_private_state_single(&mut self, player_id: &str, player_index: Option<usize>) {
        let private = bridge::build_private_state(self, player_id, player_index);
        if let Some(mut ps) = private {
            let seq = self.outbound_seq.get(player_id).copied().unwrap_or(0) + 1;
            self.outbound_seq.insert(player_id.to_string(), seq);
            ps.seq = seq;
            self.send_to_player(player_id, &ServerMessage::PrivateState(ps));
        }
    }

    fn send_to_player(&mut self, player_id: &str, msg: &ServerMessage) {
        if let Some(senders) = self.connections.get_mut(player_id) {
            senders.retain(|(_, tx)| tx.try_send(msg.clone()).is_ok());
        }
    }

    fn send_error(
        &mut self,
        player_id: &str,
        reason: ttr_proto::ErrorReason,
        action_ref: Option<String>,
    ) {
        self.send_error_detail(player_id, reason, action_ref, None);
    }

    fn send_error_detail(
        &mut self,
        player_id: &str,
        reason: ttr_proto::ErrorReason,
        action_ref: Option<String>,
        detail: Option<String>,
    ) {
        let seq = self.outbound_seq.get(player_id).copied().unwrap_or(0) + 1;
        self.outbound_seq.insert(player_id.to_string(), seq);

        let msg = ServerMessage::Error(ttr_proto::ErrorPayload {
            seq,
            reason,
            action_ref,
            detail,
        });
        self.send_to_player(player_id, &msg);
    }

    // ── Snapshot ─────────────────────────────────────────────────

    fn build_snapshot(&mut self, player_id: Option<&str>) -> Option<ServerMessage> {
        let connected = self.connected_player_ids();
        let public = bridge::build_public_state(self, &connected);
        let private = player_id.and_then(|pid| {
            let engine_idx = self
                .player_index
                .get(pid)
                .and_then(|&idx| self.players.get(idx).and_then(|p| p.index));
            // Anche il `seq` del private dentro lo SNAPSHOT avanza.
            let seq = self.outbound_seq.get(pid).copied().unwrap_or(0) + 1;
            self.outbound_seq.insert(pid.to_string(), seq);
            bridge::build_private_state(self, pid, engine_idx)
        });

        Some(ServerMessage::Snapshot(ttr_proto::Snapshot {
            protocol_version: 1,
            public,
            private,
        }))
    }

    // ── GameInfo (lobby via attore) ──────────────────────────────

    fn build_game_info(&self) -> GameInfo {
        // Ordina i giocatori per indice di join (deterministico).
        let mut ordered: Vec<(usize, &String)> = self
            .player_index
            .iter()
            .map(|(id, &idx)| (idx, id))
            .collect();
        ordered.sort_by_key(|(idx, _)| *idx);

        let players: Vec<(String, String, PlayerColor)> = ordered
            .into_iter()
            .filter_map(|(idx, id)| {
                self.players
                    .get(idx)
                    .map(|p| (id.clone(), p.name.clone(), p.color))
            })
            .collect();

        let mut missing: Vec<&SavedEntry> = self
            .saved_roster
            .values()
            .filter(|e| !self.player_index.contains_key(&e.id))
            .collect();
        missing.sort_by_key(|e| e.engine_index);

        GameInfo {
            players,
            players_joined: self.player_index.len(),
            started: self.started,
            missing_players: missing.into_iter().map(|e| e.name.clone()).collect(),
        }
    }

    // ── Save ─────────────────────────────────────────────────────

    /// Partita conclusa: accoda l'archiviazione dei punteggi in `results.jsonl`
    /// (classifica generale) e l'eliminazione del save. Se l'archiviazione
    /// fallisce il save `completed` resta su disco e viene archiviato al
    /// riavvio successivo.
    fn archive_result_and_delete_save(&self) {
        let Some(writer) = &self.writer else { return };
        let Some(scores) = &self.final_scores else {
            return;
        };
        let result = crate::save::GameResult::from_scores(
            &self.game_id,
            &self.game_name,
            &self.ruleset_id,
            now_rfc3339(),
            scores.iter().map(|fs| {
                let name = self
                    .player_index
                    .get(&fs.player_id)
                    .and_then(|&i| self.players.get(i))
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| fs.player_id.clone());
                (name, fs.total, fs.winner)
            }),
        );
        writer.archive(result, build_save_data(self));
    }

    fn write_save(&self) {
        if let Some(writer) = &self.writer {
            writer.save(build_save_data(self));
        }
    }

    // ── Helper: mappa indice giocatore → player_id ───────────────

    pub(crate) fn player_id_for_index(&self, index: usize) -> Option<&str> {
        let session_pos = self.players.iter().position(|p| p.index == Some(index))?;
        self.player_index
            .iter()
            .find(|&(_, i)| *i == session_pos)
            .map(|(id, _)| id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::error::TryRecvError;

    fn lobby_session() -> GameSession {
        let (_tx, rx) = mpsc::channel(1);
        GameSession::new_lobby(
            SessionConfig {
                game_id: "g".into(),
                game_name: "g".into(),
                ruleset_id: "usa_v2".into(),
                map_id: "usa".into(),
                max_players: 5,
                host_token: "h".into(),
                save_path: None,
                results_lock: Arc::default(),
                limits: SessionLimits::default(),
                finished: Arc::default(),
            },
            rx,
            None,
        )
    }

    fn notice() -> ServerMessage {
        ServerMessage::Error(ttr_proto::ErrorPayload {
            seq: 0,
            reason: ttr_proto::ErrorReason::GameNotFound,
            action_ref: None,
            detail: None,
        })
    }

    /// Un client che non legge viene scollegato quando il suo canale è pieno.
    #[test]
    fn full_outbound_channel_drops_the_connection() {
        let mut session = lobby_session();
        let (tx, mut rx) = mpsc::channel(2);
        session
            .connections
            .entry("p".into())
            .or_default()
            .push((0, tx));
        let (spectator_tx, _spectator_rx) = mpsc::channel(8);
        session.spectators.push((1, spectator_tx));

        session.broadcast(&notice());
        session.broadcast(&notice());
        assert_eq!(session.connections["p"].len(), 1, "canale non ancora pieno");
        session.broadcast(&notice());
        assert!(
            session.connections["p"].is_empty(),
            "canale pieno: connessione rimossa"
        );
        assert_eq!(
            session.spectators.len(),
            1,
            "gli altri client non sono toccati"
        );

        assert!(rx.try_recv().is_ok() && rx.try_recv().is_ok());
        assert_eq!(rx.try_recv().unwrap_err(), TryRecvError::Disconnected);
    }

    /// Avvia l'attore di una lobby con tempo di tokio in pausa.
    fn spawn_lobby(
        limits: SessionLimits,
    ) -> (mpsc::Sender<GameCommand>, tokio::task::JoinHandle<()>) {
        let (tx, rx) = mpsc::channel(16);
        let mut session = lobby_session();
        session.rx = rx;
        session.limits = limits;
        session.last_activity = Instant::now();
        (tx, tokio::spawn(session.run()))
    }

    fn ttl_limits() -> SessionLimits {
        SessionLimits {
            lobby_idle_ttl: Duration::from_secs(10),
            empty_lobby_ttl: Duration::from_secs(1),
            ..SessionLimits::default()
        }
    }

    /// Fa trascorrere `d` di tempo simulato e lascia girare l'attore.
    async fn elapse(d: Duration) {
        tokio::time::advance(d).await;
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
    }

    async fn join(tx: &mpsc::Sender<GameCommand>, name: &str) -> Result<JoinResponse, LobbyError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        tx.send(GameCommand::Join {
            player_id: name.into(),
            name: name.into(),
            color: None,
            token: format!("tok-{name}"),
            resume_token: None,
            reply,
        })
        .await
        .unwrap();
        rx.await.unwrap()
    }

    async fn connect(tx: &mpsc::Sender<GameCommand>, player: &str, token: &str) -> ConnectOutcome {
        let (out, _out_rx) = mpsc::channel(8);
        let (reply, rx) = tokio::sync::oneshot::channel();
        tx.send(GameCommand::Connect {
            player_id: Some(player.into()),
            token: Some(token.into()),
            tx: out,
            reply,
        })
        .await
        .unwrap();
        rx.await.unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn successful_join_renews_the_lobby_ttl() {
        let (tx, task) = spawn_lobby(ttl_limits());
        join(&tx, "Anna").await.unwrap();
        for name in ["Bob", "Ciro", "Dora"] {
            elapse(Duration::from_secs(6)).await;
            assert!(!task.is_finished());
            join(&tx, name).await.unwrap();
        }
        elapse(Duration::from_secs(9)).await;
        assert!(!task.is_finished());
        elapse(Duration::from_secs(2)).await;
        assert!(task.is_finished());
    }

    #[tokio::test(start_paused = true)]
    async fn failed_join_does_not_renew_the_lobby_ttl() {
        let (tx, task) = spawn_lobby(ttl_limits());
        join(&tx, "Anna").await.unwrap();
        for _ in 0..5 {
            elapse(Duration::from_secs(1)).await;
            assert!(join(&tx, "").await.is_err());
        }
        elapse(Duration::from_secs(6)).await;
        assert!(task.is_finished());
    }

    #[tokio::test(start_paused = true)]
    async fn only_authenticated_connect_renews_the_lobby_ttl() {
        let (tx, task) = spawn_lobby(ttl_limits());
        join(&tx, "Anna").await.unwrap();
        elapse(Duration::from_secs(6)).await;
        assert!(matches!(
            connect(&tx, "Anna", "sbagliato").await,
            ConnectOutcome::Unauthorized
        ));
        elapse(Duration::from_secs(5)).await;
        assert!(task.is_finished(), "token errato: il TTL non si rinnova");

        let (tx, task) = spawn_lobby(ttl_limits());
        join(&tx, "Anna").await.unwrap();
        elapse(Duration::from_secs(6)).await;
        assert!(matches!(
            connect(&tx, "Anna", "tok-Anna").await,
            ConnectOutcome::Ok { .. }
        ));
        elapse(Duration::from_secs(6)).await;
        assert!(!task.is_finished(), "connect autenticato: TTL rinnovato");
    }

    #[tokio::test(start_paused = true)]
    async fn lobby_with_a_player_ignores_the_empty_ttl() {
        let (tx, task) = spawn_lobby(ttl_limits());
        join(&tx, "Anna").await.unwrap();
        elapse(Duration::from_secs(5)).await;
        assert!(!task.is_finished());
        elapse(Duration::from_secs(6)).await;
        assert!(task.is_finished());
    }

    #[tokio::test(start_paused = true)]
    async fn empty_lobby_expires_with_the_empty_ttl() {
        let (_tx, task) = spawn_lobby(ttl_limits());
        elapse(Duration::from_millis(900)).await;
        assert!(!task.is_finished());
        elapse(Duration::from_millis(200)).await;
        assert!(task.is_finished());
    }
}
