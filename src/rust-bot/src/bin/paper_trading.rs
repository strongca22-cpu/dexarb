//! Paper Trading Binary (v3 - V2+V3 Arbitrage)
//!
//! Reads pool state from shared JSON file (written by data-collector)
//! and runs paper trading strategies based on TOML configuration.
//!
//! Supports both V2 and V3 pools for cross-protocol arbitrage:
//! - V2↔V2: 0.6% round-trip fee (0.3% per swap)
//! - V3↔V2: 0.35% round-trip with 0.05% V3 tier (game changer!)
//! - V3↔V3: Variable based on fee tiers
//!
//! Supports hot-reloading via SIGHUP signal:
//!   kill -HUP $(pgrep paper-trading)
//!
//! Discord Alerts:
//!   Set DISCORD_WEBHOOK environment variable to enable alerts
//!   Alerts are BATCHED and sent every 15 minutes (configurable)
//!   Each batch summarizes all opportunities detected in the window
//!
//! Usage:
//!   cargo run --bin paper-trading
//!   cargo run --bin paper-trading -- --config /path/to/config.toml
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//! Modified: 2026-01-28 (V3 arbitrage support)

use anyhow::{Context, Result};
use chrono::Utc;
use dexarb_bot::data_collector::SharedPoolState;
use dexarb_bot::paper_trading::{
    AggregatedOpportunity, MetricsAggregator, OpportunityBatcher, PaperTradingConfig,
    SimulatedTradeAction, SimulatedExecutor, StrategyMatch, TraderMetrics, TomlConfig,
};
use futures::StreamExt;
use signal_hook::consts::SIGHUP;
use signal_hook_tokio::Signals;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};

/// Default config path
const DEFAULT_CONFIG_PATH: &str = "/home/botuser/bots/dexarb/config/paper_trading.toml";

/// Reload flag - set by SIGHUP handler
static RELOAD_FLAG: AtomicBool = AtomicBool::new(false);

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
    info!("   DEX Arbitrage Paper Trading (v2)");
    info!("   File-Based Architecture with Hot Reload");
    info!("   Discord Alerts Enabled");
    info!("===========================================");

    // Get config path from args or use default
    let config_path = std::env::args()
        .nth(2)
        .filter(|_| std::env::args().nth(1).map(|a| a == "--config").unwrap_or(false))
        .unwrap_or_else(|| DEFAULT_CONFIG_PATH.to_string());

    let config_path = PathBuf::from(&config_path);
    info!("Config file: {}", config_path.display());

    // Set up SIGHUP handler
    let mut signals = Signals::new([SIGHUP])?;
    tokio::spawn(async move {
        while let Some(sig) = signals.next().await {
            if sig == SIGHUP {
                info!("Received SIGHUP - flagging config reload");
                RELOAD_FLAG.store(true, Ordering::SeqCst);
            }
        }
    });

    // Main loop - supports restarts on config reload
    loop {
        match run_paper_trading(&config_path).await {
            Ok(should_restart) => {
                if should_restart {
                    info!("Restarting with new configuration...");
                    continue;
                } else {
                    info!("Paper trading stopped normally");
                    break;
                }
            }
            Err(e) => {
                error!("Paper trading error: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }

    Ok(())
}

/// Run paper trading - returns true if should restart (config reload)
async fn run_paper_trading(config_path: &PathBuf) -> Result<bool> {
    // Load configuration
    let config = TomlConfig::load(config_path)
        .context("Failed to load configuration")?;

    let state_file = PathBuf::from(&config.general.state_file);
    info!("State file: {}", state_file.display());

    // Get enabled strategies
    let strategies = config.get_enabled_strategies();
    info!("Loaded {} enabled strategies:", strategies.len());
    for s in &strategies {
        info!("  - {} (pairs: {:?}, min_profit: ${:.2})",
            s.name, s.pairs, s.min_profit_usd);
    }

    // Initialize metrics for each strategy
    let mut strategy_metrics: HashMap<String, Arc<RwLock<TraderMetrics>>> = HashMap::new();
    let mut executors: HashMap<String, SimulatedExecutor> = HashMap::new();

    for strategy in &strategies {
        let metrics = Arc::new(RwLock::new(TraderMetrics::new(strategy.name.clone())));
        let executor = SimulatedExecutor::new(strategy.clone(), Arc::clone(&metrics));
        strategy_metrics.insert(strategy.name.clone(), metrics);
        executors.insert(strategy.name.clone(), executor);
    }

    // Initialize Discord opportunity batcher (15 minute batches by default)
    let batcher = Arc::new(OpportunityBatcher::new());
    if batcher.is_enabled() {
        info!("Discord alerts: ENABLED (batched every {} minutes)", batcher.batch_interval_secs() / 60);
    } else {
        info!("Discord alerts: DISABLED (set DISCORD_WEBHOOK to enable)");
    }

    // Spawn background task for periodic Discord batch flush
    let batcher_for_flush = Arc::clone(&batcher);
    tokio::spawn(async move {
        let interval_secs = batcher_for_flush.batch_interval_secs();
        loop {
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
            batcher_for_flush.flush_and_send().await;
        }
    });

    let poll_interval = Duration::from_millis(config.general.poll_interval_ms);
    let metrics_interval = Duration::from_secs(config.general.metrics_interval_secs);
    let max_state_age = config.general.max_state_age_secs;

    info!("Poll interval: {:?}", poll_interval);
    info!("Metrics interval: {:?}", metrics_interval);
    info!("Max state age: {}s", max_state_age);

    let mut iteration: u64 = 0;
    let mut last_metrics_report = std::time::Instant::now();

    // Main processing loop
    loop {
        // Check for reload signal
        if RELOAD_FLAG.load(Ordering::SeqCst) {
            RELOAD_FLAG.store(false, Ordering::SeqCst);
            info!("Config reload requested - restarting...");
            return Ok(true); // Signal restart
        }

        // Wait for next poll interval
        tokio::time::sleep(poll_interval).await;
        iteration += 1;

        // Read shared state file
        let shared_state = match SharedPoolState::read_from_file(&state_file) {
            Ok(state) => state,
            Err(e) => {
                if iteration % 100 == 0 {
                    warn!("Failed to read state file: {} (iteration {})", e, iteration);
                }
                continue;
            }
        };

        // Check if state is stale
        if shared_state.is_stale(max_state_age) {
            if iteration % 100 == 0 {
                warn!("State file is stale (older than {}s)", max_state_age);
            }
            continue;
        }

        // Scan ALL strategies and collect opportunities
        let mut all_opportunities: Vec<RawOpportunity> = Vec::new();

        for strategy_config in &strategies {
            let opps = scan_for_opportunities_detailed(&shared_state, strategy_config, iteration);
            all_opportunities.extend(opps);
        }

        // If there are opportunities, aggregate and add to batch
        if !all_opportunities.is_empty() {
            // Aggregate by unique market event (pair + direction)
            let aggregated = aggregate_opportunities(&all_opportunities, shared_state.block_number);

            // Add to Discord batch (will be sent every 15 minutes)
            for opp in aggregated {
                batcher.add_opportunity(opp).await;
            }

            // Execute trades for each strategy that caught the opportunity
            for raw_opp in &all_opportunities {
                if let Some(executor) = executors.get(&raw_opp.strategy_name) {
                    let action = SimulatedTradeAction {
                        pair: raw_opp.pair.clone(),
                        config_name: raw_opp.strategy_name.clone(),
                        estimated_profit: raw_opp.estimated_profit,
                        trade_size: raw_opp.trade_size,
                        buy_dex: raw_opp.buy_dex.clone(),
                        sell_dex: raw_opp.sell_dex.clone(),
                        lost_to_competition: raw_opp.lost_to_competition,
                    };
                    let _result = executor.simulate_trade(&action).await;
                }
            }
        }

        // Periodic metrics report
        if last_metrics_report.elapsed() >= metrics_interval {
            last_metrics_report = std::time::Instant::now();
            report_metrics(&strategy_metrics).await;
        }

        // Debug logging
        if iteration % 1000 == 0 {
            debug!(
                "Iteration {} - block {} - {} V2 pools, {} V3 pools tracked",
                iteration,
                shared_state.block_number,
                shared_state.pools.len(),
                shared_state.v3_pools.len()
            );
        }
    }
}

/// DEX fee constants
/// V2 DEXs (Uniswap/Sushiswap/Apeswap): 0.30% per swap
/// V3 DEX fee tiers: 0.05%, 0.30%, 1.00%
const V2_FEE_PERCENT: f64 = 0.30;

/// Dead/illiquid pools to exclude from arbitrage detection
/// These pools have <$100 TVL and generate false positives
/// Format: (DEX name, pair symbol) - verified on-chain 2026-01-28
const EXCLUDED_POOLS: &[(&str, &str)] = &[
    // Apeswap dead pools (verified <$1 TVL)
    ("Apeswap", "LINK/USDC"),   // $0.01 TVL
    ("Apeswap", "UNI/USDC"),    // Low liquidity
    ("Apeswap", "WMATIC/USDC"), // Shows 0 reserves
    // Sushiswap low-liquidity pools
    ("Sushiswap", "LINK/USDC"), // ~$43 TVL - too low for $100+ trades
    // Add more as discovered
];

/// Check if a pool should be excluded from arbitrage detection
fn is_excluded_pool(dex: &str, pair_symbol: &str) -> bool {
    EXCLUDED_POOLS.iter().any(|(d, p)| *d == dex && *p == pair_symbol)
}

/// Token decimals for Polygon mainnet
/// Used to normalize V2 raw prices for comparison with V3
fn get_token_decimals(address: &str) -> u8 {
    // Polygon mainnet token addresses (lowercase for comparison)
    let addr = address.to_lowercase();
    if addr.contains("7ceb23fd6bc0add59e62ac25578270cff1b9f619") { return 18; } // WETH
    if addr.contains("2791bca1f2de4661ed88a30c99a7a9449aa84174") { return 6; }  // USDC
    if addr.contains("0d500b1d8e8ef31e21c99d1db9a6444d3adf1270") { return 18; } // WMATIC
    if addr.contains("1bfd67037b42cf73acf2047067bd4f2c47d9bfd6") { return 8; }  // WBTC
    if addr.contains("c2132d05d31c914a87c6611c10748aeb04b58e8f") { return 6; }  // USDT
    if addr.contains("8f3cf7ad23cd3cadbd9735aff958023239c6a063") { return 18; } // DAI
    if addr.contains("53e0bca35ec356bd5dddfebbd1fc0fd03fabad39") { return 18; } // LINK
    if addr.contains("b33eaad8d922b1083446dc23f610c2567fb5180f") { return 18; } // UNI
    18 // Default to 18 decimals
}

/// Normalize V2 raw price to human-readable units
/// V2 reserves are stored in ADDRESS-SORTED order (token0 < token1 by address)
/// regardless of how TradingPair stores token0/token1
/// raw_price = reserve1/reserve0 where reserve0 is the smaller-address token
/// To normalize: multiply by 10^(actual_d0 - actual_d1)
fn normalize_v2_price(raw_price: f64, token0_addr: &str, token1_addr: &str) -> f64 {
    // Determine actual pool ordering (reserves are address-sorted)
    let t0_lower = token0_addr.to_lowercase();
    let t1_lower = token1_addr.to_lowercase();

    // Compare addresses to determine which is actually reserve0
    let (actual_d0, actual_d1) = if t0_lower < t1_lower {
        // token0 < token1 by address, matches reserve order
        (get_token_decimals(token0_addr) as i32, get_token_decimals(token1_addr) as i32)
    } else {
        // token1 < token0 by address, reserves are swapped
        (get_token_decimals(token1_addr) as i32, get_token_decimals(token0_addr) as i32)
    };

    raw_price * 10_f64.powi(actual_d0 - actual_d1)
}

/// Unified pool representation for comparing V2 and V3 pools
/// All prices are normalized to the SAME direction (token1/token0 based on address order)
#[derive(Debug, Clone)]
struct UnifiedPool {
    dex: String,
    price: f64,           // Normalized price for comparison
    fee_percent: f64,     // Single swap fee (not round-trip)
    is_v3: bool,
    token0_addr: String,  // For normalization tracking
}

/// Raw opportunity from a single strategy scan
#[derive(Debug, Clone)]
struct RawOpportunity {
    pair: String,
    strategy_name: String,
    midmarket_spread: f64,
    executable_spread: f64,
    buy_dex: String,
    sell_dex: String,
    buy_price: f64,
    sell_price: f64,
    estimated_profit: f64,
    trade_size: f64,
    lost_to_competition: bool,
    round_trip_fee: f64,  // Total fees for this specific route
}

/// Scan for arbitrage opportunities based on strategy config (detailed version)
///
/// Supports both V2 and V3 pools with variable fee calculations:
/// - V2↔V2: 0.6% round-trip (0.3% + 0.3%)
/// - V3(0.05%)↔V2: 0.35% round-trip (0.05% + 0.3%) - BEST!
/// - V3(0.30%)↔V2: 0.6% round-trip (0.3% + 0.3%)
/// - V3↔V3: Variable based on fee tiers
///
/// Returns raw opportunities with all pricing details for aggregation
fn scan_for_opportunities_detailed(
    shared_state: &SharedPoolState,
    config: &PaperTradingConfig,
    _iteration: u64,
) -> Vec<RawOpportunity> {
    let mut opportunities = Vec::new();

    for pair_symbol in &config.pairs {
        // Collect all pools (V2 + V3) into unified representation
        let mut unified_pools: Vec<UnifiedPool> = Vec::new();

        // Add V2 pools (normalize price for V3 comparison)
        // V2 stores raw reserve ratio; V3 stores decimal-adjusted price
        // We need to normalize V2 to match V3's format
        for (_key, pool) in &shared_state.pools {
            if pool.pair_symbol == *pair_symbol {
                // Skip excluded dead/illiquid pools
                if is_excluded_pool(&pool.dex, &pool.pair_symbol) {
                    continue;
                }

                let raw_price = pool.price;
                if raw_price > 0.0 {
                    // Normalize V2 price using token decimals
                    // Now that syncer stores actual contract token order,
                    // the stored token0/token1 match reserve order
                    let normalized_price = normalize_v2_price(raw_price, &pool.token0, &pool.token1);
                    unified_pools.push(UnifiedPool {
                        dex: pool.dex.clone(),
                        price: normalized_price,
                        fee_percent: V2_FEE_PERCENT,
                        is_v3: false,
                        token0_addr: pool.token0.clone(),
                    });
                }
            }
        }

        // Add V3 pools (get raw data from v3_pools HashMap)
        // V3 prices are already decimal-adjusted (from tick calculation)
        for (_key, pool) in &shared_state.v3_pools {
            if pool.pair_symbol == *pair_symbol {
                // Use validated_price() to handle overflow errors
                // It returns stored price if valid, or recalculates from tick if invalid
                let price = pool.validated_price();
                if price > 0.0 && price < 1e15 {
                    let fee_percent = pool.fee as f64 / 10000.0;  // 500 -> 0.05%, 3000 -> 0.30%
                    unified_pools.push(UnifiedPool {
                        dex: pool.dex.clone(),
                        price,
                        fee_percent,
                        is_v3: true,
                        token0_addr: pool.token0.clone(),
                    });
                }
            }
        }

        if unified_pools.len() < 2 {
            continue;
        }

        // Compare each pair of pools
        for i in 0..unified_pools.len() {
            for j in (i + 1)..unified_pools.len() {
                let pool_a = &unified_pools[i];
                let pool_b = &unified_pools[j];

                // Calculate MIDMARKET spread (before fees)
                let (midmarket_spread, buy_pool, sell_pool) = if pool_b.price > pool_a.price {
                    let spread = (pool_b.price - pool_a.price) / pool_a.price;
                    (spread, pool_a, pool_b)
                } else {
                    let spread = (pool_a.price - pool_b.price) / pool_b.price;
                    (spread, pool_b, pool_a)
                };

                // Calculate round-trip fee based on ACTUAL pool fees
                // Buy on buy_pool (pay buy fee), sell on sell_pool (pay sell fee)
                let round_trip_fee = buy_pool.fee_percent + sell_pool.fee_percent;

                // Calculate EXECUTABLE spread (after DEX fees)
                let executable_spread = midmarket_spread - (round_trip_fee / 100.0);

                // Skip if executable spread is negative (fees exceed price difference)
                if executable_spread <= 0.0 {
                    continue;
                }

                // Check if executable spread exceeds threshold
                let min_spread = config.max_slippage_percent / 100.0;
                if executable_spread <= min_spread {
                    continue;
                }

                // Estimate profit using EXECUTABLE spread
                let gross = executable_spread * config.max_trade_size_usd;
                let estimated_gas = 0.50;  // ~$0.50 on Polygon
                let estimated_slippage = gross * 0.10;  // 10% price impact estimate
                let estimated_profit = (gross - estimated_gas - estimated_slippage).max(0.0);

                if estimated_profit < config.min_profit_usd {
                    continue;
                }

                // Simulate competition loss
                let lost_to_competition = if config.simulate_competition {
                    let seed = Utc::now().timestamp_nanos_opt().unwrap_or(0) as f64;
                    let roll = (seed % 1000.0) / 1000.0;
                    roll < config.competition_rate
                } else {
                    false
                };

                // Tag V3 opportunities specially
                let v3_tag = if buy_pool.is_v3 || sell_pool.is_v3 {
                    format!(" [V3 fee={:.2}%]", round_trip_fee)
                } else {
                    String::new()
                };

                info!(
                    "[{}] {} Opportunity: {}{} | Midmarket: {:.4}% | Executable: {:.4}% | Est. Profit: ${:.2} | {} -> {}",
                    config.name,
                    if lost_to_competition { "LOST" } else { "FOUND" },
                    pair_symbol,
                    v3_tag,
                    midmarket_spread * 100.0,
                    executable_spread * 100.0,
                    estimated_profit,
                    buy_pool.dex,
                    sell_pool.dex
                );

                opportunities.push(RawOpportunity {
                    pair: pair_symbol.clone(),
                    strategy_name: config.name.clone(),
                    midmarket_spread,
                    executable_spread,
                    buy_dex: buy_pool.dex.clone(),
                    sell_dex: sell_pool.dex.clone(),
                    buy_price: buy_pool.price,
                    sell_price: sell_pool.price,
                    estimated_profit,
                    trade_size: config.max_trade_size_usd,
                    lost_to_competition,
                    round_trip_fee,
                });
            }
        }
    }

    opportunities
}

/// Aggregate raw opportunities by unique market event (pair + direction)
fn aggregate_opportunities(
    raw_opportunities: &[RawOpportunity],
    block_number: u64,
) -> Vec<AggregatedOpportunity> {
    // Group by pair + direction key
    let mut groups: HashMap<String, Vec<&RawOpportunity>> = HashMap::new();

    for opp in raw_opportunities {
        let key = format!("{}:{}:{}", opp.pair, opp.buy_dex, opp.sell_dex);
        groups.entry(key).or_default().push(opp);
    }

    // Convert each group to an AggregatedOpportunity
    groups.values()
        .filter_map(|group| {
            let first = group.first()?;

            let strategies_caught: Vec<StrategyMatch> = group.iter()
                .map(|opp| StrategyMatch {
                    name: opp.strategy_name.clone(),
                    estimated_profit: opp.estimated_profit,
                    trade_size: opp.trade_size,
                    lost_to_competition: opp.lost_to_competition,
                })
                .collect();

            Some(AggregatedOpportunity {
                pair: first.pair.clone(),
                block_number,
                midmarket_spread_pct: first.midmarket_spread * 100.0,
                executable_spread_pct: first.executable_spread * 100.0,
                buy_dex: first.buy_dex.clone(),
                sell_dex: first.sell_dex.clone(),
                buy_price: first.buy_price,
                sell_price: first.sell_price,
                strategies_caught,
                timestamp: Utc::now(),
            })
        })
        .collect()
}

/// Report metrics for all strategies
async fn report_metrics(
    strategy_metrics: &HashMap<String, Arc<RwLock<TraderMetrics>>>,
) {
    let mut aggregator = MetricsAggregator::new();

    for metrics in strategy_metrics.values() {
        let snapshot = metrics.read().await.clone();
        aggregator.add(snapshot);
    }

    let report = aggregator.generate_report();
    info!("\n{}", report);
}
