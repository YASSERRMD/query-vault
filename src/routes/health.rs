//! Health and readiness endpoints

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::state::AppState;

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

/// Readiness check response
#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: &'static str,
    pub checks: ReadinessChecks,
}

#[derive(Debug, Serialize)]
pub struct ReadinessChecks {
    pub database: CheckStatus,
    pub buffer: CheckStatus,
    pub embedding_service: CheckStatus,
}

#[derive(Debug, Serialize)]
pub struct CheckStatus {
    pub healthy: bool,
    pub message: String,
}

/// GET /health
///
/// Basic health check - returns 200 if the server is running
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// GET /ready
///
/// Readiness check - verifies all dependencies are available
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadinessResponse>) {
    // Check database connection
    let db_check = match sqlx::query("SELECT 1").fetch_one(state.db.pool()).await {
        Ok(_) => CheckStatus {
            healthy: true,
            message: "Connected".to_string(),
        },
        Err(e) => CheckStatus {
            healthy: false,
            message: format!("Connection failed: {}", e),
        },
    };

    // Check buffer
    let buffer_check = CheckStatus {
        healthy: true,
        message: format!("Buffer length: {}", state.metrics_buffer.len()),
    };

    // Check embedding service
    let embedding_check = match &state.embedding_service {
        Some(_) => CheckStatus {
            healthy: true,
            message: "Loaded".to_string(),
        },
        None => CheckStatus {
            healthy: true, // Not having embeddings is OK
            message: "Not configured".to_string(),
        },
    };

    let all_healthy = db_check.healthy && buffer_check.healthy;
    let status = if all_healthy { "ready" } else { "not_ready" };
    let status_code = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadinessResponse {
            status,
            checks: ReadinessChecks {
                database: db_check,
                buffer: buffer_check,
                embedding_service: embedding_check,
            },
        }),
    )
}
