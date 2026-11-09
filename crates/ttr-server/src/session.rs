//! Sessione di gioco con attore tokio: possiede `Option<Engine>`, roster,
//! collegamenti WebSocket, contatori seq, e coordina il ciclo di vita.
//!
//! Ogni mutazione del motore passa attraverso l'attore — nessuna race.

use std::collections::BTreeMap;

use tokio::sync::mpsc;
use ttr_proto::{PlayerColor, ServerMessage};

use crate::bridge;

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
}

/// Informazioni fresche sulla partita richieste dalla lobby REST.
#[derive(Debug, Clone)]
pub struct GameInfo {
    pub players: Vec<(String, String, PlayerColor)>, // (player_id, name, color)
    pub players_joined: usize,
    pub started: bool,
}

#[derive(Debug)]
pub enum GameCommand {
    Join {
        player_id: String,
        name: String,
        /// `None` = colore automatico (primo libero in ordine, pacchetto §5).
        color: Option<PlayerColor>,
        token: String,
        reply: tokio::sync::oneshot::Sender<Result<JoinResponse, String>>,
    },
    Start {
        host_token: String,
        ruleset: ttr_core::Ruleset,
        map: ttr_core::MapData,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Action {
        player_id: String,
        inbound: ttr_proto::InboundAction,
        client_seq: Option<u64>,
    },
    Connect {
        player_id: Option<String>,
        token: Option<String>,
        tx: mpsc::UnboundedSender<ServerMessage>,
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
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct JoinResponse {
    pub player_id: String,
    pub token: String,
}

const MAX_PLAYER_NAME_CHARS: usize = 24;

/// Ordine di assegnazione automatica dei colori (pacchetto §5).
const COLOR_ORDER: [PlayerColor; 5] = [
    PlayerColor::Red,
    PlayerColor::Blue,
    PlayerColor::Green,
    PlayerColor::Yellow,
    PlayerColor::Black,
];

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
    pub index: Option<usize>,
}

pub(crate) struct GameSession {
    pub game_id: String,
    pub game_name: String,
    pub ruleset_id: String,
    pub engine: Option<ttr_core::Engine>,
    /// Mappa di gioco (per lookup punti biglietti nelle conversioni bridge).
    pub map_data: Option<ttr_core::MapData>,
    pub players: Vec<SessionPlayer>,
    pub player_index: BTreeMap<String, usize>,
    pub host_token: String,
    pub max_players: u32,
    pub started: bool,
    /// Connessioni per giocatore: ogni entry è (conn_id, sender).
    pub connections: BTreeMap<String, Vec<(u64, mpsc::UnboundedSender<ServerMessage>)>>,
    /// Spettatori: (conn_id, sender).
    pub spectators: Vec<(u64, mpsc::UnboundedSender<ServerMessage>)>,
    pub outbound_seq: BTreeMap<String, u64>,
    /// Ultimo `client_seq` ricevuto per giocatore (`None` = mai visto, F5).
    pub last_client_seq: BTreeMap<String, Option<u64>>,
    /// Contatore per assegnare connection ID univoci (F6).
    pub next_conn_id: u64,
    pub final_scores: Option<Vec<ttr_proto::FinalScoreInfo>>,
    pub longest_path_player_ids: Option<Vec<String>>,
    pub save_path: Option<std::path::PathBuf>,
    pub rx: mpsc::Receiver<GameCommand>,
    /// Seed RNG opzionale (creazione partita); `None` = seed time-based.
    pub seed: Option<u64>,
    /// Motore pre-replay per resume (D18 §8).
    pub resumed_engine: Option<ttr_core::Engine>,
    /// Mappa pre-caricata per resume.
    pub resumed_map_data: Option<ttr_core::MapData>,
    /// Roster salvato: nome (lowercase) → (player_id, engine_index, colore).
    pub saved_roster: BTreeMap<String, (String, usize, ttr_proto::PlayerColor)>,
}

impl GameSession {
    #[allow(clippy::too_many_arguments)]
    pub fn new_lobby(
        game_id: String,
        game_name: String,
        ruleset_id: String,
        max_players: u32,
        host_token: String,
        save_path: Option<std::path::PathBuf>,
        rx: mpsc::Receiver<GameCommand>,
        seed: Option<u64>,
    ) -> Self {
        Self {
            game_id,
            game_name,
            ruleset_id,
            engine: None,
            map_data: None,
            players: Vec::new(),
            player_index: BTreeMap::new(),
            host_token,
            max_players,
            started: false,
            connections: BTreeMap::new(),
            spectators: Vec::new(),
            outbound_seq: BTreeMap::new(),
            last_client_seq: BTreeMap::new(),
            next_conn_id: 0,
            final_scores: None,
            longest_path_player_ids: None,
            save_path,
            rx,
            seed,
            resumed_engine: None,
            resumed_map_data: None,
            saved_roster: BTreeMap::new(),
        }
    }

    /// Crea una sessione in modalità resume da un save file (D18 §8).
    /// Il motore è già ricostruito via replay; la lobby parte col roster
    /// salvato e i giocatori si riagganciano per nome.
    #[allow(clippy::too_many_arguments)]
    pub fn new_resumed(
        game_id: String,
        game_name: String,
        ruleset_id: String,
        max_players: u32,
        host_token: String,
        save_path: Option<std::path::PathBuf>,
        rx: mpsc::Receiver<GameCommand>,
        engine: ttr_core::Engine,
        map_data: ttr_core::MapData,
        saved_player_entries: Vec<(String, String, ttr_proto::PlayerColor)>, // (id, name, color)
    ) -> Self {
        let mut saved_roster = BTreeMap::new();
        for (idx, (id, name, color)) in saved_player_entries.iter().enumerate() {
            saved_roster.insert(name.to_lowercase(), (id.clone(), idx, *color));
        }

        Self {
            game_id,
            game_name,
            ruleset_id,
            engine: None,
            map_data: None,
            players: Vec::new(),
            player_index: BTreeMap::new(),
            host_token,
            max_players,
            started: false,
            connections: BTreeMap::new(),
            spectators: Vec::new(),
            outbound_seq: BTreeMap::new(),
            last_client_seq: BTreeMap::new(),
            next_conn_id: 0,
            final_scores: None,
            longest_path_player_ids: None,
            save_path,
            rx,
            seed: None,
            resumed_engine: Some(engine),
            resumed_map_data: Some(map_data),
            saved_roster,
        }
    }

    pub async fn run(mut self) {
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                GameCommand::Join {
                    player_id,
                    name,
                    color,
                    token,
                    reply,
                } => {
                    let result = self.handle_join(player_id, name, color, token);
                    let _ = reply.send(result);
                }
                GameCommand::Start {
                    host_token,
                    ruleset,
                    map,
                    reply,
                } => {
                    let result = self.handle_start(host_token, ruleset, map);
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
                GameCommand::Disconnect {
                    player_id,
                    conn_id,
                } => self.handle_disconnect(player_id, conn_id),
                GameCommand::RequestSnapshot { player_id, reply } => {
                    let snap = self.build_snapshot(player_id.as_deref());
                    let _ = reply.send(snap);
                }
                GameCommand::GetInfo { reply } => {
                    let info = self.build_game_info();
                    let _ = reply.send(info);
                }
                GameCommand::Shutdown => break,
            }
        }
    }

    fn handle_join(
        &mut self,
        player_id: String,
        name: String,
        color: Option<PlayerColor>,
        token: String,
    ) -> Result<JoinResponse, String> {
        if self.started {
            return Err("GAME_STARTED".into());
        }

        let name_lower = name.to_lowercase();

        // Resume mode: verifica che il nome sia nel roster salvato
        if !self.saved_roster.is_empty() {
            let (saved_id, saved_index, saved_color) = self
                .saved_roster
                .get(&name_lower)
                .cloned()
                .ok_or_else(|| "NAME_NOT_IN_SAVE".to_string())?;

            // Rifiuta se il giocatore ha già fatto join
            if self.player_index.contains_key(&saved_id) {
                return Err("NAME_TAKEN".into());
            }

            let actual_id = saved_id;
            // F3: player_index mappa player_id → posizione in self.players,
            // NON l'indice del motore (che sta in SessionPlayer.index).
            let roster_idx = self.players.len();
            self.player_index.insert(actual_id.clone(), roster_idx);
            self.outbound_seq.insert(actual_id.clone(), 0);
            self.last_client_seq.insert(actual_id.clone(), None);
            self.connections.insert(actual_id.clone(), Vec::new());
            self.players.push(SessionPlayer {
                name,
                color: saved_color,
                token: token.clone(),
                index: Some(saved_index),
            });
            self.broadcast_public_state();
            return Ok(JoinResponse {
                player_id: actual_id,
                token,
            });
        }

        // Modalità normale: i limiti valgono solo qui, i nomi di un save
        // esistente restano accettati così come sono.
        let name = name.trim().to_string();
        if name.is_empty() || name.chars().count() > MAX_PLAYER_NAME_CHARS {
            return Err("INVALID_NAME".into());
        }
        let name_lower = name.to_lowercase();

        // Primo join = nuovo playerId
        if self.players.len() >= self.max_players as usize {
            return Err("GAME_FULL".into());
        }
        if self
            .players
            .iter()
            .any(|p| p.name.to_lowercase() == name_lower)
        {
            return Err("NAME_TAKEN".into());
        }
        if let Some(requested) = color
            && self.players.iter().any(|p| p.color == requested)
        {
            return Err("COLOR_TAKEN".into());
        }

        let idx = self.players.len();
        self.player_index.insert(player_id.clone(), idx);
        self.outbound_seq.insert(player_id.clone(), 0);
        // F5: None = mai visto un client_seq da questo giocatore.
        self.last_client_seq.insert(player_id.clone(), None);
        self.connections.insert(player_id.clone(), Vec::new());

        // Colore: esplicito se richiesto, altrimenti primo libero in ordine
        // RED,BLUE,GREEN,YELLOW,BLACK (pacchetto §5).
        let resolved_color = color.unwrap_or_else(|| first_free_color(&self.players));

        self.players.push(SessionPlayer {
            name,
            color: resolved_color,
            token: token.clone(),
            index: None,
        });

        self.broadcast_public_state();

        Ok(JoinResponse { player_id, token })
    }

    fn handle_start(
        &mut self,
        host_token: String,
        ruleset: ttr_core::Ruleset,
        map: ttr_core::MapData,
    ) -> Result<(), String> {
        if self.started {
            return Err("GAME_STARTED".into());
        }
        if host_token != self.host_token {
            return Err("UNAUTHORIZED".into());
        }

        // Il controllo precede `take()`: un rifiuto non deve consumare il motore.
        // Confronto col motore: `saved_roster` ha chiavi lowercase, quindi può
        // contenere meno voci dei giocatori di un save manomesso.
        if let Some(engine) = &self.resumed_engine
            && self.player_index.len() != engine.num_players()
        {
            return Err("ROSTER_INCOMPLETE".into());
        }

        // Resume mode: usa il motore pre-replay (D18 §8)
        if let Some(engine) = self.resumed_engine.take() {
            let map_data = self.resumed_map_data.take().unwrap_or(map);
            // In resume mode gli indici sono già assegnati da handle_join
            self.engine = Some(engine);
            self.map_data = Some(map_data);
            self.started = true;
            self.broadcast_public_state();
            self.push_private_states();
            return Ok(());
        }

        let min_players = ruleset.players.min as usize;
        if self.players.len() < min_players {
            return Err(format!(
                "MIN_PLAYERS: servono almeno {} giocatori",
                min_players
            ));
        }

        for (i, p) in self.players.iter_mut().enumerate() {
            p.index = Some(i);
        }

        // Seed deterministico se fornito alla creazione (test + replay),
        // altrimenti time-based (comportamento storico).
        let seed = self.seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        });

        // Ordina per indice di join
        let mut ordered: Vec<(usize, String)> = self
            .player_index
            .iter()
            .map(|(id, &idx)| (idx, id.clone()))
            .collect();
        ordered.sort_by_key(|(idx, _)| *idx);
        let ordered_ids: Vec<String> = ordered.iter().map(|(_, id)| id.clone()).collect();
        let ordered_names: Vec<String> = ordered_ids
            .iter()
            .map(|id| {
                let idx = self.player_index[id];
                self.players[idx].name.clone()
            })
            .collect();
        let ordered_colors: Vec<PlayerColor> = ordered_ids
            .iter()
            .map(|id| {
                let idx = self.player_index[id];
                self.players[idx].color
            })
            .collect();

        let engine = ttr_core::Engine::new(
            seed,
            ruleset,
            map.clone(),
            ordered_ids,
            ordered_names,
            ordered_colors,
            0, // first_player
        );

        self.engine = Some(engine);
        self.map_data = Some(map);
        self.started = true;

        // Scrivi save iniziale
        self.write_save();

        // Broadcast + private states
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
        // Idempotenza inbound (E6, F5).
        // last_client_seq: None = mai visto; accetta qualsiasi seq.
        // Some(last): accetta solo seq > last, altrimenti dedup.
        if let Some(seq) = client_seq {
            let last = self
                .last_client_seq
                .get(&player_id)
                .and_then(|v| *v);
            if let Some(last_seq) = last
                && seq <= last_seq
            {
                // F3: risolvi l'indice del motore via SessionPlayer.index.
                let engine_idx = self
                    .player_index
                    .get(&player_id)
                    .and_then(|&session_pos| {
                        self.players.get(session_pos).and_then(|p| p.index)
                    });
                self.push_private_state_single(&player_id, engine_idx);
                return;
            }
            self.last_client_seq.insert(player_id.clone(), Some(seq));
        }

        // RequestSnapshot non arriva qui — gestito dal chiamante in ws.rs

        // Risolvi l'indice del motore per questo giocatore (F3: via SessionPlayer.index).
        let session_pos = match self.player_index.get(&player_id).copied() {
            Some(idx) => idx,
            None => {
                let _ = self.send_error(
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
                let _ = self.send_error(
                    &player_id,
                    ttr_proto::ErrorReason::InvalidAction,
                    Some("giocatore non ancora inizializzato".into()),
                );
                return;
            }
        };

        // Converti InboundAction → ttr_core::Action
        let action = crate::lobby::inbound_to_core_action(&inbound, engine_idx);

        let engine = match &mut self.engine {
            Some(e) => e,
            None => {
                let _ = self.send_error(
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
                        longest_path_bonus: _,
                    } = event
                    {
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
                                    // F3: ei è un indice del motore; cerca il SessionPlayer con .index == ei.
                                    self.players
                                        .iter()
                                        .position(|p| p.index == Some(ei))
                                        .and_then(|session_pos| {
                                            self.player_index.iter().find(|&(_, &sp)| sp == session_pos).map(|(id, _)| id.clone())
                                        })
                                        .unwrap_or_default()
                                })
                                .collect(),
                        );
                    }
                }

                self.broadcast_public_state();
                self.push_private_states();
                self.write_save();
            }
            Err(err) => {
                let reason = bridge::engine_error_to_reason(&err);
                let action_ref = bridge::error_action_ref(&err);
                let _ = self.send_error(&player_id, reason, action_ref);
            }
        }
    }

    fn handle_connect(
        &mut self,
        player_id: Option<String>,
        token: Option<String>,
        tx: mpsc::UnboundedSender<ServerMessage>,
        reply: tokio::sync::oneshot::Sender<ConnectOutcome>,
    ) {
        let authenticated = match (&player_id, &token) {
            (Some(pid), Some(tok)) => self
                .player_index
                .get(pid)
                .and_then(|&idx| self.players.get(idx))
                .map(|p| p.token == *tok)
                .unwrap_or(false),
            (None, _) => true, // spettatore: sempre ok
            _ => false,
        };

        if !authenticated {
            // F2: rispondi via oneshot e lascia che l'handler WS chiuda il socket.
            let _ = reply.send(ConnectOutcome::Unauthorized);
            return;
        }

        let conn_id = self.next_conn_id;
        self.next_conn_id += 1;

        // Presence: registra la soglia 0→1 (prima connessione attiva) per decidere
        // se broadcastare lo stato pubblico aggiornato. Gli spettatori non contano.
        let mut became_connected = false;
        if let Some(ref pid) = player_id {
            // F1 (review 3e): una connessione è "attiva" solo se il canale è
            // ancora aperto — un task ws abortito senza Disconnect (iOS che
            // uccide la tab in background) non deve lasciare il player
            // online per sempre.
            let was_connected = self
                .connections
                .get(pid)
                .is_some_and(|v| v.iter().any(|(_, tx)| !tx.is_closed()));
            self.connections
                .entry(pid.clone())
                .or_default()
                .push((conn_id, tx.clone()));
            became_connected = !was_connected;
        } else {
            self.spectators.push((conn_id, tx.clone()));
        }

        // F2: notifica esito positivo con conn_id per il cleanup.
        let _ = reply.send(ConnectOutcome::Ok { conn_id });

        let snap = self.build_snapshot(player_id.as_deref());
        if let Some(msg) = snap {
            let _ = tx.send(msg);
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

        // F6: rimuove la singola connessione per conn_id, non tutte del giocatore.
        if let Some(ref pid) = player_id {
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
            // F1 (review 3e): conta solo i canali ancora aperti.
            .filter(|(_, senders)| senders.iter().any(|(_, tx)| !tx.is_closed()))
            .map(|(pid, _)| pid.clone())
            .collect()
    }

    fn broadcast_public_state(&self) {
        let connected = self.connected_player_ids();
        let public = bridge::build_public_state(self, &connected);
        let msg = ServerMessage::PublicState(public);

        for (_cid, tx) in &self.spectators {
            let _ = tx.send(msg.clone());
        }
        for senders in self.connections.values() {
            for (_cid, tx) in senders {
                let _ = tx.send(msg.clone());
            }
        }
    }

    fn push_private_states(&mut self) {
        // F3: passa l'indice del motore (SessionPlayer.index), non la posizione in self.players.
        let players: Vec<(String, usize)> = self
            .player_index
            .iter()
            .filter_map(|(pid, &idx)| {
                self.players.get(idx).and_then(|p| p.index).map(|ei| (pid.clone(), ei))
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
            let msg = ServerMessage::PrivateState(ps);
            if let Some(senders) = self.connections.get(player_id) {
                for (_cid, tx) in senders {
                    let _ = tx.send(msg.clone());
                }
            }
        }
    }

    fn send_error(
        &mut self,
        player_id: &str,
        reason: ttr_proto::ErrorReason,
        action_ref: Option<String>,
    ) -> Result<(), ()> {
        let seq = self.outbound_seq.get(player_id).copied().unwrap_or(0) + 1;
        self.outbound_seq.insert(player_id.to_string(), seq);

        let msg = ServerMessage::Error(ttr_proto::ErrorPayload {
            seq,
            reason,
            action_ref,
        });

        if let Some(senders) = self.connections.get(player_id) {
            for (_cid, tx) in senders {
                let _ = tx.send(msg.clone());
            }
        }
        Ok(())
    }

    // ── Snapshot ─────────────────────────────────────────────────

    fn build_snapshot(&mut self, player_id: Option<&str>) -> Option<ServerMessage> {
        let connected = self.connected_player_ids();
        let public = bridge::build_public_state(self, &connected);
        let private = player_id.and_then(|pid| {
            // F3: risolvi l'indice del motore via SessionPlayer.index.
            let engine_idx = self
                .player_index
                .get(pid)
                .and_then(|&idx| self.players.get(idx).and_then(|p| p.index));
            // E6: il `seq` del private dentro SNAPSHOT avanza anch'esso.
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

    // ── GameInfo (F4: lobby via attore) ────────────────────────────

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
                self.players.get(idx).map(|p| (id.clone(), p.name.clone(), p.color))
            })
            .collect();

        GameInfo {
            players,
            players_joined: self.player_index.len(),
            started: self.started,
        }
    }

    // ── Save ─────────────────────────────────────────────────────

    fn write_save(&self) {
        if let Some(ref path) = self.save_path {
            let data = crate::save::build_save_data(self);
            if let Err(e) = crate::save::write_atomic(path, &data) {
                tracing::error!(
                    game_id = %self.game_id,
                    path = %path.display(),
                    "write_save fallita: {}",
                    e
                );
            }
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
