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
    
    // =========================================================================
    // VECTOR EMBEDDING METHODS (Neo4j 5.21+)
    // =========================================================================
    
    /// Create a vector index for similarity search
    /// 
    /// Note: Neo4j 5.21+ supports native vector indexes
    pub async fn create_vector_index(
        &self,
        index_name: &str,
        label: &str,
        property: &str,
        dimension: usize,
    ) -> GraphResult<()> {
        let cypher = format!(
            r#"
            CREATE VECTOR INDEX {} IF NOT EXISTS
            FOR (n:{})
            ON (n.{})
            OPTIONS {{
                indexConfig: {{
                    `vector.dimensions`: {},
                    `vector.similarity_function`: 'cosine'
                }}
            }}
            "#,
            index_name, label, property, dimension
        );
        
        self.graph.execute(query(&cypher))
            .await
            .map_err(|e| GraphError::Neo4j(format!("Failed to create vector index: {}", e)))?;
        
        tracing::info!("✅ Created vector index '{}' on {}({})", index_name, label, property);
        Ok(())
    }
    
    /// Set embedding on an existing node
    pub async fn set_node_embedding(
        &self,
        node_id: &str,
        embedding: Vec<f32>,
        model: &str,
        provider: &str,
    ) -> GraphResult<()> {
        let cypher = r#"
            MATCH (n {id: $node_id})
            SET n.embedding = $embedding,
                n.embedding_model = $model,
                n.embedding_provider = $provider,
                n.embedding_timestamp = datetime()
            RETURN n.id
        "#;
        
        // Convert Vec<f32> to Vec<f64> for Neo4j
        let embedding_f64: Vec<f64> = embedding.iter().map(|&x| x as f64).collect();
        
        self.graph.execute(
            query(cypher)
                .param("node_id", node_id)
                .param("embedding", embedding_f64)
                .param("model", model)
                .param("provider", provider)
        )
        .await
        .map_err(|e| GraphError::Neo4j(format!("Failed to set embedding: {}", e)))?;
        
        Ok(())
    }
    
    /// Batch set embeddings on multiple nodes
    pub async fn batch_set_embeddings(
        &self,
        updates: Vec<(String, Vec<f32>, String, String)>, // (node_id, embedding, model, provider)
    ) -> GraphResult<usize> {
        if updates.is_empty() {
            return Ok(0);
        }
        
        let cypher = r#"
            UNWIND $updates AS update
            MATCH (n {id: update.node_id})
            SET n.embedding = update.embedding,
                n.embedding_model = update.model,
                n.embedding_provider = update.provider,
                n.embedding_timestamp = datetime()
            RETURN count(n) as updated_count
        "#;
        
        // Convert to parameter format
        let updates_param: Vec<serde_json::Value> = updates.iter().map(|(id, emb, model, provider)| {
            let emb_f64: Vec<f64> = emb.iter().map(|&x| x as f64).collect();
            serde_json::json!({
                "node_id": id,
                "embedding": emb_f64,
                "model": model,
                "provider": provider
            })
        }).collect();
        
        let mut result = self.graph.execute(
            query(cypher).param("updates", serde_json::to_string(&updates_param).unwrap_or_default())
        )
        .await
        .map_err(|e| GraphError::Neo4j(format!("Failed to batch set embeddings: {}", e)))?;
        
        if let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            let count: i64 = row.get("updated_count").unwrap_or(0);
            Ok(count as usize)
        } else {
            Ok(0)
        }
    }
    
    /// Find similar nodes using vector index
    /// 
    /// Returns Vec<(node_id, similarity_score)>
    pub async fn find_similar_nodes(
        &self,
        embedding: Vec<f32>,
        index_name: &str,
        limit: usize,
        min_score: f32,
    ) -> GraphResult<Vec<(String, f32)>> {
        let cypher = format!(
            r#"
            CALL db.index.vector.queryNodes('{}', $limit, $embedding)
            YIELD node, score
            WHERE score >= $min_score
            RETURN node.id as node_id, score
            "#,
            index_name
        );
        
        let embedding_f64: Vec<f64> = embedding.iter().map(|&x| x as f64).collect();
        
        let mut result = self.graph.execute(
            query(&cypher)
                .param("embedding", embedding_f64)
                .param("limit", limit as i64)
                .param("min_score", min_score as f64)
        )
        .await
        .map_err(|e| GraphError::Neo4j(format!("Vector search failed: {}", e)))?;
        
        let mut similar = Vec::new();
        while let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            if let (Ok(id), Ok(score)) = (
                row.get::<String>("node_id"),
                row.get::<f64>("score"),
            ) {
                similar.push((id, score as f32));
            }
        }
        
        Ok(similar)
    }
    
    /// Find similar chunks for cross-source linking
    /// 
    /// Combines vector similarity with confidence boosters in a single query
    pub async fn find_similar_chunks_for_linking(
        &self,
        source_chunk_id: &str,
        target_source_kind: &str,
        limit: usize,
        min_similarity: f32,
    ) -> GraphResult<Vec<CrossSourceMatch>> {
        let cypher = r#"
            // Get source chunk and its embedding
            MATCH (source:CHUNK {id: $source_id})
            WHERE source.embedding IS NOT NULL
            
            // Vector similarity search
            CALL db.index.vector.queryNodes('chunk_embedding_idx', $limit * 2, source.embedding)
            YIELD node AS target, score
            
            // Filter by target source kind and minimum similarity
            WHERE target.source_kind = $target_kind
              AND target.id <> $source_id
              AND score >= $min_similarity
            
            // Calculate confidence boosters
            WITH source, target, score,
                 // Explicit mention boost: +15% if document mentions entity name
                 CASE 
                     WHEN source.entity_names IS NOT NULL 
                          AND any(name IN source.entity_names WHERE target.content CONTAINS name)
                     THEN 0.15 
                     ELSE 0 
                 END AS mention_boost,
                 // Author overlap boost: +10% if authors match
                 CASE 
                     WHEN source.author IS NOT NULL 
                          AND target.author IS NOT NULL 
                          AND source.author = target.author 
                     THEN 0.10 
                     ELSE 0 
                 END AS author_boost
            
            // Calculate final confidence
            WITH source, target, score, mention_boost, author_boost,
                 (score + mention_boost + author_boost) AS confidence
            
            RETURN 
                target.id AS target_id,
                target.content AS target_content,
                target.source_type AS target_source_type,
                target.file_path AS target_file_path,
                score AS similarity_score,
                confidence,
                mention_boost > 0 AS has_explicit_mention,
                author_boost > 0 AS has_author_overlap
            ORDER BY confidence DESC
            LIMIT $limit
        "#;
        
        let mut result = self.graph.execute(
            query(cypher)
                .param("source_id", source_chunk_id)
                .param("target_kind", target_source_kind)
                .param("limit", limit as i64)
                .param("min_similarity", min_similarity as f64)
        )
        .await
        .map_err(|e| GraphError::Neo4j(format!("Cross-source search failed: {}", e)))?;
        
        let mut matches = Vec::new();
        while let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            matches.push(CrossSourceMatch {
                target_id: row.get("target_id").unwrap_or_default(),
                target_content: row.get("target_content").ok(),
                target_source_type: row.get("target_source_type").ok(),
                target_file_path: row.get("target_file_path").ok(),
                similarity_score: row.get::<f64>("similarity_score").unwrap_or(0.0) as f32,
                confidence: row.get::<f64>("confidence").unwrap_or(0.0) as f32,
                has_explicit_mention: row.get("has_explicit_mention").unwrap_or(false),
                has_author_overlap: row.get("has_author_overlap").unwrap_or(false),
            });
        }
        
        Ok(matches)
    }
    
    /// Create cross-source relationship with evidence
    pub async fn create_cross_source_link(
        &self,
        from_id: &str,
        to_id: &str,
        confidence: f32,
        similarity_score: f32,
        has_explicit_mention: bool,
        has_author_overlap: bool,
    ) -> GraphResult<String> {
        let cypher = r#"
            MATCH (a {id: $from_id}), (b {id: $to_id})
            MERGE (a)-[r:SEMANTICALLY_SIMILAR]->(b)
            SET r.confidence = $confidence,
                r.similarity_score = $similarity_score,
                r.explicit_mention = $explicit_mention,
                r.author_overlap = $author_overlap,
                r.created_at = datetime(),
                r.updated_at = datetime()
            RETURN elementId(r) as rel_id
        "#;
        
        let mut result = self.graph.execute(
            query(cypher)
                .param("from_id", from_id)
                .param("to_id", to_id)
                .param("confidence", confidence as f64)
                .param("similarity_score", similarity_score as f64)
                .param("explicit_mention", has_explicit_mention)
                .param("author_overlap", has_author_overlap)
        )
        .await
        .map_err(|e| GraphError::Neo4j(e.to_string()))?;
        
        if let Some(row) = result.next().await.map_err(|e| GraphError::Neo4j(e.to_string()))? {
            let rel_id: String = row.get("rel_id").map_err(|e| GraphError::Neo4j(e.to_string()))?;
            Ok(rel_id)
        } else {
            Err(GraphError::Neo4j("Failed to create cross-source link".to_string()))
        }
    }
    
    /// Initialize vector indexes for the knowledge graph
    pub async fn initialize_vector_indexes(&self, dimension: usize) -> GraphResult<()> {
        // Create index for chunks
        self.create_vector_index("chunk_embedding_idx", "CHUNK", "embedding", dimension).await?;
        
        // Create indexes for main entity types
        for label in &["FUNCTION", "CLASS", "DOCUMENT", "SECTION", "CONCEPT", "FILE", "MODULE"] {
            let index_name = format!("{}_embedding_idx", label.to_lowercase());
            self.create_vector_index(&index_name, label, "embedding", dimension).await?;
        }
        
        tracing::info!("✅ All vector indexes initialized");
        Ok(())
    }
}

/// Result of a cross-source similarity search
#[derive(Debug, Clone)]
pub struct CrossSourceMatch {
    pub target_id: String,
    pub target_content: Option<String>,
    pub target_source_type: Option<String>,
    pub target_file_path: Option<String>,
    pub similarity_score: f32,
    pub confidence: f32,
    pub has_explicit_mention: bool,
    pub has_author_overlap: bool,
}

