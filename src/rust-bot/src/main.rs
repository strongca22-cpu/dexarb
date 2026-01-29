//! Phase 1 DEX Arbitrage Bot (V3-only, shared data architecture)
//!
//! Main entry point for the arbitrage bot.
//! Reads V3 pool state from shared JSON file (written by data collector).
//! Detects cross-fee-tier arbitrage opportunities and executes via Quoter+swap.
//!
//! Architecture:
//! - Data collector (separate process) syncs pools via RPC, writes JSON
//! - This bot reads JSON for pool prices (zero RPC for price discovery)
//! - RPC used ONLY for Quoter pre-checks and trade execution
//! - Adding new pairs = update data collector config, no rebuild needed here
//!
//! Author: AI-Generated
//! Created: 2026-01-27
//! Modified: 2026-01-29 - Shared data architecture: read from JSON, eliminate RPC sync
//! Modified: 2026-01-29 - Fix buy-then-continue bug: halt on committed capital (tx_hash check)

use anyhow::Result;
use dexarb_bot::arbitrage::{OpportunityDetector, TradeExecutor};
use dexarb_bot::config::load_config_from_file;
use dexarb_bot::data_collector::shared_state::SharedPoolState;
use dexarb_bot::pool::PoolStateManager;
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

    info!("Phase 1 DEX Arbitrage Bot Starting (shared data, JSON-based)...");

    // Load configuration from .env.live (separate from dev/paper .env)
    let config = load_config_from_file(".env.live")?;
    info!("Configuration loaded from .env.live");
    info!("RPC URL: {}", &config.rpc_url[..40.min(config.rpc_url.len())]);
    info!("Trading pairs: {}", config.pairs.len());
    info!("Poll interval: {}ms", config.poll_interval_ms);

    // Resolve pool state file path
    let state_file = match &config.pool_state_file {
        Some(path) => path.clone(),
        None => {
            error!("POOL_STATE_FILE not set in .env.live — required for shared data mode");
            return Err(anyhow::anyhow!("POOL_STATE_FILE not configured"));
        }
    };
    info!("Pool state file: {}", state_file);

    // Initialize provider (WebSocket — needed for Quoter checks and trade execution)
    info!("Connecting to Polygon via WebSocket (for Quoter + execution only)...");
    let provider = Provider::<Ws>::connect(&config.rpc_url).await?;
    let provider = Arc::new(provider);

    // Verify connection
    let block = provider.get_block_number().await?;
    info!("Connected! Current block: {}", block);

    // Initialize pool state manager (populated from JSON, not RPC)
    let state_manager = PoolStateManager::new();
    info!("Pool state manager initialized (shared data mode — reads from JSON)");

    // Initialize opportunity detector
    let detector = OpportunityDetector::new(config.clone(), state_manager.clone());
    info!("Opportunity detector initialized");

    // Initialize trade executor
    let wallet: LocalWallet = config
        .private_key
        .parse::<LocalWallet>()?
        .with_chain_id(config.chain_id);
    info!("Wallet loaded: {:?}", wallet.address());

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

    // Wait for initial JSON file from data collector
    info!("Waiting for data collector JSON at {}...", state_file);
    loop {
        match SharedPoolState::read_from_file(&state_file) {
            Ok(state) => {
                let v3_count = state.v3_pools.len();
                info!("Initial state loaded: {} V3 pools, block {}", v3_count, state.block_number);

                // Display pool summary
                for (key, pool) in &state.v3_pools {
                    info!("  {} | price={:.6} | tick={} | fee={}bps",
                        key, pool.validated_price(), pool.tick, pool.fee);
                }

                // Populate state manager from JSON
                for pool in state.v3_pools.values() {
                    if let Ok(p) = pool.to_v3_pool_state() {
                        state_manager.update_v3_pool(p);
                    }
                }
                break;
            }
            Err(_) => {
                warn!("State file not ready, retrying in 5s... (start data collector first)");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }

    info!("Bot initialized successfully (shared data mode)");
    info!("Starting opportunity detection loop (JSON-based, no RPC sync)...");

    // Statistics tracking
    let mut total_opportunities: u64 = 0;
    let mut total_scans: u64 = 0;
    let mut last_block: u64 = 0;

    // Main monitoring loop
    let poll_interval = std::time::Duration::from_millis(config.poll_interval_ms);
    let mut iteration = 0u64;

    loop {
        iteration += 1;
        total_scans += 1;

        // Read shared pool state from JSON (replaces V3 RPC sync)
        let shared_state = match SharedPoolState::read_from_file(&state_file) {
            Ok(state) => state,
            Err(e) => {
                if iteration % 100 == 0 {
                    warn!("Failed to read state file: {} (data collector down?)", e);
                }
                tokio::time::sleep(poll_interval).await;
                continue;
            }
        };

        // Skip if data hasn't advanced (same block = same opportunities)
        if shared_state.block_number == last_block {
            tokio::time::sleep(poll_interval).await;
            continue;
        }
        last_block = shared_state.block_number;

        // Check staleness (data collector may be dead)
        if shared_state.is_stale(60) {
            if iteration % 100 == 0 {
                warn!("State file stale (>60s old) — data collector may be down");
            }
            tokio::time::sleep(poll_interval).await;
            continue;
        }

        // Update state manager from JSON (V3 pools)
        for pool in shared_state.v3_pools.values() {
            if let Ok(p) = pool.to_v3_pool_state() {
                state_manager.update_v3_pool(p);
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

                            // CRITICAL: If a transaction was submitted on-chain, capital is
                            // committed. HALT immediately — do NOT try next opportunity.
                            // This catches: buy succeeded but sell Quoter rejected (holding tokens).
                            if result.tx_hash.is_some() {
                                error!(
                                    "🚨 HALT: On-chain tx submitted but trade failed: {} | Error: {} | TX: {}",
                                    result.opportunity, error_msg,
                                    result.tx_hash.as_deref().unwrap_or("?")
                                );
                                error!("🚨 Capital committed — manual recovery needed. Stopping all trading.");
                                break;
                            }

                            // No tx submitted = pre-trade rejection (zero capital risk)
                            if error_msg.contains("Quoter") || error_msg.contains("Gas price") {
                                info!(
                                    "⏭️ Quoter rejected #{} {} ({}), trying next...",
                                    rank + 1, result.opportunity, error_msg
                                );
                                continue;
                            } else {
                                // Unknown pre-trade failure — stop for safety
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
