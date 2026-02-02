//! A4 Mempool Monitor — Observation Loop
//!
//! Purpose:
//!     Subscribe to pending DEX swap transactions via Alchemy's
//!     alchemy_pendingTransactions WebSocket subscription (filtered by router address).
//!     Decode calldata, log to CSV, and cross-reference against confirmed blocks
//!     to measure mempool visibility and lead time.
//!
//! Author: AI-Generated
//! Created: 2026-02-01
//! Modified: 2026-02-01
//!
//! Dependencies:
//!     - alloy (WS provider, subscription)
//!     - tokio (async runtime, select!, interval)
//!     - chrono (timestamps)
//!
//! Usage:
//!     Called from main.rs via tokio::spawn when MEMPOOL_MONITOR=observe.
//!     Creates its own WS connections (separate from the main block loop).
//!
//! Notes:
//!     - Uses Alchemy's alchemy_pendingTransactions for filtered subscription
//!     - V3 routers only for Phase 1 (~2 txs/min, ~3.5M CU/month)
//!     - Cross-reference uses a separate WS connection for block + get_block calls
//!     - CSV output: data/{chain}/mempool/pending_swaps_YYYYMMDD.csv
//!     - Phase 2: simulated_opportunities + simulation_accuracy CSVs

use anyhow::{Context, Result};
use chrono::Utc;
use alloy::consensus::Transaction as TransactionTrait;
use alloy::network::TransactionResponse;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use futures::StreamExt;
use std::collections::HashMap;
use std::io::Write;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use crate::pool::PoolStateManager;
use crate::types::BotConfig;

use super::decoder;
use super::simulator;
use super::types::{ConfirmationTracker, MempoolSignal, PendingSwap, SimulatedOpportunity, SimulationTracker};

/// Phase 3: Minimum spread (%) to trigger execution signal.
/// 0.01% = 1 bps net-of-fees — filters noise.
const MEMPOOL_MIN_SPREAD_PCT: f64 = 0.01;

/// Run the mempool execution monitor (Phase 3).
/// Wraps run_observation_impl with a signal sender — sends MempoolSignal to the
/// main loop when a simulated opportunity exceeds the execution threshold.
pub async fn run_execution(
    config: BotConfig,
    pool_state: PoolStateManager,
    signal_tx: mpsc::Sender<MempoolSignal>,
) -> Result<()> {
    run_observation_impl(config, pool_state, Some(signal_tx)).await
}

/// Run the mempool observation monitor.
/// This is the main entry point, called from main.rs via tokio::spawn.
/// Creates its own WS connections and runs indefinitely with auto-reconnect.
/// Phase 2: accepts PoolStateManager for AMM state simulation.
pub async fn run_observation(config: BotConfig, pool_state: PoolStateManager) -> Result<()> {
    run_observation_impl(config, pool_state, None).await
}

/// Implementation: observation + optional execution signaling.
/// When signal_tx is Some, sends MempoolSignal on SIM OPP exceeding threshold.
/// When None, observe-only mode (Phase 1/2 behavior preserved).
async fn run_observation_impl(
    config: BotConfig,
    pool_state: PoolStateManager,
    signal_tx: Option<mpsc::Sender<MempoolSignal>>,
) -> Result<()> {
    let chain = &config.chain_name;

    // Create data directory for CSV logs
    let data_dir = format!("/home/botuser/bots/dexarb/data/{}/mempool", chain);
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("Failed to create mempool data dir: {}", data_dir))?;

    // Collect V3 router addresses for the Alchemy toAddress filter.
    // V2 routers are excluded from Phase 1 (99% of pending volume = too expensive
    // at 40 CU/tx for full objects; V3 is ~2/min = ~3.5M CU/month).
    let mut routers: Vec<(Address, String)> = Vec::new();
    if let Some(addr) = config.uniswap_v3_router {
        routers.push((addr, "UniswapV3".to_string()));
    }
    if let Some(addr) = config.sushiswap_v3_router {
        routers.push((addr, "SushiV3".to_string()));
    }
    if let Some(addr) = config.quickswap_v3_router {
        routers.push((addr, "AlgebraV3".to_string()));
    }

    if routers.is_empty() {
        error!("No V3 router addresses configured — mempool monitor has nothing to watch");
        return Ok(());
    }

    // Build lookup map: Address → router_name
    let router_lookup: HashMap<Address, String> = routers.iter().cloned().collect();

    // Build Alchemy toAddress filter (hex strings with checksum)
    let router_hex: Vec<String> = routers.iter().map(|(a, _)| format!("{:?}", a)).collect();

    info!(
        "Mempool monitor starting (observation mode) | chain={} | routers={}",
        chain,
        routers
            .iter()
            .map(|(a, n)| format!("{}({:?})", n, a))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Reconnect loop — if subscriptions drop, reconnect and continue
    let mut reconnects = 0u32;
    const MAX_RECONNECTS: u32 = 50;

    loop {
        match run_observation_inner(&config, &data_dir, &router_hex, &router_lookup, &pool_state, &signal_tx).await {
            Ok(()) => {
                // Clean exit (shouldn't happen in observe mode)
                info!("Mempool monitor exited cleanly");
                break;
            }
            Err(e) => {
                reconnects += 1;
                if reconnects > MAX_RECONNECTS {
                    error!(
                        "Mempool monitor: {} reconnects exhausted — giving up: {}",
                        MAX_RECONNECTS, e
                    );
                    return Err(e);
                }
                warn!(
                    "Mempool monitor error (reconnect {}/{}): {} — retrying in 5s...",
                    reconnects, MAX_RECONNECTS, e
                );
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }

    Ok(())
}

/// Inner observation loop — one WS session.
/// Returns Err on connection failure (caller retries).
/// Phase 3: signal_tx sends MempoolSignal to main loop when in execute mode.
async fn run_observation_inner(
    config: &BotConfig,
    data_dir: &str,
    router_hex: &[String],
    router_lookup: &HashMap<Address, String>,
    pool_state: &PoolStateManager,
    signal_tx: &Option<mpsc::Sender<MempoolSignal>>,
) -> Result<()> {
    // Create WS provider for pending tx subscription
    let sub_provider = ProviderBuilder::new()
        .connect_ws(WsConnect::new(&config.rpc_url))
        .await
        .context("Mempool WS connect failed")?;

    // Create separate WS provider for RPC calls (get_block, get_block_number)
    // Avoids borrow conflicts with the subscription stream.
    let rpc_provider = ProviderBuilder::new()
        .connect_ws(WsConnect::new(&config.rpc_url))
        .await
        .context("Mempool RPC WS connect failed")?;

    // Subscribe to alchemy_pendingTransactions filtered to V3 routers.
    // Returns full transaction objects (hashesOnly: false).
    // CU cost: ~40 CU per notification, ~2 V3 txs/min = ~3.5M CU/month.
    //
    // alloy: Use raw subscribe() with Alchemy-specific params. The standard
    // subscribe_full_pending_transactions() sends "newPendingTransactions" which
    // Alchemy returns as hashes only. Alchemy's custom method supports server-side
    // toAddress filtering and full tx object delivery.
    let alchemy_params = serde_json::json!({
        "toAddress": router_hex,
        "hashesOnly": false
    });
    let pending_sub: alloy::pubsub::Subscription<alloy::rpc::types::Transaction> = sub_provider
        .subscribe(("alchemy_pendingTransactions", alchemy_params))
        .await
        .context("Alchemy pending tx subscription failed")?;
    let mut pending_stream = pending_sub.into_stream();

    info!("Mempool: alchemy_pendingTransactions subscription active ({} routers)", router_hex.len());

    // Open CSV file for logging (append mode, date-stamped)
    let date_str = Utc::now().format("%Y%m%d").to_string();
    let csv_path = format!("{}/pending_swaps_{}.csv", data_dir, date_str);
    let mut csv_file = open_csv(&csv_path)?;
    info!("Mempool: logging to {}", csv_path);

    // Cross-reference tracker
    let mut tracker = ConfirmationTracker::new();

    // Phase 2: Simulation tracker + CSV files
    let mut sim_tracker = SimulationTracker::new();
    let sim_csv_path = format!("{}/simulated_opportunities_{}.csv", data_dir, date_str);
    let mut sim_csv_file = open_sim_csv(&sim_csv_path)?;
    let accuracy_csv_path = format!("{}/simulation_accuracy_{}.csv", data_dir, date_str);
    let mut accuracy_csv_file = open_accuracy_csv(&accuracy_csv_path)?;
    info!("Phase 2: simulation CSVs → {} , {}", sim_csv_path, accuracy_csv_path);

    // Block tracking for cross-reference
    let mut last_checked_block = rpc_provider.get_block_number().await?;

    // Stats
    let mut total_decoded = 0u64;
    let mut total_undecoded = 0u64;
    let mut blocks_checked = 0u64;

    // Periodic timer for cross-reference checks and stats
    let mut check_interval = interval(Duration::from_secs(6));
    // Skip the first immediate tick
    check_interval.tick().await;

    // Stats reporting interval (every 100 ticks = ~10 min)
    let mut tick_count = 0u64;

    loop {
        tokio::select! {
            maybe_tx = pending_stream.next() => {
                match maybe_tx {
                    Some(tx) => {
                        // Filter: verify tx is to a monitored router (should already
                        // be filtered server-side by Alchemy's toAddress param)
                        let tx_to = tx.to();
                        let router_name = match tx_to
                            .and_then(|to| router_lookup.get(&to))
                        {
                            Some(name) => name.clone(),
                            None => continue, // Not a monitored router — skip
                        };

                        // Decode calldata
                        match decoder::decode_calldata(tx.input()) {
                            Some(decoded) => {
                                total_decoded += 1;

                                let tx_hash = tx.tx_hash();
                                let swap = PendingSwap {
                                    timestamp_utc: Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
                                    tx_hash,
                                    router: tx.to().unwrap_or_default(),
                                    router_name: router_name.clone(),
                                    function_name: decoded.function_name.clone(),
                                    token_in: decoded.token_in,
                                    token_out: decoded.token_out,
                                    amount_in: decoded.amount_in,
                                    amount_out_min: decoded.amount_out_min,
                                    fee_tier: decoded.fee_tier,
                                    gas_price_gwei: TransactionTrait::gas_price(&tx)
                                        .map(|gp| gp as f64 / 1e9)
                                        .unwrap_or(0.0),
                                    max_priority_fee_gwei: tx.max_priority_fee_per_gas()
                                        .map(|pf| pf as f64 / 1e9)
                                        .unwrap_or(0.0),
                                };

                                // Log to CSV
                                if let Err(e) = write_csv_row(&mut csv_file, &swap) {
                                    warn!("CSV write error: {}", e);
                                }

                                // Track for cross-reference
                                tracker.track(tx_hash, &router_name);

                                info!(
                                    "PENDING: {} | {} | {} | in={} out={} | amt={} | fee={} | gas={:.1}gwei",
                                    format!("{:?}", tx_hash).chars().take(10).collect::<String>(),
                                    router_name,
                                    decoded.function_name,
                                    decoded.token_in.map(|a| format!("{:?}", a).chars().skip(2).take(8).collect::<String>()).unwrap_or_else(|| "?".to_string()),
                                    decoded.token_out.map(|a| format!("{:?}", a).chars().skip(2).take(8).collect::<String>()).unwrap_or_else(|| "?".to_string()),
                                    decoded.amount_in.map(|a| a.to_string()).unwrap_or_else(|| "?".to_string()),
                                    decoded.fee_tier.map(|f| f.to_string()).unwrap_or_else(|| "dyn".to_string()),
                                    swap.gas_price_gwei,
                                );

                                // ── Phase 2: Simulate post-swap state ──
                                if let Some(amount) = decoded.amount_in {
                                    if let Some((dex, pair_sym, zero_for_one)) =
                                        simulator::identify_affected_pool(&decoded, &router_name, pool_state)
                                    {
                                        let sim_result = if dex.is_v3() {
                                            pool_state
                                                .get_v3_pool(dex, &pair_sym)
                                                .and_then(|pool| simulator::simulate_v3_swap(&pool, amount, zero_for_one))
                                        } else {
                                            pool_state
                                                .get_pool(dex, &pair_sym)
                                                .and_then(|pool| {
                                                    let token_in = decoded.token_in.unwrap_or_default();
                                                    simulator::simulate_v2_swap(&pool, amount, token_in)
                                                })
                                        };

                                        if sim_result.is_none() {
                                            debug!(
                                                "SIM FAIL: {:?}/{} z4o={} amt={} — simulation returned None",
                                                dex, pair_sym, zero_for_one, amount
                                            );
                                        }

                                        if let Some(ref simulated) = sim_result {
                                            debug!(
                                                "SIM OK: {:?}/{} pre={:.6} post={:.6} impact={:.4}%",
                                                dex, pair_sym, simulated.pre_swap_price, simulated.post_swap_price,
                                                (simulated.post_swap_price - simulated.pre_swap_price).abs()
                                                    / simulated.pre_swap_price * 100.0
                                            );
                                            let opportunities = simulator::check_post_swap_opportunities(
                                                pool_state, simulated, config, tx_hash,
                                                &decoded.function_name, amount, zero_for_one,
                                                &swap.timestamp_utc,
                                            );

                                            for opp in &opportunities {
                                                info!(
                                                    "SIM OPP: {:?} | {} | {:.3}% spread | ${:.2} est | impact={:.4}%",
                                                    tx_hash, opp.pair_symbol, opp.arb_spread_pct,
                                                    opp.arb_est_profit_usd, opp.price_impact_pct,
                                                );
                                                if let Err(e) = write_sim_csv_row(&mut sim_csv_file, opp) {
                                                    warn!("Sim CSV write error: {}", e);
                                                }

                                                // Phase 3: Send execution signal if thresholds met
                                                if let Some(ref stx) = signal_tx {
                                                    if opp.arb_est_profit_usd >= config.mempool_min_profit_usd
                                                        && opp.arb_spread_pct >= MEMPOOL_MIN_SPREAD_PCT
                                                    {
                                                        let signal = MempoolSignal {
                                                            opportunity: opp.clone(),
                                                            trigger_gas_price: U256::from(TransactionTrait::gas_price(&tx).unwrap_or(0)),
                                                            trigger_max_priority_fee: tx.max_priority_fee_per_gas().map(U256::from),
                                                            seen_at: Instant::now(),
                                                        };
                                                        match stx.try_send(signal) {
                                                            Ok(()) => info!(
                                                                "MEMPOOL EXEC: signal sent | {} | ${:.2} | {:.3}%",
                                                                opp.pair_symbol, opp.arb_est_profit_usd, opp.arb_spread_pct
                                                            ),
                                                            Err(mpsc::error::TrySendError::Full(_)) => warn!(
                                                                "MEMPOOL EXEC: channel full, dropping signal"
                                                            ),
                                                            Err(e) => error!(
                                                                "MEMPOOL EXEC: channel error: {}", e
                                                            ),
                                                        }
                                                    }
                                                }
                                            }

                                            // Track simulation for accuracy validation (with or without opportunity)
                                            let best_opp = opportunities.into_iter().next();
                                            sim_tracker.track(tx_hash, simulated.clone(), best_opp);
                                        }
                                    }
                                }
                            }
                            None => {
                                total_undecoded += 1;
                                let sel = decoder::selector_hex(tx.input());
                                info!(
                                    "PENDING (undecoded): {:?} | {} | selector={} | {} bytes",
                                    tx.tx_hash(), router_name, sel, tx.input().len()
                                );
                            }
                        }
                    }
                    None => {
                        warn!("Mempool pending stream ended (None)");
                        return Err(anyhow::anyhow!("Pending stream ended"));
                    }
                }
            }

            _ = check_interval.tick() => {
                tick_count += 1;

                // Check for new blocks and cross-reference
                match rpc_provider.get_block_number().await {
                    Ok(current) => {
                        // Process new blocks since last check
                        for block_num in (last_checked_block + 1)..=current {
                            match rpc_provider.get_block_by_number(block_num.into()).await {
                                Ok(Some(block)) => {
                                    blocks_checked += 1;
                                    let tx_hashes: Vec<B256> = block.transactions.hashes().collect();

                                    let matches = tracker.check_block(&tx_hashes);
                                    for (hash, lead_time_ms, router_name) in &matches {
                                        info!(
                                            "CONFIRMED: {:?} | {} | lead_time={}ms | block={}",
                                            hash, router_name, lead_time_ms, block_num
                                        );

                                        // Phase 2: Accuracy validation — compare simulated vs actual
                                        if let Some((simulated, _opp)) = sim_tracker.check_confirmation(*hash) {
                                            let actual_price = if simulated.is_v3 {
                                                pool_state
                                                    .get_v3_pool(simulated.dex, &simulated.pair_symbol)
                                                    .map(|p| p.price())
                                            } else {
                                                pool_state
                                                    .get_pool(simulated.dex, &simulated.pair_symbol)
                                                    .map(|p| p.price_adjusted())
                                            };

                                            if let Some(actual) = actual_price {
                                                let predicted = simulated.post_swap_price;
                                                let error_pct = if actual != 0.0 {
                                                    ((predicted - actual) / actual * 100.0).abs()
                                                } else {
                                                    f64::MAX
                                                };

                                                info!(
                                                    "SIM VALIDATE: {:?} | {} | predicted={:.8} actual={:.8} | error={:.4}% | lead={}ms",
                                                    hash, simulated.pair_symbol, predicted, actual, error_pct, lead_time_ms
                                                );

                                                sim_tracker.record_accuracy(error_pct);

                                                let ts = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                                if let Err(e) = write_accuracy_csv_row(
                                                    &mut accuracy_csv_file, &ts, hash,
                                                    &simulated.pair_symbol, &format!("{:?}", simulated.dex),
                                                    predicted, actual, error_pct, *lead_time_ms,
                                                ) {
                                                    warn!("Accuracy CSV write error: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    warn!("get_block({}) failed: {}", block_num, e);
                                    break;
                                }
                            }
                        }
                        last_checked_block = current;
                    }
                    Err(e) => {
                        warn!("get_block_number failed: {}", e);
                    }
                }

                // Cleanup stale tracker entries (>2 min old = probably dropped)
                tracker.cleanup(Duration::from_secs(120));
                sim_tracker.cleanup(Duration::from_secs(120));

                // Report stats every ~10 minutes (100 ticks × 6s)
                if tick_count % 100 == 0 {
                    info!(
                        "MEMPOOL STATS | decoded={} undecoded={} | confirmed={}/{} ({:.1}%) | \
                         median_lead={}ms mean_lead={}ms | tracking={} | blocks_checked={} | \
                         sim: opps={} validated={} median_err={:.3}%",
                        total_decoded,
                        total_undecoded,
                        tracker.total_confirmed,
                        tracker.total_pending_seen,
                        tracker.confirmation_rate(),
                        tracker.median_lead_time_ms(),
                        tracker.mean_lead_time_ms(),
                        tracker.tracking_count(),
                        blocks_checked,
                        sim_tracker.total_opportunities,
                        sim_tracker.total_validated,
                        sim_tracker.median_error_pct(),
                    );
                }
            }
        }
    }
}

// ── CSV Helpers ─────────────────────────────────────────────────────

/// Open or create a CSV file. Writes header if the file is new.
fn open_csv(path: &str) -> Result<std::fs::File> {
    let exists = std::path::Path::new(path).exists();

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open CSV: {}", path))?;

    if !exists {
        let mut f = file;
        writeln!(
            f,
            "timestamp_utc,tx_hash,router,router_name,function,token_in,token_out,\
             amount_in,amount_out_min,fee_tier,gas_price_gwei,max_priority_fee_gwei"
        )?;
        Ok(f)
    } else {
        Ok(file)
    }
}

/// Write a single pending swap observation to the CSV file
fn write_csv_row(file: &mut std::fs::File, swap: &PendingSwap) -> Result<()> {
    writeln!(
        file,
        "{},{:?},{:?},{},{},{},{},{},{},{},{:.4},{:.4}",
        swap.timestamp_utc,
        swap.tx_hash,
        swap.router,
        swap.router_name,
        swap.function_name,
        swap.token_in
            .map(|a| format!("{:?}", a))
            .unwrap_or_default(),
        swap.token_out
            .map(|a| format!("{:?}", a))
            .unwrap_or_default(),
        swap.amount_in
            .map(|a| a.to_string())
            .unwrap_or_default(),
        swap.amount_out_min
            .map(|a| a.to_string())
            .unwrap_or_default(),
        swap.fee_tier
            .map(|f| f.to_string())
            .unwrap_or_default(),
        swap.gas_price_gwei,
        swap.max_priority_fee_gwei,
    )?;
    file.flush()?;
    Ok(())
}

// ── Phase 2: Simulation CSV Helpers ─────────────────────────────────

/// Open simulated opportunities CSV (append mode, write header if new)
fn open_sim_csv(path: &str) -> Result<std::fs::File> {
    let exists = std::path::Path::new(path).exists();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open sim CSV: {}", path))?;

    if !exists {
        let mut f = file;
        writeln!(
            f,
            "timestamp_utc,tx_hash,trigger_dex,trigger_function,pair_symbol,zero_for_one,\
             amount_in,pre_swap_price,post_swap_price,price_impact_pct,\
             arb_buy_dex,arb_sell_dex,arb_spread_pct,arb_est_profit_usd"
        )?;
        Ok(f)
    } else {
        Ok(file)
    }
}

/// Open simulation accuracy CSV (append mode, write header if new)
fn open_accuracy_csv(path: &str) -> Result<std::fs::File> {
    let exists = std::path::Path::new(path).exists();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open accuracy CSV: {}", path))?;

    if !exists {
        let mut f = file;
        writeln!(
            f,
            "timestamp_utc,tx_hash,pair_symbol,dex,predicted_price,actual_price,error_pct,lead_time_ms"
        )?;
        Ok(f)
    } else {
        Ok(file)
    }
}

/// Write a simulated opportunity row
fn write_sim_csv_row(file: &mut std::fs::File, opp: &SimulatedOpportunity) -> Result<()> {
    writeln!(
        file,
        "{},{:?},{:?},{},{},{},{},{:.10},{:.10},{:.6},{:?},{:?},{:.6},{:.4}",
        opp.timestamp_utc,
        opp.tx_hash,
        opp.trigger_dex,
        opp.trigger_function,
        opp.pair_symbol,
        opp.zero_for_one,
        opp.amount_in,
        opp.pre_swap_price,
        opp.post_swap_price,
        opp.price_impact_pct,
        opp.arb_buy_dex,
        opp.arb_sell_dex,
        opp.arb_spread_pct,
        opp.arb_est_profit_usd,
    )?;
    file.flush()?;
    Ok(())
}

/// Write an accuracy validation row
#[allow(clippy::too_many_arguments)]
fn write_accuracy_csv_row(
    file: &mut std::fs::File,
    timestamp: &str,
    tx_hash: &B256,
    pair_symbol: &str,
    dex: &str,
    predicted: f64,
    actual: f64,
    error_pct: f64,
    lead_time_ms: u64,
) -> Result<()> {
    writeln!(
        file,
        "{},{:?},{},{},{:.10},{:.10},{:.6},{}",
        timestamp, tx_hash, pair_symbol, dex, predicted, actual, error_pct, lead_time_ms,
    )?;
    file.flush()?;
    Ok(())
}
