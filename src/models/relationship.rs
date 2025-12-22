//! Relationship types for the knowledge graph

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Relationship types in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationshipType {
    // Code structure relationships
    Contains,       // Repo -> File, File -> Class, Class -> Function
    Imports,        // File -> Module
    Calls,          // Function -> Function
    Implements,     // Class -> Trait/Interface
    Extends,        // Class -> Class
    
    // Document structure relationships
    ParentOf,       // Section -> Section (heading hierarchy)
    References,     // Section -> Concept
    Defines,        // Section -> Concept
    
    // Cross-source relationships (the unique value!)
    Explains,       // Document -> Function (doc explains code)
    Documents,      // Document -> Repository
    SemanticallySimilar, // Chunk -> Chunk (vector similarity)
    MentionsExplicitly,  // Document -> Entity (explicit name mention)
    
    // Authorship
    AuthoredBy,     // Commit/Document -> Person
    ContributedTo,  // Person -> Repository
    
    // Temporal
    CommittedAt,    // Commit -> Timestamp
    UpdatedNear,    // Document -> Commit (temporal proximity)
    
    // Generic
    RelatedTo,
}

impl RelationshipType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationshipType::Contains => "CONTAINS",
            RelationshipType::Imports => "IMPORTS",
            RelationshipType::Calls => "CALLS",
            RelationshipType::Implements => "IMPLEMENTS",
            RelationshipType::Extends => "EXTENDS",
            RelationshipType::ParentOf => "PARENT_OF",
            RelationshipType::References => "REFERENCES",
            RelationshipType::Defines => "DEFINES",
            RelationshipType::Explains => "EXPLAINS",
            RelationshipType::Documents => "DOCUMENTS",
            RelationshipType::SemanticallySimilar => "SEMANTICALLY_SIMILAR",
            RelationshipType::MentionsExplicitly => "MENTIONS_EXPLICITLY",
            RelationshipType::AuthoredBy => "AUTHORED_BY",
            RelationshipType::ContributedTo => "CONTRIBUTED_TO",
            RelationshipType::CommittedAt => "COMMITTED_AT",
            RelationshipType::UpdatedNear => "UPDATED_NEAR",
            RelationshipType::RelatedTo => "RELATED_TO",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "CONTAINS" => Some(RelationshipType::Contains),
            "IMPORTS" => Some(RelationshipType::Imports),
            "CALLS" => Some(RelationshipType::Calls),
            "IMPLEMENTS" => Some(RelationshipType::Implements),
            "EXTENDS" => Some(RelationshipType::Extends),
            "PARENT_OF" => Some(RelationshipType::ParentOf),
            "REFERENCES" => Some(RelationshipType::References),
            "DEFINES" => Some(RelationshipType::Defines),
            "EXPLAINS" => Some(RelationshipType::Explains),
            "DOCUMENTS" => Some(RelationshipType::Documents),
            "SEMANTICALLY_SIMILAR" | "SIMILAR" => Some(RelationshipType::SemanticallySimilar),
            "MENTIONS_EXPLICITLY" | "MENTIONS" => Some(RelationshipType::MentionsExplicitly),
            "AUTHORED_BY" => Some(RelationshipType::AuthoredBy),
            "CONTRIBUTED_TO" => Some(RelationshipType::ContributedTo),
            "COMMITTED_AT" => Some(RelationshipType::CommittedAt),
            "UPDATED_NEAR" => Some(RelationshipType::UpdatedNear),
            "RELATED_TO" => Some(RelationshipType::RelatedTo),
            _ => None,
        }
    }
    
    /// Returns true if this is a cross-source relationship (the unique value prop)
    pub fn is_cross_source(&self) -> bool {
        matches!(
            self,
            RelationshipType::Explains
                | RelationshipType::Documents
                | RelationshipType::SemanticallySimilar
                | RelationshipType::MentionsExplicitly
                | RelationshipType::UpdatedNear
        )
    }
}

/// Relationship in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Relationship {
    pub id: Uuid,
    pub from_entity_id: Uuid,
    pub to_entity_id: Uuid,
    pub relationship_type: String,
    pub confidence: f32,
    pub properties: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl Relationship {
    pub fn new(
        from_entity_id: Uuid,
        to_entity_id: Uuid,
        relationship_type: RelationshipType,
        confidence: f32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            from_entity_id,
            to_entity_id,
            relationship_type: relationship_type.as_str().to_string(),
            confidence,
            properties: serde_json::json!({}),
            created_at: Utc::now(),
        }
    }
    
    pub fn with_properties(mut self, properties: serde_json::Value) -> Self {
        self.properties = properties;
        self
    }
}

/// Request to create a relationship
#[derive(Debug, Deserialize)]
pub struct CreateRelationshipRequest {
    pub from_entity_id: Uuid,
    pub to_entity_id: Uuid,
    pub relationship_type: String,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    #[serde(default)]
    pub properties: serde_json::Value,
}

fn default_confidence() -> f32 {
    1.0
}

/// Response after creating relationship
#[derive(Debug, Serialize)]
pub struct CreateRelationshipResponse {
    pub relationship_id: Uuid,
    pub neo4j_rel_id: Option<String>,
}
