#!/usr/bin/env rust
//! Historical Event Backfill
//!
//! Purpose:
//!     Pulls historical V3 Swap events from Alchemy via eth_getLogs and writes
//!     them to CSV files in the exact same format as PriceLogger (prices_YYYYMMDD.csv).
//!     Produces a unified, portable dataset for simulation and analysis on the
//!     Hetzner VPS, where the live data collector can seamlessly continue from.
//!
//! Author: AI-Generated
//! Created: 2026-02-02
//!
//! Dependencies:
//!     - alloy (RPC provider, primitives)
//!     - tokio (async runtime)
//!     - chrono (timestamps)
//!     - clap (CLI args)
//!     - anyhow (error handling)
//!     - tracing (logging)
//!
//! Usage:
//!     cargo run --release --bin backfill-events -- --chain polygon --weeks 2
//!     cargo run --release --bin backfill-events -- --chain polygon --start-block 81830000 --end-block 82435000
//!
//! Notes:
//!     - V3 pools only (matches PriceLogger behavior)
//!     - Uses HTTP provider (not WS) for reliability on batch queries
//!     - Rate-limited to respect Alchemy CU budget
//!     - Resume-capable: detects last backfilled block from existing CSVs
//!     - Block timestamps fetched only for blocks with events (cached)

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use clap::Parser;
use dexarb_bot::contracts::{IERC20, UniswapV3Pool};
use dexarb_bot::filters::WhitelistFilter;
use dexarb_bot::pool::{SUSHI_V3_FEE_TIERS, V3_FEE_TIERS};
use dexarb_bot::types::DexType;
use alloy::primitives::{keccak256, Address, B256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

// ── Constants ───────────────────────────────────────────────────────────

/// CSV header — must match PriceLogger exactly
const CSV_HEADER: &str = "timestamp,block,pair,dex,fee,price,tick,liquidity,sqrt_price_x96,address";

/// Alchemy eth_getLogs response limit
const ALCHEMY_LOG_LIMIT: usize = 10_000;

/// Maximum retries per batch on RPC failure
const MAX_RETRIES: u32 = 3;

// ── CLI Arguments ───────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "backfill-events", about = "Backfill historical V3 Swap events to price_history CSVs")]
struct Args {
    /// Chain to backfill (polygon, base)
    #[arg(short, long, default_value = "polygon")]
    chain: String,

    /// Number of weeks to backfill (default: 2)
    #[arg(short, long, default_value = "2")]
    weeks: u64,

    /// Start block (overrides --weeks if set)
    #[arg(long)]
    start_block: Option<u64>,

    /// End block (default: latest)
    #[arg(long)]
    end_block: Option<u64>,

    /// Batch size for eth_getLogs calls (blocks per call)
    #[arg(long, default_value = "2000")]
    batch_size: u64,

    /// Delay between batches in milliseconds (rate limiting)
    #[arg(long, default_value = "200")]
    batch_delay_ms: u64,

    /// Output directory override (default: data/{chain}/price_history)
    #[arg(long)]
    output_dir: Option<String>,
}

// ── Pool Metadata ───────────────────────────────────────────────────────

/// Metadata for a whitelisted V3 pool (built at startup, used during event parsing)
struct PoolMeta {
    dex: DexType,
    pair_symbol: String,
    fee: u32,
    token0_decimals: u8,
    token1_decimals: u8,
}

// ── Backfill Writer ─────────────────────────────────────────────────────

/// Manages daily CSV file rotation — writes rows to prices_YYYYMMDD.csv files.
/// Keeps file handles open for efficiency when writing across date boundaries.
struct BackfillWriter {
    log_dir: PathBuf,
    open_files: HashMap<NaiveDate, File>,
    rows_written: u64,
}

impl BackfillWriter {
    fn new(log_dir: &str) -> Result<Self> {
        let path = PathBuf::from(log_dir);
        fs::create_dir_all(&path)
            .with_context(|| format!("Failed to create output directory: {}", log_dir))?;
        Ok(Self {
            log_dir: path,
            open_files: HashMap::new(),
            rows_written: 0,
        })
    }

    fn write_row(&mut self, date: NaiveDate, line: &str) -> Result<()> {
        if !self.open_files.contains_key(&date) {
            let filename = format!("prices_{}.csv", date.format("%Y%m%d"));
            let filepath = self.log_dir.join(&filename);
            let file_exists = filepath.exists();

            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&filepath)
                .with_context(|| format!("Failed to open {}", filepath.display()))?;

            if !file_exists {
                writeln!(f, "{}", CSV_HEADER)?;
                info!("Created new file: {}", filename);
            } else {
                info!("Appending to existing file: {}", filename);
            }
            self.open_files.insert(date, f);
        }

        let file = self.open_files.get_mut(&date).unwrap();
        file.write_all(line.as_bytes())?;
        self.rows_written += 1;
        Ok(())
    }

    fn flush_all(&mut self) {
        for (_, file) in self.open_files.iter_mut() {
            let _ = file.flush();
        }
    }
}

// ── Statistics ──────────────────────────────────────────────────────────

struct BackfillStats {
    total_logs: u64,
    events_written: u64,
    blocks_with_events: u64,
    timestamp_fetches: u64,
    batches_processed: u64,
    errors: u64,
}

impl BackfillStats {
    fn new() -> Self {
        Self {
            total_logs: 0,
            events_written: 0,
            blocks_with_events: 0,
            timestamp_fetches: 0,
            batches_processed: 0,
            errors: 0,
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Compute price from tick (matches V3PoolState::price_from_tick in types.rs)
fn compute_price(tick: i32, token0_decimals: u8, token1_decimals: u8) -> f64 {
    let base: f64 = 1.0001;
    let price = base.powi(tick);
    let decimal_adjustment = 10_f64.powi(token0_decimals as i32 - token1_decimals as i32);
    price * decimal_adjustment
}

/// Find the highest block number already backfilled in existing CSV files.
/// Scans the last few lines of each file to find the max block.
fn find_last_backfilled_block(log_dir: &str) -> Option<u64> {
    let dir = match fs::read_dir(log_dir) {
        Ok(d) => d,
        Err(_) => return None,
    };

    let mut max_block: u64 = 0;

    for entry in dir.flatten() {
        let filename = entry.file_name().to_string_lossy().to_string();
        if !filename.starts_with("prices_") || !filename.ends_with(".csv") {
            continue;
        }

        // Read last 20 lines of each file to find the highest block
        if let Ok(file) = File::open(entry.path()) {
            let reader = BufReader::new(file);
            let lines: Vec<String> = reader.lines().flatten().collect();
            // Check last 20 lines (or fewer if file is small)
            let start = if lines.len() > 20 { lines.len() - 20 } else { 0 };
            for line in &lines[start..] {
                if line.starts_with("timestamp") {
                    continue; // header
                }
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 2 {
                    if let Ok(block) = parts[1].parse::<u64>() {
                        if block > max_block {
                            max_block = block;
                        }
                    }
                }
            }
        }
    }

    if max_block > 0 {
        Some(max_block)
    } else {
        None
    }
}

/// Resolve DexType from whitelist dex name + fee tier.
/// Matches the logic in main.rs lines 126-147.
fn resolve_dex_type(dex_name: &str, fee_tier: u32) -> Option<DexType> {
    match dex_name {
        "UniswapV3" => V3_FEE_TIERS.iter()
            .find(|(fee, _)| *fee == fee_tier)
            .map(|(_, dt)| *dt),
        "SushiswapV3" => SUSHI_V3_FEE_TIERS.iter()
            .find(|(fee, _)| *fee == fee_tier)
            .map(|(_, dt)| *dt),
        "QuickswapV3" => Some(DexType::QuickswapV3),
        _ => None,
    }
}

/// Get block time estimate for Polygon (used in block range calculation)
fn block_time_secs(chain: &str) -> u64 {
    match chain {
        "polygon" => 2,
        "base" => 2,
        _ => 2,
    }
}

// ── Main ────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    // Load .env.{chain} for RPC URL
    let env_file = format!(".env.{}", args.chain);
    dotenv::from_filename(&env_file).ok();

    // Convert WS URL to HTTP (or use BACKFILL_RPC_URL override)
    let rpc_url = std::env::var("BACKFILL_RPC_URL").unwrap_or_else(|_| {
        let ws_url = std::env::var("RPC_URL").expect("RPC_URL not set in .env");
        ws_url
            .replace("wss://", "https://")
            .replace("ws://", "http://")
    });

    info!("===========================================");
    info!("   DEX Arbitrage Event Backfill");
    info!("===========================================");
    info!("Chain: {}", args.chain);
    info!("RPC: {}...{}", &rpc_url[..30.min(rpc_url.len())],
        if rpc_url.len() > 40 { &rpc_url[rpc_url.len()-10..] } else { "" });

    // Create HTTP provider
    let provider = ProviderBuilder::new()
        .connect_http(rpc_url.parse().context("Invalid RPC URL")?);

    // Verify connection
    let latest_block = provider.get_block_number().await
        .context("Failed to connect to RPC")?;
    info!("Connected! Latest block: {}", latest_block);

    // Load whitelist
    let whitelist_path = format!(
        "/home/botuser/bots/dexarb/config/{}/pools_whitelist.json",
        args.chain
    );
    let whitelist = WhitelistFilter::load(&whitelist_path)
        .context("Failed to load whitelist")?;

    // Filter to V3 pools only (status="active")
    let v3_whitelist: Vec<_> = whitelist.raw.whitelist.pools.iter()
        .filter(|p| p.status == "active")
        .collect();
    info!("Whitelist: {} V3 pools loaded", v3_whitelist.len());

    // Build pool metadata: resolve DexType, fetch token decimals
    let mut pool_lookup: HashMap<Address, PoolMeta> = HashMap::new();
    let mut pool_addresses: Vec<Address> = Vec::new();
    let mut decimals_cache: HashMap<Address, u8> = HashMap::new();

    for wl_pool in &v3_whitelist {
        let dex_type = match resolve_dex_type(&wl_pool.dex, wl_pool.fee_tier) {
            Some(dt) => dt,
            None => {
                warn!("Unknown dex/fee for {} {} @ {} — skipping",
                    wl_pool.pair, wl_pool.dex, wl_pool.fee_tier);
                continue;
            }
        };

        let pool_address: Address = match wl_pool.address.parse() {
            Ok(addr) => addr,
            Err(e) => {
                warn!("Invalid address '{}' — skipping: {}", wl_pool.address, e);
                continue;
            }
        };

        // Fetch token addresses from pool contract
        let pool_contract = UniswapV3Pool::new(pool_address, &provider);
        let token0_call = pool_contract.token0();
        let token1_call = pool_contract.token1();
        let (token0_res, token1_res) = tokio::join!(
            token0_call.call(),
            token1_call.call()
        );

        let token0 = match token0_res {
            Ok(t) => t,
            Err(e) => {
                warn!("Failed to get token0 for {} — skipping: {}", wl_pool.pair, e);
                continue;
            }
        };
        let token1 = match token1_res {
            Ok(t) => t,
            Err(e) => {
                warn!("Failed to get token1 for {} — skipping: {}", wl_pool.pair, e);
                continue;
            }
        };

        // Fetch decimals (cached)
        let token0_decimals = match decimals_cache.get(&token0) {
            Some(&d) => d,
            None => {
                let d = IERC20::new(token0, &provider)
                    .decimals().call().await
                    .with_context(|| format!("Failed to get decimals for {:?}", token0))?;
                decimals_cache.insert(token0, d);
                d
            }
        };
        let token1_decimals = match decimals_cache.get(&token1) {
            Some(&d) => d,
            None => {
                let d = IERC20::new(token1, &provider)
                    .decimals().call().await
                    .with_context(|| format!("Failed to get decimals for {:?}", token1))?;
                decimals_cache.insert(token1, d);
                d
            }
        };

        pool_lookup.insert(pool_address, PoolMeta {
            dex: dex_type,
            pair_symbol: wl_pool.pair.clone(),
            fee: wl_pool.fee_tier,
            token0_decimals,
            token1_decimals,
        });
        pool_addresses.push(pool_address);

        info!("  {} | {} | fee={} | decimals=({},{})",
            wl_pool.pair, dex_type, wl_pool.fee_tier, token0_decimals, token1_decimals);
    }

    if pool_addresses.is_empty() {
        anyhow::bail!("No pools resolved from whitelist — nothing to backfill");
    }

    // Compute block range
    let end_block = args.end_block.unwrap_or(latest_block);
    let blocks_per_week = 7 * 86400 / block_time_secs(&args.chain);
    let default_start = end_block.saturating_sub(args.weeks * blocks_per_week);
    let mut start_block = args.start_block.unwrap_or(default_start);

    // Output directory
    let output_dir = args.output_dir.unwrap_or_else(|| {
        format!("/home/botuser/bots/dexarb/data/{}/price_history", args.chain)
    });

    // Resume check
    if let Some(last_block) = find_last_backfilled_block(&output_dir) {
        if last_block >= start_block {
            info!("Resume: found data up to block {}. Adjusting start to {}",
                last_block, last_block + 1);
            start_block = last_block + 1;
        }
    }

    let total_blocks = end_block.saturating_sub(start_block);
    info!("Block range: {} - {} ({} blocks, ~{} weeks)",
        start_block, end_block, total_blocks,
        total_blocks as f64 / blocks_per_week as f64);
    info!("Output: {}", output_dir);
    info!("Batch size: {} blocks, delay: {}ms", args.batch_size, args.batch_delay_ms);

    if start_block >= end_block {
        info!("Nothing to backfill — start_block >= end_block");
        return Ok(());
    }

    // V3 Swap event topic (matches main.rs line 408-410)
    let v3_swap_topic: B256 = keccak256(
        b"Swap(address,address,int256,int256,uint160,uint128,int24)"
    );

    // Create writer
    let mut writer = BackfillWriter::new(&output_dir)?;
    let mut stats = BackfillStats::new();
    let mut timestamp_cache: HashMap<u64, u64> = HashMap::new();
    let start_time = std::time::Instant::now();

    // Main backfill loop
    let mut batch_start = start_block;
    let mut current_batch_size = args.batch_size;

    while batch_start <= end_block {
        let batch_end = (batch_start + current_batch_size - 1).min(end_block);

        // Build filter
        let filter = Filter::new()
            .from_block(batch_start)
            .to_block(batch_end)
            .address(pool_addresses.clone())
            .event_signature(vec![v3_swap_topic]);

        // Fetch logs with retry
        let logs = {
            let mut attempt = 0;
            loop {
                match provider.get_logs(&filter).await {
                    Ok(logs) => break logs,
                    Err(e) => {
                        attempt += 1;
                        if attempt >= MAX_RETRIES {
                            warn!("Failed to fetch logs for blocks {}-{} after {} retries: {}",
                                batch_start, batch_end, MAX_RETRIES, e);
                            stats.errors += 1;
                            break Vec::new();
                        }
                        warn!("Retry {}/{} for blocks {}-{}: {}",
                            attempt, MAX_RETRIES, batch_start, batch_end, e);
                        sleep(Duration::from_millis(1000 * 2u64.pow(attempt))).await;
                    }
                }
            }
        };

        // Check for Alchemy log limit — if hit, reduce batch size and retry
        if logs.len() >= ALCHEMY_LOG_LIMIT {
            let new_size = current_batch_size / 2;
            if new_size < 10 {
                warn!("Batch size too small ({}) — skipping blocks {}-{}", new_size, batch_start, batch_end);
                batch_start = batch_end + 1;
                current_batch_size = args.batch_size;
                continue;
            }
            warn!("Hit Alchemy log limit ({}). Reducing batch size {} → {}",
                ALCHEMY_LOG_LIMIT, current_batch_size, new_size);
            current_batch_size = new_size;
            continue; // Retry with smaller batch (don't advance batch_start)
        }

        // Restore batch size if we had reduced it
        if current_batch_size < args.batch_size {
            current_batch_size = args.batch_size;
        }

        stats.total_logs += logs.len() as u64;

        // Collect unique block numbers from logs
        let event_blocks: BTreeSet<u64> = logs.iter()
            .filter_map(|log| log.block_number)
            .collect();

        // Fetch block timestamps for new blocks
        for &block_num in &event_blocks {
            if !timestamp_cache.contains_key(&block_num) {
                match provider.get_block_by_number(block_num.into()).await {
                    Ok(Some(block)) => {
                        timestamp_cache.insert(block_num, block.header.timestamp);
                        stats.timestamp_fetches += 1;
                    }
                    Ok(None) => {
                        warn!("Block {} not found — skipping events from this block", block_num);
                    }
                    Err(e) => {
                        warn!("Failed to fetch block {}: {} — skipping", block_num, e);
                        stats.errors += 1;
                    }
                }

                // Small delay between block fetches to avoid rate limiting
                if stats.timestamp_fetches % 50 == 0 {
                    sleep(Duration::from_millis(50)).await;
                }
            }
        }
        stats.blocks_with_events += event_blocks.len() as u64;

        // Parse each log and write CSV row
        for log in &logs {
            let topics = log.topics();
            if topics.is_empty() || topics[0] != v3_swap_topic {
                continue;
            }

            let pool_addr = log.address();
            let meta = match pool_lookup.get(&pool_addr) {
                Some(m) => m,
                None => continue, // Unknown pool (shouldn't happen with address filter)
            };

            let data = &log.inner.data.data;
            if data.len() < 160 {
                warn!("Malformed V3 Swap event (data len {}): skipping", data.len());
                continue;
            }

            // Parse V3 Swap event data (matches main.rs lines 730-743)
            let sqrt_price_x96 = U256::from_be_slice(&data[64..96]);
            let liquidity = U256::from_be_slice(&data[96..128]).to::<u128>();
            let tick = i32::from_be_bytes([
                data[156], data[157],
                data[158], data[159],
            ]);

            // Compute price (matches V3PoolState::price_from_tick)
            let price = compute_price(tick, meta.token0_decimals, meta.token1_decimals);

            // Get block timestamp
            let block_num = match log.block_number {
                Some(bn) => bn,
                None => continue,
            };
            let unix_ts = match timestamp_cache.get(&block_num) {
                Some(&ts) => ts,
                None => continue, // Block fetch failed earlier
            };

            // Format timestamp as ISO 8601 (block timestamps are whole seconds → .000Z)
            let dt = DateTime::from_timestamp(unix_ts as i64, 0)
                .unwrap_or_else(|| Utc::now());
            let timestamp = dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
            let date = dt.date_naive();

            // Write CSV row (format matches price_logger.rs line 74 exactly)
            let line = format!(
                "{},{},{},{},{},{:.10},{},{},{},{:?}\n",
                timestamp,
                block_num,
                meta.pair_symbol,
                meta.dex,
                meta.fee,
                price,
                tick,
                liquidity,
                sqrt_price_x96,
                pool_addr,
            );

            if let Err(e) = writer.write_row(date, &line) {
                warn!("Failed to write row: {}", e);
                stats.errors += 1;
            } else {
                stats.events_written += 1;
            }
        }

        // Flush periodically
        if stats.batches_processed % 10 == 0 {
            writer.flush_all();
        }

        stats.batches_processed += 1;

        // Progress report
        let progress = if total_blocks > 0 {
            ((batch_end - start_block) as f64 / total_blocks as f64) * 100.0
        } else {
            100.0
        };
        let elapsed = start_time.elapsed().as_secs();
        info!(
            "{:5.1}% | blocks {}-{} | {} logs | {} written | {} ts fetched | {}s elapsed",
            progress, batch_start, batch_end, logs.len(), stats.events_written,
            stats.timestamp_fetches, elapsed
        );

        // Advance to next batch
        batch_start = batch_end + 1;

        // Rate limit
        if batch_start <= end_block {
            sleep(Duration::from_millis(args.batch_delay_ms)).await;
        }
    }

    // Final flush
    writer.flush_all();

    // Summary
    let elapsed = start_time.elapsed();
    info!("===========================================");
    info!("   Backfill Complete");
    info!("===========================================");
    info!("Total logs processed:  {}", stats.total_logs);
    info!("Events written:        {}", stats.events_written);
    info!("Blocks with events:    {}", stats.blocks_with_events);
    info!("Timestamp fetches:     {}", stats.timestamp_fetches);
    info!("Batches processed:     {}", stats.batches_processed);
    info!("Errors:                {}", stats.errors);
    info!("Duration:              {:.1}s", elapsed.as_secs_f64());
    info!("Output directory:      {}", output_dir);

    // List generated files
    if let Ok(entries) = fs::read_dir(&output_dir) {
        let mut files: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("prices_") && name.ends_with(".csv") {
                    Some(name)
                } else {
                    None
                }
            })
            .collect();
        files.sort();
        info!("Files: {} ({})", files.len(), files.join(", "));
    }

    Ok(())
}
