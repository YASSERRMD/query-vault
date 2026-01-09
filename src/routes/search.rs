//! Similarity search API endpoint

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::SimilarQuery;
use crate::error::{AppError, Result};
use crate::state::AppState;

/// Request body for similarity search
#[derive(Debug, Deserialize)]
pub struct SimilarSearchRequest {
    /// SQL query to find similar queries for
    pub query: String,
    /// Maximum number of results (default: 10)
    #[serde(default = "default_limit")]
    pub limit: i32,
    /// Minimum similarity threshold (default: 0.85)
    #[serde(default = "default_threshold")]
    pub threshold: f32,
}

fn default_limit() -> i32 {
    10
}

fn default_threshold() -> f32 {
    0.85
}

/// Response for similarity search
#[derive(Debug, Serialize)]
pub struct SimilarSearchResponse {
    pub query: String,
    pub results: Vec<SimilarQuery>,
}

/// POST /api/v1/workspaces/:workspace_id/search/similar
///
/// Searches for queries similar to the provided query text using vector embeddings.
///
/// Request body:
/// - query: The SQL query to find similar queries for
/// - limit: Maximum results (default: 10)
/// - threshold: Minimum cosine similarity (default: 0.85)
pub async fn search_similar(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<SimilarSearchRequest>,
) -> Result<Json<SimilarSearchResponse>> {
    // Check if embedding service is available
    let embedding_service = state
        .embedding_service
        .as_ref()
        .ok_or_else(|| AppError::InternalError("Embedding service not configured".into()))?;

    // Embed the query
    let embedding = embedding_service
        .embed_query(&request.query)
        .map_err(|e| AppError::InternalError(format!("Failed to embed query: {}", e)))?;

    // Search for similar queries
    let results = state
        .db
        .search_similar_queries(workspace_id, &embedding, request.limit, request.threshold)
        .await?;

    Ok(Json(SimilarSearchResponse {
        query: request.query,
        results,
    }))
}

/// GET /api/v1/workspaces/:workspace_id/anomalies
///
/// Returns recent anomalies detected for the workspace
pub async fn get_anomalies(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<AnomaliesResponse>> {
    let rows = sqlx::query(
        r#"
        SELECT 
            id, workspace_id, service_id, metric_id, query_text,
            duration_ms, mean_duration_ms, stddev_duration_ms, z_score,
            detected_at
        FROM query_anomalies
        WHERE workspace_id = $1
        ORDER BY detected_at DESC
        LIMIT 100
        "#,
    )
    .bind(workspace_id)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    use sqlx::Row;
    let anomalies: Vec<AnomalyRecord> = rows
        .into_iter()
        .map(|row| AnomalyRecord {
            id: row.get("id"),
            workspace_id: row.get("workspace_id"),
            service_id: row.get("service_id"),
            metric_id: row.get("metric_id"),
            query_text: row.get("query_text"),
            duration_ms: row.get("duration_ms"),
            mean_duration_ms: row.get("mean_duration_ms"),
            stddev_duration_ms: row.get("stddev_duration_ms"),
            z_score: row.get("z_score"),
            detected_at: row.get("detected_at"),
        })
        .collect();

    Ok(Json(AnomaliesResponse {
        workspace_id,
        count: anomalies.len(),
        anomalies,
    }))
}

#[derive(Debug, Serialize)]
pub struct AnomaliesResponse {
    pub workspace_id: Uuid,
    pub count: usize,
    pub anomalies: Vec<AnomalyRecord>,
}

#[derive(Debug, Serialize)]
pub struct AnomalyRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub service_id: Uuid,
    pub metric_id: Uuid,
    pub query_text: String,
    pub duration_ms: i64,
    pub mean_duration_ms: i64,
    pub stddev_duration_ms: i64,
    pub z_score: f64,
    pub detected_at: chrono::DateTime<chrono::Utc>,
}
