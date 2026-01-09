//! Embedding background task - processes queries and generates embeddings

use crate::db::Database;
use crate::services::embedding::EmbeddingService;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Background task that embeds queries that haven't been processed yet.
///
/// Runs every 30 seconds, fetches unembedded queries, generates embeddings,
/// and stores them in the database for similarity search.
pub async fn embedding_task(db: Arc<Database>, embedding_service: Option<Arc<EmbeddingService>>) {
    let service = match embedding_service {
        Some(s) => s,
        None => {
            warn!("Embedding service not configured, embedding task disabled");
            return;
        }
    };

    let mut interval = tokio::time::interval(Duration::from_secs(30));

    info!("Embedding task started (30s interval)");

    loop {
        interval.tick().await;

        // Get all workspaces
        let workspaces = match db.get_all_workspace_ids().await {
            Ok(w) => w,
            Err(e) => {
                error!(error = %e, "Failed to get workspaces for embedding");
                continue;
            }
        };

        for workspace_id in workspaces {
            // Get unembedded queries for this workspace
            let queries = match db.get_unembedded_queries(workspace_id, 100).await {
                Ok(q) => q,
                Err(e) => {
                    error!(error = %e, workspace_id = %workspace_id, "Failed to get unembedded queries");
                    continue;
                }
            };

            if queries.is_empty() {
                continue;
            }

            debug!(
                workspace_id = %workspace_id,
                count = queries.len(),
                "Processing unembedded queries"
            );

            // Embed each query
            for (query_text, query_hash) in queries {
                match service.embed_query(&query_text) {
                    Ok(embedding) => {
                        if let Err(e) = db
                            .insert_query_embedding(
                                workspace_id,
                                &query_hash,
                                &query_text,
                                &embedding,
                            )
                            .await
                        {
                            error!(error = %e, query_hash = %query_hash, "Failed to store embedding");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to embed query");
                    }
                }
            }
        }
    }
}
