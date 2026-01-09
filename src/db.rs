//! Database access layer with SQLx and PostgreSQL/TimescaleDB

use crate::error::{AppError, Result};
use crate::models::{QueryMetric, QueryStatus, Workspace};
use chrono::{DateTime, Utc};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use std::time::Duration;
use tracing::{error, info};
use uuid::Uuid;

/// Database connection pool and operations
#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    /// Create a new database connection pool
    pub async fn new(connection_string: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(50)
            .min_connections(5)
            .acquire_timeout(Duration::from_secs(5))
            .idle_timeout(Duration::from_secs(600))
            .connect(connection_string)
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to connect: {}", e)))?;

        info!("Database connection pool established");
        Ok(Self { pool })
    }

    /// Get the underlying connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Verify an API key and return the associated workspace
    pub async fn verify_api_key(&self, api_key: &str) -> Result<Workspace> {
        let workspace = sqlx::query_as!(
            Workspace,
            r#"
            SELECT id, name, api_key, created_at, updated_at
            FROM workspaces
            WHERE api_key = $1
            "#,
            api_key
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid API key".into()))?;

        Ok(workspace)
    }

    /// Insert a single metric
    pub async fn insert_metric(&self, metric: &QueryMetric) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO query_metrics (
                id, workspace_id, service_id, query_text, status,
                duration_ms, rows_affected, error_message,
                started_at, completed_at, tags
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            metric.id,
            metric.workspace_id,
            metric.service_id,
            metric.query_text,
            status_to_string(&metric.status),
            metric.duration_ms as i64,
            metric.rows_affected,
            metric.error_message,
            metric.started_at,
            metric.completed_at,
            &metric.tags,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Batch insert metrics for better performance
    pub async fn insert_metrics_batch(&self, metrics: &[QueryMetric]) -> Result<usize> {
        if metrics.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;
        let mut inserted = 0;

        for metric in metrics {
            match sqlx::query!(
                r#"
                INSERT INTO query_metrics (
                    id, workspace_id, service_id, query_text, status,
                    duration_ms, rows_affected, error_message,
                    started_at, completed_at, tags
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "#,
                metric.id,
                metric.workspace_id,
                metric.service_id,
                metric.query_text,
                status_to_string(&metric.status),
                metric.duration_ms as i64,
                metric.rows_affected,
                metric.error_message,
                metric.started_at,
                metric.completed_at,
                &metric.tags,
            )
            .execute(&mut *tx)
            .await
            {
                Ok(_) => inserted += 1,
                Err(e) => {
                    error!(error = %e, metric_id = %metric.id, "Failed to insert metric");
                }
            }
        }

        tx.commit().await?;
        Ok(inserted)
    }

    /// Get recent metrics for a workspace
    pub async fn get_recent_metrics(
        &self,
        workspace_id: Uuid,
        limit: i64,
    ) -> Result<Vec<QueryMetric>> {
        let rows = sqlx::query!(
            r#"
            SELECT 
                id, workspace_id, service_id, query_text, status,
                duration_ms, rows_affected, error_message,
                started_at, completed_at, tags
            FROM query_metrics
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
            workspace_id,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        let metrics = rows
            .into_iter()
            .map(|row| QueryMetric {
                id: row.id,
                workspace_id: row.workspace_id,
                service_id: row.service_id,
                query_text: row.query_text,
                status: string_to_status(&row.status),
                duration_ms: row.duration_ms as u64,
                rows_affected: row.rows_affected,
                error_message: row.error_message,
                started_at: row.started_at,
                completed_at: row.completed_at,
                tags: row.tags.unwrap_or_default(),
            })
            .collect();

        Ok(metrics)
    }

    /// Get aggregated metrics from continuous aggregate views
    pub async fn get_aggregations(
        &self,
        workspace_id: Uuid,
        window: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<AggregatedMetric>> {
        let view_name = match window {
            "5s" => "metrics_5s",
            "1m" => "metrics_1m",
            "5m" => "metrics_5m",
            _ => return Err(AppError::InvalidRequest(format!("Invalid window: {}", window))),
        };

        // Using dynamic query since view name can't be parameterized
        let query = format!(
            r#"
            SELECT 
                workspace_id, service_id, bucket,
                query_count, avg_duration_ms, min_duration_ms, max_duration_ms,
                p95_duration_ms, p99_duration_ms,
                success_count, failed_count, total_rows_affected
            FROM {}
            WHERE workspace_id = $1 AND bucket >= $2 AND bucket < $3
            ORDER BY bucket ASC
            "#,
            view_name
        );

        let rows = sqlx::query(&query)
            .bind(workspace_id)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pool)
            .await?;

        let aggregations = rows
            .into_iter()
            .map(|row| AggregatedMetric {
                workspace_id: row.get("workspace_id"),
                service_id: row.get("service_id"),
                bucket: row.get("bucket"),
                query_count: row.get("query_count"),
                avg_duration_ms: row.get("avg_duration_ms"),
                min_duration_ms: row.get("min_duration_ms"),
                max_duration_ms: row.get("max_duration_ms"),
                p95_duration_ms: row.get("p95_duration_ms"),
                p99_duration_ms: row.get("p99_duration_ms"),
                success_count: row.get("success_count"),
                failed_count: row.get("failed_count"),
                total_rows_affected: row.get("total_rows_affected"),
            })
            .collect();

        Ok(aggregations)
    }

    /// Manually prune old data (backup for TimescaleDB retention policies)
    pub async fn prune_old_metrics(&self, older_than_days: i32) -> Result<u64> {
        let result = sqlx::query!(
            r#"
            DELETE FROM query_metrics
            WHERE created_at < NOW() - make_interval(days => $1)
            "#,
            older_than_days
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

/// Aggregated metric from continuous aggregate views
#[derive(Debug, Clone, serde::Serialize)]
pub struct AggregatedMetric {
    pub workspace_id: Uuid,
    pub service_id: Uuid,
    pub bucket: DateTime<Utc>,
    pub query_count: i64,
    pub avg_duration_ms: Option<i64>,
    pub min_duration_ms: Option<i64>,
    pub max_duration_ms: Option<i64>,
    pub p95_duration_ms: Option<i64>,
    pub p99_duration_ms: Option<i64>,
    pub success_count: Option<i64>,
    pub failed_count: Option<i64>,
    pub total_rows_affected: Option<i64>,
}

/// Convert QueryStatus to database string
fn status_to_string(status: &QueryStatus) -> String {
    match status {
        QueryStatus::Running => "running".to_string(),
        QueryStatus::Success => "success".to_string(),
        QueryStatus::Failed => "failed".to_string(),
        QueryStatus::Cancelled => "cancelled".to_string(),
        QueryStatus::Timeout => "timeout".to_string(),
    }
}

/// Convert database string to QueryStatus
fn string_to_status(s: &str) -> QueryStatus {
    match s {
        "running" => QueryStatus::Running,
        "success" => QueryStatus::Success,
        "failed" => QueryStatus::Failed,
        "cancelled" => QueryStatus::Cancelled,
        "timeout" => QueryStatus::Timeout,
        _ => QueryStatus::Failed,
    }
}
