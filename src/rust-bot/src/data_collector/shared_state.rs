//! Shared Pool State
//!
//! Provides JSON-based shared state for pool data between
//! the data collector and paper trading processes.
//!
//! Supports both V2 and V3 pools:
//! - V2: Uses reserves for constant product pricing
//! - V3: Uses sqrtPriceX96/tick for concentrated liquidity pricing
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//! Modified: 2026-01-28 (added V3 pool support)

use crate::types::{DexType, PoolState, TradingPair, V3PoolState};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use alloy::primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Serializable pool state for JSON storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializablePoolState {
    pub dex: String,
    pub pair_symbol: String,
    pub address: String,
    pub token0: String,
    pub token1: String,
    pub reserve0: String,
    pub reserve1: String,
    pub last_updated: u64,
    pub price: f64,
}

impl From<&PoolState> for SerializablePoolState {
    fn from(pool: &PoolState) -> Self {
        Self {
            dex: pool.dex.to_string(),
            pair_symbol: pool.pair.symbol.clone(),
            address: format!("{:?}", pool.address),
            token0: format!("{:?}", pool.pair.token0),
            token1: format!("{:?}", pool.pair.token1),
            reserve0: pool.reserve0.to_string(),
            reserve1: pool.reserve1.to_string(),
            last_updated: pool.last_updated,
            price: pool.price(),
        }
    }
}

impl SerializablePoolState {
    /// Convert back to PoolState
    pub fn to_pool_state(&self) -> Result<PoolState> {
        let dex = match self.dex.as_str() {
            "Uniswap" => DexType::Uniswap,
            "Sushiswap" => DexType::Sushiswap,
            "Quickswap" => DexType::Quickswap,
            "Apeswap" => DexType::Apeswap,
            _ => DexType::Uniswap,
        };

        let pair = TradingPair {
            token0: self.token0.parse().unwrap_or(Address::ZERO),
            token1: self.token1.parse().unwrap_or(Address::ZERO),
            symbol: self.pair_symbol.clone(),
        };

        Ok(PoolState {
            dex,
            pair,
            address: self.address.parse().unwrap_or(Address::ZERO),
            reserve0: self.reserve0.parse::<U256>().unwrap_or(U256::ZERO),
            reserve1: self.reserve1.parse::<U256>().unwrap_or(U256::ZERO),
            last_updated: self.last_updated,
            token0_decimals: 18, // Legacy shared state — decimals not stored
            token1_decimals: 18,
        })
    }
}

/// Serializable V3 pool state for JSON storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableV3PoolState {
    pub dex: String,
    pub pair_symbol: String,
    pub address: String,
    pub token0: String,
    pub token1: String,
    pub sqrt_price_x96: String,
    pub tick: i32,
    pub fee: u32,
    pub liquidity: String,
    pub token0_decimals: u8,
    pub token1_decimals: u8,
    pub last_updated: u64,
    pub price: f64,
}

impl From<&V3PoolState> for SerializableV3PoolState {
    fn from(pool: &V3PoolState) -> Self {
        Self {
            dex: pool.dex.to_string(),
            pair_symbol: pool.pair.symbol.clone(),
            address: format!("{:?}", pool.address),
            token0: format!("{:?}", pool.pair.token0),
            token1: format!("{:?}", pool.pair.token1),
            sqrt_price_x96: pool.sqrt_price_x96.to_string(),
            tick: pool.tick,
            fee: pool.fee,
            liquidity: pool.liquidity.to_string(),
            token0_decimals: pool.token0_decimals,
            token1_decimals: pool.token1_decimals,
            last_updated: pool.last_updated,
            price: pool.price(),
        }
    }
}

impl SerializableV3PoolState {
    /// Convert back to V3PoolState
    pub fn to_v3_pool_state(&self) -> Result<V3PoolState> {
        let dex = match self.dex.as_str() {
            "UniswapV3_0.01%" => DexType::UniswapV3_001,
            "UniswapV3_0.05%" => DexType::UniswapV3_005,
            "UniswapV3_0.30%" => DexType::UniswapV3_030,
            "UniswapV3_1.00%" => DexType::UniswapV3_100,
            _ => DexType::UniswapV3_030, // Default to 0.30%
        };

        let pair = TradingPair {
            token0: self.token0.parse().unwrap_or(Address::ZERO),
            token1: self.token1.parse().unwrap_or(Address::ZERO),
            symbol: self.pair_symbol.clone(),
        };

        Ok(V3PoolState {
            dex,
            pair,
            address: self.address.parse().unwrap_or(Address::ZERO),
            sqrt_price_x96: self.sqrt_price_x96.parse::<U256>().unwrap_or(U256::ZERO),
            tick: self.tick,
            fee: self.fee,
            liquidity: self.liquidity.parse().unwrap_or(0),
            token0_decimals: self.token0_decimals,
            token1_decimals: self.token1_decimals,
            last_updated: self.last_updated,
        })
    }

    /// Calculate price from tick (useful for fixing overflow errors)
    /// Price = 1.0001^tick * 10^(decimals0 - decimals1)
    pub fn price_from_tick(&self) -> f64 {
        let base: f64 = 1.0001;
        let price = base.powi(self.tick);

        // Adjust for decimals
        let decimal_adjustment =
            10_f64.powi(self.token0_decimals as i32 - self.token1_decimals as i32);

        price * decimal_adjustment
    }

    /// Get validated price (recalculates from tick if stored price looks invalid)
    /// Stored prices > 1e15 are likely overflow errors from sqrtPriceX96.as_u128() truncation
    pub fn validated_price(&self) -> f64 {
        if self.price > 0.0 && self.price < 1e15 {
            self.price
        } else {
            // Recalculate from tick (always works correctly)
            self.price_from_tick()
        }
    }
}

/// Shared state file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedPoolState {
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
    /// Current block number
    pub block_number: u64,
    /// Chain ID
    pub chain_id: u64,
    /// V2 pool states, keyed by "dex:pair"
    pub pools: HashMap<String, SerializablePoolState>,
    /// V3 pool states, keyed by "dex:pair" (Phase 2)
    #[serde(default)]
    pub v3_pools: HashMap<String, SerializableV3PoolState>,
    /// Sync statistics
    pub stats: SyncStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncStats {
    pub total_syncs: u64,
    pub successful_syncs: u64,
    pub failed_syncs: u64,
    pub start_time: Option<DateTime<Utc>>,
}

impl SharedPoolState {
    pub fn new(chain_id: u64) -> Self {
        Self {
            last_updated: Utc::now(),
            block_number: 0,
            chain_id,
            pools: HashMap::new(),
            v3_pools: HashMap::new(),
            stats: SyncStats {
                start_time: Some(Utc::now()),
                ..Default::default()
            },
        }
    }

    /// Update a V2 pool in the shared state
    pub fn update_pool(&mut self, pool: &PoolState) {
        let key = format!("{}:{}", pool.dex, pool.pair.symbol);
        self.pools.insert(key, SerializablePoolState::from(pool));
        self.last_updated = Utc::now();
    }

    /// Update a V3 pool in the shared state
    pub fn update_v3_pool(&mut self, pool: &V3PoolState) {
        let key = format!("{}:{}", pool.dex, pool.pair.symbol);
        self.v3_pools.insert(key, SerializableV3PoolState::from(pool));
        self.last_updated = Utc::now();
    }

    /// Get all V2 pools for a specific pair
    pub fn get_pools_for_pair(&self, pair_symbol: &str) -> Vec<PoolState> {
        self.pools
            .values()
            .filter(|p| p.pair_symbol == pair_symbol)
            .filter_map(|p| p.to_pool_state().ok())
            .collect()
    }

    /// Get all V3 pools for a specific pair
    pub fn get_v3_pools_for_pair(&self, pair_symbol: &str) -> Vec<V3PoolState> {
        self.v3_pools
            .values()
            .filter(|p| p.pair_symbol == pair_symbol)
            .filter_map(|p| p.to_v3_pool_state().ok())
            .collect()
    }

    /// Get combined prices for a pair from both V2 and V3 pools
    /// Returns a vec of (dex_name, price, fee_percent) tuples
    pub fn get_all_prices_for_pair(&self, pair_symbol: &str) -> Vec<(String, f64, f64)> {
        let mut prices = Vec::new();

        // V2 pools (0.30% fee)
        for pool in self.pools.values() {
            if pool.pair_symbol == pair_symbol {
                prices.push((pool.dex.clone(), pool.price, 0.30));
            }
        }

        // V3 pools (variable fee)
        for pool in self.v3_pools.values() {
            if pool.pair_symbol == pair_symbol {
                let fee_percent = pool.fee as f64 / 10000.0;
                prices.push((pool.dex.clone(), pool.price, fee_percent));
            }
        }

        prices
    }

    /// Write to JSON file
    pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize shared state")?;

        // Write to temp file first, then rename (atomic)
        let temp_path = path.as_ref().with_extension("tmp");
        std::fs::write(&temp_path, &json)
            .context("Failed to write temp file")?;
        std::fs::rename(&temp_path, path.as_ref())
            .context("Failed to rename temp file")?;

        Ok(())
    }

    /// Read from JSON file
    pub fn read_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json = std::fs::read_to_string(path.as_ref())
            .context("Failed to read shared state file")?;
        let state: Self = serde_json::from_str(&json)
            .context("Failed to parse shared state JSON")?;
        Ok(state)
    }

    /// Check if state is stale (older than threshold)
    pub fn is_stale(&self, max_age_secs: i64) -> bool {
        let age = Utc::now().signed_duration_since(self.last_updated);
        age.num_seconds() > max_age_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_state_roundtrip() {
        let mut state = SharedPoolState::new(137);
        state.block_number = 12345;

        let pair = TradingPair {
            token0: Address::ZERO,
            token1: Address::ZERO,
            symbol: "WETH/USDC".to_string(),
        };

        let pool = PoolState {
            dex: DexType::Uniswap,
            pair,
            address: Address::ZERO,
            reserve0: U256::from(1000000),
            reserve1: U256::from(2000000),
            last_updated: 12345,
            token0_decimals: 18,
            token1_decimals: 18,
        };

        state.update_pool(&pool);

        let json = serde_json::to_string(&state).unwrap();
        let restored: SharedPoolState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.block_number, 12345);
        assert_eq!(restored.pools.len(), 1);
    }
}
