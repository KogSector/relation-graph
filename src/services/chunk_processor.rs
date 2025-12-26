//! Chunk processor service
//!
//! Processes incoming chunks, extracts entities, and stores in Neo4j with native vector embeddings.

use crate::config::Config;
use crate::error::{GraphError, GraphResult};
use crate::graph_db::Neo4jClient;
use crate::extractors::{CodeEntityExtractor, DocumentEntityExtractor};
use crate::models::{
    Chunk, ChunkInput,
    IngestChunksRequest, IngestChunksResponse,
    Entity, DataSource,
};
use crate::services::EmbeddingClient;
use std::sync::Arc;

/// Chunk processor for ingesting and processing chunks
pub struct ChunkProcessor {
    config: Config,
    neo4j: Option<Arc<Neo4jClient>>,
    code_extractor: CodeEntityExtractor,
    doc_extractor: DocumentEntityExtractor,
    embedding_client: EmbeddingClient,
}

impl ChunkProcessor {
    pub fn new(
        config: Config,
        neo4j: Option<Arc<Neo4jClient>>,
    ) -> Self {
        let embedding_client = EmbeddingClient::new(&config.embedding_service_url);
        
        Self {
            config,
            neo4j,
            code_extractor: CodeEntityExtractor::new(),
            doc_extractor: DocumentEntityExtractor::new(),
            embedding_client,
        }
    }
    
    /// Process and ingest chunks with embeddings stored directly in Neo4j
    pub async fn ingest_chunks(&self, request: IngestChunksRequest) -> GraphResult<IngestChunksResponse> {
        let mut chunks_ingested = 0;
        let mut entities_extracted = 0;
        let mut relationships_created = 0;
        let mut vectors_stored = 0;
        let mut errors = Vec::new();
        
        let extract_entities = request.extract_entities.unwrap_or(true);
        let create_cross_links = request.create_cross_links.unwrap_or(true);
        
        // Separate code and document chunks
        let mut code_chunks: Vec<(Chunk, Vec<f32>)> = Vec::new();
        let mut doc_chunks: Vec<(Chunk, Vec<f32>)> = Vec::new();
        
        for chunk_input in request.chunks {
            // Extract embedding before consuming chunk_input
            let input_embedding = chunk_input.embedding.clone();
            let chunk = chunk_input.into_chunk();
            
            // Get or generate embedding
            let embedding = if let Some(emb) = input_embedding {
                emb
            } else {
                match self.embedding_client.embed(&chunk.content).await {
                    Ok(emb) => emb,
                    Err(e) => {
                        errors.push(format!("Embedding failed for chunk {}: {}", chunk.id, e));
                        continue;
                    }
                }
            };
            
            // Store chunk in Neo4j with embedding (graph + vector in one place)
            if let Some(neo4j) = &self.neo4j {
                // Create chunk node with embedding
                match self.create_chunk_node_with_embedding(neo4j, &chunk, &embedding).await {
                    Ok(_) => {
                        vectors_stored += 1;
                        chunks_ingested += 1;
                    }
                    Err(e) => {
                        errors.push(format!("Chunk storage failed: {}", e));
                        continue;
                    }
                }
            }
            
            // Categorize chunks for cross-linking
            if chunk.source_kind == "code" {
                code_chunks.push((chunk, embedding));
            } else {
                doc_chunks.push((chunk, embedding));
            }
        }
        
        // Extract entities from chunks
        if extract_entities {
            // Process code chunks
            for (chunk, _embedding) in &code_chunks {
                let extraction = self.code_extractor.extract_with_relationships(
                    &chunk.content,
                    chunk.language.as_deref(),
                );
                
                for entity in extraction.entities {
                    if let Some(neo4j) = &self.neo4j {
                        let entity_obj = Entity::new(
                            entity.entity_type,
                            DataSource::from_str(&chunk.source_type).unwrap_or(DataSource::LocalFile),
                            format!("{}:{}", chunk.id, entity.name),
                            entity.name.clone(),
                            std::collections::HashMap::from([
                                ("chunk_id".to_string(), serde_json::json!(chunk.id.to_string())),
                                ("file_path".to_string(), serde_json::json!(chunk.file_path)),
                                ("confidence".to_string(), serde_json::json!(entity.confidence)),
                            ]),
                        );
                        
                        match neo4j.upsert_entity_node(&entity_obj).await {
                            Ok(_) => entities_extracted += 1,
                            Err(e) => errors.push(format!("Entity creation failed: {}", e)),
                        }
                    }
                }
                
                // Create relationships
                for rel in extraction.relationships {
                    if let Some(neo4j) = &self.neo4j {
                        match neo4j.create_relationship(
                            &rel.from_name,
                            &rel.to_name,
                            rel.relationship_type,
                            rel.confidence,
                            None,
                        ).await {
                            Ok(_) => relationships_created += 1,
                            Err(_) => {} // Silently skip relationship errors (entity may not exist)
                        }
                    }
                }
            }
            
            // Process document chunks
            for (chunk, _embedding) in &doc_chunks {
                let extraction = self.doc_extractor.extract_with_relationships(&chunk.content);
                
                for entity in extraction.entities {
                    if let Some(neo4j) = &self.neo4j {
                        let entity_obj = Entity::new(
                            entity.entity_type,
                            DataSource::from_str(&chunk.source_type).unwrap_or(DataSource::LocalFile),
                            format!("{}:{}", chunk.id, entity.name),
                            entity.name.clone(),
                            std::collections::HashMap::from([
                                ("chunk_id".to_string(), serde_json::json!(chunk.id.to_string())),
                                ("heading_path".to_string(), serde_json::json!(chunk.heading_path)),
                                ("confidence".to_string(), serde_json::json!(entity.confidence)),
                            ]),
                        );
                        
                        match neo4j.upsert_entity_node(&entity_obj).await {
                            Ok(_) => entities_extracted += 1,
                            Err(e) => errors.push(format!("Entity creation failed: {}", e)),
                        }
                    }
                }
            }
        }
        
        // Create cross-source links using Neo4j native vector search
        if create_cross_links && !code_chunks.is_empty() && !doc_chunks.is_empty() {
            if let Some(neo4j) = &self.neo4j {
                let links_created = self.create_cross_source_links(
                    neo4j, 
                    &code_chunks, 
                    &doc_chunks
                ).await;
                relationships_created += links_created;
            }
        }
        
        Ok(IngestChunksResponse {
            chunks_ingested,
            entities_extracted,
            relationships_created,
            vectors_stored,
            errors,
        })
    }
    
    /// Create a chunk node in Neo4j with its embedding
    async fn create_chunk_node_with_embedding(
        &self,
        neo4j: &Neo4jClient,
        chunk: &Chunk,
        embedding: &[f32],
    ) -> GraphResult<()> {
        // First create the chunk as an entity node
        let chunk_entity = Entity::new(
            crate::models::EntityType::CodeEntity, // Use CodeEntity as generic chunk type
            DataSource::from_str(&chunk.source_type).unwrap_or(DataSource::LocalFile),
            chunk.id.to_string(),
            chunk.file_path.clone().unwrap_or_else(|| "unknown".to_string()),
            std::collections::HashMap::from([
                ("content".to_string(), serde_json::json!(chunk.content)),
                ("source_kind".to_string(), serde_json::json!(chunk.source_kind)),
                ("source_type".to_string(), serde_json::json!(chunk.source_type)),
                ("file_path".to_string(), serde_json::json!(chunk.file_path)),
                ("language".to_string(), serde_json::json!(chunk.language)),
                ("author".to_string(), serde_json::json!(chunk.author)),
                ("owner_id".to_string(), serde_json::json!(chunk.owner_id)),
            ]),
        );
        
        // Create node
        neo4j.upsert_entity_node(&chunk_entity).await?;
        
        // Set embedding on the node
        neo4j.set_node_embedding(
            &chunk.id.to_string(),
            embedding.to_vec(),
            "sentence-transformers-384",  // Default model
            "embeddings-service",
        ).await?;
        
        Ok(())
    }
    
    /// Create cross-source links using Neo4j vector similarity
    async fn create_cross_source_links(
        &self,
        neo4j: &Neo4jClient,
        code_chunks: &[(Chunk, Vec<f32>)],
        doc_chunks: &[(Chunk, Vec<f32>)],
    ) -> usize {
        let mut links_created = 0;
        
        // For each document chunk, find similar code chunks
        for (doc_chunk, _) in doc_chunks {
            match neo4j.find_similar_chunks_for_linking(
                &doc_chunk.id.to_string(),
                "code",
                self.config.max_cross_links_per_chunk,
                self.config.similarity_threshold,
            ).await {
                Ok(matches) => {
                    for m in matches {
                        if let Ok(_) = neo4j.create_cross_source_link(
                            &doc_chunk.id.to_string(),
                            &m.target_id,
                            m.confidence,
                            m.similarity_score,
                            m.has_explicit_mention,
                            m.has_author_overlap,
                        ).await {
                            links_created += 1;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Cross-source linking failed for chunk {}: {}", doc_chunk.id, e);
                }
            }
        }
        
        // For each code chunk, find similar document chunks
        for (code_chunk, _) in code_chunks {
            match neo4j.find_similar_chunks_for_linking(
                &code_chunk.id.to_string(),
                "document",
                self.config.max_cross_links_per_chunk,
                self.config.similarity_threshold,
            ).await {
                Ok(matches) => {
                    for m in matches {
                        if let Ok(_) = neo4j.create_cross_source_link(
                            &code_chunk.id.to_string(),
                            &m.target_id,
                            m.confidence,
                            m.similarity_score,
                            m.has_explicit_mention,
                            m.has_author_overlap,
                        ).await {
                            links_created += 1;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Cross-source linking failed for chunk {}: {}", code_chunk.id, e);
                }
            }
        }
        
        links_created
    }
}
