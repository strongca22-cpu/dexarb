//! Data Collector Module
//!
//! Continuously syncs pool state from the blockchain and writes
//! to a shared JSON file for other processes to consume.
//!
//! Supports both V2 and V3 pools:
//! - V2: Uniswap/Sushiswap/Quickswap/ApeSwap (constant product)
//! - V3: Uniswap V3 with multiple fee tiers (concentrated liquidity)
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//! Modified: 2026-01-28 (added V3 pool support)

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

    // Initial V3 sync (if enabled)
    let mut v3_pools = Vec::new();
    if v3_enabled {
        info!("Performing initial V3 sync...");
        match v3_syncer.sync_all_v3_pools().await {
            Ok(pools) => {
                v3_pools = pools;
                info!("Initial V3 sync complete: {} pools", v3_pools.len());
            }
            Err(e) => {
                warn!("Initial V3 sync failed: {} (continuing with V2 only)", e);
            }
        }
    }

    // Main loop
    let mut interval = tokio::time::interval(poll_interval);
    let mut v3_sync_counter = 0u64;
    let v3_sync_frequency = 10; // Sync V3 every 10 iterations (less frequent due to higher cost)

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

        // Sync V3 pools (less frequently)
        if v3_enabled && v3_sync_counter % v3_sync_frequency == 0 {
            match v3_syncer.sync_all_v3_pools().await {
                Ok(pools) => {
                    v3_pools = pools;
                }
                Err(e) => {
                    warn!("V3 sync error: {}", e);
                }
            }
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
