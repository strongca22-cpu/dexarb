//! Phase 1 DEX Arbitrage Bot (V2 + V3)
//!
//! Main entry point for the arbitrage bot.
//! Connects to Polygon, syncs pool states, and monitors for opportunities.
//! Now supports both V2 and V3 pools for cross-fee-tier arbitrage.
//!
//! Author: AI-Generated
//! Created: 2026-01-27
//! Modified: 2026-01-27 - Added opportunity detection (Day 3)
//! Modified: 2026-01-28 - Added trade execution (Day 4)
//! Modified: 2026-01-29 - Added V3 pool support for fee tier arbitrage

use anyhow::Result;
use dexarb_bot::arbitrage::{OpportunityDetector, TradeExecutor};
use dexarb_bot::config::load_config;
use dexarb_bot::pool::{PoolStateManager, PoolSyncer, V3PoolSyncer};
use dexarb_bot::types;
use ethers::prelude::*;
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

    // Initialize V2 pool syncer
    let syncer = PoolSyncer::new(Arc::clone(&provider), config.clone(), state_manager.clone());

    // Initialize V3 pool syncer
    let mut v3_syncer = V3PoolSyncer::new(Arc::clone(&provider), config.clone());
    let v3_enabled = config.uniswap_v3_factory.is_some();
    if v3_enabled {
        info!("V3 pool syncer initialized (Uniswap V3 factory configured)");
    } else {
        warn!("V3 pools DISABLED - UNISWAP_V3_FACTORY not configured");
    }

    // Initialize opportunity detector
    let detector = OpportunityDetector::new(config.clone(), state_manager.clone());
    info!("Opportunity detector initialized (V2 + V3)");

    // Initialize trade executor
    // Parse wallet from private key
    let wallet: LocalWallet = config
        .private_key
        .parse::<LocalWallet>()?
        .with_chain_id(config.chain_id);
    info!("Wallet loaded: {:?}", wallet.address());

    // Create executor
    let mut executor = TradeExecutor::new(Arc::clone(&provider), wallet, config.clone());

    // Set live/dry run mode based on config
    if config.live_mode {
        executor.set_dry_run(false);
        warn!("⚠️⚠️⚠️ LIVE TRADING MODE ENABLED - REAL MONEY AT RISK! ⚠️⚠️⚠️");
    } else {
        info!("Trade executor initialized (DRY RUN mode)");
    }

    // Enable tax logging for IRS compliance
    if config.tax_log_enabled {
        let tax_dir = config.tax_log_dir.clone()
            .unwrap_or_else(|| "/home/botuser/bots/dexarb/data/tax".to_string());
        match executor.enable_tax_logging(&tax_dir) {
            Ok(_) => info!("Tax logging enabled: {}", tax_dir),
            Err(e) => warn!("Failed to enable tax logging: {} - trades will NOT be logged for taxes!", e),
        }
    } else {
        warn!("Tax logging DISABLED - trades will NOT be logged for IRS compliance!");
    }

    // Initial V2 pool sync
    info!("Performing initial V2 pool sync...");
    syncer.initial_sync().await?;

    // Initial V3 pool sync
    if v3_enabled {
        info!("Performing initial V3 pool sync...");
        match v3_syncer.sync_all_v3_pools().await {
            Ok(v3_pools) => {
                for pool in v3_pools {
                    state_manager.update_v3_pool(pool);
                }
                info!("V3 sync complete: {} pools", state_manager.v3_pool_count());
            }
            Err(e) => {
                warn!("V3 sync failed: {} - continuing with V2 only", e);
            }
        }
    }

    let (v2_count, v3_count, _, _) = state_manager.combined_stats();
    info!("Synced {} V2 pools + {} V3 pools = {} total", v2_count, v3_count, v2_count + v3_count);

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

        // Display V3 pools
        if let Some(v3_005) = state_manager.get_v3_pool(types::DexType::UniswapV3_005, &pair_config.symbol) {
            info!("  V3 0.05%:  price={:.6}, tick={}", v3_005.price(), v3_005.tick);
        }
        if let Some(v3_030) = state_manager.get_v3_pool(types::DexType::UniswapV3_030, &pair_config.symbol) {
            info!("  V3 0.30%:  price={:.6}, tick={}", v3_030.price(), v3_030.tick);
        }
        if let Some(v3_100) = state_manager.get_v3_pool(types::DexType::UniswapV3_100, &pair_config.symbol) {
            info!("  V3 1.00%:  price={:.6}, tick={}", v3_100.price(), v3_100.tick);
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

        // Re-sync V2 pools
        if let Err(e) = syncer.initial_sync().await {
            error!("Failed to sync V2 pools: {}", e);
            tokio::time::sleep(poll_interval).await;
            continue;
        }

        // Re-sync V3 pools (every iteration for now, can stagger later)
        if v3_enabled {
            match v3_syncer.sync_all_v3_pools().await {
                Ok(v3_pools) => {
                    for pool in v3_pools {
                        state_manager.update_v3_pool(pool);
                    }
                }
                Err(e) => {
                    warn!("V3 sync failed: {}", e);
                }
            }
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

            // Execute best opportunity
            if let Some(best) = opportunities.first() {
                info!(
                    "🎯 BEST: {} - Buy {:?} Sell {:?} - ${:.2}",
                    best.pair.symbol,
                    best.buy_dex,
                    best.sell_dex,
                    best.estimated_profit
                );

                match executor.execute(best).await {
                    Ok(result) => {
                        if result.success {
                            info!(
                                "✅ Trade complete: {} | Net profit: ${:.2} | Time: {}ms",
                                result.opportunity,
                                result.net_profit_usd,
                                result.execution_time_ms
                            );
                        } else {
                            warn!(
                                "❌ Trade failed: {} | Error: {}",
                                result.opportunity,
                                result.error.unwrap_or_else(|| "Unknown".to_string())
                            );
                        }
                    }
                    Err(e) => {
                        error!("Execution error: {}", e);
                    }
                }
            }
        }

        // Log status periodically
        if iteration % 100 == 0 {
            let (v2_count, v3_count, min_block, max_block) = state_manager.combined_stats();
            info!(
                "📈 Iteration {} | {} V2 + {} V3 pools | blocks {}-{} | {} opps found / {} scans",
                iteration, v2_count, v3_count, min_block, max_block, total_opportunities, total_scans
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}
