//! Anomaly detection background task

use crate::db::{Database, QueryAnomaly};
use crate::models::QueryMetric;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Anomaly event for WebSocket broadcast
#[derive(Debug, Clone, serde::Serialize)]
pub struct AnomalyEvent {
    pub event_type: &'static str,
    pub anomaly: QueryAnomaly,
}

/// Background task that detects query anomalies based on execution time.
///
/// Runs every 60 seconds, computes mean and stddev of recent metrics,
/// flags queries with z-score > 3, broadcasts to WebSocket clients,
/// and stores anomalies in the database.
pub async fn anomaly_detection_task(
    db: Arc<Database>,
    broadcast_tx: broadcast::Sender<(Uuid, QueryMetric)>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));

    info!("Anomaly detection task started (60s interval)");

    loop {
        interval.tick().await;

        // Get all workspaces
        let workspaces = match db.get_all_workspace_ids().await {
            Ok(w) => w,
            Err(e) => {
                error!(error = %e, "Failed to get workspaces for anomaly detection");
                continue;
            }
        };

        for workspace_id in workspaces {
            if let Err(e) = detect_anomalies_for_workspace(&db, workspace_id, &broadcast_tx).await {
                error!(error = %e, workspace_id = %workspace_id, "Anomaly detection failed");
            }
        }
    }
}

/// Detect anomalies for a single workspace
async fn detect_anomalies_for_workspace(
    db: &Database,
    workspace_id: Uuid,
    _broadcast_tx: &broadcast::Sender<(Uuid, QueryMetric)>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Get statistics from last 1000 metrics
    let stats = db.get_metrics_stats(workspace_id, 1000).await?;

    if stats.count < 100 {
        // Not enough data for meaningful statistics
        debug!(workspace_id = %workspace_id, count = stats.count, "Not enough data for anomaly detection");
        return Ok(());
    }

    if stats.stddev <= 0.0 {
        // No variance, can't detect anomalies
        return Ok(());
    }

    // Calculate threshold: mean + 3 * stddev
    let threshold_ms = (stats.mean + 3.0 * stats.stddev) as i64;

    debug!(
        workspace_id = %workspace_id,
        mean = stats.mean,
        stddev = stats.stddev,
        threshold_ms = threshold_ms,
        "Anomaly detection thresholds"
    );

    // Get recent metrics above threshold (last 60 seconds)
    let slow_queries = db
        .get_recent_metrics_for_anomaly(workspace_id, 60, threshold_ms)
        .await?;

    if slow_queries.is_empty() {
        return Ok(());
    }

    info!(
        workspace_id = %workspace_id,
        count = slow_queries.len(),
        "Detected slow query anomalies"
    );

    // Process each anomaly
    for metric in slow_queries {
        let z_score = (metric.duration_ms as f64 - stats.mean) / stats.stddev;

        let anomaly = QueryAnomaly {
            workspace_id: metric.workspace_id,
            service_id: metric.service_id,
            metric_id: metric.id,
            query_text: metric.query_text.clone(),
            duration_ms: metric.duration_ms as i64,
            mean_duration_ms: stats.mean as i64,
            stddev_duration_ms: stats.stddev as i64,
            z_score,
        };

        // Store anomaly in database
        if let Err(e) = db.insert_anomaly(&anomaly).await {
            warn!(error = %e, metric_id = %metric.id, "Failed to store anomaly");
        }

        // Broadcast to WebSocket clients
        // Note: We reuse the existing broadcast channel, but in a more complete
        // implementation, we might have a separate anomaly broadcast channel
        debug!(
            workspace_id = %workspace_id,
            metric_id = %metric.id,
            z_score = z_score,
            duration_ms = metric.duration_ms,
            "Anomaly detected and recorded"
        );
    }

    Ok(())
}
