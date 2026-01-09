//! WebSocket streaming endpoint for real-time metrics

use axum::extract::ws::{Message, WebSocket};
use axum::{
    extract::{Path, State, WebSocketUpgrade},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

use crate::models::QueryMetric;
use crate::state::AppState;

/// GET /api/v1/workspaces/:workspace_id/ws
///
/// Upgrades connection to WebSocket for real-time metric streaming.
/// Filters metrics to only those belonging to the specified workspace.
pub async fn ws_handler(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, workspace_id))
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, state: AppState, workspace_id: Uuid) {
    info!(workspace_id = %workspace_id, "WebSocket client connected");

    let (mut sender, mut receiver) = socket.split();
    let mut broadcast_rx = state.broadcast_tx.subscribe();

    // Task to send metrics to client
    let send_task = tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok((metric_workspace_id, metric)) => {
                    // Only send metrics for this workspace
                    if metric_workspace_id == workspace_id {
                        let json = match serde_json::to_string(&metric) {
                            Ok(j) => j,
                            Err(e) => {
                                warn!(error = %e, "Failed to serialize metric");
                                continue;
                            }
                        };

                        if sender.send(Message::Text(json.into())).await.is_err() {
                            // Client disconnected
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    warn!(
                        lagged = count,
                        "Broadcast receiver lagged, some metrics dropped"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    // Task to receive pings/messages from client (keep-alive)
    let recv_task = tokio::spawn(async move {
        while let Some(result) = receiver.next().await {
            match result {
                Ok(Message::Close(_)) => break,
                Ok(Message::Ping(data)) => {
                    // Pong is handled automatically by axum
                    let _ = data;
                }
                Ok(_) => {} // Ignore other messages
                Err(_) => break,
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    info!(workspace_id = %workspace_id, "WebSocket client disconnected");
}

/// Background task that broadcasts metrics from buffer to WebSocket clients.
///
/// Runs every 100ms, pops batches from buffer and broadcasts to all subscribers.
pub async fn broadcast_task(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

    loop {
        interval.tick().await;

        let batch = state.metrics_buffer.pop_batch(1000);
        if batch.is_empty() {
            continue;
        }

        for metric in batch {
            let workspace_id = metric.workspace_id;
            // Ignore send errors (no receivers connected)
            let _ = state.broadcast_tx.send((workspace_id, metric));
        }
    }
}
