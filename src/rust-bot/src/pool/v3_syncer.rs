//! Uniswap V3 Pool Synchronization
//!
//! Fetches V3 pool state (sqrtPriceX96, tick, liquidity, fee)
//! from the blockchain. V3 uses concentrated liquidity with
//! tick-based pricing.
//!
//! Key differences from V2:
//! - Price stored as sqrtPriceX96 (Q64.96 fixed point)
//! - Multiple fee tiers (0.05%, 0.30%, 1.00%)
//! - Concentrated liquidity (ticks instead of reserves)
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use crate::types::{BotConfig, DexType, TradingPair, V3PoolState};
use anyhow::{Context, Result};
use ethers::prelude::*;
use std::sync::Arc;
use tracing::{debug, info, warn};

// Uniswap V3 Factory ABI
abigen!(
    UniswapV3Factory,
    r#"[
        function getPool(address tokenA, address tokenB, uint24 fee) external view returns (address pool)
    ]"#
);

// Uniswap V3 Pool ABI (slot0 returns current price/tick/liquidity)
abigen!(
    UniswapV3Pool,
    r#"[
        function slot0() external view returns (uint160 sqrtPriceX96, int24 tick, uint16 observationIndex, uint16 observationCardinality, uint16 observationCardinalityNext, uint8 feeProtocol, bool unlocked)
        function liquidity() external view returns (uint128)
        function fee() external view returns (uint24)
        function token0() external view returns (address)
        function token1() external view returns (address)
    ]"#
);

// ERC20 for decimals
abigen!(
    ERC20Metadata,
    r#"[
        function decimals() external view returns (uint8)
    ]"#
);

/// V3 fee tiers to check for each pair
pub const V3_FEE_TIERS: [(u32, DexType); 3] = [
    (500, DexType::UniswapV3_005),   // 0.05% - best for stable/correlated pairs
    (3000, DexType::UniswapV3_030),  // 0.30% - standard tier
    (10000, DexType::UniswapV3_100), // 1.00% - exotic pairs
];

/// Syncs V3 pool state from blockchain
pub struct V3PoolSyncer<P> {
    provider: Arc<P>,
    config: BotConfig,
    /// Cache of token decimals to avoid repeated calls
    decimals_cache: std::collections::HashMap<Address, u8>,
}

impl<P: Middleware + 'static> V3PoolSyncer<P> {
    /// Create a new V3PoolSyncer
    pub fn new(provider: Arc<P>, config: BotConfig) -> Self {
        Self {
            provider,
            config,
            decimals_cache: std::collections::HashMap::new(),
        }
    }

    /// Sync all V3 pools for configured pairs
    pub async fn sync_all_v3_pools(&mut self) -> Result<Vec<V3PoolState>> {
        let factory_address = match self.config.uniswap_v3_factory {
            Some(addr) => addr,
            None => {
                info!("Uniswap V3 factory not configured, skipping V3 sync");
                return Ok(vec![]);
            }
        };

        info!("Starting V3 pool sync with factory {:?}", factory_address);

        let mut pools = Vec::new();

        for pair_config in &self.config.pairs.clone() {
            let token0: Address = pair_config
                .token0
                .parse()
                .context("Invalid token0 address")?;
            let token1: Address = pair_config
                .token1
                .parse()
                .context("Invalid token1 address")?;

            let pair = TradingPair::new(token0, token1, pair_config.symbol.clone());

            // Try each fee tier
            for (fee_tier, dex_type) in V3_FEE_TIERS {
                match self.sync_v3_pool(factory_address, &pair, fee_tier, dex_type).await {
                    Ok(Some(pool)) => {
                        info!(
                            "Synced V3 pool: {} @ {}% fee | price={:.6} | liquidity={}",
                            pair.symbol,
                            fee_tier as f64 / 10000.0,
                            pool.price(),
                            pool.liquidity
                        );
                        pools.push(pool);
                    }
                    Ok(None) => {
                        debug!("No V3 pool for {} @ {}% fee", pair.symbol, fee_tier as f64 / 10000.0);
                    }
                    Err(e) => {
                        warn!("Failed to sync V3 pool {} @ {}%: {}", pair.symbol, fee_tier as f64 / 10000.0, e);
                    }
                }
            }
        }

        info!("V3 sync complete: {} pools synced", pools.len());
        Ok(pools)
    }

    /// Sync a specific V3 pool
    ///
    /// IMPORTANT: V3 pools sort tokens by address (token0 < token1).
    /// The tick/sqrtPriceX96 represent price = token1/token0 in the pool's ordering.
    /// We must get the actual token0/token1 from the pool contract, not from config.
    async fn sync_v3_pool(
        &mut self,
        factory_address: Address,
        pair: &TradingPair,
        fee_tier: u32,
        dex_type: DexType,
    ) -> Result<Option<V3PoolState>> {
        // Get pool address from factory
        let factory = UniswapV3Factory::new(factory_address, Arc::clone(&self.provider));

        let pool_address = factory
            .get_pool(pair.token0, pair.token1, fee_tier)
            .call()
            .await
            .context("Failed to get V3 pool address")?;

        // Check if pool exists
        if pool_address == Address::zero() {
            return Ok(None);
        }

        // Get pool state
        let pool = UniswapV3Pool::new(pool_address, Arc::clone(&self.provider));

        // Get slot0 (contains sqrtPriceX96 and tick)
        let (sqrt_price_x96, tick, _, _, _, _, _) = pool
            .slot_0()
            .call()
            .await
            .context("Failed to get slot0")?;

        // Get current liquidity
        let liquidity = pool
            .liquidity()
            .call()
            .await
            .context("Failed to get liquidity")?;

        // CRITICAL: Get the ACTUAL token0/token1 from the pool contract
        // V3 pools sort tokens by address, which may differ from config order
        let actual_token0 = pool.token_0().call().await.context("Failed to get token0")?;
        let actual_token1 = pool.token_1().call().await.context("Failed to get token1")?;

        // Get decimals for the ACTUAL pool tokens
        let token0_decimals = self.get_decimals(actual_token0).await?;
        let token1_decimals = self.get_decimals(actual_token1).await?;

        // Get current block
        let current_block = self
            .provider
            .get_block_number()
            .await
            .context("Failed to get block number")?
            .as_u64();

        // Create pair with actual pool ordering (preserves symbol from config)
        let actual_pair = TradingPair {
            token0: actual_token0,
            token1: actual_token1,
            symbol: pair.symbol.clone(),
        };

        Ok(Some(V3PoolState {
            address: pool_address,
            dex: dex_type,
            pair: actual_pair,
            sqrt_price_x96: U256::from(sqrt_price_x96.as_u128()),
            tick: tick as i32,
            fee: fee_tier,
            liquidity,
            token0_decimals,
            token1_decimals,
            last_updated: current_block,
        }))
    }

    /// Get token decimals (cached)
    async fn get_decimals(&mut self, token: Address) -> Result<u8> {
        if let Some(&decimals) = self.decimals_cache.get(&token) {
            return Ok(decimals);
        }

        let token_contract = ERC20Metadata::new(token, Arc::clone(&self.provider));
        let decimals = token_contract
            .decimals()
            .call()
            .await
            .context("Failed to get token decimals")?;

        self.decimals_cache.insert(token, decimals);
        Ok(decimals)
    }

    /// Sync a subset of V3 pools (staggered sync to avoid rate limiting)
    ///
    /// Instead of syncing all pairs at once (causing RPC bursts), this method
    /// syncs only pairs[start_idx..end_idx]. Call repeatedly with different
    /// ranges to eventually sync all pools.
    ///
    /// With 7 pairs and syncing 2 at a time, full refresh takes 4 calls.
    pub async fn sync_v3_pools_subset(
        &mut self,
        start_idx: usize,
        end_idx: usize,
    ) -> Result<Vec<V3PoolState>> {
        let factory_address = match self.config.uniswap_v3_factory {
            Some(addr) => addr,
            None => {
                return Ok(vec![]);
            }
        };

        let mut pools = Vec::new();
        let pairs: Vec<_> = self.config.pairs.clone();

        // Only sync pairs in the specified range
        for pair_config in pairs.iter().skip(start_idx).take(end_idx - start_idx) {
            let token0: Address = pair_config
                .token0
                .parse()
                .context("Invalid token0 address")?;
            let token1: Address = pair_config
                .token1
                .parse()
                .context("Invalid token1 address")?;

            let pair = TradingPair::new(token0, token1, pair_config.symbol.clone());

            // Try each fee tier
            for (fee_tier, dex_type) in V3_FEE_TIERS {
                match self.sync_v3_pool(factory_address, &pair, fee_tier, dex_type).await {
                    Ok(Some(pool)) => {
                        debug!(
                            "Synced V3 pool: {} @ {}% fee | price={:.6}",
                            pair.symbol,
                            fee_tier as f64 / 10000.0,
                            pool.price(),
                        );
                        pools.push(pool);
                    }
                    Ok(None) => {
                        // Pool doesn't exist for this fee tier
                    }
                    Err(e) => {
                        warn!("Failed to sync V3 pool {} @ {}%: {}", pair.symbol, fee_tier as f64 / 10000.0, e);
                    }
                }
            }
        }

        debug!("V3 subset sync complete: {} pools (pairs {}-{})", pools.len(), start_idx, end_idx);
        Ok(pools)
    }

    /// Sync a single V3 pool by address (for event-driven updates)
    pub async fn sync_pool_by_address(
        &mut self,
        pool_address: Address,
        dex_type: DexType,
    ) -> Result<V3PoolState> {
        let pool = UniswapV3Pool::new(pool_address, Arc::clone(&self.provider));

        // Get slot0
        let (sqrt_price_x96, tick, _, _, _, _, _) = pool
            .slot_0()
            .call()
            .await
            .context("Failed to get slot0")?;

        // Get liquidity
        let liquidity = pool.liquidity().call().await?;

        // Get fee
        let fee = pool.fee().call().await?;

        // Get tokens (ethers-rs abigen converts token0/token1 to token_0/token_1)
        let token0 = pool.token_0().call().await?;
        let token1 = pool.token_1().call().await?;

        // Get decimals
        let token0_decimals = self.get_decimals(token0).await?;
        let token1_decimals = self.get_decimals(token1).await?;

        // Get current block
        let current_block = self
            .provider
            .get_block_number()
            .await?
            .as_u64();

        Ok(V3PoolState {
            address: pool_address,
            dex: dex_type,
            pair: TradingPair::new(token0, token1, "UNKNOWN".to_string()),
            sqrt_price_x96: U256::from(sqrt_price_x96.as_u128()),
            tick: tick as i32,
            fee,
            liquidity,
            token0_decimals,
            token1_decimals,
            last_updated: current_block,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v3_fee_tiers() {
        assert_eq!(V3_FEE_TIERS[0].0, 500);
        assert_eq!(V3_FEE_TIERS[1].0, 3000);
        assert_eq!(V3_FEE_TIERS[2].0, 10000);
    }
}
