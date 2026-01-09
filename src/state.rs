//! Application state shared across handlers

use crate::buffer::MetricsBuffer;
use crate::db::Database;
use crate::models::QueryMetric;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool
    pub db: Arc<Database>,
    /// Lock-free metrics buffer for high-throughput ingestion
    pub metrics_buffer: MetricsBuffer,
    /// Broadcast channel for real-time metric streaming
    pub broadcast_tx: broadcast::Sender<(Uuid, QueryMetric)>,
}

impl AppState {
    /// Create new application state
    /// 
    /// # Arguments
    /// * `db` - Database connection
    /// * `buffer_capacity` - Capacity of the metrics buffer
    /// * `broadcast_capacity` - Capacity of the broadcast channel
    pub fn new(db: Database, buffer_capacity: usize, broadcast_capacity: usize) -> Self {
        let (broadcast_tx, _) = broadcast::channel(broadcast_capacity);
        Self {
            db: Arc::new(db),
            metrics_buffer: MetricsBuffer::new(buffer_capacity),
            broadcast_tx,
        }
    }
}
