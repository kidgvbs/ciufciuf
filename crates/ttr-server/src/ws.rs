//! WebSocket handler: upgrade, auth, ciclo di vita connessione.
//!
//! Il WS riceve `InboundAction` e li inoltra all'attore della sessione
//! come `GameCommand::Action`. L'outbound arriva via `mpsc::UnboundedSender`.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;

use crate::session::{ConnectOutcome, GameCommand};
use crate::state::AppState;

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
    ws.on_upgrade(move |socket| handle_ws(socket, game_id, params, state.0))
}

async fn handle_ws(socket: WebSocket, game_id: String, params: WsParams, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Cerca la partita
    let handle = {
        let games = state.games.read().await;
        games.get(&game_id).cloned()
    };

    let handle = match handle {
        Some(h) => h,
        None => {
            let _ = sender
                .send(Message::Text(
                    serde_json::to_string(&ttr_proto::ServerMessage::Error(
                        ttr_proto::ErrorPayload {
                            seq: 0,
                            reason: ttr_proto::ErrorReason::GameNotFound,
                            action_ref: None,
                        },
                    ))
                    .unwrap_or_default()
                    .into(),
                ))
                .await;
            return;
        }
    };

    // Crea un canale per i messaggi outbound
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ttr_proto::ServerMessage>();

    // Autenticazione via attore (F2): invia Connect e attende esito.
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let _ = handle
        .tx
        .send(GameCommand::Connect {
            player_id: params.player_id.clone(),
            token: params.token.clone(),
            tx: tx.clone(),
            reply: reply_tx,
        })
        .await;

    let conn_id: u64 = match reply_rx.await.unwrap_or(ConnectOutcome::GameNotFound) {
        ConnectOutcome::Ok { conn_id } => conn_id,
        ConnectOutcome::Unauthorized => {
            // F2: chiudi il socket su auth fallita (E9).
            let _ = sender
                .send(Message::Text(
                    serde_json::to_string(&ttr_proto::ServerMessage::Error(
                        ttr_proto::ErrorPayload {
                            seq: 0,
                            reason: ttr_proto::ErrorReason::Unauthorized,
                            action_ref: None,
                        },
                    ))
                    .unwrap_or_default()
                    .into(),
                ))
                .await;
            return;
        }
        ConnectOutcome::GameNotFound => {
            let _ = sender
                .send(Message::Text(
                    serde_json::to_string(&ttr_proto::ServerMessage::Error(
                        ttr_proto::ErrorPayload {
                            seq: 0,
                            reason: ttr_proto::ErrorReason::GameNotFound,
                            action_ref: None,
                        },
                    ))
                    .unwrap_or_default()
                    .into(),
                ))
                .await;
            return;
        }
    };

    // F1: l'identità è quella della connessione, risolta una volta sola.
    let connection_identity: Option<String> = params.player_id.clone();

    // Task per inoltrare messaggi dall'attore al WS
    let outbound_task = {
        let mut sender = sender;
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let json = serde_json::to_string(&msg).unwrap_or_default();
                if sender
                    .send(Message::Text(json.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        })
    };

    // Loop inbound: leggi messaggi dal client.
    // F1: l'identity della connessione decide se l'azione è autorizzata.
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                let action: Result<ttr_proto::InboundAction, _> = serde_json::from_str(&text);
                match action {
                    Ok(inbound) => {
                        let client_seq = extract_client_seq(&inbound);

                        match &connection_identity {
                            None => {
                                // Spettatore: nessuna azione inbound consentita (F1).
                                let _ = tx.send(ttr_proto::ServerMessage::Error(
                                    ttr_proto::ErrorPayload {
                                        seq: 0,
                                        reason: ttr_proto::ErrorReason::Unauthorized,
                                        action_ref: None,
                                    },
                                ));
                            }
                            Some(pid) => {
                                if matches!(
                                    inbound,
                                    ttr_proto::InboundAction::RequestSnapshot(_)
                                ) {
                                    let (snap_tx, snap_rx) =
                                        tokio::sync::oneshot::channel();
                                    let _ = handle
                                        .tx
                                        .send(GameCommand::RequestSnapshot {
                                            player_id: Some(pid.clone()),
                                            reply: snap_tx,
                                        })
                                        .await;
                                    if let Ok(Some(snap)) = snap_rx.await {
                                        let _ = tx.send(snap);
                                    }
                                } else {
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
                    }
                    Err(_) => {
                        // Messaggio malformato: ignora silenziosamente
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Cleanup disconnessione — F6: passa il conn_id per rimuovere solo questa.
    let _ = handle
        .tx
        .send(GameCommand::Disconnect {
            player_id: connection_identity,
            conn_id,
        })
        .await;

    outbound_task.abort();
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
