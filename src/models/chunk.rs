//! Chunk model - the atomic unit of knowledge

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Source kind classification for chunks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Code,
    Document,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceKind::Code => "code",
            SourceKind::Document => "document",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "code" => Some(SourceKind::Code),
            "document" => Some(SourceKind::Document),
            _ => None,
        }
    }
}

/// A chunk of content (code or document)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Chunk {
    pub id: Uuid,
    pub content: String,
    pub content_hash: String,
    pub source_kind: String,           // "code" or "document"
    pub source_type: String,           // "github", "dropbox", etc.
    pub source_id: String,             // unique identifier in source
    pub file_path: Option<String>,
    pub repo_name: Option<String>,
    pub branch: Option<String>,
    pub language: Option<String>,      // for code
    pub heading_path: Option<String>,  // for docs: "# Intro > ## Setup"
    pub section_title: Option<String>,
    pub owner_id: String,
    pub author: Option<String>,
    pub commit_sha: Option<String>,
    pub commit_date: Option<DateTime<Utc>>,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub token_count: Option<i32>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Chunk with its embedding for vector operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkWithEmbedding {
    pub chunk: Chunk,
    pub embedding: Vec<f32>,
}

/// Metadata for vector storage (Zilliz)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkVectorMetadata {
    pub chunk_id: String,
    pub source_kind: String,
    pub source_type: String,
    pub file_path: Option<String>,
    pub repo_name: Option<String>,
    pub language: Option<String>,
    pub heading_path: Option<String>,
    pub owner_id: String,
    pub author: Option<String>,
    pub created_at: i64,  // Unix timestamp for filtering
}

/// Request to ingest chunks from the chunker service
#[derive(Debug, Deserialize)]
pub struct IngestChunksRequest {
    pub chunks: Vec<ChunkInput>,
    pub extract_entities: Option<bool>,
    pub create_cross_links: Option<bool>,
}

/// Input format for a single chunk
#[derive(Debug, Deserialize)]
pub struct ChunkInput {
    pub id: Option<Uuid>,
    pub content: String,
    pub source_kind: String,
    pub source_type: String,
    pub source_id: String,
    pub file_path: Option<String>,
    pub repo_name: Option<String>,
    pub branch: Option<String>,
    pub language: Option<String>,
    pub heading_path: Option<String>,
    pub section_title: Option<String>,
    pub owner_id: String,
    pub author: Option<String>,
    pub commit_sha: Option<String>,
    pub commit_date: Option<DateTime<Utc>>,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub token_count: Option<i32>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub embedding: Option<Vec<f32>>,
}

impl ChunkInput {
    pub fn into_chunk(self) -> Chunk {
        let content_hash = format!("{:x}", md5::compute(&self.content));
        Chunk {
            id: self.id.unwrap_or_else(Uuid::new_v4),
            content: self.content,
            content_hash,
            source_kind: self.source_kind,
            source_type: self.source_type,
            source_id: self.source_id,
            file_path: self.file_path,
            repo_name: self.repo_name,
            branch: self.branch,
            language: self.language,
            heading_path: self.heading_path,
            section_title: self.section_title,
            owner_id: self.owner_id,
            author: self.author,
            commit_sha: self.commit_sha,
            commit_date: self.commit_date,
            start_line: self.start_line,
            end_line: self.end_line,
            token_count: self.token_count,
            metadata: self.metadata,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// Response after ingesting chunks
#[derive(Debug, Serialize)]
pub struct IngestChunksResponse {
    pub chunks_ingested: usize,
    pub entities_extracted: usize,
    pub relationships_created: usize,
    pub vectors_stored: usize,
    pub errors: Vec<String>,
}
