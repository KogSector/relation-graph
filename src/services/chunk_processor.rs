//! Chunk processor service
//!
//! Processes incoming chunks, extracts entities, and stores in graph + vector DBs.

use crate::config::Config;
use crate::error::{GraphError, GraphResult};
use crate::graph_db::Neo4jClient;
use crate::vector_db::ZillizClient;
use crate::extractors::{CodeEntityExtractor, DocumentEntityExtractor};
use crate::models::{
    Chunk, ChunkInput, ChunkWithEmbedding, ChunkVectorMetadata,
    IngestChunksRequest, IngestChunksResponse,
    Entity, EntityType, DataSource, RelationshipType,
};
use crate::services::{EmbeddingClient, CrossSourceLinker};
use std::sync::Arc;
use uuid::Uuid;

/// Chunk processor for ingesting and processing chunks
pub struct ChunkProcessor {
    config: Config,
    neo4j: Option<Arc<Neo4jClient>>,
    zilliz: Option<Arc<ZillizClient>>,
    code_extractor: CodeEntityExtractor,
    doc_extractor: DocumentEntityExtractor,
    embedding_client: EmbeddingClient,
    cross_source_linker: CrossSourceLinker,
}

impl ChunkProcessor {
    pub fn new(
        config: Config,
        neo4j: Option<Arc<Neo4jClient>>,
        zilliz: Option<Arc<ZillizClient>>,
    ) -> Self {
        let embedding_client = EmbeddingClient::new(&config.embedding_service_url);
        let cross_source_linker = CrossSourceLinker::new(
            config.clone(),
            neo4j.clone(),
            zilliz.clone(),
        );
        
        Self {
            config,
            neo4j,
            zilliz,
            code_extractor: CodeEntityExtractor::new(),
            doc_extractor: DocumentEntityExtractor::new(),
            embedding_client,
            cross_source_linker,
        }
    }
    
    /// Process and ingest chunks from the chunker service
    pub async fn ingest_chunks(&self, request: IngestChunksRequest) -> GraphResult<IngestChunksResponse> {
        let mut chunks_ingested = 0;
        let mut entities_extracted = 0;
        let mut relationships_created = 0;
        let mut vectors_stored = 0;
        let mut errors = Vec::new();
        
        let extract_entities = request.extract_entities.unwrap_or(true);
        let create_cross_links = request.create_cross_links.unwrap_or(true);
        
        // Separate code and document chunks
        let mut code_chunks = Vec::new();
        let mut doc_chunks = Vec::new();
        let mut code_embeddings = Vec::new();
        let mut doc_embeddings = Vec::new();
        
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
            
            // Store in vector DB
            if let Some(zilliz) = &self.zilliz {
                let metadata = ChunkVectorMetadata {
                    chunk_id: chunk.id.to_string(),
                    source_kind: chunk.source_kind.clone(),
                    source_type: chunk.source_type.clone(),
                    file_path: chunk.file_path.clone(),
                    repo_name: chunk.repo_name.clone(),
                    language: chunk.language.clone(),
                    heading_path: chunk.heading_path.clone(),
                    owner_id: chunk.owner_id.clone(),
                    author: chunk.author.clone(),
                    created_at: chunk.created_at.timestamp(),
                };
                
                match zilliz.insert_vectors(vec![(chunk.id, embedding.clone(), metadata)]).await {
                    Ok(count) => vectors_stored += count,
                    Err(e) => errors.push(format!("Vector insert failed: {}", e)),
                }
            }
            
            // Categorize chunks
            if chunk.source_kind == "code" {
                code_embeddings.push((chunk.id, embedding));
                code_chunks.push(chunk);
            } else {
                doc_embeddings.push((chunk.id, embedding));
                doc_chunks.push(chunk);
            }
            
            chunks_ingested += 1;
        }
        
        // Extract entities from chunks
        if extract_entities {
            // Process code chunks
            for chunk in &code_chunks {
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
            for chunk in &doc_chunks {
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
        
        // Create cross-source links
        if create_cross_links && !code_chunks.is_empty() && !doc_chunks.is_empty() {
            match self.cross_source_linker.link_chunks(
                &code_chunks,
                &doc_chunks,
                &code_embeddings,
                &doc_embeddings,
            ).await {
                Ok(result) => {
                    relationships_created += result.links_created;
                    errors.extend(result.errors);
                }
                Err(e) => errors.push(format!("Cross-source linking failed: {}", e)),
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
}
