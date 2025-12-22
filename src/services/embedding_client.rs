//! Embedding client for calling the embeddings service

use crate::error::{GraphError, GraphResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Client for the embeddings microservice
pub struct EmbeddingClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct EmbedRequest {
    text: String,
}

#[derive(Debug, Serialize)]
struct BatchEmbedRequest {
    texts: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct BatchEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl EmbeddingClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
    
    /// Embed a single text
    pub async fn embed(&self, text: &str) -> GraphResult<Vec<f32>> {
        let url = format!("{}/embed", self.base_url);
        
        let response = self.client
            .post(&url)
            .json(&EmbedRequest { text: text.to_string() })
            .send()
            .await
            .map_err(|e| GraphError::Embedding(format!("Request failed: {}", e)))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(GraphError::Embedding(format!("Embed failed: {} - {}", status, body)));
        }
        
        let result: EmbedResponse = response
            .json()
            .await
            .map_err(|e| GraphError::Embedding(format!("Parse failed: {}", e)))?;
        
        Ok(result.embedding)
    }
    
    /// Embed multiple texts in a batch
    pub async fn embed_batch(&self, texts: Vec<String>) -> GraphResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        
        let url = format!("{}/batch/embed", self.base_url);
        
        let response = self.client
            .post(&url)
            .json(&BatchEmbedRequest { texts })
            .send()
            .await
            .map_err(|e| GraphError::Embedding(format!("Batch request failed: {}", e)))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(GraphError::Embedding(format!("Batch embed failed: {} - {}", status, body)));
        }
        
        let result: BatchEmbedResponse = response
            .json()
            .await
            .map_err(|e| GraphError::Embedding(format!("Parse failed: {}", e)))?;
        
        Ok(result.embeddings)
    }
    
    /// Health check
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/health", self.base_url);
        
        match self.client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}
