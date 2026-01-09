-- QueryVault Phase 3: Vector Embeddings
-- pgvector extension and query_embeddings table

-- =============================================================================
-- QUERY EMBEDDINGS TABLE
-- =============================================================================

CREATE TABLE IF NOT EXISTS query_embeddings (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    query_hash VARCHAR(64) NOT NULL,  -- SHA256 hash of normalized query
    sql_query TEXT NOT NULL,
    embedding vector(384) NOT NULL,   -- All-MiniLM-L6-v2 produces 384-dim embeddings
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id, query_hash)
);

-- Index for fast similarity search using IVFFlat
-- Lists should be ~sqrt(number of rows), start with 100
CREATE INDEX IF NOT EXISTS idx_query_embeddings_vector 
ON query_embeddings USING ivfflat (embedding vector_cosine_ops) 
WITH (lists = 100);

-- Index for workspace lookups
CREATE INDEX IF NOT EXISTS idx_query_embeddings_workspace 
ON query_embeddings(workspace_id, created_at DESC);

-- =============================================================================
-- ANOMALIES TABLE
-- =============================================================================

CREATE TABLE IF NOT EXISTS query_anomalies (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    service_id UUID NOT NULL,
    metric_id UUID NOT NULL,
    query_text TEXT NOT NULL,
    duration_ms BIGINT NOT NULL,
    mean_duration_ms BIGINT NOT NULL,
    stddev_duration_ms BIGINT NOT NULL,
    z_score DOUBLE PRECISION NOT NULL,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_anomalies_workspace_time 
ON query_anomalies(workspace_id, detected_at DESC);

-- =============================================================================
-- HELPER FUNCTION: Normalize SQL for deduplication
-- =============================================================================

CREATE OR REPLACE FUNCTION normalize_sql(query TEXT) 
RETURNS TEXT AS $$
BEGIN
    -- Simple normalization: lowercase, collapse whitespace
    RETURN LOWER(REGEXP_REPLACE(TRIM(query), '\s+', ' ', 'g'));
END;
$$ LANGUAGE plpgsql IMMUTABLE;
