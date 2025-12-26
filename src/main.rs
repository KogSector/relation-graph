//! Relation Graph Service - Main Entry Point
//!
//! A unified knowledge graph service combining Neo4j graph relationships
//! with native vector embeddings for hybrid code and documentation search.

use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod error;
mod models;
mod graph_db;
mod extractors;
mod services;
mod handlers;

use config::Config;
use graph_db::Neo4jClient;
use handlers::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "relation_graph=info,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    dotenvy::dotenv().ok();
    let config = Config::from_env();

    info!("🔷 Starting Relation Graph Service v{}", env!("CARGO_PKG_VERSION"));
    info!("Port: {}", config.port);

    // Initialize Neo4j client (now handles both graph AND vector operations)
    let neo4j_client = match Neo4jClient::new(
        &config.neo4j_uri,
        &config.neo4j_user,
        &config.neo4j_password,
    ).await {
        Ok(client) => {
            info!("✅ Neo4j connection established");
            
            // Initialize vector indexes on startup
            if let Err(e) = client.initialize_vector_indexes(384).await {
                tracing::warn!("⚠️ Failed to initialize vector indexes: {}. Will retry on first use.", e);
            } else {
                info!("✅ Neo4j vector indexes initialized (384-dim)");
            }
            
            Some(Arc::new(client))
        }
        Err(e) => {
            tracing::warn!("⚠️ Neo4j connection failed: {}. Graph and vector operations will be limited.", e);
            None
        }
    };

    // Initialize PostgreSQL pool
    let db_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;
    
    info!("✅ PostgreSQL connection established");

    // Build application state
    let state = Arc::new(AppState {
        config: config.clone(),
        neo4j: neo4j_client,
        db_pool,
    });

    // Build HTTP routes
    let app = Router::new()
        // Health check
        .route("/health", get(handlers::health_check))
        
        // Graph entity endpoints
        .route("/api/graph/entities", post(handlers::create_entity))
        .route("/api/graph/entities/:id", get(handlers::get_entity))
        .route("/api/graph/entities/:id/neighbors", get(handlers::get_neighbors))
        
        // Chunk ingestion (receives from chunker service)
        .route("/api/graph/chunks", post(handlers::ingest_chunks))
        
        // Cross-source linking
        .route("/api/graph/link", post(handlers::trigger_cross_source_linking))
        
        // Hybrid search (main query API)
        .route("/api/search", post(handlers::hybrid_search))
        .route("/api/search/vector", post(handlers::vector_search))
        .route("/api/search/graph", post(handlers::graph_search))
        
        // Statistics
        .route("/api/graph/statistics", get(handlers::get_statistics))
        
        // State
        .with_state(state)
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!("🚀 Relation Graph Service listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
