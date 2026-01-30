//! Phase 1 DEX Arbitrage Bot (V3-only, monolithic architecture)
//!
//! Main entry point for the arbitrage bot.
//! Syncs V3 pool state directly via RPC, detects cross-fee-tier and cross-DEX
//! arbitrage opportunities, and executes via Quoter+swap — all in one process.
//!
//! Architecture:
//! - Loads whitelist at startup, initial sync via sync_pool_by_address()
//! - Supports Uniswap V3 + SushiSwap V3 pools (cross-DEX arb)
//! - Main loop: fetch block → skip if same → sync_known_pools_parallel() → detect → execute
//! - ~1s cycle latency (vs ~5s with split file-polling architecture)
//! - Data collector preserved separately for paper trading / research
//!
//! Author: AI-Generated
//! Created: 2026-01-27
//! Modified: 2026-01-29 - Shared data architecture: read from JSON, eliminate RPC sync
//! Modified: 2026-01-29 - Fix buy-then-continue bug: halt on committed capital (tx_hash check)
//! Modified: 2026-01-29 - Multicall3 batch Quoter pre-screening (Phase 2.1)
//! Modified: 2026-01-30 - Monolithic architecture: direct RPC sync, no data collector dependency
//! Modified: 2026-01-30 - SushiSwap V3 cross-DEX: dual-quoter, multi-DEX whitelist mapping
//! Modified: 2026-01-30 - Historical price logging + gas estimate fix ($0.50→$0.05)
//! Modified: 2026-01-30 - QuickSwap V3 (Algebra): tri-quoter, Algebra sync, fee=0 sentinel

use anyhow::Result;
use dexarb_bot::arbitrage::{MulticallQuoter, OpportunityDetector, TradeExecutor, VerifiedOpportunity};
use dexarb_bot::config::load_config_from_file;
use dexarb_bot::filters::WhitelistFilter;
use dexarb_bot::pool::{PoolStateManager, V3PoolSyncer, SUSHI_V3_FEE_TIERS, V3_FEE_TIERS};
use dexarb_bot::types::DexType;
use dexarb_bot::price_logger::PriceLogger;
use dexarb_bot::types::V3PoolState;
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

    info!("Phase 1 DEX Arbitrage Bot Starting (monolithic, cross-DEX UniV3+SushiV3+QuickSwapV3)...");

    // Load configuration from .env.live (separate from dev/paper .env)
    let config = load_config_from_file(".env.live")?;
    info!("Configuration loaded from .env.live");
    info!("RPC URL: {}", &config.rpc_url[..40.min(config.rpc_url.len())]);
    info!("Trading pairs: {}", config.pairs.len());
    info!("Poll interval: {}ms", config.poll_interval_ms);

    // Initialize provider (WebSocket — used for sync, Quoter, and execution)
    info!("Connecting to Polygon via WebSocket...");
    let provider = Provider::<Ws>::connect(&config.rpc_url).await?;
    let provider = Arc::new(provider);

    // Verify connection
    let block = provider.get_block_number().await?;
    info!("Connected! Current block: {}", block);

    // Load whitelist
    let whitelist_path = config.whitelist_file.as_deref()
        .unwrap_or("/home/botuser/bots/dexarb/config/pools_whitelist.json");
    let whitelist = WhitelistFilter::load(whitelist_path)?;
    info!("Whitelist loaded: {} active pools from {}", whitelist.active_pool_count(), whitelist_path);

    // Initial V3 sync: discover full state for each whitelisted pool
    let mut v3_syncer = V3PoolSyncer::new(Arc::clone(&provider), config.clone());
    info!("Initial V3 sync: discovering {} whitelisted pools...", whitelist.active_pool_count());

    let mut v3_pools: Vec<V3PoolState> = Vec::new();
    let active_pools: Vec<_> = whitelist.raw.whitelist.pools.iter()
        .filter(|p| p.status == "active")
        .collect();

    for wl_pool in &active_pools {
        // Map (dex, fee_tier) → DexType using the correct fee tier table
        let dex_type = match wl_pool.dex.as_str() {
            "UniswapV3" => V3_FEE_TIERS.iter()
                .find(|(fee, _)| *fee == wl_pool.fee_tier)
                .map(|(_, dt)| *dt),
            "SushiswapV3" => SUSHI_V3_FEE_TIERS.iter()
                .find(|(fee, _)| *fee == wl_pool.fee_tier)
                .map(|(_, dt)| *dt),
            // QuickSwap V3 (Algebra): no fee tiers — single pool per pair, dynamic fees
            "QuickswapV3" => Some(DexType::QuickswapV3),
            other => {
                warn!("Unknown dex '{}' for {} — skipping", other, wl_pool.pair);
                continue;
            }
        };

        let dex_type = match dex_type {
            Some(dt) => dt,
            None => {
                warn!("Unknown fee tier {} for {} on {} — skipping", wl_pool.fee_tier, wl_pool.pair, wl_pool.dex);
                continue;
            }
        };

        let pool_address: Address = match wl_pool.address.parse() {
            Ok(addr) => addr,
            Err(e) => {
                warn!("Invalid address '{}' for {} — skipping: {}", wl_pool.address, wl_pool.pair, e);
                continue;
            }
        };

        match v3_syncer.sync_pool_by_address(pool_address, dex_type).await {
            Ok(mut pool_state) => {
                pool_state.pair.symbol = wl_pool.pair.clone();
                info!("  Synced: {} @ {}bps fee | liquidity={}", wl_pool.pair, wl_pool.fee_tier, pool_state.liquidity);
                v3_pools.push(pool_state);
            }
            Err(e) => {
                warn!("  Failed to sync {} ({}): {}", wl_pool.pair, wl_pool.address, e);
            }
        }
    }
    info!("Initial V3 sync complete: {}/{} pools discovered", v3_pools.len(), active_pools.len());

    // Initialize pool state manager and populate with initial sync data
    let state_manager = PoolStateManager::new();
    for pool in &v3_pools {
        state_manager.update_v3_pool(pool.clone());
    }
    info!("Pool state manager initialized with {} V3 pools (monolithic mode)", v3_pools.len());

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

    // Initialize Multicall3 batch Quoter pre-screener (Phase 2.1)
    // Batch-verifies all detected opportunities in 1 RPC call before execution.
    // Falls back to unfiltered execution if Multicall fails.
    let multicall_quoter = MulticallQuoter::new(Arc::clone(&provider), &config)?;

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

    // Initialize historical price logger (research)
    let mut price_logger: Option<PriceLogger> = if config.price_log_enabled {
        let log_dir = config.price_log_dir.clone()
            .unwrap_or_else(|| "/home/botuser/bots/dexarb/data/price_history".to_string());
        info!("Price logging enabled: {}", log_dir);
        Some(PriceLogger::new(&log_dir))
    } else {
        info!("Price logging disabled");
        None
    };

    // Log atomic executor status
    if let Some(addr) = config.arb_executor_address {
        info!("⚡ Atomic executor ENABLED: {:?}", addr);
    } else {
        info!("Atomic executor disabled (legacy two-tx mode)");
    }

    info!("Bot initialized successfully (monolithic mode)");
    info!("Starting opportunity detection loop (direct RPC sync)...");

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

        // Fetch current block number (1 RPC call — skip full sync if same block)
        let current_block = match provider.get_block_number().await {
            Ok(b) => b.as_u64(),
            Err(e) => {
                if iteration % 100 == 0 {
                    warn!("Failed to get block number: {}", e);
                }
                tokio::time::sleep(poll_interval).await;
                continue;
            }
        };

        // Log status periodically (BEFORE block-change gate — fires every 100 iterations)
        if iteration % 100 == 0 {
            let (_, v3_count, min_block, max_block) = state_manager.combined_stats();
            info!(
                "Iteration {} | {} V3 pools | blocks {}-{} | {} opps found / {} scans | block {}",
                iteration, v3_count, min_block, max_block, total_opportunities, total_scans, current_block
            );
        }

        // Skip if same block (no new data — saves ~21 RPC calls per skip)
        if current_block == last_block {
            tokio::time::sleep(poll_interval).await;
            continue;
        }
        last_block = current_block;

        // Sync all V3 pools concurrently (~21 RPC calls, ~400ms wall-clock)
        let updated = v3_syncer.sync_known_pools_parallel(&v3_pools).await;
        if !updated.is_empty() {
            v3_pools = updated;
            for pool in &v3_pools {
                state_manager.update_v3_pool(pool.clone());
            }

            // Log price snapshots (research — no RPC cost, just CSV writes)
            if let Some(ref mut logger) = price_logger {
                logger.log_prices(current_block, &v3_pools);
            }
        } else {
            warn!("Parallel V3 sync returned empty — keeping previous state");
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

            // Multicall3 batch pre-screen: verify all opportunities in 1 RPC call
            let verified = match multicall_quoter.batch_verify(&opportunities, &config).await {
                Ok(v) => v,
                Err(e) => {
                    warn!("Multicall batch verify failed: {} — falling back to unfiltered", e);
                    // Fallback: pass all opps through (executor's own Quoter checks still apply)
                    opportunities.iter().enumerate()
                        .map(|(i, _)| VerifiedOpportunity::passthrough(i))
                        .collect()
                }
            };

            // Filter to verified-only AND quoted-profitable, rank by quoted profit
            let mut ranked: Vec<&VerifiedOpportunity> = verified.iter()
                .filter(|v| v.both_legs_valid && v.quoted_profit_raw > 0)
                .collect();
            ranked.sort_by(|a, b| b.quoted_profit_raw.cmp(&a.quoted_profit_raw));

            let filtered_count = opportunities.len() - ranked.len();
            if filtered_count > 0 {
                info!(
                    "Multicall pre-screen: {}/{} verified, {} filtered out",
                    ranked.len(), opportunities.len(), filtered_count
                );
            }

            // Try verified opportunities (best quoted profit first, fall through on Quoter rejections)
            for (rank, verified_opp) in ranked.iter().enumerate() {
                let opp = &opportunities[verified_opp.original_index];
                info!(
                    "TRY #{}: {} - Buy {:?} Sell {:?} - ${:.2} (quoted_profit_raw={})",
                    rank + 1,
                    opp.pair.symbol,
                    opp.buy_dex,
                    opp.sell_dex,
                    opp.estimated_profit,
                    verified_opp.quoted_profit_raw
                );

                match executor.execute(opp).await {
                    Ok(result) => {
                        if result.success {
                            info!(
                                "Trade complete: {} | Net profit: ${:.2} | Time: {}ms",
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
                                    "HALT: On-chain tx submitted but trade failed: {} | Error: {} | TX: {}",
                                    result.opportunity, error_msg,
                                    result.tx_hash.as_deref().unwrap_or("?")
                                );
                                error!("Capital committed — manual recovery needed. Stopping all trading.");
                                break;
                            }

                            // No tx submitted = pre-trade rejection (zero capital risk)
                            if error_msg.contains("Quoter") || error_msg.contains("Gas price") {
                                info!(
                                    "Quoter rejected #{} {} ({}), trying next...",
                                    rank + 1, result.opportunity, error_msg
                                );
                                continue;
                            } else {
                                // Unknown pre-trade failure — stop for safety
                                warn!(
                                    "Trade failed: {} | Error: {}",
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

        tokio::time::sleep(poll_interval).await;
    }
}
