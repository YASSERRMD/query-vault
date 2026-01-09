//! Historical aggregations API endpoint

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::AggregatedMetric;
use crate::error::{AppError, Result};
use crate::state::AppState;

/// Query parameters for aggregations endpoint
#[derive(Debug, Deserialize)]
pub struct AggregationsQuery {
    /// Aggregation window: "5s", "1m", "5m"
    #[serde(default = "default_window")]
    pub window: String,
    /// Start time (defaults to 1 hour ago)
    pub from: Option<DateTime<Utc>>,
    /// End time (defaults to now)
    pub to: Option<DateTime<Utc>>,
    /// Optional service_id filter
    pub service_id: Option<Uuid>,
}

fn default_window() -> String {
    "1m".to_string()
}

/// Response for aggregations endpoint
#[derive(Debug, Serialize)]
pub struct AggregationsResponse {
    pub workspace_id: Uuid,
    pub window: String,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub buckets: Vec<AggregatedMetric>,
}

/// GET /api/v1/workspaces/:workspace_id/aggregations
///
/// Returns aggregated metrics for the specified workspace and time window.
///
/// Query parameters:
/// - window: "5s", "1m", or "5m" (default: "1m")
/// - from: Start time (default: 1 hour ago)
/// - to: End time (default: now)
/// - service_id: Optional filter by service
pub async fn get_aggregations(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    Query(params): Query<AggregationsQuery>,
) -> Result<Json<AggregationsResponse>> {
    // Validate window parameter
    let valid_windows = ["5s", "1m", "5m"];
    if !valid_windows.contains(&params.window.as_str()) {
        return Err(AppError::InvalidRequest(format!(
            "Invalid window '{}'. Valid options: 5s, 1m, 5m",
            params.window
        )));
    }

    // Set default time range
    let now = Utc::now();
    let from = params.from.unwrap_or_else(|| now - Duration::hours(1));
    let to = params.to.unwrap_or(now);

    // Validate time range
    if from >= to {
        return Err(AppError::InvalidRequest(
            "'from' must be before 'to'".into(),
        ));
    }

    // Query aggregations from database
    let mut buckets = state
        .db
        .get_aggregations(workspace_id, &params.window, from, to)
        .await?;

    // Filter by service_id if provided
    if let Some(service_id) = params.service_id {
        buckets.retain(|b| b.service_id == service_id);
    }

    Ok(Json(AggregationsResponse {
        workspace_id,
        window: params.window,
        from,
        to,
        buckets,
    }))
}

/// GET /api/v1/workspaces/:workspace_id/metrics
///
/// Returns recent raw metrics for the specified workspace.
pub async fn get_recent_metrics(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    Query(params): Query<RecentMetricsQuery>,
) -> Result<Json<RecentMetricsResponse>> {
    let limit = params.limit.unwrap_or(100).min(1000);

    let metrics = state.db.get_recent_metrics(workspace_id, limit).await?;

    Ok(Json(RecentMetricsResponse {
        workspace_id,
        count: metrics.len(),
        metrics,
    }))
}

#[derive(Debug, Deserialize)]
pub struct RecentMetricsQuery {
    /// Maximum number of metrics to return (default: 100, max: 1000)
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RecentMetricsResponse {
    pub workspace_id: Uuid,
    pub count: usize,
    pub metrics: Vec<crate::models::QueryMetric>,
}
