//! Phase 1 DEX Arbitrage Bot
//!
//! Main entry point for the arbitrage bot.
//! Connects to Polygon, syncs pool states, and monitors for opportunities.
//!
//! Author: AI-Generated
//! Created: 2026-01-27
//! Modified: 2026-01-27 - Added opportunity detection (Day 3)

mod arbitrage;
mod config;
mod pool;
mod types;

use anyhow::Result;
use arbitrage::OpportunityDetector;
use config::load_config;
use ethers::prelude::*;
use pool::{PoolStateManager, PoolSyncer};
use std::sync::Arc;
use tracing::{error, info, warn, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    info!("Phase 1 DEX Arbitrage Bot Starting...");

    // Load configuration
    let config = load_config()?;
    info!("Configuration loaded");
    info!("RPC URL: {}", &config.rpc_url[..40.min(config.rpc_url.len())]);
    info!("Trading pairs: {}", config.pairs.len());

    // Initialize provider (WebSocket for low latency)
    info!("Connecting to Polygon via WebSocket...");
    let provider = Provider::<Ws>::connect(&config.rpc_url).await?;
    let provider = Arc::new(provider);

    // Verify connection
    let block = provider.get_block_number().await?;
    info!("Connected! Current block: {}", block);

    // Initialize pool state manager
    let state_manager = PoolStateManager::new();
    info!("Pool state manager initialized");

    // Initialize pool syncer
    let syncer = PoolSyncer::new(Arc::clone(&provider), config.clone(), state_manager.clone());

    // Initialize opportunity detector
    let detector = OpportunityDetector::new(config.clone(), state_manager.clone());
    info!("Opportunity detector initialized");

    // Initial pool sync
    info!("Performing initial pool sync...");
    syncer.initial_sync().await?;

    let (pool_count, _, _) = state_manager.stats();
    info!("Synced {} pools", pool_count);

    // Display pool states
    for pair_config in &config.pairs {
        info!("--- {} ---", pair_config.symbol);

        if let Some(uni_pool) = state_manager.get_pool(types::DexType::Uniswap, &pair_config.symbol)
        {
            info!(
                "  Uniswap:   price={:.6}, reserves=({}, {})",
                uni_pool.price(),
                uni_pool.reserve0,
                uni_pool.reserve1
            );
        }

        if let Some(sushi_pool) =
            state_manager.get_pool(types::DexType::Sushiswap, &pair_config.symbol)
        {
            info!(
                "  Sushiswap: price={:.6}, reserves=({}, {})",
                sushi_pool.price(),
                sushi_pool.reserve0,
                sushi_pool.reserve1
            );
        }
    }

    info!("Bot initialized successfully");
    info!("Starting opportunity detection loop...");

    // Statistics tracking
    let mut total_opportunities: u64 = 0;
    let mut total_scans: u64 = 0;

    // Main monitoring loop
    let poll_interval = std::time::Duration::from_millis(config.poll_interval_ms);
    let mut iteration = 0u64;

    loop {
        iteration += 1;
        total_scans += 1;

        // Re-sync pools
        if let Err(e) = syncer.initial_sync().await {
            error!("Failed to sync pools: {}", e);
            tokio::time::sleep(poll_interval).await;
            continue;
        }

        // Scan for opportunities
        let opportunities = detector.scan_opportunities();

        if !opportunities.is_empty() {
            total_opportunities += opportunities.len() as u64;

            for opp in &opportunities {
                info!(
                    "📊 {} | Spread: {:.2}% | Est. Profit: ${:.2} | Size: {}",
                    opp.pair.symbol,
                    opp.spread_percent,
                    opp.estimated_profit,
                    opp.trade_size
                );
            }

            // TODO Day 4: Execute best opportunity
            if let Some(best) = opportunities.first() {
                warn!(
                    "⏸️  BEST: {} - Buy {:?} Sell {:?} - ${:.2} (execution not yet implemented)",
                    best.pair.symbol,
                    best.buy_dex,
                    best.sell_dex,
                    best.estimated_profit
                );
            }
        }

        // Log status periodically
        if iteration % 100 == 0 {
            let (count, min_block, max_block) = state_manager.stats();
            info!(
                "📈 Iteration {} | {} pools | blocks {}-{} | {} opps found / {} scans",
                iteration, count, min_block, max_block, total_opportunities, total_scans
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}
