//! Core domain models for QueryVault

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of a query execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryStatus {
    /// Query is currently executing
    Running,
    /// Query completed successfully
    Success,
    /// Query failed with an error
    Failed,
    /// Query was cancelled
    Cancelled,
    /// Query timed out
    Timeout,
}

/// A single query metric event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetric {
    /// Unique identifier for this metric
    pub id: Uuid,
    /// Workspace this metric belongs to
    pub workspace_id: Uuid,
    /// Service that generated this metric
    pub service_id: Uuid,
    /// The SQL query text
    pub query_text: String,
    /// Query execution status
    pub status: QueryStatus,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Number of rows returned/affected
    pub rows_affected: Option<i64>,
    /// Error message if status is Failed
    pub error_message: Option<String>,
    /// When the query started
    pub started_at: DateTime<Utc>,
    /// When the query completed
    pub completed_at: DateTime<Utc>,
    /// Optional metadata tags
    #[serde(default)]
    pub tags: Vec<String>,
}

impl QueryMetric {
    /// Create a new QueryMetric with generated ID
    #[allow(dead_code)]
    pub fn new(
        workspace_id: Uuid,
        service_id: Uuid,
        query_text: String,
        status: QueryStatus,
        duration_ms: u64,
        started_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id,
            service_id,
            query_text,
            status,
            duration_ms,
            rows_affected: None,
            error_message: None,
            started_at,
            completed_at: Utc::now(),
            tags: Vec::new(),
        }
    }
}

/// Workspace represents a tenant/organization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub api_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Service represents an application within a workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Service {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request payload for ingesting metrics
#[derive(Debug, Clone, Deserialize)]
pub struct IngestRequest {
    pub metrics: Vec<QueryMetric>,
}

/// Response payload for ingestion
#[derive(Debug, Clone, Serialize)]
pub struct IngestResponse {
    /// Number of metrics successfully ingested
    pub ingested: usize,
    /// Number of metrics dropped (buffer full)
    pub dropped: usize,
}

/// Health check response
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}
