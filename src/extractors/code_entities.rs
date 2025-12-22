//! Code entity extractor
//!
//! Extracts functions, classes, modules, and relationships from code content
//! using regex-based pattern matching.

use lazy_static::lazy_static;
use regex::Regex;
use crate::models::{EntityType, RelationshipType};

lazy_static! {
    /// Function definitions across languages
    static ref FUNCTION_PATTERN: Regex = Regex::new(
        r"(?m)^[\t ]*(pub\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)|function\s+(\w+)|def\s+(\w+)|func\s+(\w+)"
    ).unwrap();
    
    /// Class/struct/enum definitions
    static ref CLASS_PATTERN: Regex = Regex::new(
        r"(?m)^[\t ]*(pub\s+)?(?:class|struct|enum|trait|interface)\s+(\w+)"
    ).unwrap();
    
    /// API endpoint patterns
    static ref API_ENDPOINT_PATTERN: Regex = Regex::new(
        r"(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\s+(/[a-zA-Z0-9_/\-{}:]*)"
    ).unwrap();
    
    /// Issue/ticket references
    static ref TICKET_PATTERN: Regex = Regex::new(
        r"([A-Z]{2,10}-\d+)"
    ).unwrap();
    
    /// PR/MR references
    static ref PR_PATTERN: Regex = Regex::new(
        r"(?:PR|MR|#)(\d+)"
    ).unwrap();
    
    /// Import patterns for various languages
    static ref IMPORT_PATTERN: Regex = Regex::new(
        r#"(?m)^(?:use\s+([a-zA-Z_][a-zA-Z0-9_:]*)|import\s+(?:\{[^}]+\}\s+from\s+)?['"]([^'"]+)['"]|from\s+([a-zA-Z_][a-zA-Z0-9_.]*)\s+import|require\s*\(['"]([^'"]+)['"]\))"#
    ).unwrap();
    
    /// Function call patterns
    static ref FUNCTION_CALL_PATTERN: Regex = Regex::new(
        r"(?m)(?:self\.)?(\w+)\s*\("
    ).unwrap();
    
    /// Impl/extends patterns
    static ref IMPL_PATTERN: Regex = Regex::new(
        r"(?m)impl(?:<[^>]+>)?\s+(\w+)\s+for\s+(\w+)|class\s+(\w+)\s+extends\s+(\w+)|(\w+)\s*:\s*(\w+)"
    ).unwrap();
    
    /// Module/package declaration
    static ref MODULE_PATTERN: Regex = Regex::new(
        r"(?m)^(?:mod\s+(\w+)|package\s+([a-zA-Z_][a-zA-Z0-9_.]*)|namespace\s+([a-zA-Z_][a-zA-Z0-9_.]*))"
    ).unwrap();
}

/// An extracted entity from code
#[derive(Debug, Clone)]
pub struct ExtractedEntity {
    pub entity_type: EntityType,
    pub name: String,
    pub confidence: f32,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

/// An extracted relationship between entities
#[derive(Debug, Clone)]
pub struct ExtractedRelationship {
    pub from_name: String,
    pub to_name: String,
    pub relationship_type: RelationshipType,
    pub confidence: f32,
}

/// Result of code entity extraction
#[derive(Debug, Clone, Default)]
pub struct ExtractionResult {
    pub entities: Vec<ExtractedEntity>,
    pub relationships: Vec<ExtractedRelationship>,
}

/// Code entity extractor
pub struct CodeEntityExtractor;

impl CodeEntityExtractor {
    pub fn new() -> Self {
        Self
    }
    
    /// Extract entities from code content
    pub fn extract(&self, content: &str, language: Option<&str>) -> Vec<ExtractedEntity> {
        self.extract_with_relationships(content, language).entities
    }
    
    /// Extract entities and relationships from code content
    pub fn extract_with_relationships(&self, content: &str, _language: Option<&str>) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        let mut function_names: Vec<String> = Vec::new();
        let mut class_names: Vec<String> = Vec::new();
        
        // Track line numbers for entities
        let lines: Vec<&str> = content.lines().collect();
        
        // Extract modules
        for cap in MODULE_PATTERN.captures_iter(content) {
            for i in 1..4 {
                if let Some(name) = cap.get(i) {
                    result.entities.push(ExtractedEntity {
                        entity_type: EntityType::Module,
                        name: name.as_str().to_string(),
                        confidence: 0.9,
                        start_line: None,
                        end_line: None,
                    });
                    break;
                }
            }
        }
        
        // Extract functions
        for cap in FUNCTION_PATTERN.captures_iter(content) {
            for i in 2..6 {
                if let Some(name) = cap.get(i) {
                    let fn_name = name.as_str().to_string();
                    // Skip common noise
                    if ["if", "for", "while", "match", "return"].contains(&fn_name.as_str()) {
                        continue;
                    }
                    function_names.push(fn_name.clone());
                    
                    // Find line number
                    let start_pos = cap.get(0).map(|m| m.start()).unwrap_or(0);
                    let line_num = content[..start_pos].matches('\n').count() + 1;
                    
                    result.entities.push(ExtractedEntity {
                        entity_type: EntityType::Function,
                        name: fn_name,
                        confidence: 0.9,
                        start_line: Some(line_num),
                        end_line: None,
                    });
                    break;
                }
            }
        }
        
        // Extract classes/structs/enums
        for cap in CLASS_PATTERN.captures_iter(content) {
            if let Some(name) = cap.get(2) {
                let class_name = name.as_str().to_string();
                class_names.push(class_name.clone());
                
                let start_pos = cap.get(0).map(|m| m.start()).unwrap_or(0);
                let line_num = content[..start_pos].matches('\n').count() + 1;
                
                result.entities.push(ExtractedEntity {
                    entity_type: EntityType::Class,
                    name: class_name,
                    confidence: 0.9,
                    start_line: Some(line_num),
                    end_line: None,
                });
            }
        }
        
        // Extract API endpoints
        for cap in API_ENDPOINT_PATTERN.captures_iter(content) {
            if let Some(endpoint) = cap.get(1) {
                result.entities.push(ExtractedEntity {
                    entity_type: EntityType::CodeEntity,
                    name: endpoint.as_str().to_string(),
                    confidence: 0.85,
                    start_line: None,
                    end_line: None,
                });
            }
        }
        
        // Extract ticket references
        for cap in TICKET_PATTERN.captures_iter(content) {
            if let Some(ticket) = cap.get(1) {
                result.entities.push(ExtractedEntity {
                    entity_type: EntityType::Issue,
                    name: ticket.as_str().to_string(),
                    confidence: 0.9,
                    start_line: None,
                    end_line: None,
                });
            }
        }
        
        // Extract imports and create IMPORTS relationships
        for cap in IMPORT_PATTERN.captures_iter(content) {
            for i in 1..5 {
                if let Some(import_name) = cap.get(i) {
                    let import_str = import_name.as_str().to_string();
                    
                    result.entities.push(ExtractedEntity {
                        entity_type: EntityType::Module,
                        name: import_str.clone(),
                        confidence: 0.8,
                        start_line: None,
                        end_line: None,
                    });
                    
                    // If we have classes, they import this module
                    for class_name in &class_names {
                        result.relationships.push(ExtractedRelationship {
                            from_name: class_name.clone(),
                            to_name: import_str.clone(),
                            relationship_type: RelationshipType::Imports,
                            confidence: 0.85,
                        });
                    }
                    break;
                }
            }
        }
        
        // Extract impl/extends relationships
        for cap in IMPL_PATTERN.captures_iter(content) {
            // Rust: impl Trait for Struct
            if let (Some(trait_name), Some(struct_name)) = (cap.get(1), cap.get(2)) {
                result.relationships.push(ExtractedRelationship {
                    from_name: struct_name.as_str().to_string(),
                    to_name: trait_name.as_str().to_string(),
                    relationship_type: RelationshipType::Implements,
                    confidence: 0.95,
                });
            }
            // JS/TS/Java: class Child extends Parent
            if let (Some(child), Some(parent)) = (cap.get(3), cap.get(4)) {
                result.relationships.push(ExtractedRelationship {
                    from_name: child.as_str().to_string(),
                    to_name: parent.as_str().to_string(),
                    relationship_type: RelationshipType::Extends,
                    confidence: 0.95,
                });
            }
        }
        
        // Create CONTAINS relationships: classes contain functions
        if !class_names.is_empty() && !function_names.is_empty() {
            let primary_class = &class_names[0];
            for fn_name in &function_names {
                result.relationships.push(ExtractedRelationship {
                    from_name: primary_class.clone(),
                    to_name: fn_name.clone(),
                    relationship_type: RelationshipType::Contains,
                    confidence: 0.8,
                });
            }
        }
        
        // Extract function calls (CALLS relationships)
        let defined_functions: std::collections::HashSet<&str> = 
            function_names.iter().map(|s| s.as_str()).collect();
        
        for cap in FUNCTION_CALL_PATTERN.captures_iter(content) {
            if let Some(called_fn) = cap.get(1) {
                let called_name = called_fn.as_str();
                // Only track calls to functions defined in this file
                if defined_functions.contains(called_name) {
                    if let Some(caller) = function_names.first() {
                        if caller != called_name {
                            result.relationships.push(ExtractedRelationship {
                                from_name: caller.clone(),
                                to_name: called_name.to_string(),
                                relationship_type: RelationshipType::Calls,
                                confidence: 0.7,
                            });
                        }
                    }
                }
            }
        }
        
        result
    }
}

impl Default for CodeEntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_rust_function() {
        let extractor = CodeEntityExtractor::new();
        let code = r#"
        pub fn calculate_sum(a: i32, b: i32) -> i32 {
            a + b
        }
        "#;
        
        let result = extractor.extract(code, Some("rust"));
        assert!(!result.is_empty());
        assert!(result.iter().any(|e| e.name == "calculate_sum"));
    }
    
    #[test]
    fn test_extract_class() {
        let extractor = CodeEntityExtractor::new();
        let code = r#"
        pub struct UserService {
            db: Database,
        }
        "#;
        
        let result = extractor.extract(code, Some("rust"));
        assert!(result.iter().any(|e| e.name == "UserService"));
    }
}
