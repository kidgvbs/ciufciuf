//! WebSocket handler: upgrade, auth, ciclo di vita connessione.
//!
//! Il WS riceve `InboundAction` e li inoltra all'attore della sessione
//! come `GameCommand::Action`. L'outbound arriva via un canale limitato:
//! se l'attore lo trova pieno chiude la connessione e il client si riconnette.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::session::{ConnectOutcome, GameCommand, GameHandle};
use crate::state::AppState;

/// Dimensione massima di un messaggio o frame WebSocket in ingresso.
const MAX_WS_MESSAGE_BYTES: usize = 16 * 1024;

/// Messaggi in coda per connessione prima che venga chiusa.
const OUTBOUND_BUFFER: usize = 64;

/// Parametri query string per il WS.
#[derive(Debug, Deserialize)]
pub struct WsParams {
    #[serde(rename = "playerId")]
    pub player_id: Option<String>,
    pub token: Option<String>,
}

/// Handler axum per l'upgrade WebSocket.
pub async fn ws_handler(
    Path(game_id): Path<String>,
    Query(params): Query<WsParams>,
    ws: WebSocketUpgrade,
    state: axum::extract::State<AppState>,
) -> impl IntoResponse {
    ws.max_message_size(MAX_WS_MESSAGE_BYTES)
        .max_frame_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_ws(socket, game_id, params, state.0))
}

fn error_message(reason: ttr_proto::ErrorReason, detail: Option<&str>) -> ttr_proto::ServerMessage {
    ttr_proto::ServerMessage::Error(ttr_proto::ErrorPayload {
        seq: 0,
        reason,
        action_ref: None,
        detail: detail.map(str::to_string),
    })
}

/// Invia un errore e chiude il socket.
async fn reject(
    mut sender: SplitSink<WebSocket, Message>,
    reason: ttr_proto::ErrorReason,
    detail: Option<&str>,
) {
    let json = serde_json::to_string(&error_message(reason, detail)).unwrap_or_default();
    let _ = sender.send(Message::Text(json.into())).await;
    let _ = sender.close().await;
}

async fn handle_ws(socket: WebSocket, game_id: String, params: WsParams, state: AppState) {
    let (sender, mut receiver) = socket.split();

    let handle = {
        let games = state.games.read().await;
        games.get(&game_id).cloned()
    };
    let Some(handle) = handle else {
        reject(sender, ttr_proto::ErrorReason::GameNotFound, None).await;
        return;
    };

    // Il canale è limitato; questo handler ne tiene solo un riferimento debole,
    // così la rimozione della connessione da parte dell'attore chiude il canale.
    let (tx, mut rx) = mpsc::channel::<ttr_proto::ServerMessage>(OUTBOUND_BUFFER);
    let local_tx = tx.downgrade();

    // Autenticazione via attore: invia Connect e attende esito.
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let _ = handle
        .tx
        .send(GameCommand::Connect {
            player_id: params.player_id.clone(),
            token: params.token.clone(),
            tx,
            reply: reply_tx,
        })
        .await;

    let conn_id: u64 = match reply_rx.await.unwrap_or(ConnectOutcome::GameNotFound) {
        ConnectOutcome::Ok { conn_id } => conn_id,
        ConnectOutcome::Unauthorized => {
            reject(sender, ttr_proto::ErrorReason::Unauthorized, None).await;
            return;
        }
        ConnectOutcome::GameNotFound => {
            reject(sender, ttr_proto::ErrorReason::GameNotFound, None).await;
            return;
        }
        ConnectOutcome::TooManySpectators => {
            reject(
                sender,
                ttr_proto::ErrorReason::GameFull,
                Some("TOO_MANY_SPECTATORS"),
            )
            .await;
            return;
        }
    };

    // L'identità è quella della connessione, risolta una volta sola.
    let connection_identity: Option<String> = params.player_id.clone();

    // Task per inoltrare messaggi dall'attore al WS; chiude il socket quando
    // il canale si chiude.
    let mut outbound_task = tokio::spawn(async move {
        let mut sender = sender;
        while let Some(msg) = rx.recv().await {
            let json = serde_json::to_string(&msg).unwrap_or_default();
            if sender.send(Message::Text(json.into())).await.is_err() {
                return;
            }
        }
        let _ = sender.close().await;
    });

    // Loop inbound: l'identità della connessione decide se l'azione è autorizzata.
    loop {
        let msg = tokio::select! {
            msg = receiver.next() => msg,
            _ = &mut outbound_task => break,
        };
        match msg {
            Some(Ok(Message::Text(text))) => {
                handle_text(&text, &connection_identity, &handle, &local_tx).await;
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
            Some(Ok(_)) => {}
        }
    }

    // Cleanup disconnessione: passa il conn_id per rimuovere solo questa.
    let _ = handle
        .tx
        .send(GameCommand::Disconnect {
            player_id: connection_identity,
            conn_id,
        })
        .await;

    outbound_task.abort();
}

/// Gestisce un messaggio di testo del client.
async fn handle_text(
    text: &str,
    identity: &Option<String>,
    handle: &GameHandle,
    local_tx: &mpsc::WeakSender<ttr_proto::ServerMessage>,
) {
    let inbound: ttr_proto::InboundAction = match serde_json::from_str(text) {
        Ok(inbound) => inbound,
        Err(_) => {
            // Uno spettatore non può produrre un `InboundAction` valido per
            // REQUEST_SNAPSHOT, perché nel payload proto `player_id` è
            // obbligatorio: una sonda su `actionType` lo riconosce. Gli altri
            // messaggi malformati sono ignorati.
            if identity.is_none() && is_snapshot_request_probe(text) {
                forward_snapshot_request(handle, local_tx, None).await;
            }
            return;
        }
    };
    let is_snapshot_request = matches!(inbound, ttr_proto::InboundAction::RequestSnapshot(_));

    match identity {
        // Spettatore: solo REQUEST_SNAPSHOT è consentita.
        None if is_snapshot_request => forward_snapshot_request(handle, local_tx, None).await,
        None => {
            if let Some(tx) = local_tx.upgrade() {
                let _ = tx.try_send(error_message(ttr_proto::ErrorReason::Unauthorized, None));
            }
        }
        Some(pid) if is_snapshot_request => {
            forward_snapshot_request(handle, local_tx, Some(pid.clone())).await;
        }
        Some(pid) => {
            let client_seq = extract_client_seq(&inbound);
            let _ = handle
                .tx
                .send(GameCommand::Action {
                    player_id: pid.clone(),
                    inbound,
                    client_seq,
                })
                .await;
        }
    }
}

/// Inoltra una REQUEST_SNAPSHOT all'attore e risponde sul canale della connessione.
/// `player_id: None` produce uno snapshot solo-pubblico (spettatori).
async fn forward_snapshot_request(
    handle: &GameHandle,
    tx: &mpsc::WeakSender<ttr_proto::ServerMessage>,
    player_id: Option<String>,
) {
    let (snap_tx, snap_rx) = tokio::sync::oneshot::channel();
    let _ = handle
        .tx
        .send(GameCommand::RequestSnapshot {
            player_id,
            reply: snap_tx,
        })
        .await;
    if let Ok(Some(snap)) = snap_rx.await
        && let Some(tx) = tx.upgrade()
    {
        let _ = tx.try_send(snap);
    }
}

/// Sonda leggera: il messaggio è un REQUEST_SNAPSHOT?
/// Usata solo per gli spettatori, il cui REQUEST_SNAPSHOT senza `player_id`
/// non è deserializzabile come `InboundAction`.
fn is_snapshot_request_probe(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| {
            v.get("actionType")?
                .as_str()
                .map(|s| s == "REQUEST_SNAPSHOT")
        })
        .unwrap_or(false)
}

fn extract_client_seq(inbound: &ttr_proto::InboundAction) -> Option<u64> {
    match inbound {
        ttr_proto::InboundAction::DrawTrainCard(p) => p.client_seq,
        ttr_proto::InboundAction::DrawTickets(p) => p.client_seq,
        ttr_proto::InboundAction::ChooseTickets(p) => p.client_seq,
        ttr_proto::InboundAction::ClaimRoute(p) => p.client_seq,
        ttr_proto::InboundAction::RequestSnapshot(p) => p.client_seq,
    }
}
