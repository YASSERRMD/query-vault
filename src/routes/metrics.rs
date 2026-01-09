//! Prometheus metrics endpoint

use axum::response::IntoResponse;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Application metrics for Prometheus
#[derive(Default)]
pub struct Metrics {
    /// Total metrics ingested
    pub metrics_ingested_total: AtomicU64,
    /// Total metrics dropped (buffer full)
    pub metrics_dropped_total: AtomicU64,
    /// Total requests processed
    pub requests_total: AtomicU64,
    /// Current buffer depth
    buffer_depth: AtomicU64,
    /// Active WebSocket connections
    ws_connections: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_ingested(&self, count: u64) {
        self.metrics_ingested_total.fetch_add(count, Ordering::Relaxed);
    }

    pub fn inc_dropped(&self, count: u64) {
        self.metrics_dropped_total.fetch_add(count, Ordering::Relaxed);
    }

    pub fn inc_requests(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_buffer_depth(&self, depth: u64) {
        self.buffer_depth.store(depth, Ordering::Relaxed);
    }

    pub fn inc_ws_connections(&self) {
        self.ws_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_ws_connections(&self) {
        self.ws_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn get_metrics(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            metrics_ingested_total: self.metrics_ingested_total.load(Ordering::Relaxed),
            metrics_dropped_total: self.metrics_dropped_total.load(Ordering::Relaxed),
            requests_total: self.requests_total.load(Ordering::Relaxed),
            buffer_depth: self.buffer_depth.load(Ordering::Relaxed),
            ws_connections: self.ws_connections.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug)]
pub struct MetricsSnapshot {
    pub metrics_ingested_total: u64,
    pub metrics_dropped_total: u64,
    pub requests_total: u64,
    pub buffer_depth: u64,
    pub ws_connections: u64,
}

/// GET /metrics
/// 
/// Returns Prometheus-format metrics
pub async fn prometheus_metrics(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
) -> impl IntoResponse {
    let snapshot = state.metrics.get_metrics();
    let buffer_len = state.metrics_buffer.len() as u64;
    
    // Update buffer depth
    state.metrics.set_buffer_depth(buffer_len);

    let output = format!(
        r#"# HELP queryvault_metrics_ingested_total Total number of metrics ingested
# TYPE queryvault_metrics_ingested_total counter
queryvault_metrics_ingested_total {}

# HELP queryvault_metrics_dropped_total Total number of metrics dropped due to buffer full
# TYPE queryvault_metrics_dropped_total counter
queryvault_metrics_dropped_total {}

# HELP queryvault_requests_total Total number of HTTP requests processed
# TYPE queryvault_requests_total counter
queryvault_requests_total {}

# HELP queryvault_buffer_depth Current number of metrics in buffer
# TYPE queryvault_buffer_depth gauge
queryvault_buffer_depth {}

# HELP queryvault_websocket_connections Current number of active WebSocket connections
# TYPE queryvault_websocket_connections gauge
queryvault_websocket_connections {}

# HELP queryvault_info Build information
# TYPE queryvault_info gauge
queryvault_info{{version="{}"}} 1
"#,
        snapshot.metrics_ingested_total,
        snapshot.metrics_dropped_total,
        snapshot.requests_total,
        buffer_len,
        snapshot.ws_connections,
        env!("CARGO_PKG_VERSION"),
    );

    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        output,
    )
}
