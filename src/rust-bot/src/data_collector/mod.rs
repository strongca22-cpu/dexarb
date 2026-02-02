//! Data Collector Module
//!
//! Continuously syncs pool state from the blockchain and writes
//! to a shared JSON file for other processes to consume.
//!
//! V3-only whitelist mode:
//! - Only syncs whitelisted V3 pools (from pools_whitelist.json)
//! - V2 sync removed — live bot reads V3 data only
//! - Initial sync: discovers full pool state via sync_pool_by_address()
//! - Loop: concurrent refresh via sync_known_pools_parallel()
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//! Modified: 2026-01-28 (added V3 pool support)
//! Modified: 2026-01-28 (staggered V3 sync to avoid rate limiting)
//! Modified: 2026-01-30 (V3-only whitelist sync — removed V2, added parallel refresh)

pub mod shared_state;

pub use shared_state::{SerializablePoolState, SerializableV3PoolState, SharedPoolState, SyncStats};

use crate::filters::WhitelistFilter;
use crate::pool::{V3PoolSyncer, V3_FEE_TIERS};
use crate::types::BotConfig;
use anyhow::Result;
use alloy::primitives::Address;
use alloy::providers::Provider;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Default path for shared state file
pub const DEFAULT_STATE_PATH: &str = "/home/botuser/bots/dexarb/data/pool_state.json";

/// Run the data collector loop
///
/// This continuously syncs pool state and writes to a shared JSON file.
/// V3-only mode: syncs only whitelisted V3 pools.
pub async fn run_data_collector<P>(
    provider: Arc<P>,
    config: BotConfig,
    state_path: PathBuf,
) -> Result<()>
where
    P: Provider + 'static,
{
    info!("Starting Data Collector (V3-only, whitelist mode)");
    info!("  Chain ID: {}", config.chain_id);
    info!("  Poll interval: {}ms", config.poll_interval_ms);
    info!("  State file: {}", state_path.display());

    // V3 pool syncer
    let mut v3_syncer = V3PoolSyncer::new(Arc::clone(&provider), config.clone());
    let v3_enabled = config.uniswap_v3_factory.is_some();
    if !v3_enabled {
        error!("UNISWAP_V3_FACTORY not set — V3-only mode requires it");
        anyhow::bail!("UNISWAP_V3_FACTORY is required for V3-only whitelist mode");
    }
    info!("V3 factory: {:?}", config.uniswap_v3_factory);

    // Load whitelist
    let whitelist_path = config.whitelist_file.as_deref()
        .unwrap_or("/home/botuser/bots/dexarb/config/pools_whitelist.json");
    let whitelist = match WhitelistFilter::load(whitelist_path) {
        Ok(wl) => {
            info!("Whitelist loaded: {} active pools from {}", wl.active_pool_count(), whitelist_path);
            wl
        }
        Err(e) => {
            error!("Failed to load whitelist from {}: {}", whitelist_path, e);
            anyhow::bail!("Whitelist is required for V3-only mode: {}", e);
        }
    };

    let mut shared_state = SharedPoolState::new(config.chain_id);
    let poll_interval = Duration::from_millis(config.poll_interval_ms);

    // V2 sync: SKIPPED — live bot reads V3 data only
    info!("V2 sync: Skipped (live bot uses V3 data only)");

    // Initial V3 sync: discover full state for each whitelisted pool
    info!("Initial V3 sync: discovering {} whitelisted pools...", whitelist.active_pool_count());
    let mut v3_pools = Vec::new();
    let active_pools: Vec<_> = whitelist.raw.whitelist.pools.iter()
        .filter(|p| p.status == "active")
        .collect();

    for wl_pool in &active_pools {
        // Map fee_tier to DexType
        let dex_type = match V3_FEE_TIERS.iter().find(|(fee, _)| *fee == wl_pool.fee_tier) {
            Some((_, dt)) => *dt,
            None => {
                warn!("Unknown fee tier {} for {} — skipping", wl_pool.fee_tier, wl_pool.pair);
                continue;
            }
        };

        // Parse address
        let pool_address: Address = match wl_pool.address.parse() {
            Ok(addr) => addr,
            Err(e) => {
                warn!("Invalid address '{}' for {} — skipping: {}", wl_pool.address, wl_pool.pair, e);
                continue;
            }
        };

        match v3_syncer.sync_pool_by_address(pool_address, dex_type).await {
            Ok(mut pool_state) => {
                // Override pair symbol from whitelist (sync_pool_by_address sets "UNKNOWN")
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
    shared_state.stats.successful_syncs += 1;

    // Write initial state
    for pool in &v3_pools {
        shared_state.update_v3_pool(pool);
    }
    if let Err(e) = shared_state.write_to_file(&state_path) {
        error!("Failed to write initial state file: {}", e);
    }

    // Main loop: concurrent refresh of all known V3 pools
    let mut interval = tokio::time::interval(poll_interval);

    loop {
        interval.tick().await;

        shared_state.stats.total_syncs += 1;

        // Refresh all known V3 pools concurrently
        if !v3_pools.is_empty() {
            let updated = v3_syncer.sync_known_pools_parallel(&v3_pools).await;
            if !updated.is_empty() {
                v3_pools = updated;
                shared_state.stats.successful_syncs += 1;
            } else {
                shared_state.stats.failed_syncs += 1;
                warn!("Parallel V3 sync returned empty — keeping previous state");
            }
        }

        // Update shared state with V3 pools
        for pool in &v3_pools {
            shared_state.update_v3_pool(pool);
        }

        // Block number is already fetched inside sync_known_pools_parallel,
        // but update shared_state with a fresh call for the state file
        let block_number = provider
            .get_block_number()
            .await
            .unwrap_or(shared_state.block_number);
        shared_state.block_number = block_number;

        // Write to file
        if let Err(e) = shared_state.write_to_file(&state_path) {
            error!("Failed to write state file: {}", e);
        }

        // Log progress periodically (every 60 syncs)
        if shared_state.stats.total_syncs % 60 == 0 {
            info!(
                "Collector stats: {} syncs, {} V3 pools, block {}",
                shared_state.stats.total_syncs,
                shared_state.v3_pools.len(),
                shared_state.block_number
            );
        }
    }
}
