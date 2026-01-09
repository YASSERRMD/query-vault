//! Application state shared across handlers

use crate::buffer::MetricsBuffer;
use crate::models::QueryMetric;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Placeholder database handle (real implementation in later phases)
#[derive(Clone)]
pub struct Database {
    /// In-memory API key store for Phase 1 (workspace_id -> api_key)
    api_keys: Arc<RwLock<HashMap<String, Uuid>>>,
}

impl Database {
    /// Create a new placeholder database
    pub fn new() -> Self {
        let mut keys = HashMap::new();
        // Add a default test API key for Phase 1
        keys.insert(
            "test-api-key-12345".to_string(),
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
        );
        Self {
            api_keys: Arc::new(RwLock::new(keys)),
        }
    }

    /// Verify an API key and return the associated workspace_id
    pub async fn verify_api_key(&self, api_key: &str) -> Option<Uuid> {
        self.api_keys.read().get(api_key).copied()
    }

    /// Add an API key (for testing)
    pub fn add_api_key(&self, api_key: String, workspace_id: Uuid) {
        self.api_keys.write().insert(api_key, workspace_id);
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Database connection (placeholder in Phase 1)
    pub db: Database,
    /// Lock-free metrics buffer for high-throughput ingestion
    pub metrics_buffer: MetricsBuffer,
    /// Broadcast channel for real-time metric streaming
    pub broadcast_tx: broadcast::Sender<(Uuid, QueryMetric)>,
}

impl AppState {
    /// Create new application state
    /// 
    /// # Arguments
    /// * `buffer_capacity` - Capacity of the metrics buffer
    /// * `broadcast_capacity` - Capacity of the broadcast channel
    pub fn new(buffer_capacity: usize, broadcast_capacity: usize) -> Self {
        let (broadcast_tx, _) = broadcast::channel(broadcast_capacity);
        Self {
            db: Database::new(),
            metrics_buffer: MetricsBuffer::new(buffer_capacity),
            broadcast_tx,
        }
    }
}
