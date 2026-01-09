-- QueryVault Database Schema
-- Extensions, tables, hypertables, and continuous aggregates

-- =============================================================================
-- EXTENSIONS
-- =============================================================================

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS timescaledb;
CREATE EXTENSION IF NOT EXISTS vector;

-- =============================================================================
-- WORKSPACES
-- =============================================================================

CREATE TABLE IF NOT EXISTS workspaces (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    api_key VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_workspaces_api_key ON workspaces(api_key);

-- =============================================================================
-- SERVICES
-- =============================================================================

CREATE TABLE IF NOT EXISTS services (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id, name)
);

CREATE INDEX idx_services_workspace ON services(workspace_id);

-- =============================================================================
-- QUERY METRICS (TimescaleDB Hypertable)
-- =============================================================================

CREATE TABLE IF NOT EXISTS query_metrics (
    id UUID NOT NULL DEFAULT uuid_generate_v4(),
    workspace_id UUID NOT NULL,
    service_id UUID NOT NULL,
    query_text TEXT NOT NULL,
    status VARCHAR(20) NOT NULL,
    duration_ms BIGINT NOT NULL,
    rows_affected BIGINT,
    error_message TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    tags TEXT[] DEFAULT '{}',
    -- Optional: query embedding for similarity search (Phase 6)
    -- embedding vector(384),
    PRIMARY KEY (id, created_at)
);

-- Convert to hypertable partitioned by created_at
SELECT create_hypertable('query_metrics', 'created_at', 
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);

-- Performance indexes
CREATE INDEX idx_metrics_workspace_time ON query_metrics(workspace_id, created_at DESC);
CREATE INDEX idx_metrics_slow_queries ON query_metrics(workspace_id, duration_ms DESC) 
    WHERE duration_ms > 1000;
CREATE INDEX idx_metrics_status ON query_metrics(workspace_id, status, created_at DESC);
CREATE INDEX idx_metrics_service ON query_metrics(service_id, created_at DESC);

-- =============================================================================
-- CONTINUOUS AGGREGATES (5s, 1m, 5m windows)
-- =============================================================================

-- 5-second aggregates
CREATE MATERIALIZED VIEW IF NOT EXISTS metrics_5s
WITH (timescaledb.continuous) AS
SELECT
    workspace_id,
    service_id,
    time_bucket('5 seconds', created_at) AS bucket,
    COUNT(*) AS query_count,
    AVG(duration_ms)::BIGINT AS avg_duration_ms,
    MIN(duration_ms) AS min_duration_ms,
    MAX(duration_ms) AS max_duration_ms,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY duration_ms)::BIGINT AS p95_duration_ms,
    PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY duration_ms)::BIGINT AS p99_duration_ms,
    SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) AS success_count,
    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failed_count,
    SUM(COALESCE(rows_affected, 0)) AS total_rows_affected
FROM query_metrics
GROUP BY workspace_id, service_id, bucket
WITH NO DATA;

-- 1-minute aggregates
CREATE MATERIALIZED VIEW IF NOT EXISTS metrics_1m
WITH (timescaledb.continuous) AS
SELECT
    workspace_id,
    service_id,
    time_bucket('1 minute', created_at) AS bucket,
    COUNT(*) AS query_count,
    AVG(duration_ms)::BIGINT AS avg_duration_ms,
    MIN(duration_ms) AS min_duration_ms,
    MAX(duration_ms) AS max_duration_ms,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY duration_ms)::BIGINT AS p95_duration_ms,
    PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY duration_ms)::BIGINT AS p99_duration_ms,
    SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) AS success_count,
    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failed_count,
    SUM(COALESCE(rows_affected, 0)) AS total_rows_affected
FROM query_metrics
GROUP BY workspace_id, service_id, bucket
WITH NO DATA;

-- 5-minute aggregates
CREATE MATERIALIZED VIEW IF NOT EXISTS metrics_5m
WITH (timescaledb.continuous) AS
SELECT
    workspace_id,
    service_id,
    time_bucket('5 minutes', created_at) AS bucket,
    COUNT(*) AS query_count,
    AVG(duration_ms)::BIGINT AS avg_duration_ms,
    MIN(duration_ms) AS min_duration_ms,
    MAX(duration_ms) AS max_duration_ms,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY duration_ms)::BIGINT AS p95_duration_ms,
    PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY duration_ms)::BIGINT AS p99_duration_ms,
    SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) AS success_count,
    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failed_count,
    SUM(COALESCE(rows_affected, 0)) AS total_rows_affected
FROM query_metrics
GROUP BY workspace_id, service_id, bucket
WITH NO DATA;

-- =============================================================================
-- CONTINUOUS AGGREGATE REFRESH POLICIES
-- =============================================================================

SELECT add_continuous_aggregate_policy('metrics_5s',
    start_offset => INTERVAL '1 hour',
    end_offset => INTERVAL '5 seconds',
    schedule_interval => INTERVAL '5 seconds',
    if_not_exists => TRUE
);

SELECT add_continuous_aggregate_policy('metrics_1m',
    start_offset => INTERVAL '6 hours',
    end_offset => INTERVAL '1 minute',
    schedule_interval => INTERVAL '1 minute',
    if_not_exists => TRUE
);

SELECT add_continuous_aggregate_policy('metrics_5m',
    start_offset => INTERVAL '1 day',
    end_offset => INTERVAL '5 minutes',
    schedule_interval => INTERVAL '5 minutes',
    if_not_exists => TRUE
);

-- =============================================================================
-- RETENTION POLICIES
-- =============================================================================

-- Raw metrics: keep 30 days
SELECT add_retention_policy('query_metrics', INTERVAL '30 days', if_not_exists => TRUE);

-- 5s aggregates: keep 7 days
SELECT add_retention_policy('metrics_5s', INTERVAL '7 days', if_not_exists => TRUE);

-- 1m aggregates: keep 90 days
SELECT add_retention_policy('metrics_1m', INTERVAL '90 days', if_not_exists => TRUE);

-- 5m aggregates: keep 1 year
SELECT add_retention_policy('metrics_5m', INTERVAL '365 days', if_not_exists => TRUE);

-- =============================================================================
-- SEED DATA (for testing)
-- =============================================================================

INSERT INTO workspaces (id, name, api_key) VALUES
    ('550e8400-e29b-41d4-a716-446655440000', 'Default Workspace', 'test-api-key-12345')
ON CONFLICT (api_key) DO NOTHING;

INSERT INTO services (id, workspace_id, name, description) VALUES
    ('550e8400-e29b-41d4-a716-446655440001', '550e8400-e29b-41d4-a716-446655440000', 'default-service', 'Default test service')
ON CONFLICT (workspace_id, name) DO NOTHING;
