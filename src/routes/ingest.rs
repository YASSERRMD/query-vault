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

    let _workspace_id = state
        .db
        .verify_api_key(api_key)
        .await
        .ok_or_else(|| AppError::Unauthorized("Invalid API key".into()))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QueryMetric, QueryStatus};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
        routing::post,
    };
    use chrono::Utc;
    use tower::ServiceExt;
    use uuid::Uuid;

    fn create_test_app() -> Router {
        let state = AppState::new(1000, 100);
        Router::new()
            .route("/api/v1/metrics/ingest", post(ingest_metrics))
            .with_state(state)
    }

    fn create_test_metric() -> QueryMetric {
        QueryMetric::new(
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            Uuid::new_v4(),
            "SELECT * FROM users".to_string(),
            QueryStatus::Success,
            42,
            Utc::now(),
        )
    }

    #[tokio::test]
    async fn test_ingest_unauthorized() {
        let app = create_test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/metrics/ingest")
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"metrics":[]}"#))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_ingest_success() {
        let app = create_test_app();
        let metrics = vec![create_test_metric()];
        let body = serde_json::to_string(&IngestRequest { metrics }).unwrap();

        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/metrics/ingest")
            .header("Content-Type", "application/json")
            .header("Authorization", "Bearer test-api-key-12345")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }
}
