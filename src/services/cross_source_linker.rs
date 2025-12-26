//! Cross-source linker service
//!
//! The core innovation: links code chunks to document chunks based on
//! vector similarity (via Neo4j native indexes), explicit mentions, 
//! temporal proximity, and author overlap.

use crate::config::Config;
use crate::error::GraphResult;
use crate::graph_db::Neo4jClient;
use crate::models::{
    Chunk, RelationshipType, RelationshipEvidence, ExtractionMethod, SemanticLink,
};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use uuid::Uuid;

/// Cross-source linker for creating semantic relationships
/// 
/// Now uses Neo4j native vector indexes instead of separate Zilliz database.
pub struct CrossSourceLinker {
    config: Config,
    neo4j: Option<Arc<Neo4jClient>>,
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
    ) -> Self {
        Self { config, neo4j }
    }
    
    /// Create cross-source links between code and document chunks using Neo4j
    /// 
    /// This is the main algorithm that makes the system unique:
    /// 1. Find semantically similar chunks via Neo4j vector index
    /// 2. Boost confidence with explicit mentions
    /// 3. Boost with temporal proximity
    /// 4. Boost with author overlap
    /// 
    /// All operations happen in Neo4j, eliminating the need for separate Zilliz queries.
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
        
        // For each document chunk, find similar code chunks via Neo4j vector index
        if let Some(neo4j) = &self.neo4j {
            for (doc_id, _doc_embedding) in doc_embeddings {
                let doc_chunk = match doc_map.get(doc_id) {
                    Some(c) => *c,
                    None => continue,
                };
                
                // Use Neo4j native vector search with confidence boosters
                match neo4j.find_similar_chunks_for_linking(
                    &doc_id.to_string(),
                    "code",
                    self.config.max_cross_links_per_chunk,
                    self.config.similarity_threshold,
                ).await {
                    Ok(matches) => {
                        for m in matches {
                            let code_id = Uuid::parse_str(&m.target_id).unwrap_or_else(|_| Uuid::new_v4());
                            let code_chunk = match code_map.get(&code_id) {
                                Some(c) => *c,
                                None => continue,
                            };
                            
                            // Calculate additional confidence boosters if needed
                            let mut confidence = m.confidence;
                            let mut extraction_methods = vec![ExtractionMethod::VectorSimilarity];
                            let mut evidence_text = None;
                            let mut author_match = m.has_author_overlap;
                            let mut temporal_distance = None;
                            
                            // Additional explicit mention detection (beyond what Neo4j does)
                            if self.config.enable_explicit_mentions && !m.has_explicit_mention {
                                if let Some(mention) = self.detect_explicit_mention(&doc_chunk.content, code_chunk) {
                                    confidence += self.config.explicit_mention_boost;
                                    confidence = confidence.min(1.0);
                                    extraction_methods.push(ExtractionMethod::ExplicitMention);
                                    evidence_text = Some(mention);
                                }
                            } else if m.has_explicit_mention {
                                extraction_methods.push(ExtractionMethod::ExplicitMention);
                            }
                            
                            // Temporal proximity boost
                            if self.config.enable_temporal_proximity {
                                if let Some(code_date) = code_chunk.commit_date {
                                    let doc_date = doc_chunk.updated_at;
                                    let days = self.temporal_proximity_score(doc_date, code_date);
                                    if days <= self.config.temporal_proximity_days {
                                        let boost = self.config.temporal_proximity_boost 
                                            * (1.0 - (days as f32 / self.config.temporal_proximity_days as f32));
                                        confidence += boost;
                                        confidence = confidence.min(1.0);
                                        extraction_methods.push(ExtractionMethod::TemporalProximity);
                                        temporal_distance = Some(days as i32);
                                    }
                                }
                            }
                            
                            // Author overlap (may already be detected by Neo4j)
                            if m.has_author_overlap {
                                extraction_methods.push(ExtractionMethod::AuthorOverlap);
                            }
                            
                            // Determine relationship type
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
                                .with_similarity_score(m.similarity_score)
                                .with_author_match(author_match);
                            
                            if let Some(days) = temporal_distance {
                                evidence = evidence.with_temporal_distance(days);
                            }
                            if let Some(text) = evidence_text {
                                evidence = evidence.with_evidence_text(text);
                            }
                            
                            evidence_records.push(evidence);
                            
                            // Create relationship in Neo4j
                            match neo4j.create_cross_source_link(
                                &doc_id.to_string(),
                                &m.target_id,
                                confidence,
                                m.similarity_score,
                                m.has_explicit_mention,
                                m.has_author_overlap,
                            ).await {
                                Ok(_) => links_created += 1,
                                Err(e) => errors.push(format!("Neo4j relationship error: {}", e)),
                            }
                        }
                    }
                    Err(e) => errors.push(format!("Vector search error: {}", e)),
                }
            }
        } else {
            // Fallback: in-memory linking without Neo4j
            let code_embedding_map: std::collections::HashMap<Uuid, &Vec<f32>> = 
                code_embeddings.iter().map(|(id, emb)| (*id, emb)).collect();
            
            for (doc_id, doc_embedding) in doc_embeddings {
                let doc_chunk = match doc_map.get(doc_id) {
                    Some(c) => *c,
                    None => continue,
                };
                
                // Find similar code chunks via in-memory cosine similarity
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
                    
                    let rel_type = self.determine_relationship_type(doc_chunk, code_chunk);
                    
                    let evidence = RelationshipEvidence::new(
                        *doc_id,
                        code_id,
                        rel_type.as_str().to_string(),
                        similarity,
                        ExtractionMethod::VectorSimilarity,
                    ).with_similarity_score(similarity);
                    
                    evidence_records.push(evidence);
                    links_created += 1;
                }
            }
        }
        
        Ok(LinkResult {
            links_created,
            evidence_records,
            errors,
        })
    }
    
    /// Find similar vectors using cosine similarity (fallback for when Neo4j unavailable)
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
        (doc_date - commit_date).num_days().abs()
    }
    
    /// Determine the type of cross-source relationship
    fn determine_relationship_type(&self, doc_chunk: &Chunk, _code_chunk: &Chunk) -> RelationshipType {
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
                    extraction_methods: vec!["neo4j_vector_similarity".to_string()],
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
