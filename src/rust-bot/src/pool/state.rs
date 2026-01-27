//! Pool State Management
//!
//! Thread-safe storage for DEX pool states using DashMap.
//!
//! Author: AI-Generated
//! Created: 2026-01-27

use crate::types::{DexType, PoolState};
use dashmap::DashMap;
use std::sync::Arc;
use tracing::debug;

/// Thread-safe pool state manager
///
/// Uses DashMap for concurrent read/write access to pool states.
/// Key is (DexType, pair_symbol) tuple for fast lookups.
#[derive(Debug)]
pub struct PoolStateManager {
    /// Pool states indexed by (DEX, pair_symbol)
    pools: Arc<DashMap<(DexType, String), PoolState>>,
}

impl PoolStateManager {
    /// Create a new empty PoolStateManager
    pub fn new() -> Self {
        Self {
            pools: Arc::new(DashMap::new()),
        }
    }

    /// Add or update a pool state
    pub fn update_pool(&self, pool: PoolState) {
        let key = (pool.dex, pool.pair.symbol.clone());
        debug!(
            "Updating pool: {} on {:?} - reserves: ({}, {})",
            pool.pair.symbol, pool.dex, pool.reserve0, pool.reserve1
        );
        self.pools.insert(key, pool);
    }

    /// Get pool state for a specific DEX and pair
    pub fn get_pool(&self, dex: DexType, pair_symbol: &str) -> Option<PoolState> {
        let key = (dex, pair_symbol.to_string());
        self.pools.get(&key).map(|entry| entry.clone())
    }

    /// Get all pools for a specific pair across all DEXs
    pub fn get_pools_for_pair(&self, pair_symbol: &str) -> Vec<PoolState> {
        self.pools
            .iter()
            .filter(|entry| entry.key().1 == pair_symbol)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get all pool states
    pub fn get_all_pools(&self) -> Vec<PoolState> {
        self.pools.iter().map(|entry| entry.value().clone()).collect()
    }

    /// Check if any pool data is stale (more than `max_blocks` old)
    pub fn is_stale(&self, current_block: u64, max_blocks: u64) -> bool {
        self.pools
            .iter()
            .any(|entry| current_block.saturating_sub(entry.value().last_updated) > max_blocks)
    }

    /// Get statistics: (pool_count, oldest_block, newest_block)
    pub fn stats(&self) -> (usize, u64, u64) {
        let count = self.pools.len();
        let min_block = self
            .pools
            .iter()
            .map(|entry| entry.value().last_updated)
            .min()
            .unwrap_or(0);
        let max_block = self
            .pools
            .iter()
            .map(|entry| entry.value().last_updated)
            .max()
            .unwrap_or(0);

        (count, min_block, max_block)
    }

    /// Remove a pool from state
    pub fn remove_pool(&self, dex: DexType, pair_symbol: &str) -> Option<PoolState> {
        let key = (dex, pair_symbol.to_string());
        self.pools.remove(&key).map(|(_, v)| v)
    }

    /// Clear all pool states
    pub fn clear(&self) {
        self.pools.clear();
    }

    /// Check if a pool exists
    pub fn contains(&self, dex: DexType, pair_symbol: &str) -> bool {
        let key = (dex, pair_symbol.to_string());
        self.pools.contains_key(&key)
    }
}

impl Default for PoolStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PoolStateManager {
    fn clone(&self) -> Self {
        Self {
            pools: Arc::clone(&self.pools),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TradingPair;
    use ethers::types::{Address, U256};

    fn create_test_pool(dex: DexType, symbol: &str, reserve0: u64, reserve1: u64) -> PoolState {
        PoolState {
            address: Address::zero(),
            dex,
            pair: TradingPair::new(Address::zero(), Address::zero(), symbol.to_string()),
            reserve0: U256::from(reserve0),
            reserve1: U256::from(reserve1),
            last_updated: 100,
        }
    }

    #[test]
    fn test_update_and_get_pool() {
        let manager = PoolStateManager::new();
        let pool = create_test_pool(DexType::Uniswap, "ETH/USDC", 1000, 2000);

        manager.update_pool(pool.clone());

        let retrieved = manager.get_pool(DexType::Uniswap, "ETH/USDC");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().reserve0, U256::from(1000));
    }

    #[test]
    fn test_get_pools_for_pair() {
        let manager = PoolStateManager::new();

        let pool1 = create_test_pool(DexType::Uniswap, "ETH/USDC", 1000, 2000);
        let pool2 = create_test_pool(DexType::Sushiswap, "ETH/USDC", 1100, 2100);

        manager.update_pool(pool1);
        manager.update_pool(pool2);

        let pools = manager.get_pools_for_pair("ETH/USDC");
        assert_eq!(pools.len(), 2);
    }

    #[test]
    fn test_stats() {
        let manager = PoolStateManager::new();
        let pool = create_test_pool(DexType::Uniswap, "ETH/USDC", 1000, 2000);
        manager.update_pool(pool);

        let (count, min, max) = manager.stats();
        assert_eq!(count, 1);
        assert_eq!(min, 100);
        assert_eq!(max, 100);
    }
}
