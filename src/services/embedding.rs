//! ONNX-based embedding service for SQL queries

use ndarray::{Array1, Array2, Axis};
use ort::{GraphOptimizationLevel, Session};
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;
use tracing::{debug, info};

use crate::error::{AppError, Result};

/// Embedding service using ONNX Runtime for transformer models
#[derive(Clone)]
pub struct EmbeddingService {
    session: Arc<Session>,
    tokenizer: Arc<Tokenizer>,
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

        // Load ONNX model
        let session = Session::builder()
            .map_err(|e| AppError::InternalError(format!("Failed to create session builder: {}", e)))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| AppError::InternalError(format!("Failed to set optimization level: {}", e)))?
            .commit_from_file(model_path)
            .map_err(|e| AppError::InternalError(format!("Failed to load ONNX model: {}", e)))?;

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| AppError::InternalError(format!("Failed to load tokenizer: {}", e)))?;

        // Determine embedding dimension from model output (typically 384 for MiniLM-L6)
        let embedding_dim = 384;

        info!(embedding_dim = embedding_dim, "Embedding service ready");

        Ok(Self {
            session: Arc::new(session),
            tokenizer: Arc::new(tokenizer),
            embedding_dim,
        })
    }

    /// Embed a single query string
    /// 
    /// Returns a normalized embedding vector
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed_batch(&[query])?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    /// Embed a batch of queries
    /// 
    /// Returns normalized embedding vectors
    pub fn embed_batch(&self, queries: &[&str]) -> Result<Vec<Vec<f32>>> {
        if queries.is_empty() {
            return Ok(vec![]);
        }

        // Tokenize inputs
        let encodings = self.tokenizer
            .encode_batch(queries.to_vec(), true)
            .map_err(|e| AppError::InternalError(format!("Tokenization failed: {}", e)))?;

        let batch_size = encodings.len();
        let max_len = encodings.iter().map(|e| e.get_ids().len()).max().unwrap_or(0);

        // Prepare input tensors
        let mut input_ids = vec![0i64; batch_size * max_len];
        let mut attention_mask = vec![0i64; batch_size * max_len];
        let mut token_type_ids = vec![0i64; batch_size * max_len];

        for (i, encoding) in encodings.iter().enumerate() {
            let ids = encoding.get_ids();
            let mask = encoding.get_attention_mask();
            let type_ids = encoding.get_type_ids();

            for (j, (&id, &m)) in ids.iter().zip(mask.iter()).enumerate() {
                input_ids[i * max_len + j] = id as i64;
                attention_mask[i * max_len + j] = m as i64;
                if j < type_ids.len() {
                    token_type_ids[i * max_len + j] = type_ids[j] as i64;
                }
            }
        }

        // Create ndarray views
        let input_ids_array = Array2::from_shape_vec((batch_size, max_len), input_ids)
            .map_err(|e| AppError::InternalError(format!("Failed to create input_ids array: {}", e)))?;
        let attention_mask_array = Array2::from_shape_vec((batch_size, max_len), attention_mask)
            .map_err(|e| AppError::InternalError(format!("Failed to create attention_mask array: {}", e)))?;
        let token_type_ids_array = Array2::from_shape_vec((batch_size, max_len), token_type_ids)
            .map_err(|e| AppError::InternalError(format!("Failed to create token_type_ids array: {}", e)))?;

        // Run inference
        let outputs = self.session
            .run(ort::inputs![
                "input_ids" => input_ids_array.view(),
                "attention_mask" => attention_mask_array.view(),
                "token_type_ids" => token_type_ids_array.view(),
            ].map_err(|e| AppError::InternalError(format!("Failed to create inputs: {}", e)))?)
            .map_err(|e| AppError::InternalError(format!("Inference failed: {}", e)))?;

        // Extract embeddings (typically "last_hidden_state" or "sentence_embedding")
        let output = outputs.get("last_hidden_state")
            .or_else(|| outputs.get("sentence_embedding"))
            .ok_or_else(|| AppError::InternalError("No embedding output found".into()))?;

        let output_tensor = output
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::InternalError(format!("Failed to extract tensor: {}", e)))?;

        let output_view = output_tensor.view();
        let shape = output_view.shape();

        debug!(shape = ?shape, "Model output shape");

        // Mean pooling over sequence dimension
        let mut embeddings = Vec::with_capacity(batch_size);
        
        if shape.len() == 3 {
            // Shape: [batch, seq_len, hidden_dim]
            let hidden_dim = shape[2];
            
            for i in 0..batch_size {
                let seq_len = encodings[i].get_attention_mask()
                    .iter()
                    .filter(|&&m| m == 1)
                    .count();
                
                let mut embedding = vec![0.0f32; hidden_dim];
                for j in 0..seq_len {
                    for k in 0..hidden_dim {
                        embedding[k] += output_view[[i, j, k]];
                    }
                }
                for v in &mut embedding {
                    *v /= seq_len as f32;
                }
                
                // Normalize to unit vector
                let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for v in &mut embedding {
                        *v /= norm;
                    }
                }
                
                embeddings.push(embedding);
            }
        } else if shape.len() == 2 {
            // Shape: [batch, hidden_dim] - already pooled
            for i in 0..batch_size {
                let mut embedding: Vec<f32> = (0..shape[1])
                    .map(|j| output_view[[i, j]])
                    .collect();
                
                // Normalize
                let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for v in &mut embedding {
                        *v /= norm;
                    }
                }
                
                embeddings.push(embedding);
            }
        } else {
            return Err(AppError::InternalError(format!(
                "Unexpected output shape: {:?}", shape
            )));
        }

        Ok(embeddings)
    }

    /// Get the embedding dimension
    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }
}

/// Compute cosine similarity between two normalized vectors
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

/// Compute SHA256 hash of normalized query
pub fn query_hash(query: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let normalized = normalize_query(query);
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
