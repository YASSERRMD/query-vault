//! QueryVault - High-performance query analytics platform

mod buffer;
mod db;
mod error;
mod models;
mod routes;
mod services;
mod state;
mod tasks;

use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::db::Database;
use crate::routes::{aggregations, health, ingest, metrics, search, ws};
use crate::services::embedding::EmbeddingService;
use crate::state::AppState;
use crate::tasks::{aggregation, anomaly_detection, embedding_task, retention};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "query_vault=info,tower_http=info".into()),
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

    // Load embedding service (optional)
    let embedding_service = match (
        std::env::var("EMBEDDING_MODEL_PATH"),
        std::env::var("EMBEDDING_TOKENIZER_PATH"),
    ) {
        (Ok(model_path), Ok(tokenizer_path)) => {
            info!("Loading embedding model from {}", model_path);
            match EmbeddingService::new(Path::new(&model_path), Path::new(&tokenizer_path)) {
                Ok(service) => {
                    info!("Embedding service loaded successfully");
                    Some(service)
                }
                Err(e) => {
                    warn!(error = %e, "Failed to load embedding service, vector search disabled");
                    None
                }
            }
        }
        _ => {
            info!("EMBEDDING_MODEL_PATH not set, vector search disabled");
            None
        }
    };

    // Create application state
    let state = AppState::new(db, buffer_capacity, broadcast_capacity, embedding_service);

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

    // 4. Embedding task - embeds queries for vector search
    let emb_db = Arc::clone(&state.db);
    let emb_service = state.embedding_service.clone();
    tokio::spawn(async move {
        embedding_task::embedding_task(emb_db, emb_service).await;
    });

    // 5. Anomaly detection task - detects slow queries
    let anomaly_db = Arc::clone(&state.db);
    let anomaly_tx = state.broadcast_tx.clone();
    tokio::spawn(async move {
        anomaly_detection::anomaly_detection_task(anomaly_db, anomaly_tx).await;
    });

    // Build router
    let app = Router::new()
        // Health and metrics (Kubernetes probes + Prometheus)
        .route("/health", get(health::health))
        .route("/ready", get(health::ready))
        .route("/metrics", get(metrics::prometheus_metrics))
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
        // Vector search
        .route(
            "/api/v1/workspaces/{workspace_id}/search/similar",
            post(search::search_similar),
        )
        // Anomalies
        .route(
            "/api/v1/workspaces/{workspace_id}/anomalies",
            get(search::get_anomalies),
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

    info!("QueryVault v{} starting on {}", env!("CARGO_PKG_VERSION"), listen_addr);
    info!("Database: {}", database_url.split('@').last().unwrap_or("***"));
    info!("Buffer capacity: {}", buffer_capacity);
    info!("Broadcast capacity: {}", broadcast_capacity);

    // Start server
    let listener = tokio::net::TcpListener::bind(listen_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
