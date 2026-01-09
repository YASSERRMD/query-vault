//! Retention task - prunes old data as backup to TimescaleDB policies

use crate::db::Database;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

/// Background task that periodically prunes old metrics.
/// 
/// This is a backup to TimescaleDB's built-in retention policies.
/// Runs every 6 hours and deletes raw metrics older than 30 days.
pub async fn retention_task(db: Arc<Database>) {
    // Wait 1 minute before starting to allow system to stabilize
    tokio::time::sleep(Duration::from_secs(60)).await;

    let mut interval = tokio::time::interval(Duration::from_secs(6 * 60 * 60)); // 6 hours
    
    info!("Retention task started (6h interval)");

    loop {
        interval.tick().await;

        info!("Running retention cleanup...");

        match db.prune_old_metrics(30).await {
            Ok(deleted) => {
                if deleted > 0 {
                    info!(deleted = deleted, "Pruned old metrics");
                } else {
                    info!("No old metrics to prune");
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to prune old metrics");
            }
        }
    }
}
