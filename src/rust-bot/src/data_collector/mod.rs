//! Data Collector Module
//!
//! Continuously syncs pool state from the blockchain and writes
//! to a shared JSON file for other processes to consume.
//!
//! Supports both V2 and V3 pools:
//! - V2: Uniswap/Sushiswap/Quickswap/ApeSwap (constant product)
//! - V3: Uniswap V3 with multiple fee tiers (concentrated liquidity)
//!
//! V3 Sync Strategy (Staggered to avoid rate limiting):
//! - V3 pools are synced in batches of 2 pairs per iteration
//! - With 7 pairs, full V3 refresh takes ~4 iterations
//! - This spreads RPC load evenly instead of bursting 105 calls at once
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//! Modified: 2026-01-28 (added V3 pool support)
//! Modified: 2026-01-28 (staggered V3 sync to avoid rate limiting)

pub mod shared_state;

pub use shared_state::{SerializablePoolState, SerializableV3PoolState, SharedPoolState, SyncStats};

use crate::pool::{PoolStateManager, PoolSyncer, V3PoolSyncer};
use crate::types::BotConfig;
use anyhow::Result;
use ethers::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Default path for shared state file
pub const DEFAULT_STATE_PATH: &str = "/home/botuser/bots/dexarb/data/pool_state.json";

/// Run the data collector loop
///
/// This continuously syncs pool state and writes to a shared JSON file.
/// Supports both V2 and V3 pools.
pub async fn run_data_collector<M>(
    provider: Arc<M>,
    config: BotConfig,
    state_path: PathBuf,
) -> Result<()>
where
    M: Middleware + 'static,
    M::Error: 'static,
{
    info!("Starting Data Collector (V2 + V3)");
    info!("  Chain ID: {}", config.chain_id);
    info!("  Poll interval: {}ms", config.poll_interval_ms);
    info!("  State file: {}", state_path.display());
    info!("  Pairs: {:?}", config.pairs.iter().map(|p| &p.symbol).collect::<Vec<_>>());

    // V2 pool syncer
    let state_manager = PoolStateManager::new();
    let syncer = PoolSyncer::new(
        Arc::clone(&provider),
        config.clone(),
        state_manager.clone(),
    );

    // V3 pool syncer (Phase 2)
    let mut v3_syncer = V3PoolSyncer::new(Arc::clone(&provider), config.clone());
    let v3_enabled = config.uniswap_v3_factory.is_some();
    if v3_enabled {
        info!("V3 pool syncing: ENABLED (factory: {:?})", config.uniswap_v3_factory);
    } else {
        info!("V3 pool syncing: DISABLED (UNISWAP_V3_FACTORY not set)");
    }

    let mut shared_state = SharedPoolState::new(config.chain_id);
    let poll_interval = Duration::from_millis(config.poll_interval_ms);

    // Initial V2 sync
    info!("Performing initial V2 sync...");
    match syncer.initial_sync().await {
        Ok(_) => {
            shared_state.stats.successful_syncs += 1;
            info!("Initial V2 sync complete");
        }
        Err(e) => {
            shared_state.stats.failed_syncs += 1;
            error!("Initial V2 sync failed: {}", e);
        }
    }

    // V3 sync: Skip initial bulk sync to avoid rate limiting
    // V3 pools will be populated gradually via staggered sync in main loop
    // Full V3 coverage achieved in ~4 iterations (20 seconds at 5000ms)
    let mut v3_pools = Vec::new();
    if v3_enabled {
        info!("V3 sync: Skipping initial bulk sync (will populate via staggered sync)");
        info!("V3 sync: Full V3 coverage expected in ~{} iterations",
              (config.pairs.len() + 1) / 2); // pairs_per_v3_sync = 2
    }

    // Main loop
    let mut interval = tokio::time::interval(poll_interval);
    let mut v3_sync_counter = 0u64;

    // Staggered V3 sync configuration:
    // - Sync V3 every iteration (but only a subset of pairs)
    // - Sync 1 pair per iteration (3 pools = 1 pair × 3 fee tiers)
    // - With 7 pairs, full refresh takes ~7 iterations (70 seconds at 10000ms)
    // - This keeps RPC calls very low to avoid Alchemy free tier limits
    // - At 10s poll: ~6 calls/sec V2 + ~1.5 calls/sec V3 = ~7.5 calls/sec
    let pairs_per_v3_sync = 1;
    let total_pairs = config.pairs.len();
    let mut v3_pair_offset = 0usize;

    loop {
        interval.tick().await;

        shared_state.stats.total_syncs += 1;
        v3_sync_counter += 1;

        // Sync V2 pools
        match syncer.initial_sync().await {
            Ok(_) => {
                shared_state.stats.successful_syncs += 1;
            }
            Err(e) => {
                shared_state.stats.failed_syncs += 1;
                error!("V2 sync error: {}", e);
                continue;
            }
        }

        // Sync V3 pools (staggered - only 2 pairs per iteration)
        if v3_enabled {
            // Calculate which pairs to sync this iteration
            let start_idx = v3_pair_offset;
            let end_idx = std::cmp::min(start_idx + pairs_per_v3_sync, total_pairs);

            // Sync the subset of pairs
            match v3_syncer.sync_v3_pools_subset(start_idx, end_idx).await {
                Ok(new_pools) => {
                    // Merge new pools into existing v3_pools
                    // Remove old pools for these pairs, add new ones
                    for pool in new_pools {
                        // Remove any existing pool with same address
                        v3_pools.retain(|p: &crate::types::V3PoolState| p.address != pool.address);
                        v3_pools.push(pool);
                    }
                }
                Err(e) => {
                    warn!("V3 sync error (pairs {}-{}): {}", start_idx, end_idx, e);
                }
            }

            // Advance to next batch of pairs
            v3_pair_offset = if end_idx >= total_pairs { 0 } else { end_idx };
        }

        // Get current block
        let block_number = provider
            .get_block_number()
            .await
            .map(|b| b.as_u64())
            .unwrap_or(shared_state.block_number);

        shared_state.block_number = block_number;

        // Update shared state with all V2 pools
        for pool in state_manager.get_all_pools() {
            shared_state.update_pool(&pool);
        }

        // Update shared state with V3 pools
        for pool in &v3_pools {
            shared_state.update_v3_pool(pool);
        }

        // Write to file
        if let Err(e) = shared_state.write_to_file(&state_path) {
            error!("Failed to write state file: {}", e);
        }

        // Log progress periodically (every 60 syncs)
        if shared_state.stats.total_syncs % 60 == 0 {
            info!(
                "Collector stats: {} syncs, {} V2 pools, {} V3 pools, block {}",
                shared_state.stats.total_syncs,
                shared_state.pools.len(),
                shared_state.v3_pools.len(),
                shared_state.block_number
            );
        }
    }
}
