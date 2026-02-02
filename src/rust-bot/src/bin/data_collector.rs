//! Data Collector Binary
//!
//! Continuously syncs pool state from the blockchain and writes
//! to a shared JSON file. Run this in a persistent tmux session.
//!
//! Usage:
//!   cargo run --bin data-collector
//!
//! The collector writes to /home/botuser/bots/dexarb/data/pool_state.json
//! which can be read by paper trading bots.
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use anyhow::Result;
use dexarb_bot::config::load_config;
use dexarb_bot::data_collector::{run_data_collector, DEFAULT_STATE_PATH};
use alloy::providers::{ProviderBuilder, WsConnect};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("===========================================");
    info!("   DEX Arbitrage Data Collector");
    info!("===========================================");

    // Load configuration
    let config = load_config()?;
    info!("Configuration loaded");

    // Create provider (alloy WebSocket)
    let ws = WsConnect::new(&config.rpc_url);
    let provider = ProviderBuilder::new().connect_ws(ws).await?;
    let provider = Arc::new(provider);
    info!("Connected to RPC: {}", &config.rpc_url[..50.min(config.rpc_url.len())]);

    // Get state file path from env or use default
    let state_path = std::env::var("STATE_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_STATE_PATH));

    // Ensure parent directory exists
    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Run the collector (runs forever)
    run_data_collector(provider, config, state_path).await?;

    Ok(())
}
