//! Hybrid query engine
//!
//! Combines vector search with graph traversal using Neo4j native vector indexes.

use crate::config::Config;
use crate::error::{GraphError, GraphResult};
use crate::graph_db::Neo4jClient;
use crate::models::{
    HybridSearchRequest, HybridSearchResponse, SearchOptions, SearchMetadata,
    ChunkResult, EntityResult, RelationshipResult, SemanticLink,
    VectorSearchRequest, VectorSearchResponse,
    GraphSearchRequest, GraphSearchResponse,
    RelationshipType,
};
use crate::services::EmbeddingClient;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

/// Hybrid query engine combining vector and graph search
/// 
/// Now uses Neo4j native vector indexes instead of separate Zilliz database.
pub struct HybridQueryEngine {
    config: Config,
    neo4j: Option<Arc<Neo4jClient>>,
    embedding_client: EmbeddingClient,
}

impl HybridQueryEngine {
    pub fn new(
        config: Config,
        neo4j: Option<Arc<Neo4jClient>>,
    ) -> Self {
        let embedding_client = EmbeddingClient::new(&config.embedding_service_url);
        Self {
            config,
            neo4j,
            embedding_client,
        }
    }
    
    /// Execute a hybrid search combining vector and graph results
    pub async fn search(&self, request: HybridSearchRequest) -> GraphResult<HybridSearchResponse> {
        let start_time = Instant::now();
        let options = request.options;
        
        // Step 1: Embed the query
        let query_embedding = self.embedding_client
            .embed(&request.query)
            .await
            .map_err(|e| GraphError::Embedding(e.to_string()))?;
        
        // Step 2: Vector search using Neo4j native vector index
        let vector_results = self.vector_search_internal(
            query_embedding.clone(),
            &options,
        ).await?;
        
        // Step 3: Graph expansion for each vector hit
        let mut related_entities = Vec::new();
        let mut relationships = Vec::new();
        let mut cross_source_links = Vec::new();
        
        if let Some(neo4j) = &self.neo4j {
            for chunk in &vector_results {
                // Expand via graph traversal
                let (entities, rels) = self.graph_expand(
                    &chunk.chunk_id.to_string(),
                    options.graph_hops,
                    neo4j,
                ).await?;
                
                related_entities.extend(entities);
                relationships.extend(rels);
                
                // Get cross-source links if enabled
                if options.include_cross_source {
                    let cross_links = neo4j
                        .get_cross_source_relationships(&chunk.chunk_id.to_string())
                        .await?;
                    
                    for (target_id, target_name, rel_type, confidence) in cross_links {
                        cross_source_links.push(SemanticLink {
                            from_chunk_id: chunk.chunk_id,
                            to_chunk_id: Uuid::parse_str(&target_id).unwrap_or_else(|_| Uuid::new_v4()),
                            relationship_type: rel_type,
                            confidence,
                            extraction_methods: vec!["neo4j_vector_similarity".to_string()],
                            similarity_score: Some(chunk.similarity_score),
                            explicit_mention: None,
                            temporal_distance_days: None,
                            author_overlap: false,
                        });
                    }
                }
            }
        }
        
        // Deduplicate entities and relationships
        related_entities.sort_by(|a, b| a.id.cmp(&b.id));
        related_entities.dedup_by(|a, b| a.id == b.id);
        
        relationships.sort_by(|a, b| {
            (&a.from_id, &a.to_id, &a.relationship_type)
                .cmp(&(&b.from_id, &b.to_id, &b.relationship_type))
        });
        relationships.dedup_by(|a, b| {
            a.from_id == b.from_id && a.to_id == b.to_id && a.relationship_type == b.relationship_type
        });
        
        let execution_time = start_time.elapsed().as_millis() as u64;
        let cross_source_links_count = cross_source_links.len();
        
        Ok(HybridSearchResponse {
            chunks: vector_results.clone(),
            related_entities,
            relationships,
            cross_source_links,
            metadata: SearchMetadata {
                query: request.query,
                vector_results_count: vector_results.len(),
                graph_entities_count: 0,
                graph_hops_performed: options.graph_hops,
                cross_source_links_count,
                execution_time_ms: execution_time,
            },
        })
    }
    
    /// Vector-only search using Neo4j native vector index
    pub async fn vector_search(&self, request: VectorSearchRequest) -> GraphResult<VectorSearchResponse> {
        let query_embedding = self.embedding_client
            .embed(&request.query)
            .await
            .map_err(|e| GraphError::Embedding(e.to_string()))?;
        
        let options = SearchOptions {
            limit: request.limit,
            source_kind: request.source_kind.unwrap_or_else(|| "all".to_string()),
            source_types: request.source_types,
            owner_id: request.owner_id,
            ..Default::default()
        };
        
        let results = self.vector_search_internal(query_embedding, &options).await?;
        
        Ok(VectorSearchResponse {
            results: results.clone(),
            total_count: results.len(),
        })
    }
    
    /// Internal vector search using Neo4j native vector index
    async fn vector_search_internal(
        &self,
        query_embedding: Vec<f32>,
        options: &SearchOptions,
    ) -> GraphResult<Vec<ChunkResult>> {
        let neo4j = self.neo4j.as_ref()
            .ok_or_else(|| GraphError::ServiceUnavailable("Neo4j not available for vector search".to_string()))?;
        
        // Use the chunk embedding index
        let similar_nodes = neo4j.find_similar_nodes(
            query_embedding,
            "chunk_embedding_idx",
            options.limit,
            options.min_similarity,
        ).await?;
        
        // Convert to ChunkResult (basic info from vector search)
        Ok(similar_nodes
            .into_iter()
            .map(|(id, score)| ChunkResult {
                chunk_id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                content: String::new(), // Would need additional query to get content
                source_kind: options.source_kind.clone(),
                source_type: String::new(),
                file_path: None,
                repo_name: None,
                language: None,
                heading_path: None,
                similarity_score: score,
            })
            .collect())
    }
    
    /// Graph expansion from a starting entity
    async fn graph_expand(
        &self,
        entity_id: &str,
        hops: usize,
        neo4j: &Neo4jClient,
    ) -> GraphResult<(Vec<EntityResult>, Vec<RelationshipResult>)> {
        let neighbors = neo4j.get_neighbors(
            entity_id,
            None, // All relationship types
            "both",
            hops,
        ).await?;
        
        let entities: Vec<EntityResult> = neighbors
            .iter()
            .map(|(id, name, _rel_type, _conf)| EntityResult {
                id: Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4()),
                entity_type: "unknown".to_string(),
                name: name.clone(),
                source: "graph".to_string(),
                properties: serde_json::json!({}),
            })
            .collect();
        
        let relationships: Vec<RelationshipResult> = neighbors
            .iter()
            .map(|(id, name, rel_type, conf)| {
                let is_cross_source = RelationshipType::from_str(rel_type)
                    .map(|rt| rt.is_cross_source())
                    .unwrap_or(false);
                
                RelationshipResult {
                    from_id: Uuid::parse_str(entity_id).unwrap_or_else(|_| Uuid::new_v4()),
                    to_id: Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4()),
                    from_name: "source".to_string(),
                    to_name: name.clone(),
                    relationship_type: rel_type.clone(),
                    confidence: *conf,
                    is_cross_source,
                }
            })
            .collect();
        
        Ok((entities, relationships))
    }
    
    /// Graph-only search
    pub async fn graph_search(&self, request: GraphSearchRequest) -> GraphResult<GraphSearchResponse> {
        let neo4j = self.neo4j.as_ref()
            .ok_or_else(|| GraphError::ServiceUnavailable("Neo4j not available".to_string()))?;
        
        let mut all_entities = Vec::new();
        let mut all_relationships = Vec::new();
        let all_paths = Vec::new();
        
        for start_entity in &request.start_entities {
            let neighbors = neo4j.get_neighbors(
                start_entity,
                None,
                &request.direction,
                request.hops,
            ).await?;
            
            for (id, name, rel_type, conf) in neighbors {
                all_entities.push(EntityResult {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    entity_type: "unknown".to_string(),
                    name: name.clone(),
                    source: "graph".to_string(),
                    properties: serde_json::json!({}),
                });
                
                let is_cross_source = RelationshipType::from_str(&rel_type)
                    .map(|rt| rt.is_cross_source())
                    .unwrap_or(false);
                
                all_relationships.push(RelationshipResult {
                    from_id: Uuid::parse_str(start_entity).unwrap_or_else(|_| Uuid::new_v4()),
                    to_id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    from_name: start_entity.clone(),
                    to_name: name,
                    relationship_type: rel_type,
                    confidence: conf,
                    is_cross_source,
                });
            }
        }
        
        // Deduplicate
        all_entities.sort_by(|a, b| a.id.cmp(&b.id));
        all_entities.dedup_by(|a, b| a.id == b.id);
        all_entities.truncate(request.limit);
        
        Ok(GraphSearchResponse {
            entities: all_entities,
            relationships: all_relationships,
            paths: all_paths,
        })
    }
}
