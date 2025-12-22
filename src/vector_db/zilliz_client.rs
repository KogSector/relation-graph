//! Zilliz Cloud client for vector operations
//!
//! Provides vector storage and similarity search using Zilliz Cloud (Milvus-compatible).

use crate::error::{GraphError, GraphResult};
use crate::models::{ChunkVectorMetadata, ChunkResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Zilliz Cloud client
pub struct ZillizClient {
    client: Client,
    endpoint: String,
    api_key: String,
    collection: String,
}

#[derive(Debug, Serialize)]
struct InsertRequest {
    #[serde(rename = "collectionName")]
    collection_name: String,
    data: Vec<VectorData>,
}

#[derive(Debug, Serialize)]
struct VectorData {
    id: String,
    vector: Vec<f32>,
    chunk_id: String,
    source_kind: String,
    source_type: String,
    file_path: String,
    repo_name: String,
    language: String,
    heading_path: String,
    owner_id: String,
    author: String,
    created_at: i64,
}

#[derive(Debug, Serialize)]
struct SearchRequest {
    #[serde(rename = "collectionName")]
    collection_name: String,
    vector: Vec<f32>,
    limit: usize,
    #[serde(rename = "outputFields")]
    output_fields: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    code: i32,
    data: Option<T>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResult {
    id: String,
    distance: f32,
    chunk_id: Option<String>,
    source_kind: Option<String>,
    source_type: Option<String>,
    file_path: Option<String>,
    repo_name: Option<String>,
    language: Option<String>,
    heading_path: Option<String>,
}

impl ZillizClient {
    /// Create a new Zilliz client
    pub async fn new(endpoint: &str, api_key: &str, collection: &str) -> GraphResult<Self> {
        let client = Client::builder()
            .build()
            .map_err(|e| GraphError::Zilliz(format!("Failed to create HTTP client: {}", e)))?;
        
        let zilliz = Self {
            client,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            collection: collection.to_string(),
        };
        
        // Test connection by checking collection
        zilliz.health_check().await?;
        
        Ok(zilliz)
    }
    
    /// Health check - verify connection
    async fn health_check(&self) -> GraphResult<()> {
        let url = format!("{}/v2/vectordb/collections/describe", self.endpoint);
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "collectionName": self.collection
            }))
            .send()
            .await
            .map_err(|e| GraphError::Zilliz(format!("Health check failed: {}", e)))?;
        
        if response.status().is_success() {
            tracing::info!("✅ Zilliz collection '{}' accessible", self.collection);
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(GraphError::Zilliz(format!("Health check failed: {} - {}", status, body)))
        }
    }
    
    /// Insert vectors with metadata
    pub async fn insert_vectors(
        &self,
        vectors: Vec<(Uuid, Vec<f32>, ChunkVectorMetadata)>,
    ) -> GraphResult<usize> {
        if vectors.is_empty() {
            return Ok(0);
        }
        
        let url = format!("{}/v2/vectordb/entities/insert", self.endpoint);
        
        let data: Vec<VectorData> = vectors
            .into_iter()
            .map(|(id, vector, meta)| VectorData {
                id: id.to_string(),
                vector,
                chunk_id: meta.chunk_id,
                source_kind: meta.source_kind,
                source_type: meta.source_type,
                file_path: meta.file_path.unwrap_or_default(),
                repo_name: meta.repo_name.unwrap_or_default(),
                language: meta.language.unwrap_or_default(),
                heading_path: meta.heading_path.unwrap_or_default(),
                owner_id: meta.owner_id,
                author: meta.author.unwrap_or_default(),
                created_at: meta.created_at,
            })
            .collect();
        
        let count = data.len();
        
        let request = InsertRequest {
            collection_name: self.collection.clone(),
            data,
        };
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| GraphError::Zilliz(format!("Insert failed: {}", e)))?;
        
        if response.status().is_success() {
            Ok(count)
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(GraphError::Zilliz(format!("Insert failed: {}", body)))
        }
    }
    
    /// Search for similar vectors
    pub async fn search(
        &self,
        query_vector: Vec<f32>,
        limit: usize,
        source_kind_filter: Option<&str>,
        source_type_filter: Option<&[String]>,
        owner_id_filter: Option<&str>,
    ) -> GraphResult<Vec<(Uuid, f32, ChunkVectorMetadata)>> {
        let url = format!("{}/v2/vectordb/entities/search", self.endpoint);
        
        // Build filter expression
        let mut filters = Vec::new();
        
        if let Some(kind) = source_kind_filter {
            if kind != "all" {
                filters.push(format!("source_kind == \"{}\"", kind));
            }
        }
        
        if let Some(types) = source_type_filter {
            if !types.is_empty() {
                let type_list: Vec<String> = types.iter().map(|t| format!("\"{}\"", t)).collect();
                filters.push(format!("source_type in [{}]", type_list.join(",")));
            }
        }
        
        if let Some(owner) = owner_id_filter {
            filters.push(format!("owner_id == \"{}\"", owner));
        }
        
        let filter = if filters.is_empty() {
            None
        } else {
            Some(filters.join(" && "))
        };
        
        let request = SearchRequest {
            collection_name: self.collection.clone(),
            vector: query_vector,
            limit,
            output_fields: vec![
                "chunk_id".to_string(),
                "source_kind".to_string(),
                "source_type".to_string(),
                "file_path".to_string(),
                "repo_name".to_string(),
                "language".to_string(),
                "heading_path".to_string(),
                "owner_id".to_string(),
                "author".to_string(),
                "created_at".to_string(),
            ],
            filter,
        };
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| GraphError::Zilliz(format!("Search failed: {}", e)))?;
        
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(GraphError::Zilliz(format!("Search failed: {}", body)));
        }
        
        let api_response: ApiResponse<Vec<SearchResult>> = response
            .json()
            .await
            .map_err(|e| GraphError::Zilliz(format!("Failed to parse response: {}", e)))?;
        
        let results = api_response.data.unwrap_or_default();
        
        let mut output = Vec::new();
        for result in results {
            let chunk_id = result.chunk_id
                .and_then(|s| Uuid::parse_str(&s).ok())
                .unwrap_or_else(|| Uuid::parse_str(&result.id).unwrap_or_else(|_| Uuid::new_v4()));
            
            let similarity = 1.0 - result.distance; // Convert distance to similarity
            
            let metadata = ChunkVectorMetadata {
                chunk_id: chunk_id.to_string(),
                source_kind: result.source_kind.unwrap_or_default(),
                source_type: result.source_type.unwrap_or_default(),
                file_path: result.file_path,
                repo_name: result.repo_name,
                language: result.language,
                heading_path: result.heading_path,
                owner_id: String::new(),
                author: None,
                created_at: 0,
            };
            
            output.push((chunk_id, similarity, metadata));
        }
        
        Ok(output)
    }
    
    /// Search for vectors similar to another vector (for cross-source linking)
    pub async fn find_similar_chunks(
        &self,
        source_vector: Vec<f32>,
        target_source_kind: &str,
        limit: usize,
        min_similarity: f32,
    ) -> GraphResult<Vec<(Uuid, f32)>> {
        let results = self.search(
            source_vector,
            limit * 2, // Request more to filter by similarity
            Some(target_source_kind),
            None,
            None,
        ).await?;
        
        Ok(results
            .into_iter()
            .filter(|(_, sim, _)| *sim >= min_similarity)
            .take(limit)
            .map(|(id, sim, _)| (id, sim))
            .collect())
    }
    
    /// Delete vectors by IDs
    pub async fn delete_vectors(&self, ids: &[Uuid]) -> GraphResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        
        let url = format!("{}/v2/vectordb/entities/delete", self.endpoint);
        
        let id_strings: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "collectionName": self.collection,
                "filter": format!("id in {:?}", id_strings)
            }))
            .send()
            .await
            .map_err(|e| GraphError::Zilliz(format!("Delete failed: {}", e)))?;
        
        if response.status().is_success() {
            Ok(ids.len())
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(GraphError::Zilliz(format!("Delete failed: {}", body)))
        }
    }
    
    /// Get collection statistics
    pub async fn get_statistics(&self) -> GraphResult<serde_json::Value> {
        let url = format!("{}/v2/vectordb/collections/get_stats", self.endpoint);
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "collectionName": self.collection
            }))
            .send()
            .await
            .map_err(|e| GraphError::Zilliz(format!("Stats failed: {}", e)))?;
        
        if response.status().is_success() {
            let data: serde_json::Value = response.json().await.unwrap_or(serde_json::json!({}));
            Ok(serde_json::json!({
                "connected": true,
                "collection": self.collection,
                "stats": data
            }))
        } else {
            Ok(serde_json::json!({
                "connected": false,
                "collection": self.collection
            }))
        }
    }
}
