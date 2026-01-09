//! QueryVault - High-performance query analytics platform

mod buffer;
mod error;
mod models;
mod routes;
mod state;

use axum::{
    routing::{get, post},
    Json, Router,
};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::models::HealthResponse;
use crate::routes::{ingest, ws};
use crate::state::AppState;

/// Health check endpoint
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "query_vault=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Configuration
    let listen_addr: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()
        .expect("Invalid LISTEN_ADDR");

    let buffer_capacity: usize = std::env::var("BUFFER_CAPACITY")
        .unwrap_or_else(|_| "100000".to_string())
        .parse()
        .expect("Invalid BUFFER_CAPACITY");

    let broadcast_capacity: usize = std::env::var("BROADCAST_CAPACITY")
        .unwrap_or_else(|_| "10000".to_string())
        .parse()
        .expect("Invalid BROADCAST_CAPACITY");

    // Create application state
    let state = AppState::new(buffer_capacity, broadcast_capacity);

    // Spawn broadcast task
    let broadcast_state = state.clone();
    tokio::spawn(async move {
        ws::broadcast_task(broadcast_state).await;
    });

    // Build router
    let app = Router::new()
        // Health check
        .route("/health", get(health))
        // Ingestion
        .route("/api/v1/metrics/ingest", post(ingest::ingest_metrics))
        // WebSocket streaming
        .route("/api/v1/workspaces/{workspace_id}/ws", get(ws::ws_handler))
        // State and middleware
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    info!("QueryVault starting on {}", listen_addr);
    info!("Buffer capacity: {}", buffer_capacity);
    info!("Broadcast capacity: {}", broadcast_capacity);

    // Start server
    let listener = tokio::net::TcpListener::bind(listen_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
