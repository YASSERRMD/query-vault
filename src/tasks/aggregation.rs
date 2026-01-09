//! Aggregation task - moves metrics from buffer to database

use crate::buffer::MetricsBuffer;
use crate::db::Database;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

/// Background task that periodically flushes metrics from the buffer to the database.
///
/// Runs every 5 seconds, pulls a batch from the buffer, and batch-inserts into TimescaleDB.
/// TimescaleDB continuous aggregates handle the actual aggregation.
pub async fn aggregation_task(buffer: MetricsBuffer, db: Arc<Database>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    info!("Aggregation task started (5s interval)");

    loop {
        interval.tick().await;

        // Pop batch from buffer
        let batch = buffer.pop_batch(10_000);
        if batch.is_empty() {
            continue;
        }

        let batch_size = batch.len();
        debug!(
            batch_size = batch_size,
            "Flushing metrics batch to database"
        );

        // Insert batch into database
        match db.insert_metrics_batch(&batch).await {
            Ok(inserted) => {
                if inserted < batch_size {
                    error!(
                        inserted = inserted,
                        expected = batch_size,
                        "Some metrics failed to insert"
                    );
                } else {
                    debug!(inserted = inserted, "Metrics batch inserted successfully");
                }
            }
            Err(e) => {
                error!(error = %e, batch_size = batch_size, "Failed to insert metrics batch");
                // Note: metrics are lost if insert fails
                // In production, consider retry logic or dead-letter queue
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QueryMetric, QueryStatus};
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_metric() -> QueryMetric {
        QueryMetric::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "SELECT 1".to_string(),
            QueryStatus::Success,
            10,
            Utc::now(),
        )
    }

    #[test]
    fn test_pop_batch() {
        let buffer = MetricsBuffer::new(1000);

        for _ in 0..100 {
            buffer.try_push(create_test_metric()).unwrap();
        }

        let batch = buffer.pop_batch(50);
        assert_eq!(batch.len(), 50);
        assert_eq!(buffer.len(), 50);
    }
}
