//! Helpers condivisi per i test di integrazione WS del server.
//!
//! Spawna un server axum reale su porta effimera, offre un mini client REST
//! (reqwest) e un client WebSocket "scemo" (tokio-tungstenite) che decide
//! solo dai messaggi WS (`PRIVATE_STATE`/`PUBLIC_STATE`).

use std::path::PathBuf;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use ttr_core::MapData;
use ttr_proto::{CardColor, InboundAction, PublicState, ServerMessage};

// ── Dati ────────────────────────────────────────────────────────────

/// Percorso base `data/` relativo al crate ttr-server.
pub fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

/// Carica la mappa USA da `data/maps/usa.json`.
pub fn usa_map() -> MapData {
    let path = data_dir().join("maps/usa.json");
    let json = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("impossibile leggere {:?}: {}", path, e);
    });
    serde_json::from_str(&json).expect("parsing usa.json")
}

/// Directory temporanea unica per i save (evita collisioni tra test).
pub fn temp_saves_dir() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ciufciuf-test-saves-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&p).expect("creazione dir saves di test");
    p
}

// ── Server ──────────────────────────────────────────────────────────

/// Server di test in esecuzione: base URL e join handle del task axum.
pub struct TestServer {
    pub base_url: String,
    pub saves_dir: PathBuf,
    #[allow(dead_code)]
    pub state: ttr_server::AppState,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    /// Avvia un server axum su `127.0.0.1:0` (porta effimera).
    pub async fn start() -> Self {
        let data = data_dir();
        let saves = temp_saves_dir();
        let state =
            ttr_server::AppState::new(data.clone(), saves.clone(), 0).expect("AppState::new");
        Self::start_with_state(state, saves).await
    }

    /// Avvia un server axum da un `AppState` già costruito (es. test resume).
    pub async fn start_with_state(state: ttr_server::AppState, saves: PathBuf) -> Self {
        Self::start_with_client_dist(state, saves, PathBuf::new()).await
    }

    /// Avvia un server axum servendo i statici del client da `client_dist`
    /// (Fase 3b: fallback SPA a index.html).
    #[allow(dead_code)]
    pub async fn start_with_client_dist(
        state: ttr_server::AppState,
        saves: PathBuf,
        client_dist: PathBuf,
    ) -> Self {
        let state_clone = state.clone();
        let router = ttr_server::build_router(state, client_dist);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind porta effimera");
        let addr = listener.local_addr().expect("local_addr");
        let base_url = format!("http://{}", addr);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("axum serve");
        });

        TestServer {
            base_url,
            saves_dir: saves,
            state: state_clone,
            shutdown_tx: Some(shutdown_tx),
            task: Some(task),
        }
    }

    /// URL WebSocket per una partita (path senza parametri di query).
    pub fn ws_url(&self, game_id: &str) -> String {
        format!("ws://{}/ws/{}", host_of(&self.base_url), game_id)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.task.take() {
            t.abort();
        }
        // Pulisce la cartella save di test (best effort)
        let _ = std::fs::remove_dir_all(&self.saves_dir);
    }
}

fn host_of(base_url: &str) -> String {
    base_url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string()
}

// ── Client REST ─────────────────────────────────────────────────────

/// Risposta di creazione partita (POST /api/games).
#[derive(serde::Deserialize, Clone)]
pub struct CreatedGame {
    #[serde(rename = "gameId")]
    pub game_id: String,
    #[serde(rename = "hostToken")]
    pub host_token: String,
    #[serde(rename = "rulesetId")]
    #[allow(dead_code)]
    pub ruleset_id: String,
    #[serde(rename = "maxPlayers")]
    #[allow(dead_code)]
    pub max_players: u32,
}

/// Risposta di join (POST /api/games/{id}/join).
#[derive(serde::Deserialize, Clone)]
pub struct JoinedPlayer {
    #[serde(rename = "playerId")]
    pub player_id: String,
    pub token: String,
    #[serde(rename = "wsUrl")]
    #[allow(dead_code)]
    pub ws_url: String,
}

/// Crea una partita via REST.
#[allow(dead_code)]
pub async fn create_game(base: &str, name: &str) -> CreatedGame {
    create_game_with_seed(base, name, None).await
}

/// Crea una partita via REST con un seed RNG opzionale (determinismo test).
pub async fn create_game_with_seed(base: &str, name: &str, seed: Option<u64>) -> CreatedGame {
    let client = reqwest::Client::new();
    let mut body = serde_json::json!({
        "rulesetId": "usa_v2",
        "name": name,
    });
    if let Some(s) = seed {
        body["seed"] = serde_json::json!(s);
    }
    let resp = client
        .post(format!("{}/api/games", base))
        .json(&body)
        .send()
        .await
        .expect("POST /api/games");
    assert_eq!(resp.status(), 201, "creazione partita deve tornare 201");
    resp.json().await.expect("parse CreatedGame")
}

/// Join di un giocatore via REST.
pub async fn join_game(base: &str, game_id: &str, name: &str, color: Option<&str>) -> JoinedPlayer {
    let client = reqwest::Client::new();
    let mut body = serde_json::json!({ "name": name });
    if let Some(c) = color {
        body["color"] = serde_json::json!(c);
    }
    let resp = client
        .post(format!("{}/api/games/{}/join", base, game_id))
        .json(&body)
        .send()
        .await
        .expect("POST /api/games/{id}/join");
    assert_eq!(resp.status(), 200, "join deve tornare 200: {}", name);
    resp.json().await.expect("parse JoinedPlayer")
}

/// Avvia una partita via REST (richiede hostToken).
pub async fn start_game(base: &str, game_id: &str, host_token: &str) {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/games/{}/start", base, game_id))
        .json(&serde_json::json!({ "hostToken": host_token }))
        .send()
        .await
        .expect("POST /api/games/{id}/start");
    assert_eq!(resp.status(), 204, "start deve tornare 204");
}

// ── Client WebSocket "scemo" ────────────────────────────────────────

/// Stato privato più recente di un giocatore ricavato dai messaggi WS.
#[derive(Clone)]
pub struct PlayerView {
    pub seq: u64,
    pub hand: Vec<CardColor>,
    pub tickets: Vec<ttr_proto::TicketInfo>,
    pub offered_tickets: Vec<ttr_proto::TicketInfo>,
    pub legal_moves: ttr_proto::LegalMoves,
    pub public: Option<PublicState>,
    /// Ultimo errore ricevuto (per fail-fast nei test).
    pub last_error: Option<ttr_proto::ErrorPayload>,
}

impl Default for PlayerView {
    fn default() -> Self {
        Self {
            seq: 0,
            hand: Vec::new(),
            tickets: Vec::new(),
            offered_tickets: Vec::new(),
            legal_moves: ttr_proto::LegalMoves {
                routes: Vec::new(),
                draw_face_up: Vec::new(),
                can_draw_deck: false,
                can_draw_tickets: false,
            },
            public: None,
            last_error: None,
        }
    }
}

impl PlayerView {
    /// Aggiorna la vista con un `PrivateState` (dal unicast o dallo snapshot).
    pub fn apply_private(&mut self, ps: &ttr_proto::PrivateState) {
        self.seq = ps.seq;
        self.hand = ps.hand.clone();
        self.tickets = ps.tickets.clone();
        self.offered_tickets = ps.offered_tickets.clone();
        self.legal_moves = ps.legal_moves.clone();
    }
}

/// Un client WebSocket connesso a una partita.
pub struct WsClient {
    pub player_id: String,
    pub token: String,
    pub view: PlayerView,
    pub last_snapshot: Option<ttr_proto::Snapshot>,
    write: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    pub read: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
}

impl WsClient {
    /// Connette un giocatore al WebSocket della partita.
    pub async fn connect(server: &TestServer, game_id: &str, player_id: &str, token: &str) -> Self {
        let url = format!(
            "{}?playerId={}&token={}",
            server.ws_url(game_id),
            player_id,
            token
        );
        let (ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("connect WebSocket");
        let (write, read) = ws.split();
        WsClient {
            player_id: player_id.to_string(),
            token: token.to_string(),
            view: PlayerView::default(),
            last_snapshot: None,
            write,
            read,
        }
    }

    /// Connette uno spettatore (senza playerId né token).
    #[allow(dead_code)]
    pub async fn connect_spectator(server: &TestServer, game_id: &str) -> Self {
        let url = server.ws_url(game_id);
        let (ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("connect spectator WS");
        let (write, read) = ws.split();
        WsClient {
            player_id: String::new(),
            token: String::new(),
            view: PlayerView::default(),
            last_snapshot: None,
            write,
            read,
        }
    }

    /// Invia un'azione inbound (serializzata) al server.
    pub async fn send(&mut self, action: &InboundAction) {
        let json = serde_json::to_string(action).expect("serializza InboundAction");
        self.write
            .send(Message::Text(json))
            .await
            .expect("send action");
    }

    /// Invia testo grezzo (es. REQUEST_SNAPSHOT dello spettatore senza
    /// `player_id`, che non è rappresentabile come `InboundAction` — N1).
    #[allow(dead_code)]
    pub async fn send_raw(&mut self, text: &str) {
        self.write
            .send(Message::Text(text.to_string()))
            .await
            .expect("send raw");
    }

    /// Chiude la connessione (drop dello split sink → chiusura socket).
    pub async fn disconnect(mut self) {
        let _ = self.write.close().await;
    }

    /// Legge il prossimo messaggio e aggiorna la vista.
    pub async fn recv(&mut self) -> ServerMessage {
        loop {
            let msg = tokio::time::timeout(std::time::Duration::from_secs(15), self.read.next())
                .await
                .expect("timeout 15s in attesa di un messaggio WS")
                .expect("stream chiuso")
                .expect("read");
            if let Message::Text(text) = msg {
                let parsed: ServerMessage = serde_json::from_str(&text).expect("parse msg");
                match &parsed {
                    ServerMessage::PrivateState(ps) => {
                        self.view.seq = ps.seq;
                        self.view.hand = ps.hand.clone();
                        self.view.tickets = ps.tickets.clone();
                        self.view.offered_tickets = ps.offered_tickets.clone();
                        self.view.legal_moves = ps.legal_moves.clone();
                    }
                    ServerMessage::Snapshot(snap) => {
                        self.last_snapshot = Some(snap.clone());
                        if let Some(ref priv_) = snap.private {
                            self.view.seq = priv_.seq;
                            self.view.hand = priv_.hand.clone();
                            self.view.tickets = priv_.tickets.clone();
                            self.view.offered_tickets = priv_.offered_tickets.clone();
                            self.view.legal_moves = priv_.legal_moves.clone();
                        }
                        self.view.public = Some(snap.public.clone());
                    }
                    ServerMessage::PublicState(pub_) => {
                        self.view.public = Some(pub_.clone());
                    }
                    ServerMessage::Error(ep) => {
                        self.view.last_error = Some(ep.clone());
                    }
                }
                return parsed;
            }
        }
    }
}

// ── Costruzione pagamento claim (client "scemo", pacchetto §9.1) ────

/// Ordine enum dei colori carta (senza Locomotive), per il pagamento
/// deterministico delle rotte grigie (pacchetto: "primo colore con
/// conteggio sufficiente in ordine enum").
pub const CARD_COLOR_ORDER: [CardColor; 8] = [
    CardColor::Red,
    CardColor::Orange,
    CardColor::Yellow,
    CardColor::Green,
    CardColor::Blue,
    CardColor::Purple,
    CardColor::Black,
    CardColor::White,
];

/// Converte il nome colore della mappa (`"Red"`, `"Gray"`, …) in `CardColor`,
/// oppure `None` per le grigie.
pub fn route_color_card(color: &str) -> Option<CardColor> {
    match color {
        "Red" => Some(CardColor::Red),
        "Orange" => Some(CardColor::Orange),
        "Yellow" => Some(CardColor::Yellow),
        "Green" => Some(CardColor::Green),
        "Blue" => Some(CardColor::Blue),
        "Purple" => Some(CardColor::Purple),
        "Black" => Some(CardColor::Black),
        "White" => Some(CardColor::White),
        "Gray" | "grey" | "gray" => None,
        _ => None,
    }
}

/// Costruisce un pagamento valido (`cards_discarded`) per una rotta a barra
/// dalla mano corrente, secondo la regola del pacchetto §9.1: colore della
/// rotta, o (grigia) il primo colore con conteggio sufficiente in ordine enum,
/// locomotive a riempimento. `len` è la lunghezza della rotta (`route.len`).
pub fn build_payment(hand: &[CardColor], route_color: &str, len: usize) -> Option<Vec<CardColor>> {
    let loco_count = hand.iter().filter(|c| **c == CardColor::Locomotive).count();

    let chosen = if let Some(target) = route_color_card(route_color) {
        // Rotta colorata: usa quel colore, loco a riempimento.
        Some(target)
    } else {
        // Grigia: primo colore (ordine enum) con conteggio sufficiente,
        // considerando anche i loco come jolly.
        CARD_COLOR_ORDER.iter().copied().find(|&c| {
            let colored = hand.iter().filter(|h| **h == c).count();
            colored + loco_count >= len
        })
    };

    let chosen = chosen?;

    let mut payment: Vec<CardColor> = Vec::with_capacity(len);
    for &c in hand {
        if c == chosen && payment.len() < len {
            payment.push(c);
        }
    }
    for &c in hand {
        if c == CardColor::Locomotive && payment.len() < len {
            payment.push(c);
        }
    }

    if payment.len() == len {
        Some(payment)
    } else {
        None
    }
}

/// Mappa id rotta → (len, color), per costruire il pagamento nel client scemo.
pub fn route_len_color(map: &MapData, route_id: &str) -> Option<(usize, String)> {
    map.routes
        .iter()
        .find(|r| r.id == route_id)
        .map(|r| (r.len as usize, r.color.clone()))
}

/// Arc wrapper per condividere la mappa tra task di test.
#[allow(dead_code)]
pub type SharedMap = Arc<MapData>;

/// Carica il ruleset USA da `data/rulesets/usa_v2.json`.
#[allow(dead_code)]
pub fn load_usa_ruleset() -> ttr_core::Ruleset {
    let path = data_dir().join("rulesets").join("usa_v2.json");
    let json = std::fs::read_to_string(&path).expect("lettura usa_v2.json");
    serde_json::from_str(&json).expect("parsing usa_v2.json")
}

// ── Sotto-moduli ────────────────────────────────────────────────────

pub mod driver;
