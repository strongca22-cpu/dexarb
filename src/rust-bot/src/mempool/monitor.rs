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
//!     - ethers (WS provider, subscription)
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

use anyhow::{Context, Result};
use chrono::Utc;
use ethers::prelude::*;
use ethers::types::Transaction;
use futures::StreamExt;
use std::collections::HashMap;
use std::io::Write;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

use crate::types::BotConfig;

use super::decoder;
use super::types::{ConfirmationTracker, PendingSwap};

/// Run the mempool observation monitor.
/// This is the main entry point, called from main.rs via tokio::spawn.
/// Creates its own WS connections and runs indefinitely with auto-reconnect.
pub async fn run_observation(config: BotConfig) -> Result<()> {
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
        match run_observation_inner(&config, &data_dir, &router_hex, &router_lookup).await {
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
async fn run_observation_inner(
    config: &BotConfig,
    data_dir: &str,
    router_hex: &[String],
    router_lookup: &HashMap<Address, String>,
) -> Result<()> {
    // Create WS provider for pending tx subscription
    let sub_provider = Provider::<Ws>::connect(&config.rpc_url)
        .await
        .context("Mempool WS connect failed")?;

    // Create separate WS provider for RPC calls (get_block, get_block_number)
    // Avoids borrow conflicts with the subscription stream.
    let rpc_provider = Provider::<Ws>::connect(&config.rpc_url)
        .await
        .context("Mempool RPC WS connect failed")?;

    // Subscribe to alchemy_pendingTransactions filtered to V3 routers.
    // Returns full transaction objects (hashesOnly: false).
    // CU cost: ~40 CU per notification, ~2 V3 txs/min = ~3.5M CU/month.
    let alchemy_params = serde_json::json!([
        "alchemy_pendingTransactions",
        {
            "toAddress": router_hex,
            "hashesOnly": false
        }
    ]);

    let mut pending_stream: SubscriptionStream<'_, Ws, Transaction> = sub_provider
        .subscribe(alchemy_params)
        .await
        .context("alchemy_pendingTransactions subscription failed")?;

    info!("Mempool: alchemy_pendingTransactions subscription active ({} routers)", router_hex.len());

    // Open CSV file for logging (append mode, date-stamped)
    let date_str = Utc::now().format("%Y%m%d").to_string();
    let csv_path = format!("{}/pending_swaps_{}.csv", data_dir, date_str);
    let mut csv_file = open_csv(&csv_path)?;
    info!("Mempool: logging to {}", csv_path);

    // Cross-reference tracker
    let mut tracker = ConfirmationTracker::new();

    // Block tracking for cross-reference
    let mut last_checked_block = rpc_provider.get_block_number().await?.as_u64();

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
                        // Determine router name from the tx.to address
                        let router_name = tx.to
                            .and_then(|to| router_lookup.get(&to))
                            .cloned()
                            .unwrap_or_else(|| "Unknown".to_string());

                        // Decode calldata
                        match decoder::decode_calldata(&tx.input) {
                            Some(decoded) => {
                                total_decoded += 1;

                                let swap = PendingSwap {
                                    timestamp_utc: Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
                                    tx_hash: tx.hash,
                                    router: tx.to.unwrap_or_default(),
                                    router_name: router_name.clone(),
                                    function_name: decoded.function_name.clone(),
                                    token_in: decoded.token_in,
                                    token_out: decoded.token_out,
                                    amount_in: decoded.amount_in,
                                    amount_out_min: decoded.amount_out_min,
                                    fee_tier: decoded.fee_tier,
                                    gas_price_gwei: tx.gas_price
                                        .map(|gp| gp.as_u128() as f64 / 1e9)
                                        .unwrap_or(0.0),
                                    max_priority_fee_gwei: tx.max_priority_fee_per_gas
                                        .map(|pf| pf.as_u128() as f64 / 1e9)
                                        .unwrap_or(0.0),
                                };

                                // Log to CSV
                                if let Err(e) = write_csv_row(&mut csv_file, &swap) {
                                    warn!("CSV write error: {}", e);
                                }

                                // Track for cross-reference
                                tracker.track(tx.hash, &router_name);

                                info!(
                                    "PENDING: {} | {} | {} | in={} out={} | amt={} | fee={} | gas={:.1}gwei",
                                    format!("{:?}", tx.hash).chars().take(10).collect::<String>(),
                                    router_name,
                                    decoded.function_name,
                                    decoded.token_in.map(|a| format!("{:?}", a).chars().skip(2).take(8).collect::<String>()).unwrap_or_else(|| "?".to_string()),
                                    decoded.token_out.map(|a| format!("{:?}", a).chars().skip(2).take(8).collect::<String>()).unwrap_or_else(|| "?".to_string()),
                                    decoded.amount_in.map(|a| a.to_string()).unwrap_or_else(|| "?".to_string()),
                                    decoded.fee_tier.map(|f| f.to_string()).unwrap_or_else(|| "dyn".to_string()),
                                    swap.gas_price_gwei,
                                );
                            }
                            None => {
                                total_undecoded += 1;
                                let sel = decoder::selector_hex(&tx.input);
                                info!(
                                    "PENDING (undecoded): {:?} | {} | selector={} | {} bytes",
                                    tx.hash, router_name, sel, tx.input.len()
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
                    Ok(current_block_num) => {
                        let current = current_block_num.as_u64();
                        // Process new blocks since last check
                        for block_num in (last_checked_block + 1)..=current {
                            match rpc_provider.get_block(block_num).await {
                                Ok(Some(block)) => {
                                    blocks_checked += 1;
                                    let tx_hashes: Vec<TxHash> = block.transactions.clone();

                                    let matches = tracker.check_block(&tx_hashes);
                                    for (hash, lead_time_ms, router_name) in &matches {
                                        info!(
                                            "CONFIRMED: {:?} | {} | lead_time={}ms | block={}",
                                            hash, router_name, lead_time_ms, block_num
                                        );
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

                // Report stats every ~10 minutes (100 ticks × 6s)
                if tick_count % 100 == 0 {
                    info!(
                        "MEMPOOL STATS | decoded={} undecoded={} | confirmed={}/{} ({:.1}%) | \
                         median_lead={}ms mean_lead={}ms | tracking={} | blocks_checked={}",
                        total_decoded,
                        total_undecoded,
                        tracker.total_confirmed,
                        tracker.total_pending_seen,
                        tracker.confirmation_rate(),
                        tracker.median_lead_time_ms(),
                        tracker.mean_lead_time_ms(),
                        tracker.tracking_count(),
                        blocks_checked,
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
