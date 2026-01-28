//! Multi-Configuration Paper Trading Binary
//!
//! Runs 12 paper trading configurations in parallel against live market data.
//! Uses the Artemis pattern (Collector/Strategy/Executor) for clean architecture.
//!
//! Usage:
//!   cargo run --bin paper-trading
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use anyhow::Result;
use dexarb_bot::config::load_config;
use dexarb_bot::paper_trading::run_paper_trading;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    info!("Multi-Configuration Paper Trading System");
    info!("=========================================");
    info!("Testing 12 strategies simultaneously on live data");
    info!("");

    // Load configuration
    let config = load_config()?;
    info!("Configuration loaded");
    info!("RPC URL: {}", &config.rpc_url[..40.min(config.rpc_url.len())]);
    info!("Trading pairs: {}", config.pairs.len());

    // Initialize provider
    info!("Connecting to Polygon via WebSocket...");
    let provider = Provider::<Ws>::connect(&config.rpc_url).await?;
    let provider = Arc::new(provider);

    // Verify connection
    let block = provider.get_block_number().await?;
    info!("Connected! Current block: {}", block);

    info!("");
    info!("Starting paper trading with 12 configurations:");
    info!("  1. Conservative    - High thresholds, low risk");
    info!("  2. Moderate        - Balanced approach");
    info!("  3. Aggressive      - Lower thresholds, higher risk");
    info!("  4. Large Trades    - $5000 max trade size");
    info!("  5. Small Trades    - $100 max trade size");
    info!("  6. WETH Only       - Single pair focus");
    info!("  7. WMATIC Only     - Single pair focus");
    info!("  8. Multi-Pair      - 3+ pairs");
    info!("  9. Fast Polling    - 20 Hz updates");
    info!(" 10. Slow Polling    - 5 Hz updates");
    info!(" 11. High Gas Limit  - Up to 200 gwei");
    info!(" 12. Low Gas Limit   - Up to 50 gwei");
    info!("");
    info!("Performance reports every 5 minutes...");
    info!("");

    // Run paper trading
    run_paper_trading(provider, config).await?;

    Ok(())
}
