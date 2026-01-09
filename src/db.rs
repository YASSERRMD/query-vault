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
        let row = sqlx::query(
            r#"
            SELECT id, name, api_key, created_at, updated_at
            FROM workspaces
            WHERE api_key = $1
            "#,
        )
        .bind(api_key)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid API key".into()))?;

        Ok(Workspace {
            id: row.get("id"),
            name: row.get("name"),
            api_key: row.get("api_key"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    /// Insert a single metric
    pub async fn insert_metric(&self, metric: &QueryMetric) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO query_metrics (
                id, workspace_id, service_id, query_text, status,
                duration_ms, rows_affected, error_message,
                started_at, completed_at, tags
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(metric.id)
        .bind(metric.workspace_id)
        .bind(metric.service_id)
        .bind(&metric.query_text)
        .bind(status_to_string(&metric.status))
        .bind(metric.duration_ms as i64)
        .bind(metric.rows_affected)
        .bind(&metric.error_message)
        .bind(metric.started_at)
        .bind(metric.completed_at)
        .bind(&metric.tags)
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
            match sqlx::query(
                r#"
                INSERT INTO query_metrics (
                    id, workspace_id, service_id, query_text, status,
                    duration_ms, rows_affected, error_message,
                    started_at, completed_at, tags
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "#,
            )
            .bind(metric.id)
            .bind(metric.workspace_id)
            .bind(metric.service_id)
            .bind(&metric.query_text)
            .bind(status_to_string(&metric.status))
            .bind(metric.duration_ms as i64)
            .bind(metric.rows_affected)
            .bind(&metric.error_message)
            .bind(metric.started_at)
            .bind(metric.completed_at)
            .bind(&metric.tags)
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
        let rows = sqlx::query(
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
        )
        .bind(workspace_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let metrics = rows
            .into_iter()
            .map(|row| QueryMetric {
                id: row.get("id"),
                workspace_id: row.get("workspace_id"),
                service_id: row.get("service_id"),
                query_text: row.get("query_text"),
                status: string_to_status(row.get("status")),
                duration_ms: row.get::<i64, _>("duration_ms") as u64,
                rows_affected: row.get("rows_affected"),
                error_message: row.get("error_message"),
                started_at: row.get("started_at"),
                completed_at: row.get("completed_at"),
                tags: row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default(),
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
        let result = sqlx::query(
            r#"
            DELETE FROM query_metrics
            WHERE created_at < NOW() - make_interval(days => $1)
            "#,
        )
        .bind(older_than_days)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    // =========================================================================
    // EMBEDDING METHODS
    // =========================================================================

    /// Insert or update a query embedding
    pub async fn insert_query_embedding(
        &self,
        workspace_id: Uuid,
        query_hash: &str,
        sql_query: &str,
        embedding: &[f32],
    ) -> Result<()> {
        // Convert embedding to pgvector format string
        let embedding_str = format!(
            "[{}]",
            embedding.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")
        );

        sqlx::query(
            r#"
            INSERT INTO query_embeddings (workspace_id, query_hash, sql_query, embedding)
            VALUES ($1, $2, $3, $4::vector)
            ON CONFLICT (workspace_id, query_hash) 
            DO UPDATE SET embedding = $4::vector, updated_at = NOW()
            "#,
        )
        .bind(workspace_id)
        .bind(query_hash)
        .bind(sql_query)
        .bind(&embedding_str)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if a query embedding exists
    pub async fn embedding_exists(&self, workspace_id: Uuid, query_hash: &str) -> Result<bool> {
        let row = sqlx::query(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM query_embeddings 
                WHERE workspace_id = $1 AND query_hash = $2
            ) as exists
            "#,
        )
        .bind(workspace_id)
        .bind(query_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get::<bool, _>("exists"))
    }

    /// Search for similar queries using cosine similarity
    pub async fn search_similar_queries(
        &self,
        workspace_id: Uuid,
        embedding: &[f32],
        limit: i32,
        threshold: f32,
    ) -> Result<Vec<SimilarQuery>> {
        let embedding_str = format!(
            "[{}]",
            embedding.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")
        );

        let rows = sqlx::query(
            r#"
            SELECT 
                id,
                sql_query,
                1 - (embedding <=> $2::vector) as similarity
            FROM query_embeddings
            WHERE workspace_id = $1
                AND 1 - (embedding <=> $2::vector) >= $4
            ORDER BY embedding <=> $2::vector
            LIMIT $3
            "#,
        )
        .bind(workspace_id)
        .bind(&embedding_str)
        .bind(limit)
        .bind(threshold)
        .fetch_all(&self.pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|row| SimilarQuery {
                id: row.get("id"),
                sql_query: row.get("sql_query"),
                similarity: row.get("similarity"),
            })
            .collect();

        Ok(results)
    }

    /// Get queries that haven't been embedded yet
    pub async fn get_unembedded_queries(
        &self,
        workspace_id: Uuid,
        limit: i64,
    ) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT query_text, 
                   md5(lower(regexp_replace(trim(query_text), '\s+', ' ', 'g'))) as query_hash
            FROM query_metrics m
            WHERE m.workspace_id = $1
                AND NOT EXISTS (
                    SELECT 1 FROM query_embeddings e 
                    WHERE e.workspace_id = m.workspace_id 
                    AND e.query_hash = md5(lower(regexp_replace(trim(m.query_text), '\s+', ' ', 'g')))
                )
            LIMIT $2
            "#,
        )
        .bind(workspace_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|row| (row.get::<String, _>("query_text"), row.get::<String, _>("query_hash")))
            .collect();

        Ok(results)
    }

    // =========================================================================
    // ANOMALY METHODS
    // =========================================================================

    /// Get metrics statistics for anomaly detection
    pub async fn get_metrics_stats(
        &self,
        workspace_id: Uuid,
        limit: i64,
    ) -> Result<MetricsStats> {
        let row = sqlx::query(
            r#"
            SELECT 
                AVG(duration_ms)::DOUBLE PRECISION as mean,
                STDDEV(duration_ms)::DOUBLE PRECISION as stddev,
                COUNT(*) as count
            FROM (
                SELECT duration_ms 
                FROM query_metrics 
                WHERE workspace_id = $1 
                ORDER BY created_at DESC 
                LIMIT $2
            ) recent
            "#,
        )
        .bind(workspace_id)
        .bind(limit)
        .fetch_one(&self.pool)
        .await?;

        Ok(MetricsStats {
            mean: row.get::<Option<f64>, _>("mean").unwrap_or(0.0),
            stddev: row.get::<Option<f64>, _>("stddev").unwrap_or(0.0),
            count: row.get::<i64, _>("count"),
        })
    }

    /// Get recent metrics with high duration for anomaly detection
    pub async fn get_recent_metrics_for_anomaly(
        &self,
        workspace_id: Uuid,
        since_seconds: i64,
        threshold_ms: i64,
    ) -> Result<Vec<QueryMetric>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                id, workspace_id, service_id, query_text, status,
                duration_ms, rows_affected, error_message,
                started_at, completed_at, tags
            FROM query_metrics
            WHERE workspace_id = $1
                AND created_at > NOW() - make_interval(secs => $2)
                AND duration_ms > $3
            ORDER BY duration_ms DESC
            "#,
        )
        .bind(workspace_id)
        .bind(since_seconds)
        .bind(threshold_ms)
        .fetch_all(&self.pool)
        .await?;

        let metrics = rows
            .into_iter()
            .map(|row| QueryMetric {
                id: row.get("id"),
                workspace_id: row.get("workspace_id"),
                service_id: row.get("service_id"),
                query_text: row.get("query_text"),
                status: string_to_status(row.get("status")),
                duration_ms: row.get::<i64, _>("duration_ms") as u64,
                rows_affected: row.get("rows_affected"),
                error_message: row.get("error_message"),
                started_at: row.get("started_at"),
                completed_at: row.get("completed_at"),
                tags: row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default(),
            })
            .collect();

        Ok(metrics)
    }

    /// Record a detected anomaly
    pub async fn insert_anomaly(&self, anomaly: &QueryAnomaly) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO query_anomalies (
                workspace_id, service_id, metric_id, query_text,
                duration_ms, mean_duration_ms, stddev_duration_ms, z_score
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(anomaly.workspace_id)
        .bind(anomaly.service_id)
        .bind(anomaly.metric_id)
        .bind(&anomaly.query_text)
        .bind(anomaly.duration_ms)
        .bind(anomaly.mean_duration_ms)
        .bind(anomaly.stddev_duration_ms)
        .bind(anomaly.z_score)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get all workspace IDs
    pub async fn get_all_workspace_ids(&self) -> Result<Vec<Uuid>> {
        let rows = sqlx::query("SELECT id FROM workspaces")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|r| r.get("id")).collect())
    }
}

/// Similar query result from vector search
#[derive(Debug, Clone, serde::Serialize)]
pub struct SimilarQuery {
    pub id: Uuid,
    pub sql_query: String,
    pub similarity: f64,
}

/// Metrics statistics for anomaly detection
#[derive(Debug, Clone)]
pub struct MetricsStats {
    pub mean: f64,
    pub stddev: f64,
    pub count: i64,
}

/// Query anomaly record
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryAnomaly {
    pub workspace_id: Uuid,
    pub service_id: Uuid,
    pub metric_id: Uuid,
    pub query_text: String,
    pub duration_ms: i64,
    pub mean_duration_ms: i64,
    pub stddev_duration_ms: i64,
    pub z_score: f64,
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
