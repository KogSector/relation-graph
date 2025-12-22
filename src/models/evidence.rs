//! Evidence model for tracking how relationships were inferred

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// How the relationship was extracted/inferred
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMethod {
    /// AST-based extraction from code
    AstExtraction,
    /// Vector similarity between embeddings
    VectorSimilarity,
    /// Explicit mention of entity name in text
    ExplicitMention,
    /// Temporal proximity (created/updated near each other)
    TemporalProximity,
    /// Author overlap (same author for both)
    AuthorOverlap,
    /// Regex pattern matching
    PatternMatch,
    /// Manual/user-defined
    Manual,
    /// Combined methods
    Combined,
}

impl ExtractionMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExtractionMethod::AstExtraction => "ast_extraction",
            ExtractionMethod::VectorSimilarity => "vector_similarity",
            ExtractionMethod::ExplicitMention => "explicit_mention",
            ExtractionMethod::TemporalProximity => "temporal_proximity",
            ExtractionMethod::AuthorOverlap => "author_overlap",
            ExtractionMethod::PatternMatch => "pattern_match",
            ExtractionMethod::Manual => "manual",
            ExtractionMethod::Combined => "combined",
        }
    }
}

/// Evidence for a relationship between chunks/entities
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RelationshipEvidence {
    pub id: Uuid,
    pub from_chunk_id: Uuid,
    pub to_chunk_id: Uuid,
    pub from_entity_id: Option<Uuid>,
    pub to_entity_id: Option<Uuid>,
    pub relationship_type: String,
    pub confidence: f32,
    pub extraction_method: String,
    pub evidence_text: Option<String>,
    pub similarity_score: Option<f32>,
    pub temporal_distance_days: Option<i32>,
    pub author_match: bool,
    pub properties: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl RelationshipEvidence {
    pub fn new(
        from_chunk_id: Uuid,
        to_chunk_id: Uuid,
        relationship_type: String,
        confidence: f32,
        extraction_method: ExtractionMethod,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            from_chunk_id,
            to_chunk_id,
            from_entity_id: None,
            to_entity_id: None,
            relationship_type,
            confidence,
            extraction_method: extraction_method.as_str().to_string(),
            evidence_text: None,
            similarity_score: None,
            temporal_distance_days: None,
            author_match: false,
            properties: serde_json::json!({}),
            created_at: Utc::now(),
        }
    }
    
    pub fn with_entity_ids(mut self, from: Uuid, to: Uuid) -> Self {
        self.from_entity_id = Some(from);
        self.to_entity_id = Some(to);
        self
    }
    
    pub fn with_similarity_score(mut self, score: f32) -> Self {
        self.similarity_score = Some(score);
        self
    }
    
    pub fn with_temporal_distance(mut self, days: i32) -> Self {
        self.temporal_distance_days = Some(days);
        self
    }
    
    pub fn with_author_match(mut self, matched: bool) -> Self {
        self.author_match = matched;
        self
    }
    
    pub fn with_evidence_text(mut self, text: String) -> Self {
        self.evidence_text = Some(text);
        self
    }
}

/// Semantic link created by cross-source linking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticLink {
    pub from_chunk_id: Uuid,
    pub to_chunk_id: Uuid,
    pub relationship_type: String,
    pub confidence: f32,
    pub extraction_methods: Vec<String>,
    pub similarity_score: Option<f32>,
    pub explicit_mention: Option<String>,
    pub temporal_distance_days: Option<i32>,
    pub author_overlap: bool,
}
