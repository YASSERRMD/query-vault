//! HTTP ingestion endpoint for high-throughput metric collection

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use tracing::{info, warn};

use crate::error::{AppError, Result};
use crate::models::{IngestRequest, IngestResponse};
use crate::state::AppState;

/// Extract Bearer token from Authorization header
fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// POST /api/v1/metrics/ingest
///
/// Ingests a batch of query metrics into the buffer.
/// Requires Bearer token authentication.
///
/// Returns 202 Accepted with count of ingested metrics.
pub async fn ingest_metrics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<IngestRequest>,
) -> Result<(StatusCode, Json<IngestResponse>)> {
    // Extract and verify API key
    let api_key = extract_bearer_token(&headers)
        .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".into()))?;

    let _workspace = state.db.verify_api_key(api_key).await?;

    let total = payload.metrics.len();
    let mut ingested = 0;
    let mut dropped = 0;

    for metric in payload.metrics {
        match state.metrics_buffer.try_push(metric) {
            Ok(()) => ingested += 1,
            Err(_dropped_metric) => {
                dropped += 1;
            }
        }
    }

    if dropped > 0 {
        warn!(
            total = total,
            ingested = ingested,
            dropped = dropped,
            "Buffer full, some metrics dropped"
        );
    } else {
        info!(
            total = total,
            ingested = ingested,
            "Metrics ingested successfully"
        );
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(IngestResponse { ingested, dropped }),
    ))
}
