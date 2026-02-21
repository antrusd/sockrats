//! SocksRat - Reverse SOCKS5 Tunneling Client
//!
//! This is the main entry point for the SocksRat application.

use anyhow::Result;
use clap::Parser;
use socksrat::config::load_config;
use socksrat::client::run_client;
use std::path::PathBuf;
use tokio::sync::broadcast;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// SocksRat - Reverse SOCKS5 tunneling client using rathole protocol
#[derive(Parser, Debug)]
#[command(name = "socksrat")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: PathBuf,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Enable JSON logging format
    #[arg(long)]
    json_log: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging
    setup_logging(&args.log_level, args.json_log)?;

    // Load configuration
    let config = load_config(&args.config)?;

    info!("SocksRat v{}", socksrat::VERSION);
    info!("Configuration loaded from: {:?}", args.config);
    info!("Connecting to: {}", config.client.remote_addr);
    info!("Service name: {}", config.client.service_name);

    // Setup shutdown signal
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    // Handle Ctrl+C and termination signals (cross-platform)
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate())
                .expect("Failed to setup SIGTERM handler");

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Received Ctrl+C, shutting down...");
                }
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, shutting down...");
                }
            }
        }

        #[cfg(not(unix))]
        {
            // On Windows, only handle Ctrl+C
            let _ = tokio::signal::ctrl_c().await;
            info!("Received Ctrl+C, shutting down...");
        }

        let _ = shutdown_tx_clone.send(true);
    });

    // Run the client
    run_client(config, shutdown_rx).await
}

/// Setup logging based on configuration
fn setup_logging(level: &str, json: bool) -> Result<()> {
    let level = match level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" | "warning" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    if json {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(level)
            .json()
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(level)
            .with_target(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
    }

    Ok(())
}
