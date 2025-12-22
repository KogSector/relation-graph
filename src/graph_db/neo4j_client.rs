//! Neo4j client for graph operations
//!
//! Supports both local Neo4j and Neo4j AuraDB (cloud).

use crate::error::{GraphError, GraphResult};
use crate::models::{Entity, EntityType, Relationship, RelationshipType};
use neo4rs::{Graph, query, ConfigBuilder};
use std::sync::Arc;
use uuid::Uuid;

/// Neo4j client for graph database operations
pub struct Neo4jClient {
    graph: Arc<Graph>,
    uri: String,
}

impl Neo4jClient {
    /// Create a new Neo4j client
    /// 
    /// Supports:
    /// - Local: `bolt://localhost:7687`
    /// - AuraDB: `neo4j+s://xxxxx.databases.neo4j.io`
    pub async fn new(uri: &str, user: &str, password: &str) -> GraphResult<Self> {
        tracing::info!("🔷 Connecting to Neo4j at: {}", uri);
        
        let config = ConfigBuilder::default()
            .uri(uri)
            .user(user)
            .password(password)
            .db("neo4j")
            .fetch_size(500)
            .max_connections(10)
            .build()
            .map_err(|e| GraphError::Neo4j(format!("Config build failed: {}", e)))?;
        
        let graph = Graph::connect(config)
            .await
            .map_err(|e| GraphError::Neo4j(format!("Connection failed: {}", e)))?;
        
        // Test the connection
        let mut result = graph.execute(query("RETURN 1 as test"))
            .await
            .map_err(|e| GraphError::Neo4j(format!("Connection test failed: {}", e)))?;
        
        if result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))?.is_some() {
            tracing::info!("✅ Neo4j connection established");
        }
        
        Ok(Self {
            graph: Arc::new(graph),
            uri: uri.to_string(),
        })
    }
    
    /// Check if connected to AuraDB
    pub fn is_aura(&self) -> bool {
        self.uri.contains("neo4j.io") || self.uri.starts_with("neo4j+s://")
    }
    
    /// Create an entity node in the graph
    pub async fn create_entity_node(&self, entity: &Entity) -> GraphResult<String> {
        let label = entity.entity_type.to_uppercase();
        let cypher = format!(
            r#"
            CREATE (n:{} {{
                id: $id,
                name: $name,
                source: $source,
                source_id: $source_id,
                properties: $properties,
                created_at: datetime()
            }})
            RETURN elementId(n) as node_id
            "#,
            label
        );
        
        let mut result = self.graph.execute(
            query(&cypher)
                .param("id", entity.id.to_string())
                .param("name", entity.name.clone())
                .param("source", entity.source.clone())
                .param("source_id", entity.source_id.clone())
                .param("properties", entity.properties.to_string())
        )
        .await
        .map_err(|e| GraphError::Neo4j(e.to_string()))?;
        
        if let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            let node_id: String = row.get("node_id").map_err(|e| GraphError::Neo4j(e.to_string()))?;
            Ok(node_id)
        } else {
            Err(GraphError::Neo4j("Failed to create entity node".to_string()))
        }
    }
    
    /// Find or create an entity node (upsert)
    pub async fn upsert_entity_node(&self, entity: &Entity) -> GraphResult<String> {
        let label = entity.entity_type.to_uppercase();
        let cypher = format!(
            r#"
            MERGE (n:{} {{id: $id}})
            ON CREATE SET
                n.name = $name,
                n.source = $source,
                n.source_id = $source_id,
                n.properties = $properties,
                n.created_at = datetime()
            ON MATCH SET
                n.name = $name,
                n.properties = $properties,
                n.updated_at = datetime()
            RETURN elementId(n) as node_id
            "#,
            label
        );
        
        let mut result = self.graph.execute(
            query(&cypher)
                .param("id", entity.id.to_string())
                .param("name", entity.name.clone())
                .param("source", entity.source.clone())
                .param("source_id", entity.source_id.clone())
                .param("properties", entity.properties.to_string())
        )
        .await
        .map_err(|e| GraphError::Neo4j(e.to_string()))?;
        
        if let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            let node_id: String = row.get("node_id").map_err(|e| GraphError::Neo4j(e.to_string()))?;
            Ok(node_id)
        } else {
            Err(GraphError::Neo4j("Failed to upsert entity node".to_string()))
        }
    }
    
    /// Create a relationship between two entities
    pub async fn create_relationship(
        &self,
        from_id: &str,
        to_id: &str,
        rel_type: RelationshipType,
        confidence: f32,
        properties: Option<serde_json::Value>,
    ) -> GraphResult<String> {
        let props = properties.unwrap_or(serde_json::json!({}));
        let cypher = format!(
            r#"
            MATCH (a), (b)
            WHERE a.id = $from_id AND b.id = $to_id
            CREATE (a)-[r:{} {{
                confidence: $confidence,
                properties: $properties,
                created_at: datetime()
            }}]->(b)
            RETURN elementId(r) as rel_id
            "#,
            rel_type.as_str()
        );
        
        let mut result = self.graph.execute(
            query(&cypher)
                .param("from_id", from_id)
                .param("to_id", to_id)
                .param("confidence", confidence as f64)
                .param("properties", props.to_string())
        )
        .await
        .map_err(|e| GraphError::Neo4j(e.to_string()))?;
        
        if let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            let rel_id: String = row.get("rel_id").map_err(|e| GraphError::Neo4j(e.to_string()))?;
            Ok(rel_id)
        } else {
            Err(GraphError::Neo4j("Failed to create relationship".to_string()))
        }
    }
    
    /// Get neighbors of an entity (n-hop traversal)
    pub async fn get_neighbors(
        &self,
        entity_id: &str,
        relationship_types: Option<&[RelationshipType]>,
        direction: &str,
        hops: usize,
    ) -> GraphResult<Vec<(String, String, String, f32)>> {
        // Build relationship pattern
        let rel_pattern = match relationship_types {
            Some(types) => {
                let type_strs: Vec<&str> = types.iter().map(|t| t.as_str()).collect();
                format!(":{}", type_strs.join("|"))
            }
            None => String::new(),
        };
        
        let direction_pattern = match direction {
            "outgoing" => format!("-[r{}*1..{}]->", rel_pattern, hops),
            "incoming" => format!("<-[r{}*1..{}]-", rel_pattern, hops),
            _ => format!("-[r{}*1..{}]-", rel_pattern, hops),
        };
        
        let cypher = format!(
            r#"
            MATCH (start {{id: $entity_id}}){}(end)
            WITH end, r
            UNWIND r as rel
            RETURN DISTINCT
                end.id as entity_id,
                end.name as name,
                type(rel) as rel_type,
                COALESCE(rel.confidence, 1.0) as confidence
            LIMIT 100
            "#,
            direction_pattern
        );
        
        let mut result = self.graph.execute(query(&cypher).param("entity_id", entity_id))
            .await
            .map_err(|e| GraphError::Neo4j(e.to_string()))?;
        
        let mut neighbors = Vec::new();
        while let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            if let (Ok(id), Ok(name), Ok(rel), Ok(conf)) = (
                row.get::<String>("entity_id"),
                row.get::<String>("name"),
                row.get::<String>("rel_type"),
                row.get::<f64>("confidence"),
            ) {
                neighbors.push((id, name, rel, conf as f32));
            }
        }
        
        Ok(neighbors)
    }
    
    /// Find entities by type and source
    pub async fn find_entities(
        &self,
        entity_type: Option<EntityType>,
        source: Option<&str>,
        limit: usize,
    ) -> GraphResult<Vec<(String, String, String)>> {
        let type_filter = entity_type
            .map(|t| format!(":{}", t.as_str().to_uppercase()))
            .unwrap_or_default();
        
        let source_clause = source
            .map(|s| format!("WHERE n.source = '{}'", s))
            .unwrap_or_default();
        
        let cypher = format!(
            r#"
            MATCH (n{})
            {}
            RETURN n.id as id, n.name as name, labels(n)[0] as entity_type
            LIMIT {}
            "#,
            type_filter, source_clause, limit
        );
        
        let mut result = self.graph.execute(query(&cypher))
            .await
            .map_err(|e| GraphError::Neo4j(e.to_string()))?;
        
        let mut entities = Vec::new();
        while let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            if let (Ok(id), Ok(name), Ok(entity_type)) = (
                row.get::<String>("id"),
                row.get::<String>("name"),
                row.get::<String>("entity_type"),
            ) {
                entities.push((id, name, entity_type));
            }
        }
        
        Ok(entities)
    }
    
    /// Get cross-source relationships (the unique value!)
    pub async fn get_cross_source_relationships(
        &self,
        entity_id: &str,
    ) -> GraphResult<Vec<(String, String, String, f32)>> {
        let cross_source_types = [
            "EXPLAINS", "DOCUMENTS", "SEMANTICALLY_SIMILAR", 
            "MENTIONS_EXPLICITLY", "UPDATED_NEAR"
        ];
        let types_clause = cross_source_types.join("|");
        
        let cypher = format!(
            r#"
            MATCH (a {{id: $entity_id}})-[r:{}]-(b)
            RETURN 
                b.id as target_id,
                b.name as target_name,
                type(r) as rel_type,
                COALESCE(r.confidence, 1.0) as confidence
            "#,
            types_clause
        );
        
        let mut result = self.graph.execute(query(&cypher).param("entity_id", entity_id))
            .await
            .map_err(|e| GraphError::Neo4j(e.to_string()))?;
        
        let mut relationships = Vec::new();
        while let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            if let (Ok(id), Ok(name), Ok(rel), Ok(conf)) = (
                row.get::<String>("target_id"),
                row.get::<String>("target_name"),
                row.get::<String>("rel_type"),
                row.get::<f64>("confidence"),
            ) {
                relationships.push((id, name, rel, conf as f32));
            }
        }
        
        Ok(relationships)
    }
    
    /// Get graph statistics
    pub async fn get_statistics(&self) -> GraphResult<serde_json::Value> {
        let cypher = r#"
            CALL {
                MATCH (n) RETURN count(n) as node_count
            }
            CALL {
                MATCH ()-[r]->() RETURN count(r) as rel_count
            }
            RETURN node_count, rel_count
        "#;
        
        let mut result = self.graph.execute(query(cypher))
            .await
            .map_err(|e| GraphError::Neo4j(e.to_string()))?;
        
        if let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            let node_count: i64 = row.get("node_count").unwrap_or(0);
            let rel_count: i64 = row.get("rel_count").unwrap_or(0);
            
            Ok(serde_json::json!({
                "connected": true,
                "uri": self.uri,
                "is_aura": self.is_aura(),
                "node_count": node_count,
                "relationship_count": rel_count
            }))
        } else {
            Ok(serde_json::json!({
                "connected": true,
                "uri": self.uri,
                "is_aura": self.is_aura(),
                "node_count": 0,
                "relationship_count": 0
            }))
        }
    }
}
