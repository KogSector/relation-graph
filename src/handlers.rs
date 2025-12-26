//! HTTP handlers module

use axum::{
    extract::{Path, State},
    Json,
};
use sqlx::PgPool;
use std::sync::Arc;

use crate::config::Config;
use crate::error::GraphError;
use crate::graph_db::Neo4jClient;
use crate::models::*;
use crate::services::{ChunkProcessor, HybridQueryEngine};

/// Application state shared across handlers
pub struct AppState {
    pub config: Config,
    pub neo4j: Option<Arc<Neo4jClient>>,
    pub db_pool: PgPool,
}

/// Health check endpoint
pub async fn health_check(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let neo4j_status = state.neo4j.is_some();
    
    Json(serde_json::json!({
        "status": "healthy",
        "service": "relation-graph",
        "version": env!("CARGO_PKG_VERSION"),
        "components": {
            "neo4j": neo4j_status,
            "postgres": true,
            "vector_store": "neo4j-native"  // Vector storage now in Neo4j
        },
        "features": {
            "hybrid_search": true,
            "cross_source_linking": true,
            "code_entity_extraction": true,
            "document_entity_extraction": true,
            "native_vector_search": neo4j_status  // Neo4j 5.21+ vector indexes
        }
    }))
}

/// Create an entity
pub async fn create_entity(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateEntityRequest>,
) -> Result<Json<CreateEntityResponse>, GraphError> {
    let entity_type = EntityType::from_str(&request.entity_type)
        .ok_or_else(|| GraphError::InvalidEntityType(request.entity_type.clone()))?;
    
    let source = DataSource::from_str(&request.source)
        .unwrap_or(DataSource::LocalFile);
    
    let entity = Entity::new(
        entity_type,
        source,
        request.source_id,
        request.name,
        request.properties,
    );
    
    let mut neo4j_node_id = None;
    
    if let Some(neo4j) = &state.neo4j {
        neo4j_node_id = Some(neo4j.upsert_entity_node(&entity).await?);
    }
    
    Ok(Json(CreateEntityResponse {
        entity_id: entity.id,
        neo4j_node_id,
        canonical_id: None,
        resolved: false,
    }))
}

/// Get an entity by ID
pub async fn get_entity(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GraphError> {
    // Try to get from Neo4j
    if let Some(neo4j) = &state.neo4j {
        let entities = neo4j.find_entities(None, None, 1).await?;
        for (entity_id, name, entity_type) in entities {
            if entity_id == id {
                return Ok(Json(serde_json::json!({
                    "id": entity_id,
                    "name": name,
                    "entity_type": entity_type
                })));
            }
        }
    }
    
    Err(GraphError::EntityNotFound(id))
}

/// Get neighbors of an entity
pub async fn get_neighbors(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GraphError> {
    let neo4j = state.neo4j.as_ref()
        .ok_or_else(|| GraphError::ServiceUnavailable("Neo4j not available".to_string()))?;
    
    let neighbors = neo4j.get_neighbors(&id, None, "both", 1).await?;
    
    Ok(Json(serde_json::json!({
        "entity_id": id,
        "neighbors": neighbors.iter().map(|(id, name, rel, conf)| {
            serde_json::json!({
                "id": id,
                "name": name,
                "relationship": rel,
                "confidence": conf
            })
        }).collect::<Vec<_>>()
    })))
}

/// Ingest chunks from the chunker service
pub async fn ingest_chunks(
    State(state): State<Arc<AppState>>,
    Json(request): Json<IngestChunksRequest>,
) -> Result<Json<IngestChunksResponse>, GraphError> {
    let processor = ChunkProcessor::new(
        state.config.clone(),
        state.neo4j.clone(),
    );
    
    let response = processor.ingest_chunks(request).await?;
    
    Ok(Json(response))
}

/// Trigger cross-source linking
pub async fn trigger_cross_source_linking(
    State(state): State<Arc<AppState>>,
    Json(_request): Json<CrossSourceLinkRequest>,
) -> Result<Json<CrossSourceLinkResponse>, GraphError> {
    // Use Neo4j native vector search for cross-source linking
    let neo4j = state.neo4j.as_ref()
        .ok_or_else(|| GraphError::ServiceUnavailable("Neo4j not available for cross-source linking".to_string()))?;
    
    // Get statistics to show the linking capability
    let stats = neo4j.get_statistics().await?;
    
    Ok(Json(CrossSourceLinkResponse {
        links_created: 0,
        chunks_processed: stats["node_count"].as_i64().unwrap_or(0) as usize,
        errors: vec!["Cross-source linking now uses Neo4j native vector search. Use ingest_chunks with create_cross_links=true".to_string()],
    }))
}

/// Hybrid search (main query API)
pub async fn hybrid_search(
    State(state): State<Arc<AppState>>,
    Json(request): Json<HybridSearchRequest>,
) -> Result<Json<HybridSearchResponse>, GraphError> {
    let engine = HybridQueryEngine::new(
        state.config.clone(),
        state.neo4j.clone(),
    );
    
    let response = engine.search(request).await?;
    
    Ok(Json(response))
}

/// Vector-only search
pub async fn vector_search(
    State(state): State<Arc<AppState>>,
    Json(request): Json<VectorSearchRequest>,
) -> Result<Json<VectorSearchResponse>, GraphError> {
    let engine = HybridQueryEngine::new(
        state.config.clone(),
        state.neo4j.clone(),
    );
    
    let response = engine.vector_search(request).await?;
    
    Ok(Json(response))
}

/// Graph-only search
pub async fn graph_search(
    State(state): State<Arc<AppState>>,
    Json(request): Json<GraphSearchRequest>,
) -> Result<Json<GraphSearchResponse>, GraphError> {
    let engine = HybridQueryEngine::new(
        state.config.clone(),
        state.neo4j.clone(),
    );
    
    let response = engine.graph_search(request).await?;
    
    Ok(Json(response))
}

/// Get graph statistics
pub async fn get_statistics(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, GraphError> {
    let mut stats = serde_json::json!({
        "service": "relation-graph",
        "vector_store": "neo4j-native"
    });
    
    if let Some(neo4j) = &state.neo4j {
        let graph_stats = neo4j.get_statistics().await?;
        stats["graph"] = graph_stats;
        // Vector stats now included in Neo4j since vectors are stored there
        stats["vector"] = serde_json::json!({
            "store": "neo4j-native",
            "dimension": 384,
            "indexes": ["chunk_embedding_idx", "function_embedding_idx", "class_embedding_idx", "document_embedding_idx"]
        });
    }
    
    Ok(Json(stats))
}
