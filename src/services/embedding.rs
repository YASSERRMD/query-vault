//! ONNX-based embedding service for SQL queries
//!
//! Note: This is a placeholder implementation. The embedding service requires:
//! - ONNX model file (e.g., all-MiniLM-L6-v2.onnx)
//! - Tokenizer file (tokenizer.json from HuggingFace)
//!
//! The actual ONNX Runtime integration is deferred until the model files are available.
//! For now, we provide a stub that can be replaced with real ONNX inference.

use std::path::Path;
use tracing::{info, warn};

use crate::error::{AppError, Result};

/// Embedding service (stub implementation)
///
/// In production, this would use ONNX Runtime for transformer models.
/// For now, it provides a simple hash-based embedding for testing.
#[derive(Clone)]
pub struct EmbeddingService {
    embedding_dim: usize,
}

impl EmbeddingService {
    /// Create a new embedding service from ONNX model and tokenizer paths
    ///
    /// # Arguments
    /// * `model_path` - Path to the ONNX model file
    /// * `tokenizer_path` - Path to the tokenizer.json file
    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        info!(model = ?model_path, tokenizer = ?tokenizer_path, "Loading embedding model");

        // Verify paths exist
        if !model_path.exists() {
            return Err(AppError::InternalError(format!(
                "Model file not found: {:?}",
                model_path
            )));
        }
        if !tokenizer_path.exists() {
            return Err(AppError::InternalError(format!(
                "Tokenizer file not found: {:?}",
                tokenizer_path
            )));
        }

        // For now, use a simple stub implementation
        // Real implementation would load ONNX model and tokenizer
        warn!("Using stub embedding service - real ONNX inference not implemented");

        let embedding_dim = 384; // Standard for MiniLM-L6-v2

        info!(
            embedding_dim = embedding_dim,
            "Embedding service ready (stub mode)"
        );

        Ok(Self { embedding_dim })
    }

    /// Embed a single query string
    ///
    /// Returns a normalized embedding vector
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        // Stub implementation: generate deterministic embedding from query hash
        let embedding = self.generate_stub_embedding(query);
        Ok(embedding)
    }

    /// Embed a batch of queries
    ///
    /// Returns normalized embedding vectors
    #[allow(dead_code)]
    pub fn embed_batch(&self, queries: &[&str]) -> Result<Vec<Vec<f32>>> {
        queries.iter().map(|q| self.embed_query(q)).collect()
    }

    /// Generate a stub embedding based on query hash
    /// This is deterministic - same query always produces same embedding
    fn generate_stub_embedding(&self, query: &str) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let normalized = normalize_query(query);
        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        let hash = hasher.finish();

        // Generate pseudo-random but deterministic embedding
        let mut embedding = Vec::with_capacity(self.embedding_dim);
        let mut seed = hash;

        for _ in 0..self.embedding_dim {
            // Simple LCG for deterministic pseudo-random numbers
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let value = ((seed >> 33) as f32) / (u32::MAX as f32) * 2.0 - 1.0;
            embedding.push(value);
        }

        // Normalize to unit vector
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut embedding {
                *v /= norm;
            }
        }

        embedding
    }

    /// Get the embedding dimension
    #[allow(dead_code)]
    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }
}

/// Compute cosine similarity between two normalized vectors
#[allow(dead_code)]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Normalize SQL query for consistent embedding
pub fn normalize_query(query: &str) -> String {
    query
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Compute hash of normalized query
#[allow(dead_code)]
pub fn query_hash(query: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let normalized = normalize_query(query);
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
