//! SRT Ingest Gateway Binary
//!
//! Entry point for the SRT ingest gateway service.

use anyhow::Result;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::broadcast;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use remotemedia_ingest_srt::{
    api::{build_router, AppState},
    config::Config,
    jwt::JwtValidator,
    listener::SrtIngestListener,
    session::SessionManager,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting SRT Ingest Gateway...");

    // Load configuration
    let config = Config::from_env();
    let config = Arc::new(config);

    tracing::info!(
        "Configuration: HTTP port={}, SRT port={}, max_sessions={}",
        config.server.http_port,
        config.server.srt_port,
        config.limits.max_sessions
    );

    // Initialize session manager
    let session_manager = Arc::new(SessionManager::new(
        config.jwt.secret.clone(),
        config.limits.max_sessions,
    ));

    // Initialize JWT validator
    let jwt_validator = Arc::new(JwtValidator::new(config.jwt.secret.clone()));

    // Create shutdown signal channel
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // Spawn session cleanup task (runs every 10 seconds)
    let cleanup_handle = {
        let session_manager = session_manager.clone();
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            session_manager.run_cleanup_loop(10, shutdown_rx).await;
        })
    };

    // Create SRT listener with proper streamid-based session routing
    let srt_listener = SrtIngestListener::new(
        config.server.srt_port,
        session_manager.clone(),
        jwt_validator,
    )
    .with_shutdown(shutdown_tx.clone());

    // Spawn SRT listener task
    let srt_handle = tokio::spawn(async move {
        if let Err(e) = srt_listener.run().await {
            tracing::error!("SRT listener error: {}", e);
        }
    });

    // Create app state
    let state = AppState::new(session_manager.clone(), config.clone());

    // Build HTTP router
    let router = build_router(state);

    // Start HTTP server
    let bind_addr = format!("{}:{}", config.server.host, config.server.http_port);
    let listener = TcpListener::bind(&bind_addr).await?;
    tracing::info!("HTTP server listening on {}", bind_addr);

    // Run the HTTP server with graceful shutdown on SIGTERM/SIGINT
    let shutdown_tx_clone = shutdown_tx.clone();
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("Shutdown signal received, initiating graceful shutdown...");
            let _ = shutdown_tx_clone.send(());
        })
        .await?;

    // Signal shutdown to all background tasks
    let _ = shutdown_tx.send(());

    // Wait for background tasks to complete
    let _ = srt_handle.await;
    let _ = cleanup_handle.await;

    tracing::info!("SRT Ingest Gateway shutdown complete");
    Ok(())
}

/// Wait for shutdown signal (SIGTERM or SIGINT)
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
