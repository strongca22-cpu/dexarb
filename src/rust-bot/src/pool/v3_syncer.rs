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
//! Two sync modes:
//! - sync_all_v3_pools: Full discovery (factory lookup, token addresses, decimals).
//!   Used once at startup. Sequential, requires &mut self for decimals cache.
//! - sync_known_pools_parallel: Fast ongoing sync (slot0 + liquidity only).
//!   Used in main loop. Concurrent via join_all, ~200ms vs ~5.6s sequential.
//!
//! 1% fee tier excluded — all 1% pools on Polygon have phantom liquidity
//! (confirmed by live Quoter testing across UNI, WBTC, LINK).
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//! Modified: 2026-02-01 — Migrated from ethers-rs to alloy

use crate::contracts::{AlgebraPool, IERC20, UniswapV3Factory, UniswapV3Pool};
use crate::types::{BotConfig, DexType, TradingPair, V3PoolState};
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Uniswap V3 fee tiers to check for each pair
pub const V3_FEE_TIERS: [(u32, DexType); 4] = [
    (100, DexType::UniswapV3_001),   // 0.01% - stablecoin pairs (USDT/USDC, DAI/USDC)
    (500, DexType::UniswapV3_005),   // 0.05% - best for stable/correlated pairs
    (3000, DexType::UniswapV3_030),  // 0.30% - standard tier
    (10000, DexType::UniswapV3_100), // 1.00% - exotic pairs (filtered at sync/detect time)
];

/// SushiSwap V3 fee tiers (cross-DEX arb — identical ABI, different pools)
pub const SUSHI_V3_FEE_TIERS: [(u32, DexType); 3] = [
    (100, DexType::SushiV3_001),   // 0.01% - stablecoin pairs
    (500, DexType::SushiV3_005),   // 0.05% - stable/correlated pairs
    (3000, DexType::SushiV3_030),  // 0.30% - standard tier
];

/// Helper: convert u32 fee tier to alloy uint24 type for contract calls.
/// Uses from_limbs() because Uint<24, 1> doesn't impl From<u32>.
fn fee_to_u24(fee: u32) -> alloy::primitives::Uint<24, 1> {
    debug_assert!(fee <= 0xFFFFFF, "fee {} exceeds U24 max (16777215)", fee);
    alloy::primitives::Uint::from_limbs([fee as u64])
}

/// Syncs V3 pool state from blockchain
pub struct V3PoolSyncer<P> {
    provider: Arc<P>,
    config: BotConfig,
    /// Cache of token decimals to avoid repeated calls
    decimals_cache: std::collections::HashMap<Address, u8>,
}

impl<P: Provider + 'static> V3PoolSyncer<P> {
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

            // Try each fee tier (skip 1% — phantom liquidity on Polygon)
            for (fee_tier, dex_type) in V3_FEE_TIERS {
                if fee_tier >= 10000 {
                    debug!("Skipping {} @ 1% fee tier (phantom liquidity)", pair.symbol);
                    continue;
                }
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

        info!("V3 sync complete: {} pools synced (0.01% + 0.05% + 0.30% tiers)", pools.len());
        Ok(pools)
    }

    /// Fast parallel sync of known V3 pools (slot0 + liquidity only).
    ///
    /// Used in the main loop after initial sync. Since pool addresses, tokens,
    /// and decimals are already cached from the initial sync, we only need to
    /// fetch the changing state (sqrtPriceX96, tick, liquidity).
    ///
    /// All pools synced concurrently via futures::future::join_all.
    /// Each pool fires slot0 + liquidity in parallel via tokio::join!.
    /// Block number fetched once and shared across all updates.
    ///
    /// Performance: 14 pools × 2 RPC calls each + 1 block call, all concurrent.
    /// With ~200ms RPC latency: ~400ms total (vs ~5.6s sequential).
    pub async fn sync_known_pools_parallel(
        &self,
        known_pools: &[V3PoolState],
    ) -> Vec<V3PoolState> {
        use futures::future::join_all;

        // Get block number once (shared across all pool syncs)
        let current_block = match self.provider.get_block_number().await {
            Ok(bn) => bn,
            Err(e) => {
                warn!("Failed to get block number for parallel sync: {}", e);
                return Vec::new();
            }
        };

        let futs: Vec<_> = known_pools.iter().map(|pool| {
            let provider = self.provider.clone();
            let pool_state = pool.clone();

            async move {
                if pool_state.dex.is_quickswap_v3() {
                    // Algebra pool: use globalState() instead of slot0()
                    // globalState returns (price, tick, fee, ...) — fee is dynamic
                    let contract = AlgebraPool::new(pool_state.address, provider);
                    let gs_call = contract.globalState();
                    let liq_call = contract.liquidity();
                    let (gs_res, liq_res) = tokio::join!(
                        gs_call.call(),
                        liq_call.call()
                    );

                    match (gs_res, liq_res) {
                        (Ok(gs), Ok(liq)) => {
                            Some(V3PoolState {
                                sqrt_price_x96: U256::from(gs.price),
                                tick: i32::try_from(gs.tick).unwrap_or(0),
                                fee: gs.fee as u32, // dynamic fee from globalState
                                liquidity: liq,
                                last_updated: current_block,
                                ..pool_state
                            })
                        }
                        _ => {
                            warn!(
                                "Failed to fast-sync Algebra pool {} {:?} at {:?}",
                                pool_state.pair.symbol, pool_state.dex, pool_state.address
                            );
                            None
                        }
                    }
                } else {
                    // Uniswap/SushiSwap V3: use slot0()
                    let contract = UniswapV3Pool::new(pool_state.address, provider);
                    let slot0_call = contract.slot0();
                    let liq_call = contract.liquidity();
                    let (slot0_res, liq_res) = tokio::join!(
                        slot0_call.call(),
                        liq_call.call()
                    );

                    match (slot0_res, liq_res) {
                        (Ok(slot0), Ok(liq)) => {
                            Some(V3PoolState {
                                sqrt_price_x96: U256::from(slot0.sqrtPriceX96),
                                tick: i32::try_from(slot0.tick).unwrap_or(0),
                                liquidity: liq,
                                last_updated: current_block,
                                ..pool_state
                            })
                        }
                        _ => {
                            warn!(
                                "Failed to fast-sync pool {} {:?} at {:?}",
                                pool_state.pair.symbol, pool_state.dex, pool_state.address
                            );
                            None
                        }
                    }
                }
            }
        }).collect();

        join_all(futs)
            .await
            .into_iter()
            .filter_map(|r| r)
            .collect()
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
        let factory = UniswapV3Factory::new(factory_address, self.provider.clone());

        let result = factory
            .getPool(pair.token0, pair.token1, fee_to_u24(fee_tier))
            .call()
            .await
            .context("Failed to get V3 pool address")?;

        let pool_address = result;

        // Check if pool exists
        if pool_address == Address::ZERO {
            return Ok(None);
        }

        // Get pool state
        let pool = UniswapV3Pool::new(pool_address, self.provider.clone());

        // Get slot0 (contains sqrtPriceX96 and tick)
        let slot0 = pool
            .slot0()
            .call()
            .await
            .context("Failed to get slot0")?;

        let sqrt_price_x96 = U256::from(slot0.sqrtPriceX96);
        let tick = i32::try_from(slot0.tick).unwrap_or(0);

        // Get current liquidity
        let liquidity = pool
            .liquidity()
            .call()
            .await
            .context("Failed to get liquidity")?;

        // Skip pools with zero liquidity (phantom pools — have addresses and
        // stuck prices but no actual tradeable depth). Saves 4 RPC calls per
        // phantom pool and prevents them from entering the known_pools list.
        if liquidity == 0 {
            debug!(
                "Skipping {} @ {}% fee — zero liquidity (phantom pool)",
                pair.symbol,
                fee_tier as f64 / 10000.0
            );
            return Ok(None);
        }

        // CRITICAL: Get the ACTUAL token0/token1 from the pool contract
        // V3 pools sort tokens by address, which may differ from config order
        let actual_token0 = pool.token0().call().await.context("Failed to get token0")?;
        let actual_token1 = pool.token1().call().await.context("Failed to get token1")?;

        // Get decimals for the ACTUAL pool tokens
        let token0_decimals = self.get_decimals(actual_token0).await?;
        let token1_decimals = self.get_decimals(actual_token1).await?;

        // Get current block
        let current_block = self
            .provider
            .get_block_number()
            .await
            .context("Failed to get block number")?;

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
            sqrt_price_x96,
            tick,
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

        let token_contract = IERC20::new(token, self.provider.clone());
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

            // Try each fee tier (skip 1% — phantom liquidity on Polygon)
            for (fee_tier, dex_type) in V3_FEE_TIERS {
                if fee_tier >= 10000 {
                    debug!("Skipping {} @ 1% fee tier (phantom liquidity)", pair.symbol);
                    continue;
                }
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
    /// Automatically uses globalState() for Algebra (QuickSwap V3) and slot0() for Uniswap/Sushi.
    pub async fn sync_pool_by_address(
        &mut self,
        pool_address: Address,
        dex_type: DexType,
    ) -> Result<V3PoolState> {
        let (sqrt_price_x96, tick, fee, liquidity, token0, token1) = if dex_type.is_quickswap_v3() {
            // Algebra pool: globalState() returns (price, tick, fee, ...)
            let pool = AlgebraPool::new(pool_address, self.provider.clone());
            let gs = pool
                .globalState()
                .call()
                .await
                .context("Failed to get Algebra globalState")?;
            let liquidity = pool.liquidity().call().await?;
            let token0 = pool.token0().call().await?;
            let token1 = pool.token1().call().await?;
            (U256::from(gs.price), i32::try_from(gs.tick).unwrap_or(0), gs.fee as u32, liquidity, token0, token1)
        } else {
            // Uniswap/SushiSwap V3 pool: slot0()
            let pool = UniswapV3Pool::new(pool_address, self.provider.clone());
            let slot0 = pool
                .slot0()
                .call()
                .await
                .context("Failed to get slot0")?;
            let liquidity = pool.liquidity().call().await?;
            let fee_val = pool.fee().call().await?;
            let token0 = pool.token0().call().await?;
            let token1 = pool.token1().call().await?;
            (U256::from(slot0.sqrtPriceX96), i32::try_from(slot0.tick).unwrap_or(0), fee_val.to::<u32>(), liquidity, token0, token1)
        };

        // Get decimals
        let token0_decimals = self.get_decimals(token0).await?;
        let token1_decimals = self.get_decimals(token1).await?;

        // Get current block
        let current_block = self
            .provider
            .get_block_number()
            .await?;

        Ok(V3PoolState {
            address: pool_address,
            dex: dex_type,
            pair: TradingPair::new(token0, token1, "UNKNOWN".to_string()),
            sqrt_price_x96,
            tick,
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
        assert_eq!(V3_FEE_TIERS[0].0, 100);
        assert_eq!(V3_FEE_TIERS[1].0, 500);
        assert_eq!(V3_FEE_TIERS[2].0, 3000);
        assert_eq!(V3_FEE_TIERS[3].0, 10000);
    }
}
