//! Data Collector Module
//!
//! Continuously syncs pool state from the blockchain and writes
//! to a shared JSON file for other processes to consume.
//!
//! Author: AI-Generated
//! Created: 2026-01-28

pub mod shared_state;

pub use shared_state::{SerializablePoolState, SharedPoolState, SyncStats};

use crate::pool::{PoolStateManager, PoolSyncer};
use crate::types::BotConfig;
use anyhow::Result;
use ethers::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

/// Default path for shared state file
pub const DEFAULT_STATE_PATH: &str = "/home/botuser/bots/dexarb/data/pool_state.json";

/// Run the data collector loop
///
/// This continuously syncs pool state and writes to a shared JSON file.
pub async fn run_data_collector<M>(
    provider: Arc<M>,
    config: BotConfig,
    state_path: PathBuf,
) -> Result<()>
where
    M: Middleware + 'static,
    M::Error: 'static,
{
    info!("Starting Data Collector");
    info!("  Chain ID: {}", config.chain_id);
    info!("  Poll interval: {}ms", config.poll_interval_ms);
    info!("  State file: {}", state_path.display());
    info!("  Pairs: {:?}", config.pairs.iter().map(|p| &p.symbol).collect::<Vec<_>>());

    let state_manager = PoolStateManager::new();
    let syncer = PoolSyncer::new(
        Arc::clone(&provider),
        config.clone(),
        state_manager.clone(),
    );

    let mut shared_state = SharedPoolState::new(config.chain_id);
    let poll_interval = Duration::from_millis(config.poll_interval_ms);

    // Initial sync
    info!("Performing initial sync...");
    match syncer.initial_sync().await {
        Ok(_) => {
            shared_state.stats.successful_syncs += 1;
            info!("Initial sync complete");
        }
        Err(e) => {
            shared_state.stats.failed_syncs += 1;
            error!("Initial sync failed: {}", e);
        }
    }

    // Main loop
    let mut interval = tokio::time::interval(poll_interval);

    loop {
        interval.tick().await;

        shared_state.stats.total_syncs += 1;

        // Sync pools
        match syncer.initial_sync().await {
            Ok(_) => {
                shared_state.stats.successful_syncs += 1;
            }
            Err(e) => {
                shared_state.stats.failed_syncs += 1;
                error!("Sync error: {}", e);
                continue;
            }
        }

        // Get current block
        let block_number = provider
            .get_block_number()
            .await
            .map(|b| b.as_u64())
            .unwrap_or(shared_state.block_number);

        shared_state.block_number = block_number;

        // Update shared state with all pools
        for pool in state_manager.get_all_pools() {
            shared_state.update_pool(&pool);
        }

        // Write to file
        if let Err(e) = shared_state.write_to_file(&state_path) {
            error!("Failed to write state file: {}", e);
        }

        // Log progress periodically (every 60 syncs)
        if shared_state.stats.total_syncs % 60 == 0 {
            info!(
                "Collector stats: {} syncs, {} pools, block {}",
                shared_state.stats.total_syncs,
                shared_state.pools.len(),
                shared_state.block_number
            );
        }
    }
}
