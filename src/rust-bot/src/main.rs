//! Phase 1 DEX Arbitrage Bot (V3 + V2↔V3 cross-protocol, monolithic architecture)
//!
//! Main entry point for the arbitrage bot.
//! Syncs V3 and V2 pool state directly via RPC, detects cross-fee-tier,
//! cross-DEX, and V2↔V3 cross-protocol arbitrage opportunities,
//! and executes via Quoter+swap — all in one process.
//!
//! Architecture:
//! - Loads whitelist at startup: "active" → V3, "v2_ready" → V2
//! - V3: Uniswap V3 + SushiSwap V3 + QuickSwap V3 (Algebra)
//! - V2: QuickSwap V2 + SushiSwap V2 (constant product, 0.30% fee)
//! - Main loop: WS subscribe_blocks() → sync V3+V2 parallel → detect → execute
//! - V2↔V3 opportunities use legacy two-tx execution (ArbExecutor is V3-only)
//! - ~100ms block notification (vs 3s polling), auto-reconnect on WS drop
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
//! Modified: 2026-01-30 - WS block subscription: subscribe_blocks() replaces 3s polling
//! Modified: 2026-01-30 - V2↔V3 cross-protocol: V2PoolSyncer, decimal-adjusted pricing
//! Modified: 2026-01-31 - Multi-chain: --chain CLI arg, chain-aware .env + data paths

use anyhow::Result;
use clap::Parser;
use dexarb_bot::arbitrage::{MulticallQuoter, OpportunityDetector, TradeExecutor, VerifiedOpportunity};
use dexarb_bot::config::load_config_from_file;
use dexarb_bot::filters::WhitelistFilter;
use dexarb_bot::pool::{PoolStateManager, V2PoolSyncer, V3PoolSyncer, SUSHI_V3_FEE_TIERS, V3_FEE_TIERS};
use dexarb_bot::types::{DexType, PoolState, V3PoolState};
use dexarb_bot::price_logger::PriceLogger;
use ethers::prelude::*;
use futures::StreamExt;
use std::sync::Arc;
use tracing::{error, info, warn, Level};
use tracing_subscriber;

/// DEX Arbitrage Bot — Multi-Chain (Polygon, Base)
#[derive(Parser)]
#[command(name = "dexarb-bot")]
struct Args {
    /// Chain to run on (polygon, base)
    #[arg(short, long, env = "CHAIN", default_value = "polygon")]
    chain: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    // Parse CLI args (--chain polygon|base, or CHAIN env var)
    let args = Args::parse();
    let chain = args.chain.to_lowercase();

    // Validate chain
    match chain.as_str() {
        "polygon" | "base" => {},
        _ => anyhow::bail!("Unsupported chain: '{}'. Supported: polygon, base", chain),
    }

    info!("DEX Arbitrage Bot Starting — chain: {} (V3+V2 cross-protocol arb)...", chain);

    // Load chain-specific .env file (e.g., .env.polygon, .env.base)
    let env_file = format!(".env.{}", chain);
    let config = load_config_from_file(&env_file)?;
    info!("Configuration loaded from {} (chain_id: {})", env_file, config.chain_id);
    info!("RPC URL: {}", &config.rpc_url[..40.min(config.rpc_url.len())]);
    info!("Quote token: {:?}", config.quote_token_address);
    info!("Gas cost estimate: ${:.3}", config.estimated_gas_cost_usd);
    info!("Trading pairs: {}", config.pairs.len());
    info!("Poll interval: {}ms", config.poll_interval_ms);

    // Initialize providers (two WS connections to avoid subscription contention)
    // Provider 1: RPC calls (sync, Quoter, execution)
    // Provider 2: Block subscription only (dedicated reader for newHeads)
    info!("Connecting to {} via WebSocket (RPC + subscription)...", config.chain_name);
    let provider = Provider::<Ws>::connect(&config.rpc_url).await?;
    let provider = Arc::new(provider);
    let sub_provider = Provider::<Ws>::connect(&config.rpc_url).await?;

    // Verify connection
    let block = provider.get_block_number().await?;
    info!("Connected! Current block: {} (2 WS connections)", block);

    // Load whitelist (chain-specific default: config/{chain}/pools_whitelist.json)
    let default_whitelist = format!("/home/botuser/bots/dexarb/config/{}/pools_whitelist.json", config.chain_name);
    let whitelist_path = config.whitelist_file.as_deref()
        .unwrap_or(&default_whitelist);
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

    // Initial V2 sync: discover full state for each v2_ready whitelisted pool
    // V2 pools use constant-product AMM with 0.30% fee. Syncs token0, token1,
    // decimals, and reserves. Enables V2↔V3 cross-protocol arbitrage detection.
    let v2_syncer = V2PoolSyncer::new(Arc::clone(&provider));
    let mut v2_pools: Vec<PoolState> = Vec::new();
    let v2_ready_whitelist: Vec<_> = whitelist.raw.whitelist.pools.iter()
        .filter(|p| p.status == "v2_ready")
        .collect();

    if !v2_ready_whitelist.is_empty() {
        info!("Initial V2 sync: {} v2_ready pools to discover...", v2_ready_whitelist.len());

        for wl_pool in &v2_ready_whitelist {
            // Map whitelist dex field → DexType
            let dex_type = match wl_pool.dex.as_str() {
                "QuickSwapV2" => DexType::QuickSwapV2,
                "SushiSwapV2" => DexType::SushiSwapV2,
                other => {
                    warn!("Unknown V2 dex '{}' for {} — skipping", other, wl_pool.pair);
                    continue;
                }
            };

            let pool_address: Address = match wl_pool.address.parse() {
                Ok(addr) => addr,
                Err(e) => {
                    warn!("Invalid V2 address '{}' for {} — skipping: {}", wl_pool.address, wl_pool.pair, e);
                    continue;
                }
            };

            match v2_syncer.sync_pool_by_address(pool_address, dex_type).await {
                Ok(mut pool_state) => {
                    pool_state.pair.symbol = wl_pool.pair.clone();
                    info!(
                        "  V2 synced: {} on {:?} | dec=({},{}) reserves=({}, {})",
                        wl_pool.pair, dex_type,
                        pool_state.token0_decimals, pool_state.token1_decimals,
                        pool_state.reserve0, pool_state.reserve1
                    );
                    v2_pools.push(pool_state);
                }
                Err(e) => {
                    warn!("  V2 failed: {} ({}): {}", wl_pool.pair, wl_pool.address, e);
                }
            }
        }
        info!("Initial V2 sync complete: {}/{} pools discovered", v2_pools.len(), v2_ready_whitelist.len());
    }

    // Initialize pool state manager and populate with initial sync data
    let state_manager = PoolStateManager::new();
    for pool in &v3_pools {
        state_manager.update_v3_pool(pool.clone());
    }
    for pool in &v2_pools {
        state_manager.update_pool(pool.clone());
    }
    info!(
        "Pool state manager initialized: {} V3 + {} V2 pools",
        v3_pools.len(), v2_pools.len()
    );

    // Startup cross-check: compare V2 and V3 prices for same pairs.
    // Catches wrong pool addresses, unexpected token ordering, or decimal issues.
    // V2 price_adjusted() and V3 price() should produce similar values for the
    // same pair at the same block. Divergence > 5% is a red flag.
    if !v2_pools.is_empty() && !v3_pools.is_empty() {
        info!("=== V2↔V3 Price Cross-Check (startup validation) ===");
        for v2_pool in &v2_pools {
            let pair_sym = &v2_pool.pair.symbol;
            let v2_price = v2_pool.price_adjusted();
            // Find a V3 pool with the same pair symbol for comparison
            let v3_match = v3_pools.iter()
                .filter(|p| p.pair.symbol == *pair_sym)
                .min_by(|a, b| {
                    // Pick the V3 pool with the most liquidity as reference
                    b.liquidity.cmp(&a.liquidity)
                });
            if let Some(v3_pool) = v3_match {
                let v3_price = v3_pool.price();
                let divergence = if v3_price > 0.0 {
                    ((v2_price - v3_price) / v3_price * 100.0).abs()
                } else {
                    f64::MAX
                };
                let status = if divergence < 1.0 {
                    "OK"
                } else if divergence < 5.0 {
                    "WARN"
                } else {
                    "ALERT"
                };
                info!(
                    "  [{}] {} | V2({:?})={:.8} | V3({:?})={:.8} | div={:.2}% | t0={:?} t1={:?}",
                    status, pair_sym, v2_pool.dex, v2_price,
                    v3_pool.dex, v3_price, divergence,
                    v2_pool.pair.token0, v2_pool.pair.token1
                );
                if divergence > 5.0 {
                    warn!(
                        "V2↔V3 price divergence {:.2}% for {} — check pool address or token ordering!",
                        divergence, pair_sym
                    );
                }
            } else {
                info!("  [SKIP] {} | V2({:?})={:.8} | No V3 match found", pair_sym, v2_pool.dex, v2_price);
            }
        }
        info!("=== End cross-check ===");
    }

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
            .unwrap_or_else(|| format!("/home/botuser/bots/dexarb/data/{}/tax", config.chain_name));
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
            .unwrap_or_else(|| format!("/home/botuser/bots/dexarb/data/{}/price_history", config.chain_name));
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

    // Log multicall pre-screen status
    if config.skip_multicall_prescreen {
        info!("Multicall pre-screen DISABLED — opportunities go direct to executor");
    } else {
        info!("Multicall pre-screen enabled (batch Quoter verification)");
    }

    info!("Bot initialized successfully (monolithic mode)");
    info!("Starting opportunity detection loop (WS block subscription)...");

    // Statistics tracking
    let mut total_opportunities: u64 = 0;
    let mut total_scans: u64 = 0;
    let mut last_block: u64 = 0;

    // Main monitoring loop — WS block subscription (reacts to new blocks in ~100ms)
    // If subscription drops, bot exits (restart via tmux/supervisor).
    let mut iteration = 0u64;

    // Subscribe to new blocks via dedicated WS connection
    info!("Subscribing to new blocks via WebSocket (dedicated connection)...");
    let mut block_stream = sub_provider.subscribe_blocks().await?;
    info!("WS block subscription active — reacting to blocks in real-time");

    while let Some(block) = block_stream.next().await {
            let current_block = block.number.map(|n| n.as_u64()).unwrap_or(last_block + 1);

            iteration += 1;
            total_scans += 1;

            // Log status periodically
            if iteration % 100 == 0 {
                let (v2_count, v3_count, min_block, max_block) = state_manager.combined_stats();
                info!(
                    "Iteration {} (WS) | {} V3 + {} V2 pools | blocks {}-{} | {} opps found / {} scans | block {}",
                    iteration, v3_count, v2_count, min_block, max_block, total_opportunities, total_scans, current_block
                );
            }

            // Skip duplicate blocks (WS can deliver same block twice)
            if current_block <= last_block {
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
                continue;
            }

            // Sync V2 pools concurrently (reserves only — 1 RPC call per pool)
            // V2 reserves change less frequently than V3 ticks but must stay current
            // for accurate V2↔V3 cross-protocol price comparison.
            if !v2_pools.is_empty() {
                let updated_v2 = v2_syncer.sync_known_pools_parallel(&v2_pools).await;
                v2_pools = updated_v2;
                for pool in &v2_pools {
                    state_manager.update_pool(pool.clone());
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

                // Build execution order: either multicall-verified or estimated-profit-sorted
                // Vec of (original_index, optional quoted_profit_raw for logging)
                let execution_order: Vec<(usize, Option<i128>)> = if config.skip_multicall_prescreen {
                    // Direct path: skip batch_verify(), sort by estimated_profit descending
                    // Executor's own Quoter + eth_estimateGas still protects capital.
                    let mut indices: Vec<usize> = (0..opportunities.len()).collect();
                    indices.sort_by(|a, b| {
                        opportunities[*b].estimated_profit
                            .partial_cmp(&opportunities[*a].estimated_profit)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    info!(
                        "Multicall pre-screen SKIPPED — {} opportunities sorted by est. profit, direct to executor",
                        indices.len()
                    );
                    indices.into_iter().map(|i| (i, None)).collect()
                } else {
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

                    ranked.into_iter().map(|v| (v.original_index, Some(v.quoted_profit_raw))).collect()
                };

                // Try opportunities in ranked order (best first, fall through on Quoter rejections)
                for (rank, (idx, quoted_profit)) in execution_order.iter().enumerate() {
                    let opp = &opportunities[*idx];
                    if let Some(qp) = quoted_profit {
                        info!(
                            "TRY #{}: {} - Buy {:?} Sell {:?} - ${:.2} (quoted_profit_raw={})",
                            rank + 1, opp.pair.symbol, opp.buy_dex, opp.sell_dex,
                            opp.estimated_profit, qp
                        );
                    } else {
                        info!(
                            "TRY #{}: {} - Buy {:?} Sell {:?} - ${:.2} (est, direct to executor)",
                            rank + 1, opp.pair.symbol, opp.buy_dex, opp.sell_dex,
                            opp.estimated_profit
                        );
                    }

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
    } // end while let Some(block)

    // Stream ended — WS disconnected. Exit so supervisor can restart.
    error!("WS block subscription stream ended — exiting for restart");
    Ok(())
}
