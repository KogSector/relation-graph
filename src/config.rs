//! Configuration module for relation-graph service

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    // Server
    pub port: u16,
    pub host: String,
    
    // Neo4j
    pub neo4j_uri: String,
    pub neo4j_user: String,
    pub neo4j_password: String,
    pub neo4j_database: String,
    
    // Zilliz
    pub zilliz_endpoint: String,
    pub zilliz_api_key: String,
    pub zilliz_collection: String,
    pub vector_dimension: usize,
    
    // PostgreSQL
    pub database_url: String,
    
    // Service URLs
    pub embedding_service_url: String,
    pub chunker_service_url: String,
    pub data_connector_service_url: String,
    
    // Cross-source linking
    pub similarity_threshold: f32,
    pub max_cross_links_per_chunk: usize,
    pub enable_temporal_proximity: bool,
    pub enable_explicit_mentions: bool,
    pub enable_author_overlap: bool,
    pub temporal_proximity_days: i64,
    
    // Graph traversal
    pub max_graph_hops: usize,
    pub max_entities_per_traversal: usize,
    
    // Redis (optional)
    pub redis_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            port: env::var("PORT")
                .unwrap_or_else(|_| "3018".to_string())
                .parse()
                .expect("Invalid PORT"),
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            
            neo4j_uri: env::var("NEO4J_URI")
                .unwrap_or_else(|_| "bolt://localhost:7687".to_string()),
            neo4j_user: env::var("NEO4J_USER")
                .unwrap_or_else(|_| "neo4j".to_string()),
            neo4j_password: env::var("NEO4J_PASSWORD")
                .unwrap_or_else(|_| "password".to_string()),
            neo4j_database: env::var("NEO4J_DATABASE")
                .unwrap_or_else(|_| "neo4j".to_string()),
            
            zilliz_endpoint: env::var("ZILLIZ_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:19530".to_string()),
            zilliz_api_key: env::var("ZILLIZ_API_KEY")
                .unwrap_or_default(),
            zilliz_collection: env::var("ZILLIZ_COLLECTION")
                .unwrap_or_else(|_| "knowledge_vectors".to_string()),
            vector_dimension: env::var("VECTOR_DIMENSION")
                .unwrap_or_else(|_| "1024".to_string())
                .parse()
                .unwrap_or(1024),
            
            database_url: env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set"),
            
            embedding_service_url: env::var("EMBEDDING_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:8082".to_string()),
            chunker_service_url: env::var("CHUNKER_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:3017".to_string()),
            data_connector_service_url: env::var("DATA_CONNECTOR_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:3013".to_string()),
            
            similarity_threshold: env::var("SIMILARITY_THRESHOLD")
                .unwrap_or_else(|_| "0.75".to_string())
                .parse()
                .unwrap_or(0.75),
            max_cross_links_per_chunk: env::var("MAX_CROSS_LINKS_PER_CHUNK")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            enable_temporal_proximity: env::var("ENABLE_TEMPORAL_PROXIMITY")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_explicit_mentions: env::var("ENABLE_EXPLICIT_MENTIONS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_author_overlap: env::var("ENABLE_AUTHOR_OVERLAP")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            temporal_proximity_days: env::var("TEMPORAL_PROXIMITY_DAYS")
                .unwrap_or_else(|_| "7".to_string())
                .parse()
                .unwrap_or(7),
            
            max_graph_hops: env::var("MAX_GRAPH_HOPS")
                .unwrap_or_else(|_| "2".to_string())
                .parse()
                .unwrap_or(2),
            max_entities_per_traversal: env::var("MAX_ENTITIES_PER_TRAVERSAL")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .unwrap_or(50),
            
            redis_url: env::var("REDIS_URL").ok(),
        }
    }
}
