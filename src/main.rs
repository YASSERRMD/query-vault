//! QueryVault - High-performance query analytics platform

mod buffer;
mod db;
mod error;
mod models;
mod routes;
mod state;
mod tasks;

use axum::{
    routing::{get, post},
    Json, Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::db::Database;
use crate::models::HealthResponse;
use crate::routes::{aggregations, ingest, ws};
use crate::state::AppState;
use crate::tasks::{aggregation, retention};

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

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/queryvault".to_string());

    let buffer_capacity: usize = std::env::var("BUFFER_CAPACITY")
        .unwrap_or_else(|_| "100000".to_string())
        .parse()
        .expect("Invalid BUFFER_CAPACITY");

    let broadcast_capacity: usize = std::env::var("BROADCAST_CAPACITY")
        .unwrap_or_else(|_| "10000".to_string())
        .parse()
        .expect("Invalid BROADCAST_CAPACITY");

    // Connect to database
    let db = match Database::new(&database_url).await {
        Ok(db) => db,
        Err(e) => {
            error!(error = %e, "Failed to connect to database");
            std::process::exit(1);
        }
    };

    // Create application state
    let state = AppState::new(db, buffer_capacity, broadcast_capacity);

    // Spawn background tasks
    // 1. Broadcast task - sends buffer metrics to WebSocket clients
    let broadcast_state = state.clone();
    tokio::spawn(async move {
        ws::broadcast_task(broadcast_state).await;
    });

    // 2. Aggregation task - flushes buffer to database every 5s
    let agg_buffer = state.metrics_buffer.clone();
    let agg_db = Arc::clone(&state.db);
    tokio::spawn(async move {
        aggregation::aggregation_task(agg_buffer, agg_db).await;
    });

    // 3. Retention task - prunes old data every 6h
    let ret_db = Arc::clone(&state.db);
    tokio::spawn(async move {
        retention::retention_task(ret_db).await;
    });

    // Build router
    let app = Router::new()
        // Health check
        .route("/health", get(health))
        // Ingestion
        .route("/api/v1/metrics/ingest", post(ingest::ingest_metrics))
        // Aggregations & metrics
        .route(
            "/api/v1/workspaces/{workspace_id}/aggregations",
            get(aggregations::get_aggregations),
        )
        .route(
            "/api/v1/workspaces/{workspace_id}/metrics",
            get(aggregations::get_recent_metrics),
        )
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
    info!("Database: {}", database_url.split('@').last().unwrap_or("***"));
    info!("Buffer capacity: {}", buffer_capacity);
    info!("Broadcast capacity: {}", broadcast_capacity);

    // Start server
    let listener = tokio::net::TcpListener::bind(listen_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
