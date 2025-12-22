//! Document entity extractor
//!
//! Extracts sections, headings, and concepts from document content
//! (Markdown, plain text, etc.)

use lazy_static::lazy_static;
use regex::Regex;
use crate::models::{EntityType, RelationshipType};
use super::code_entities::{ExtractedEntity, ExtractedRelationship, ExtractionResult};

lazy_static! {
    /// Markdown headings
    static ref HEADING_PATTERN: Regex = Regex::new(
        r"(?m)^(#{1,6})\s+(.+)$"
    ).unwrap();
    
    /// Code block references (function names, class names)
    static ref CODE_REFERENCE_PATTERN: Regex = Regex::new(
        r"`([a-zA-Z_][a-zA-Z0-9_]*(?:\(\))?)`"
    ).unwrap();
    
    /// URL patterns
    static ref URL_PATTERN: Regex = Regex::new(
        r"https?://[^\s\)\]\>]+"
    ).unwrap();
    
    /// Concept/term patterns (capitalized phrases, technical terms)
    static ref CONCEPT_PATTERN: Regex = Regex::new(
        r"\b([A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)+)\b"
    ).unwrap();
    
    /// API endpoint mentions in docs
    static ref API_MENTION_PATTERN: Regex = Regex::new(
        r"(?:endpoint|API|route)[:.\s]+`?(/[a-zA-Z0-9_/\-{}:]+)`?"
    ).unwrap();
    
    /// Definition patterns
    static ref DEFINITION_PATTERN: Regex = Regex::new(
        r"(?i)(?:^|\n)[\*\-]\s*\*\*([^*]+)\*\*[:\s]+(.+)|(?:^|\n)([A-Z][a-zA-Z]+):\s+(.+)"
    ).unwrap();
}

/// Document structure with heading hierarchy
#[derive(Debug, Clone)]
pub struct HeadingNode {
    pub level: usize,
    pub title: String,
    pub line_number: usize,
    pub children: Vec<HeadingNode>,
}

/// Document entity extractor
pub struct DocumentEntityExtractor;

impl DocumentEntityExtractor {
    pub fn new() -> Self {
        Self
    }
    
    /// Extract entities from document content
    pub fn extract(&self, content: &str) -> Vec<ExtractedEntity> {
        self.extract_with_relationships(content).entities
    }
    
    /// Extract entities and relationships from document content
    pub fn extract_with_relationships(&self, content: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        
        // Extract heading hierarchy
        let headings = self.extract_heading_hierarchy(content);
        for heading in &headings {
            self.add_heading_entities(&mut result, heading, None);
        }
        
        // Extract code references (function/class names mentioned in docs)
        for cap in CODE_REFERENCE_PATTERN.captures_iter(content) {
            if let Some(code_ref) = cap.get(1) {
                let name = code_ref.as_str().trim_end_matches("()").to_string();
                // Skip common noise words
                if ["the", "a", "an", "is", "are", "was", "were"].contains(&name.to_lowercase().as_str()) {
                    continue;
                }
                result.entities.push(ExtractedEntity {
                    entity_type: EntityType::CodeEntity,
                    name,
                    confidence: 0.85,
                    start_line: None,
                    end_line: None,
                });
            }
        }
        
        // Extract concepts (capitalized multi-word phrases)
        let mut seen_concepts = std::collections::HashSet::new();
        for cap in CONCEPT_PATTERN.captures_iter(content) {
            if let Some(concept) = cap.get(1) {
                let name = concept.as_str().to_string();
                // Skip if too short or already seen
                if name.len() < 5 || seen_concepts.contains(&name) {
                    continue;
                }
                // Skip common false positives
                if ["The Next", "This Is", "You Can"].iter().any(|&fp| name.starts_with(fp)) {
                    continue;
                }
                seen_concepts.insert(name.clone());
                result.entities.push(ExtractedEntity {
                    entity_type: EntityType::Concept,
                    name,
                    confidence: 0.7,
                    start_line: None,
                    end_line: None,
                });
            }
        }
        
        // Extract API mentions
        for cap in API_MENTION_PATTERN.captures_iter(content) {
            if let Some(endpoint) = cap.get(1) {
                result.entities.push(ExtractedEntity {
                    entity_type: EntityType::CodeEntity,
                    name: endpoint.as_str().to_string(),
                    confidence: 0.9,
                    start_line: None,
                    end_line: None,
                });
            }
        }
        
        // Create REFERENCES relationships between sections and code entities
        self.create_reference_relationships(&mut result, content);
        
        result
    }
    
    /// Extract heading hierarchy from markdown
    fn extract_heading_hierarchy(&self, content: &str) -> Vec<HeadingNode> {
        let mut headings: Vec<(usize, String, usize)> = Vec::new();
        
        for (line_num, line) in content.lines().enumerate() {
            if let Some(cap) = HEADING_PATTERN.captures(line) {
                let level = cap.get(1).map(|m| m.as_str().len()).unwrap_or(1);
                let title = cap.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                headings.push((level, title, line_num + 1));
            }
        }
        
        self.build_heading_tree(&headings)
    }
    
    /// Build tree structure from flat heading list
    fn build_heading_tree(&self, headings: &[(usize, String, usize)]) -> Vec<HeadingNode> {
        let mut roots = Vec::new();
        let mut stack: Vec<HeadingNode> = Vec::new();
        
        for (level, title, line_num) in headings {
            let node = HeadingNode {
                level: *level,
                title: title.clone(),
                line_number: *line_num,
                children: Vec::new(),
            };
            
            // Pop stack until we find parent level
            while !stack.is_empty() && stack.last().map(|n| n.level).unwrap_or(0) >= *level {
                let completed = stack.pop().unwrap();
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(completed);
                } else {
                    roots.push(completed);
                }
            }
            
            stack.push(node);
        }
        
        // Empty remaining stack
        while let Some(completed) = stack.pop() {
            if let Some(parent) = stack.last_mut() {
                parent.children.push(completed);
            } else {
                roots.push(completed);
            }
        }
        
        roots
    }
    
    /// Add heading entities and PARENT_OF relationships recursively
    fn add_heading_entities(
        &self,
        result: &mut ExtractionResult,
        heading: &HeadingNode,
        parent_name: Option<&str>,
    ) {
        result.entities.push(ExtractedEntity {
            entity_type: EntityType::Section,
            name: heading.title.clone(),
            confidence: 0.95,
            start_line: Some(heading.line_number),
            end_line: None,
        });
        
        // Create PARENT_OF relationship if there's a parent
        if let Some(parent) = parent_name {
            result.relationships.push(ExtractedRelationship {
                from_name: parent.to_string(),
                to_name: heading.title.clone(),
                relationship_type: RelationshipType::ParentOf,
                confidence: 1.0,
            });
        }
        
        // Recursively process children
        for child in &heading.children {
            self.add_heading_entities(result, child, Some(&heading.title));
        }
    }
    
    /// Create REFERENCES relationships between sections and mentioned code entities
    fn create_reference_relationships(&self, result: &mut ExtractionResult, content: &str) {
        // Get section names
        let section_names: Vec<String> = result.entities
            .iter()
            .filter(|e| matches!(e.entity_type, EntityType::Section))
            .map(|e| e.name.clone())
            .collect();
        
        // Get code entity names
        let code_entities: Vec<String> = result.entities
            .iter()
            .filter(|e| matches!(e.entity_type, EntityType::CodeEntity))
            .map(|e| e.name.clone())
            .collect();
        
        // For each section, check if code entities are mentioned in that section
        // (simplified: just create relationships if both exist)
        if let Some(main_section) = section_names.first() {
            for code_entity in &code_entities {
                result.relationships.push(ExtractedRelationship {
                    from_name: main_section.clone(),
                    to_name: code_entity.clone(),
                    relationship_type: RelationshipType::References,
                    confidence: 0.8,
                });
            }
        }
    }
    
    /// Build heading path (e.g., "# Intro > ## Setup > ### Config")
    pub fn build_heading_path(headings: &[HeadingNode]) -> String {
        fn collect_path(node: &HeadingNode, path: &mut Vec<String>) {
            path.push(format!("{} {}", "#".repeat(node.level), node.title));
            if let Some(first_child) = node.children.first() {
                collect_path(first_child, path);
            }
        }
        
        let mut path = Vec::new();
        if let Some(root) = headings.first() {
            collect_path(root, &mut path);
        }
        path.join(" > ")
    }
}

impl Default for DocumentEntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_headings() {
        let extractor = DocumentEntityExtractor::new();
        let doc = r#"
# Getting Started

## Installation

Run `npm install` to get started.

## Configuration

Set up the `config.json` file.
        "#;
        
        let result = extractor.extract(doc);
        assert!(result.iter().any(|e| e.name == "Getting Started"));
        assert!(result.iter().any(|e| e.name == "Installation"));
        assert!(result.iter().any(|e| e.name == "Configuration"));
    }
    
    #[test]
    fn test_extract_code_references() {
        let extractor = DocumentEntityExtractor::new();
        let doc = "Use the `authenticate()` function to log in.";
        
        let result = extractor.extract(doc);
        assert!(result.iter().any(|e| e.name == "authenticate"));
    }
}
