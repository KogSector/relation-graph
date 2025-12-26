//! Search request and response models

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Entity, Relationship, Chunk, SemanticLink};

/// Options for hybrid search
#[derive(Debug, Deserialize)]
pub struct SearchOptions {
    /// Maximum number of vector results
    #[serde(default = "default_limit")]
    pub limit: usize,
    
    /// Number of hops for graph expansion
    #[serde(default = "default_hops")]
    pub graph_hops: usize,
    
    /// Filter by source kind: "code", "document", or "all"
    #[serde(default = "default_source_kind_filter")]
    pub source_kind: String,
    
    /// Filter by specific source types (e.g., ["github", "dropbox"])
    pub source_types: Option<Vec<String>>,
    
    /// Filter by repository name
    pub repo_filter: Option<String>,
    
    /// Filter by owner ID
    pub owner_id: Option<String>,
    
    /// Include cross-source relationships in results
    #[serde(default = "default_true")]
    pub include_cross_source: bool,
    
    /// Minimum similarity threshold for vector results
    #[serde(default = "default_threshold")]
    pub min_similarity: f32,
}

fn default_limit() -> usize { 10 }
fn default_hops() -> usize { 2 }
fn default_source_kind_filter() -> String { "all".to_string() }
fn default_true() -> bool { true }
fn default_threshold() -> f32 { 0.0 }

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 10,
            graph_hops: 2,
            source_kind: "all".to_string(),
            source_types: None,
            repo_filter: None,
            owner_id: None,
            include_cross_source: true,
            min_similarity: 0.0,
        }
    }
}

/// Hybrid search request
#[derive(Debug, Deserialize)]
pub struct HybridSearchRequest {
    /// The search query
    pub query: String,
    
    /// Search options
    #[serde(flatten)]
    pub options: SearchOptions,
}

/// A single search result with chunk and score
#[derive(Debug, Clone, Serialize)]
pub struct ChunkResult {
    pub chunk_id: Uuid,
    pub content: String,
    pub source_kind: String,
    pub source_type: String,
    pub file_path: Option<String>,
    pub repo_name: Option<String>,
    pub language: Option<String>,
    pub heading_path: Option<String>,
    pub similarity_score: f32,
}

/// Entity result from graph expansion
#[derive(Debug, Serialize)]
pub struct EntityResult {
    pub id: Uuid,
    pub entity_type: String,
    pub name: String,
    pub source: String,
    pub properties: serde_json::Value,
}

/// Relationship result showing connections
#[derive(Debug, Serialize)]
pub struct RelationshipResult {
    pub from_id: Uuid,
    pub to_id: Uuid,
    pub from_name: String,
    pub to_name: String,
    pub relationship_type: String,
    pub confidence: f32,
    pub is_cross_source: bool,
}

/// Full hybrid search response
#[derive(Debug, Serialize)]
pub struct HybridSearchResponse {
    /// Ranked chunk results from vector search
    pub chunks: Vec<ChunkResult>,
    
    /// Related entities from graph expansion
    pub related_entities: Vec<EntityResult>,
    
    /// Relationships between entities
    pub relationships: Vec<RelationshipResult>,
    
    /// Cross-source links (docs explaining code, etc.)
    pub cross_source_links: Vec<SemanticLink>,
    
    /// Query metadata
    pub metadata: SearchMetadata,
}

/// Metadata about the search execution
#[derive(Debug, Serialize)]
pub struct SearchMetadata {
    pub query: String,
    pub vector_results_count: usize,
    pub graph_entities_count: usize,
    pub graph_hops_performed: usize,
    pub cross_source_links_count: usize,
    pub execution_time_ms: u64,
}

/// Vector-only search request
#[derive(Debug, Deserialize)]
pub struct VectorSearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub source_kind: Option<String>,
    pub source_types: Option<Vec<String>>,
    pub owner_id: Option<String>,
}

/// Vector search response
#[derive(Debug, Serialize)]
pub struct VectorSearchResponse {
    pub results: Vec<ChunkResult>,
    pub total_count: usize,
}

/// Graph-only search request
#[derive(Debug, Deserialize)]
pub struct GraphSearchRequest {
    /// Starting entity IDs or names
    pub start_entities: Vec<String>,
    /// Relationship types to traverse
    pub relationship_types: Option<Vec<String>>,
    /// Direction: "outgoing", "incoming", "both"
    #[serde(default = "default_direction")]
    pub direction: String,
    /// Number of hops
    #[serde(default = "default_hops")]
    pub hops: usize,
    /// Maximum results
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_direction() -> String { "both".to_string() }

/// Graph search response
#[derive(Debug, Serialize)]
pub struct GraphSearchResponse {
    pub entities: Vec<EntityResult>,
    pub relationships: Vec<RelationshipResult>,
    pub paths: Vec<GraphPath>,
}

/// A path through the graph
#[derive(Debug, Serialize)]
pub struct GraphPath {
    pub nodes: Vec<Uuid>,
    pub relationships: Vec<String>,
    pub total_confidence: f32,
}

/// Request to trigger cross-source linking
#[derive(Debug, Deserialize)]
pub struct CrossSourceLinkRequest {
    /// Specific chunk IDs to process (optional, processes all if empty)
    pub chunk_ids: Option<Vec<Uuid>>,
    /// Force re-linking even if already linked
    #[serde(default)]
    pub force: bool,
    /// Source kind to link from ("code" or "document")
    pub from_source_kind: Option<String>,
    /// Source kind to link to
    pub to_source_kind: Option<String>,
}

/// Response from cross-source linking
#[derive(Debug, Serialize)]
pub struct CrossSourceLinkResponse {
    pub links_created: usize,
    pub chunks_processed: usize,
    pub errors: Vec<String>,
}
