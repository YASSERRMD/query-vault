//! Lock-free ring buffer for high-throughput metric ingestion

use crate::models::QueryMetric;
use crossbeam::queue::ArrayQueue;
use std::sync::Arc;

/// A lock-free metrics buffer backed by crossbeam's ArrayQueue.
///
/// This buffer is designed for high-throughput ingestion (60K+ req/s)
/// with minimal contention between producers and consumer.
#[derive(Clone)]
pub struct MetricsBuffer {
    queue: Arc<ArrayQueue<QueryMetric>>,
    capacity: usize,
}

impl MetricsBuffer {
    /// Create a new buffer with the specified capacity.
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of metrics the buffer can hold
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Arc::new(ArrayQueue::new(capacity)),
            capacity,
        }
    }

    /// Try to push a metric into the buffer.
    ///
    /// Returns `Ok(())` if successful, or `Err(metric)` if the buffer is full.
    pub fn try_push(&self, metric: QueryMetric) -> Result<(), QueryMetric> {
        self.queue.push(metric)
    }

    /// Pop a batch of metrics from the buffer.
    ///
    /// Returns up to `max` metrics, or fewer if the buffer has less.
    pub fn pop_batch(&self, max: usize) -> Vec<QueryMetric> {
        let mut batch = Vec::with_capacity(max.min(self.queue.len()));
        for _ in 0..max {
            match self.queue.pop() {
                Some(metric) => batch.push(metric),
                None => break,
            }
        }
        batch
    }

    /// Get the current number of metrics in the buffer.
    #[inline]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Check if the buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Get the buffer capacity.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::QueryStatus;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_metric() -> QueryMetric {
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
    fn test_push_and_pop() {
        let buffer = MetricsBuffer::new(100);
        let metric = make_metric();

        assert!(buffer.try_push(metric).is_ok());
        assert_eq!(buffer.len(), 1);

        let batch = buffer.pop_batch(10);
        assert_eq!(batch.len(), 1);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_buffer_full() {
        let buffer = MetricsBuffer::new(2);

        assert!(buffer.try_push(make_metric()).is_ok());
        assert!(buffer.try_push(make_metric()).is_ok());
        assert!(buffer.try_push(make_metric()).is_err());
    }

    #[test]
    fn test_pop_batch_max() {
        let buffer = MetricsBuffer::new(100);

        for _ in 0..50 {
            buffer.try_push(make_metric()).unwrap();
        }

        let batch = buffer.pop_batch(20);
        assert_eq!(batch.len(), 20);
        assert_eq!(buffer.len(), 30);
    }
}
