//! Paper Trading Binary (v2 - File-Based Architecture)
//!
//! Reads pool state from shared JSON file (written by data-collector)
//! and runs paper trading strategies based on TOML configuration.
//!
//! Supports hot-reloading via SIGHUP signal:
//!   kill -HUP $(pgrep paper-trading)
//!
//! Discord Alerts:
//!   Set DISCORD_WEBHOOK environment variable to enable alerts
//!   Alerts are sent when opportunities are detected, aggregating across all strategies
//!
//! Usage:
//!   cargo run --bin paper-trading
//!   cargo run --bin paper-trading -- --config /path/to/config.toml
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//! Modified: 2026-01-28 (added Discord alerts with opportunity aggregation)

use anyhow::{Context, Result};
use chrono::Utc;
use dexarb_bot::data_collector::SharedPoolState;
use dexarb_bot::paper_trading::{
    AggregatedOpportunity, DiscordAlerter, MetricsAggregator, PaperTradingConfig,
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

    // Initialize Discord alerter
    let discord = DiscordAlerter::new();
    if discord.is_enabled() {
        info!("Discord alerts: ENABLED");
    } else {
        info!("Discord alerts: DISABLED (set DISCORD_WEBHOOK to enable)");
    }

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

        // If there are opportunities, aggregate and alert
        if !all_opportunities.is_empty() {
            // Aggregate by unique market event (pair + direction)
            let aggregated = aggregate_opportunities(&all_opportunities, shared_state.block_number);

            // Send Discord alert for each unique opportunity
            for opp in &aggregated {
                discord.send_opportunity_alert(opp).await;
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
                "Iteration {} - block {} - {} pools tracked",
                iteration,
                shared_state.block_number,
                shared_state.pools.len()
            );
        }
    }
}

/// DEX fee constants (Uniswap V2 / Sushiswap = 0.3% per swap)
const DEX_FEE_PERCENT: f64 = 0.30;
const ROUND_TRIP_FEE_PERCENT: f64 = DEX_FEE_PERCENT * 2.0;  // 0.6% for buy + sell

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
}

/// Scan for arbitrage opportunities based on strategy config (detailed version)
///
/// Returns raw opportunities with all pricing details for aggregation
fn scan_for_opportunities_detailed(
    shared_state: &SharedPoolState,
    config: &PaperTradingConfig,
    _iteration: u64,
) -> Vec<RawOpportunity> {
    let mut opportunities = Vec::new();

    for pair_symbol in &config.pairs {
        // Get pools for this pair
        let pools = shared_state.get_pools_for_pair(pair_symbol);

        if pools.len() < 2 {
            continue;
        }

        // Compare each pair of pools
        for i in 0..pools.len() {
            for j in (i + 1)..pools.len() {
                let pool_a = &pools[i];
                let pool_b = &pools[j];

                let price_a = pool_a.price();
                let price_b = pool_b.price();

                if price_a == 0.0 || price_b == 0.0 {
                    continue;
                }

                // Calculate MIDMARKET spread (before fees)
                let (midmarket_spread, buy_dex, sell_dex, buy_price, sell_price) = if price_b > price_a {
                    let spread = (price_b - price_a) / price_a;
                    (spread, pool_a.dex.to_string(), pool_b.dex.to_string(), price_a, price_b)
                } else {
                    let spread = (price_a - price_b) / price_b;
                    (spread, pool_b.dex.to_string(), pool_a.dex.to_string(), price_b, price_a)
                };

                // Calculate EXECUTABLE spread (after DEX fees)
                let executable_spread = midmarket_spread - (ROUND_TRIP_FEE_PERCENT / 100.0);

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

                info!(
                    "[{}] {} Opportunity: {} | Midmarket: {:.4}% | Executable: {:.4}% | Est. Profit: ${:.2} | {} -> {}",
                    config.name,
                    if lost_to_competition { "LOST" } else { "FOUND" },
                    pair_symbol,
                    midmarket_spread * 100.0,
                    executable_spread * 100.0,
                    estimated_profit,
                    buy_dex,
                    sell_dex
                );

                opportunities.push(RawOpportunity {
                    pair: pair_symbol.clone(),
                    strategy_name: config.name.clone(),
                    midmarket_spread,
                    executable_spread,
                    buy_dex,
                    sell_dex,
                    buy_price,
                    sell_price,
                    estimated_profit,
                    trade_size: config.max_trade_size_usd,
                    lost_to_competition,
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
