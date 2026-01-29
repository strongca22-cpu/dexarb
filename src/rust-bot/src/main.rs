//! Phase 1 DEX Arbitrage Bot (V3-only)
//!
//! Main entry point for the arbitrage bot.
//! Connects to Polygon, syncs V3 pool states, and monitors for opportunities.
//! V3-only: V2 pools dropped (price inversion bug, wastes 5-8s + 84 RPC calls/cycle).
//! 1% fee tier excluded (all 1% pools on Polygon have phantom liquidity).
//! V3 sync parallelized: slot0+liquidity calls run concurrently via join_all.
//!
//! Author: AI-Generated
//! Created: 2026-01-27
//! Modified: 2026-01-27 - Added opportunity detection (Day 3)
//! Modified: 2026-01-28 - Added trade execution (Day 4)
//! Modified: 2026-01-29 - Added V3 pool support for fee tier arbitrage
//! Modified: 2026-01-29 - V3-only: drop V2, drop 1%, parallelize sync, 3s poll

use anyhow::Result;
use dexarb_bot::arbitrage::{OpportunityDetector, TradeExecutor};
use dexarb_bot::config::load_config;
use dexarb_bot::pool::{PoolStateManager, V3PoolSyncer};
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

    info!("Phase 1 DEX Arbitrage Bot Starting (V3-only, parallel sync)...");

    // Load configuration
    let config = load_config()?;
    info!("Configuration loaded");
    info!("RPC URL: {}", &config.rpc_url[..40.min(config.rpc_url.len())]);
    info!("Trading pairs: {}", config.pairs.len());
    info!("Poll interval: {}ms", config.poll_interval_ms);

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

    // Initialize V3 pool syncer (V2 dropped — price inversion bug, wastes 5-8s/cycle)
    let mut v3_syncer = V3PoolSyncer::new(Arc::clone(&provider), config.clone());
    let v3_enabled = config.uniswap_v3_factory.is_some();
    if v3_enabled {
        info!("V3 pool syncer initialized (Uniswap V3 factory configured)");
    } else {
        warn!("V3 pools DISABLED - UNISWAP_V3_FACTORY not configured");
    }

    // Initialize opportunity detector
    let detector = OpportunityDetector::new(config.clone(), state_manager.clone());
    info!("Opportunity detector initialized (V3-only, 0.05% + 0.30% tiers)");

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
        warn!("LIVE TRADING MODE ENABLED - REAL MONEY AT RISK!");
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

    // Initial V3 pool sync (full discovery: factory lookup, token addresses, decimals)
    // This is sequential — runs once at startup to discover pool addresses and cache metadata
    if v3_enabled {
        info!("Performing initial V3 pool sync (full discovery, sequential)...");
        match v3_syncer.sync_all_v3_pools().await {
            Ok(v3_pools) => {
                for pool in v3_pools {
                    state_manager.update_v3_pool(pool);
                }
                info!("V3 initial sync complete: {} pools (0.05% + 0.30% tiers)", state_manager.v3_pool_count());
            }
            Err(e) => {
                warn!("V3 sync failed: {} - cannot start without V3 pools", e);
                return Err(e);
            }
        }
    }

    let v3_count = state_manager.v3_pool_count();
    info!("Synced {} V3 pools (V2 dropped, 1% excluded)", v3_count);

    // Display V3 pool states
    for pair_config in &config.pairs {
        info!("--- {} ---", pair_config.symbol);

        if let Some(v3_005) = state_manager.get_v3_pool(types::DexType::UniswapV3_005, &pair_config.symbol) {
            info!("  V3 0.05%:  price={:.6}, tick={}, liq={}", v3_005.price(), v3_005.tick, v3_005.liquidity);
        }
        if let Some(v3_030) = state_manager.get_v3_pool(types::DexType::UniswapV3_030, &pair_config.symbol) {
            info!("  V3 0.30%:  price={:.6}, tick={}, liq={}", v3_030.price(), v3_030.tick, v3_030.liquidity);
        }
    }

    info!("Bot initialized successfully");
    info!("Starting opportunity detection loop (parallel V3 sync)...");

    // Statistics tracking
    let mut total_opportunities: u64 = 0;
    let mut total_scans: u64 = 0;

    // Main monitoring loop
    let poll_interval = std::time::Duration::from_millis(config.poll_interval_ms);
    let mut iteration = 0u64;

    loop {
        iteration += 1;
        total_scans += 1;

        // Fast parallel V3 sync: slot0 + liquidity on known pools
        // All pools synced concurrently via join_all (~200ms vs ~5.6s sequential)
        if v3_enabled {
            let known = state_manager.get_all_v3_pools();
            let updated = v3_syncer.sync_known_pools_parallel(&known).await;
            for pool in updated {
                state_manager.update_v3_pool(pool);
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

            // Try opportunities in order (best first, fall through on Quoter rejections)
            for (rank, opp) in opportunities.iter().enumerate() {
                info!(
                    "🎯 TRY #{}: {} - Buy {:?} Sell {:?} - ${:.2}",
                    rank + 1,
                    opp.pair.symbol,
                    opp.buy_dex,
                    opp.sell_dex,
                    opp.estimated_profit
                );

                match executor.execute(opp).await {
                    Ok(result) => {
                        if result.success {
                            info!(
                                "✅ Trade complete: {} | Net profit: ${:.2} | Time: {}ms",
                                result.opportunity,
                                result.net_profit_usd,
                                result.execution_time_ms
                            );
                            break; // Stop after successful trade
                        } else {
                            let error_msg = result.error.unwrap_or_else(|| "Unknown".to_string());
                            if error_msg.contains("Quoter") {
                                // Quoter rejection = zero capital risk, try next opportunity
                                info!(
                                    "⏭️ Quoter rejected #{} {} ({}), trying next...",
                                    rank + 1, result.opportunity, error_msg
                                );
                                continue;
                            } else {
                                // On-chain failure (buy/sell failed) — stop immediately
                                warn!(
                                    "❌ Trade failed: {} | Error: {}",
                                    result.opportunity, error_msg
                                );
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Execution error: {}", e);
                        break; // Stop on unexpected errors
                    }
                }
            }
        }

        // Log status periodically
        if iteration % 100 == 0 {
            let (_, v3_count, min_block, max_block) = state_manager.combined_stats();
            info!(
                "📈 Iteration {} | {} V3 pools | blocks {}-{} | {} opps found / {} scans",
                iteration, v3_count, min_block, max_block, total_opportunities, total_scans
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}
