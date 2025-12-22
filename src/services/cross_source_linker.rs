//! Cross-source linker service
//!
//! The core innovation: links code chunks to document chunks based on
//! vector similarity, explicit mentions, temporal proximity, and author overlap.

use crate::config::Config;
use crate::error::{GraphError, GraphResult};
use crate::graph_db::Neo4jClient;
use crate::vector_db::ZillizClient;
use crate::models::{
    Chunk, RelationshipType, RelationshipEvidence, ExtractionMethod, SemanticLink,
};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use uuid::Uuid;

/// Cross-source linker for creating semantic relationships
pub struct CrossSourceLinker {
    config: Config,
    neo4j: Option<Arc<Neo4jClient>>,
    zilliz: Option<Arc<ZillizClient>>,
}

/// Result of a linking operation
#[derive(Debug)]
pub struct LinkResult {
    pub links_created: usize,
    pub evidence_records: Vec<RelationshipEvidence>,
    pub errors: Vec<String>,
}

impl CrossSourceLinker {
    pub fn new(
        config: Config,
        neo4j: Option<Arc<Neo4jClient>>,
        zilliz: Option<Arc<ZillizClient>>,
    ) -> Self {
        Self { config, neo4j, zilliz }
    }
    
    /// Create cross-source links between code and document chunks
    /// 
    /// This is the main algorithm that makes the system unique:
    /// 1. Find semantically similar chunks across sources
    /// 2. Boost confidence with explicit mentions
    /// 3. Boost with temporal proximity
    /// 4. Boost with author overlap
    pub async fn link_chunks(
        &self,
        code_chunks: &[Chunk],
        doc_chunks: &[Chunk],
        code_embeddings: &[(Uuid, Vec<f32>)],
        doc_embeddings: &[(Uuid, Vec<f32>)],
    ) -> GraphResult<LinkResult> {
        let mut links_created = 0;
        let mut evidence_records = Vec::new();
        let mut errors = Vec::new();
        
        // Build lookup maps
        let code_map: std::collections::HashMap<Uuid, &Chunk> = 
            code_chunks.iter().map(|c| (c.id, c)).collect();
        let doc_map: std::collections::HashMap<Uuid, &Chunk> = 
            doc_chunks.iter().map(|c| (c.id, c)).collect();
        let code_embedding_map: std::collections::HashMap<Uuid, &Vec<f32>> = 
            code_embeddings.iter().map(|(id, emb)| (*id, emb)).collect();
        
        // For each document chunk, find similar code chunks
        for (doc_id, doc_embedding) in doc_embeddings {
            let doc_chunk = match doc_map.get(doc_id) {
                Some(c) => *c,
                None => continue,
            };
            
            // Find similar code chunks via vector similarity
            let similar_code = self.find_similar_vectors(
                doc_embedding,
                code_embeddings,
                self.config.max_cross_links_per_chunk,
            );
            
            for (code_id, similarity) in similar_code {
                if similarity < self.config.similarity_threshold {
                    continue;
                }
                
                let code_chunk = match code_map.get(&code_id) {
                    Some(c) => *c,
                    None => continue,
                };
                
                // Calculate confidence with boosts
                let mut confidence = similarity;
                let mut extraction_methods = vec![ExtractionMethod::VectorSimilarity];
                let mut evidence_text = None;
                let mut author_match = false;
                let mut temporal_distance = None;
                
                // Boost 1: Explicit mentions
                if self.config.enable_explicit_mentions {
                    if let Some(mention) = self.detect_explicit_mention(&doc_chunk.content, code_chunk) {
                        confidence += 0.15;
                        confidence = confidence.min(1.0);
                        extraction_methods.push(ExtractionMethod::ExplicitMention);
                        evidence_text = Some(mention);
                    }
                }
                
                // Boost 2: Temporal proximity
                if self.config.enable_temporal_proximity {
                    // Use updated_at for doc, commit_date for code
                    if let Some(code_date) = code_chunk.commit_date {
                        let doc_date = doc_chunk.updated_at;
                        let days = self.temporal_proximity_score(doc_date, code_date);
                        if days <= self.config.temporal_proximity_days {
                            let boost = 0.1 * (1.0 - (days as f32 / self.config.temporal_proximity_days as f32));
                            confidence += boost;
                            confidence = confidence.min(1.0);
                            extraction_methods.push(ExtractionMethod::TemporalProximity);
                            temporal_distance = Some(days as i32);
                        }
                    }
                }
                
                // Boost 3: Author overlap
                if self.config.enable_author_overlap {
                    if let (Some(doc_author), Some(code_author)) = 
                        (&doc_chunk.author, &code_chunk.author)
                    {
                        if doc_author == code_author {
                            confidence += 0.1;
                            confidence = confidence.min(1.0);
                            extraction_methods.push(ExtractionMethod::AuthorOverlap);
                            author_match = true;
                        }
                    }
                }
                
                // Determine relationship type based on context
                let rel_type = self.determine_relationship_type(doc_chunk, code_chunk);
                
                // Create evidence record
                let mut evidence = RelationshipEvidence::new(
                    *doc_id,
                    code_id,
                    rel_type.as_str().to_string(),
                    confidence,
                    if extraction_methods.len() > 1 {
                        ExtractionMethod::Combined
                    } else {
                        ExtractionMethod::VectorSimilarity
                    },
                );
                evidence = evidence
                    .with_similarity_score(similarity)
                    .with_author_match(author_match);
                
                if let Some(days) = temporal_distance {
                    evidence = evidence.with_temporal_distance(days);
                }
                if let Some(text) = evidence_text {
                    evidence = evidence.with_evidence_text(text);
                }
                
                evidence_records.push(evidence);
                
                // Create relationship in Neo4j if available
                if let Some(neo4j) = &self.neo4j {
                    match neo4j.create_relationship(
                        &doc_id.to_string(),
                        &code_id.to_string(),
                        rel_type,
                        confidence,
                        Some(serde_json::json!({
                            "similarity_score": similarity,
                            "extraction_methods": extraction_methods.iter()
                                .map(|m| m.as_str())
                                .collect::<Vec<_>>(),
                        })),
                    ).await {
                        Ok(_) => links_created += 1,
                        Err(e) => errors.push(format!("Neo4j relationship error: {}", e)),
                    }
                } else {
                    links_created += 1; // Count even without Neo4j
                }
            }
        }
        
        Ok(LinkResult {
            links_created,
            evidence_records,
            errors,
        })
    }
    
    /// Find similar vectors using cosine similarity
    fn find_similar_vectors(
        &self,
        query: &[f32],
        candidates: &[(Uuid, Vec<f32>)],
        limit: usize,
    ) -> Vec<(Uuid, f32)> {
        let mut scores: Vec<(Uuid, f32)> = candidates
            .iter()
            .map(|(id, vec)| (*id, cosine_similarity(query, vec)))
            .collect();
        
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        scores
    }
    
    /// Detect if document explicitly mentions code entities
    fn detect_explicit_mention(&self, doc_content: &str, code_chunk: &Chunk) -> Option<String> {
        // Look for function names, class names, file names
        let doc_lower = doc_content.to_lowercase();
        
        // Check file name
        if let Some(file_path) = &code_chunk.file_path {
            if let Some(file_name) = file_path.split('/').last() {
                let file_base = file_name.split('.').next().unwrap_or(file_name);
                if doc_lower.contains(&file_base.to_lowercase()) {
                    return Some(format!("Mentions file: {}", file_name));
                }
            }
        }
        
        // Check for code-style references (backticks)
        let code_content = &code_chunk.content;
        
        // Extract potential identifiers from code
        let identifier_pattern = regex::Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]{3,})\b").ok()?;
        for cap in identifier_pattern.captures_iter(code_content) {
            if let Some(identifier) = cap.get(1) {
                let id_str = identifier.as_str();
                // Skip common keywords
                if ["function", "class", "return", "import", "const", "let", "var", "pub", "fn", "struct", "impl"]
                    .contains(&id_str)
                {
                    continue;
                }
                // Check if mentioned in backticks in doc
                if doc_content.contains(&format!("`{}`", id_str)) 
                    || doc_content.contains(&format!("`{}()`", id_str))
                {
                    return Some(format!("Mentions: `{}`", id_str));
                }
            }
        }
        
        None
    }
    
    /// Calculate temporal proximity in days
    fn temporal_proximity_score(&self, doc_date: DateTime<Utc>, commit_date: DateTime<Utc>) -> i64 {
        let diff = (doc_date - commit_date).num_days().abs();
        diff
    }
    
    /// Determine the type of cross-source relationship
    fn determine_relationship_type(&self, doc_chunk: &Chunk, code_chunk: &Chunk) -> RelationshipType {
        let doc_content = doc_chunk.content.to_lowercase();
        
        // Check for documentation-style content
        if doc_content.contains("how to") 
            || doc_content.contains("example")
            || doc_content.contains("usage")
        {
            return RelationshipType::Explains;
        }
        
        // Check for API documentation
        if doc_content.contains("endpoint")
            || doc_content.contains("request")
            || doc_content.contains("response")
        {
            return RelationshipType::Documents;
        }
        
        // Check if it's a README or overview
        if let Some(path) = &doc_chunk.file_path {
            if path.to_lowercase().contains("readme") {
                return RelationshipType::Documents;
            }
        }
        
        // Default to semantic similarity
        RelationshipType::SemanticallySimilar
    }
    
    /// Get semantic links for a chunk
    pub async fn get_links_for_chunk(&self, chunk_id: Uuid) -> GraphResult<Vec<SemanticLink>> {
        if let Some(neo4j) = &self.neo4j {
            let relationships = neo4j.get_cross_source_relationships(&chunk_id.to_string()).await?;
            
            Ok(relationships
                .into_iter()
                .map(|(target_id, _target_name, rel_type, confidence)| SemanticLink {
                    from_chunk_id: chunk_id,
                    to_chunk_id: Uuid::parse_str(&target_id).unwrap_or_else(|_| Uuid::new_v4()),
                    relationship_type: rel_type,
                    confidence,
                    extraction_methods: vec!["vector_similarity".to_string()],
                    similarity_score: None,
                    explicit_mention: None,
                    temporal_distance_days: None,
                    author_overlap: false,
                })
                .collect())
        } else {
            Ok(Vec::new())
        }
    }
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    
    dot_product / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
        
        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 0.001);
    }
}
