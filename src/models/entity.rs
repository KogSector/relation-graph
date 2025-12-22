//! Entity types for the knowledge graph

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use std::collections::HashMap;

/// Universal entity types spanning all data sources
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    // Code entities
    Repository,
    File,
    Function,
    Class,
    Module,
    Commit,
    PullRequest,
    Issue,
    
    // Document entities
    Document,
    Section,
    Concept,
    
    // Communication entities
    Message,
    Thread,
    Channel,
    
    // People & orgs
    Person,
    Organization,
    
    // Generic
    CodeEntity,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Repository => "repository",
            EntityType::File => "file",
            EntityType::Function => "function",
            EntityType::Class => "class",
            EntityType::Module => "module",
            EntityType::Commit => "commit",
            EntityType::PullRequest => "pull_request",
            EntityType::Issue => "issue",
            EntityType::Document => "document",
            EntityType::Section => "section",
            EntityType::Concept => "concept",
            EntityType::Message => "message",
            EntityType::Thread => "thread",
            EntityType::Channel => "channel",
            EntityType::Person => "person",
            EntityType::Organization => "organization",
            EntityType::CodeEntity => "code_entity",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "repository" => Some(EntityType::Repository),
            "file" => Some(EntityType::File),
            "function" => Some(EntityType::Function),
            "class" => Some(EntityType::Class),
            "module" => Some(EntityType::Module),
            "commit" => Some(EntityType::Commit),
            "pull_request" => Some(EntityType::PullRequest),
            "issue" => Some(EntityType::Issue),
            "document" => Some(EntityType::Document),
            "section" => Some(EntityType::Section),
            "concept" => Some(EntityType::Concept),
            "message" => Some(EntityType::Message),
            "thread" => Some(EntityType::Thread),
            "channel" => Some(EntityType::Channel),
            "person" => Some(EntityType::Person),
            "organization" => Some(EntityType::Organization),
            "code_entity" => Some(EntityType::CodeEntity),
            _ => None,
        }
    }
    
    /// Returns true if this is a code-related entity
    pub fn is_code(&self) -> bool {
        matches!(
            self,
            EntityType::Repository
                | EntityType::File
                | EntityType::Function
                | EntityType::Class
                | EntityType::Module
                | EntityType::Commit
                | EntityType::PullRequest
                | EntityType::Issue
                | EntityType::CodeEntity
        )
    }
    
    /// Returns true if this is a document-related entity
    pub fn is_document(&self) -> bool {
        matches!(
            self,
            EntityType::Document | EntityType::Section | EntityType::Concept
        )
    }
}

/// Data source from which entity was extracted
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DataSource {
    GitHub,
    GitLab,
    Bitbucket,
    Slack,
    Notion,
    GoogleDrive,
    Dropbox,
    LocalFile,
    UrlCrawler,
    Email,
    Jira,
    Confluence,
}

impl DataSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            DataSource::GitHub => "github",
            DataSource::GitLab => "gitlab",
            DataSource::Bitbucket => "bitbucket",
            DataSource::Slack => "slack",
            DataSource::Notion => "notion",
            DataSource::GoogleDrive => "google_drive",
            DataSource::Dropbox => "dropbox",
            DataSource::LocalFile => "local_file",
            DataSource::UrlCrawler => "url_crawler",
            DataSource::Email => "email",
            DataSource::Jira => "jira",
            DataSource::Confluence => "confluence",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "github" => Some(DataSource::GitHub),
            "gitlab" => Some(DataSource::GitLab),
            "bitbucket" => Some(DataSource::Bitbucket),
            "slack" => Some(DataSource::Slack),
            "notion" => Some(DataSource::Notion),
            "google_drive" => Some(DataSource::GoogleDrive),
            "dropbox" => Some(DataSource::Dropbox),
            "local_file" => Some(DataSource::LocalFile),
            "url_crawler" => Some(DataSource::UrlCrawler),
            "email" => Some(DataSource::Email),
            "jira" => Some(DataSource::Jira),
            "confluence" => Some(DataSource::Confluence),
            _ => None,
        }
    }
    
    /// Returns the source kind (code or document)
    pub fn source_kind(&self) -> &'static str {
        match self {
            DataSource::GitHub | DataSource::GitLab | DataSource::Bitbucket | DataSource::LocalFile => "code",
            _ => "document",
        }
    }
}

/// Core entity in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Entity {
    pub id: Uuid,
    pub entity_type: String,
    pub source: String,
    pub source_id: String,
    pub name: String,
    pub canonical_id: Option<Uuid>,
    pub properties: serde_json::Value,
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Entity {
    pub fn new(
        entity_type: EntityType,
        source: DataSource,
        source_id: String,
        name: String,
        properties: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            entity_type: entity_type.as_str().to_string(),
            source: source.as_str().to_string(),
            source_id,
            name,
            canonical_id: None,
            properties: serde_json::to_value(properties).unwrap_or(serde_json::json!({})),
            embedding: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
    
    pub fn get_entity_type(&self) -> Option<EntityType> {
        EntityType::from_str(&self.entity_type)
    }
    
    pub fn get_source(&self) -> Option<DataSource> {
        DataSource::from_str(&self.source)
    }
}

/// Canonical entity representing merged view across sources
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CanonicalEntity {
    pub id: Uuid,
    pub entity_type: String,
    pub canonical_name: String,
    pub merged_properties: serde_json::Value,
    pub source_entities: serde_json::Value,
    pub confidence_score: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a new entity
#[derive(Debug, Deserialize)]
pub struct CreateEntityRequest {
    pub entity_type: String,
    pub source: String,
    pub source_id: String,
    pub name: String,
    #[serde(default)]
    pub properties: HashMap<String, serde_json::Value>,
    pub text_for_embedding: Option<String>,
}

/// Response after creating entity
#[derive(Debug, Serialize)]
pub struct CreateEntityResponse {
    pub entity_id: Uuid,
    pub neo4j_node_id: Option<String>,
    pub canonical_id: Option<Uuid>,
    pub resolved: bool,
}
