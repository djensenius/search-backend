//! # Search Engine Backend - Main Application Entry Point
//!
//! This is the main binary for the Search Engine Backend search engine.
//! It initializes all services, configures the HTTP server, and starts
//! the application with proper logging and error handling.
//!
//! ## Usage
//!
//! ```bash
//! cargo run -- --port 3000
//! ```
//!
//! ## Environment Variables
//!
//! The application requires several Azure service configuration variables.
//! See `.env.example` for a complete list of required environment variables.

use anyhow::Result;
use clap::Parser;
use dotenv::dotenv;
use search_engine_backend::{
    create_router, AppState, Args, Config, IndexerService, SearchService, StorageService,
};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info, Level};

/// Main application entry point.
///
/// Initializes logging, loads configuration, creates service instances,
/// and starts the HTTP server with appropriate middleware.
#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    let args = Args::parse();

    info!("Starting Search Engine Backend on port {}", args.port);

    // Load configuration
    let config = Arc::new(Config::from_env()?);
    info!(
        "Loaded configuration for environment: {}",
        config.environment
    );

    // Initialize services
    let storage_service = Arc::new(StorageService::new(config.clone()).await?);
    let search_service = Arc::new(SearchService::new(config.clone()).await?);
    let indexer_service = Arc::new(
        IndexerService::new(
            config.clone(),
            storage_service.clone(),
            search_service.clone(),
        )
        .await?,
    );

    // Create application state
    let app_state = AppState {
        config,
        search_service,
        storage_service,
        indexer_service: indexer_service.clone(),
    };

    // Create router with middleware
    let app = create_router(app_state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // Start server
    let listener = tokio::net::TcpListener::bind(&format!("0.0.0.0:{}", args.port)).await?;
    info!("Server listening on http://0.0.0.0:{}", args.port);

    // Start the periodic indexing service in the background
    let periodic_indexer = indexer_service.clone();
    tokio::spawn(async move {
        if let Err(e) = periodic_indexer.start_periodic_indexing().await {
            error!("Periodic indexing service failed: {}", e);
        }
    });

    // Start the crawl queue processor in the background
    let queue_processor = indexer_service.clone();
    tokio::spawn(async move {
        if let Err(e) = queue_processor.process_crawl_queue().await {
            error!("Crawl queue processor failed: {}", e);
        }
    });

    // Start the periodic duplicate removal service in the background
    let duplicate_remover = indexer_service.clone();
    tokio::spawn(async move {
        if let Err(e) = duplicate_remover.start_periodic_duplicate_removal().await {
            error!("Periodic duplicate removal service failed: {}", e);
        }
    });

    axum::serve(listener, app).await?;

    Ok(())
}
