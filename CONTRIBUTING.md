# Contributing to QueryVault

Thank you for your interest in contributing to QueryVault! This document provides guidelines and instructions for contributing.

## Code of Conduct

Please be respectful and constructive in all interactions. We aim to foster an inclusive and welcoming community.

## Getting Started

### Prerequisites

- Rust 1.75 or later
- PostgreSQL 15+ with TimescaleDB and pgvector extensions
- Docker (recommended for database setup)

### Development Setup

1. **Clone the repository**
   ```bash
   git clone https://github.com/YASSERRMD/query-vault.git
   cd query-vault
   ```

2. **Start the database**
   ```bash
   docker-compose up -d postgres
   ```

3. **Run migrations**
   ```bash
   psql postgres://postgres:postgres@localhost:5432/queryvault < migrations/001_init.sql
   psql postgres://postgres:postgres@localhost:5432/queryvault < migrations/002_embeddings.sql
   ```

4. **Run the server**
   ```bash
   cargo run
   ```

5. **Run tests**
   ```bash
   cargo test
   ```

## Making Changes

### Branch Naming

- `feature/` - New features
- `fix/` - Bug fixes
- `docs/` - Documentation updates
- `refactor/` - Code refactoring
- `perf/` - Performance improvements

### Commit Messages

Follow conventional commits:
- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation
- `refactor:` Code refactoring
- `test:` Adding tests
- `chore:` Maintenance tasks

### Code Quality

Before submitting a PR:

```bash
# Format code
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Run tests
cargo test

# Build release
cargo build --release
```

## Pull Request Process

1. Fork the repository
2. Create your feature branch from `main`
3. Make your changes with clear commit messages
4. Add or update tests as needed
5. Update documentation if applicable
6. Submit a pull request

### PR Requirements

- All CI checks must pass
- Code must be formatted with `cargo fmt`
- No clippy warnings
- Tests must pass
- Documentation updated for API changes

## Areas for Contribution

### High Priority

- **ONNX Integration**: Replace the stub embedding service with real ONNX Runtime integration
- **Redis Pub/Sub**: Add Redis support for multi-node WebSocket broadcasting
- **Query Fingerprinting**: Implement SQL query normalization for better pattern matching

### Medium Priority

- **Dashboard**: Build a web-based visualization UI
- **Client SDKs**: Create libraries for Python, Node.js, and Go
- **Alerting**: Add webhook, Slack, and PagerDuty integrations

### Good First Issues

- Add more comprehensive unit tests
- Improve error messages
- Add configuration validation
- Enhance documentation

## Architecture Overview

```
src/
├── main.rs           # Application entry point
├── lib.rs            # Library exports
├── buffer.rs         # Lock-free ingestion buffer
├── db.rs             # Database operations
├── error.rs          # Error types
├── models.rs         # Domain models
├── state.rs          # Application state
├── routes/           # HTTP handlers
│   ├── aggregations.rs
│   ├── health.rs
│   ├── ingest.rs
│   ├── metrics.rs
│   ├── search.rs
│   └── ws.rs
├── services/         # Business logic
│   └── embedding.rs
└── tasks/            # Background workers
    ├── aggregation.rs
    ├── anomaly_detection.rs
    ├── embedding_task.rs
    └── retention.rs
```

## Questions?

Open an issue or reach out to the maintainers.

Thank you for contributing!
