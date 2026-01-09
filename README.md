# QueryVault

A high-performance, self-hosted, distributed query analytics platform built in Rust.

QueryVault provides real-time visibility into your SQL query performance with sub-millisecond ingestion latency, time-series aggregation, vector similarity search, and anomaly detection.

## Features

- **High-Throughput Ingestion** - Lock-free buffer achieving 60K+ req/s on a single node
- **Real-Time Streaming** - WebSocket-based live metric updates
- **Time-Series Analytics** - TimescaleDB continuous aggregates (5s/1m/5m windows)
- **Vector Similarity Search** - pgvector-powered query deduplication and pattern matching
- **Anomaly Detection** - Automatic slow query detection using z-score analysis
- **Multi-Tenant** - Workspace and service isolation with API key authentication
- **Production Ready** - Docker, Kubernetes, Prometheus metrics, health probes

## Quick Start

### Prerequisites

- Rust 1.75+
- PostgreSQL 15+ with TimescaleDB and pgvector extensions
- Docker (optional)

### Run with Docker Compose

```bash
# Start PostgreSQL with TimescaleDB and pgvector
docker-compose up -d postgres

# Run migrations
psql $DATABASE_URL < migrations/001_init.sql
psql $DATABASE_URL < migrations/002_embeddings.sql

# Start QueryVault
docker-compose up -d queryvault
```

### Run Locally

```bash
# Set environment variables
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/queryvault"
export LISTEN_ADDR="0.0.0.0:3000"

# Run migrations
psql $DATABASE_URL < migrations/001_init.sql
psql $DATABASE_URL < migrations/002_embeddings.sql

# Build and run
cargo run --release
```

## API Reference

### Health & Metrics

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Liveness probe |
| `/ready` | GET | Readiness probe (checks DB) |
| `/metrics` | GET | Prometheus metrics |

### Ingestion

```bash
# Ingest metrics
curl -X POST http://localhost:3000/api/v1/metrics/ingest \
  -H "Authorization: Bearer test-api-key-12345" \
  -H "Content-Type: application/json" \
  -d '{
    "metrics": [{
      "id": "550e8400-e29b-41d4-a716-446655440001",
      "workspace_id": "550e8400-e29b-41d4-a716-446655440000",
      "service_id": "550e8400-e29b-41d4-a716-446655440002",
      "query_text": "SELECT * FROM users WHERE id = $1",
      "status": "success",
      "duration_ms": 42,
      "rows_affected": 1,
      "started_at": "2026-01-10T00:00:00Z",
      "completed_at": "2026-01-10T00:00:00Z",
      "tags": ["read", "users"]
    }]
  }'
```

### Query Aggregations

```bash
# Get time-series aggregations (5s, 1m, or 5m windows)
curl "http://localhost:3000/api/v1/workspaces/{workspace_id}/aggregations?window=1m&from=2026-01-09T00:00:00Z&to=2026-01-10T00:00:00Z"

# Get recent raw metrics
curl "http://localhost:3000/api/v1/workspaces/{workspace_id}/metrics?limit=100"
```

### Vector Similarity Search

```bash
# Find similar queries
curl -X POST "http://localhost:3000/api/v1/workspaces/{workspace_id}/search/similar" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "SELECT * FROM users WHERE email = $1",
    "limit": 10,
    "threshold": 0.85
  }'
```

### Anomaly Detection

```bash
# Get detected anomalies
curl "http://localhost:3000/api/v1/workspaces/{workspace_id}/anomalies"
```

### WebSocket Streaming

```bash
# Connect with websocat
websocat ws://localhost:3000/api/v1/workspaces/{workspace_id}/ws
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgres://...` | PostgreSQL connection string |
| `LISTEN_ADDR` | `0.0.0.0:3000` | Server bind address |
| `BUFFER_CAPACITY` | `100000` | Ingestion buffer size |
| `BROADCAST_CAPACITY` | `10000` | WebSocket broadcast channel size |
| `EMBEDDING_MODEL_PATH` | - | Path to ONNX model (optional) |
| `EMBEDDING_TOKENIZER_PATH` | - | Path to tokenizer.json (optional) |
| `RUST_LOG` | `info` | Log level |

## Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   API Clients   │────▶│   QueryVault    │────▶│   PostgreSQL    │
│   (Ingest)      │     │   (Axum + Rust) │     │   + TimescaleDB │
└─────────────────┘     └────────┬────────┘     │   + pgvector    │
                                 │              └─────────────────┘
┌─────────────────┐              │
│ WebSocket       │◀─────────────┘
│ Clients         │     (Real-time streaming)
└─────────────────┘
```

### Data Flow

1. **Ingestion**: Metrics pushed to lock-free ring buffer
2. **Broadcast**: Metrics broadcast to WebSocket subscribers
3. **Persistence**: Background task flushes buffer to TimescaleDB (5s)
4. **Aggregation**: Continuous aggregates materialize 5s/1m/5m views
5. **Embedding**: Queries embedded for vector similarity (30s)
6. **Anomaly Detection**: Z-score analysis flags slow queries (60s)
7. **Retention**: Old data pruned automatically (30 days raw, 1 year aggregates)

## Deployment

### Kubernetes

```bash
# Apply manifests
kubectl apply -f k8s/deployment.yaml

# The deployment includes:
# - 3 replicas with HPA
# - Liveness and readiness probes
# - Prometheus scraping annotations
# - Resource limits (256Mi-512Mi, 500m-1000m)
```

### Docker

```bash
docker build -t queryvault .
docker run -e DATABASE_URL="..." -p 3000:3000 queryvault
```

## Benchmarking

```bash
# Run buffer benchmarks
cargo bench

# Load test with wrk
wrk -t12 -c400 -d30s -s scripts/ingest.lua http://localhost:3000

# Load test with oha
oha -n 100000 -c 100 --method POST \
  --header "Authorization: Bearer test-api-key-12345" \
  --header "Content-Type: application/json" \
  --data '{"metrics":[...]}' \
  http://localhost:3000/api/v1/metrics/ingest
```

## Contributing

We welcome contributions! Please follow these guidelines:

### Development Setup

```bash
# Clone the repository
git clone https://github.com/YASSERRMD/query-vault.git
cd query-vault

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Start PostgreSQL with extensions
docker run -d --name queryvault-db \
  -e POSTGRES_PASSWORD=postgres \
  -p 5432:5432 \
  timescale/timescaledb-ha:pg15-latest

# Enable pgvector
psql -h localhost -U postgres -c "CREATE EXTENSION IF NOT EXISTS vector;"

# Run migrations
psql -h localhost -U postgres -d postgres < migrations/001_init.sql
psql -h localhost -U postgres -d postgres < migrations/002_embeddings.sql

# Run tests
cargo test

# Run with hot reload
cargo watch -x run
```

### Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy -- -D warnings` to catch issues
- Add tests for new functionality
- Update documentation for API changes

### Pull Request Process

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes with clear commit messages
4. Ensure tests pass (`cargo test`)
5. Run linting (`cargo clippy -- -D warnings`)
6. Submit a pull request

### Areas for Contribution

- **ONNX Integration**: Replace stub embedding service with real ONNX Runtime
- **Redis Pub/Sub**: Add Redis for multi-node WebSocket broadcasting
- **Query Fingerprinting**: Implement SQL query normalization
- **Dashboard**: Build a web UI for visualization
- **SDKs**: Create client libraries (Python, Node.js, Go)
- **Alerting**: Add webhook/Slack/PagerDuty integrations
- **Query Optimization Suggestions**: ML-based query improvement recommendations

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [Axum](https://github.com/tokio-rs/axum) - Web framework
- [SQLx](https://github.com/launchbadge/sqlx) - Async SQL toolkit
- [TimescaleDB](https://www.timescale.com/) - Time-series database
- [pgvector](https://github.com/pgvector/pgvector) - Vector similarity search
- [crossbeam](https://github.com/crossbeam-rs/crossbeam) - Lock-free data structures
