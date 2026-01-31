//! V2 Pool Synchronization (V2↔V3 Cross-Protocol Arbitrage)
//!
//! Fetches V2 pool reserves from blockchain for known pool addresses.
//! Supports initial sync (full state: token0, token1, decimals, reserves)
//! and parallel ongoing sync (reserves only — fast, 1 RPC call per pool).
//!
//! V2 pools use constant-product AMM (x * y = k) with 0.3% fee.
//! Price calculation uses decimal-adjusted reserves for cross-protocol
//! comparison with V3 tick-based prices.
//!
//! Author: AI-Generated
//! Created: 2026-01-30
//! Modified: 2026-01-30 - Initial implementation for V2↔V3 cross-protocol arb

use crate::types::{DexType, PoolState, TradingPair};
use anyhow::{Context, Result};
use ethers::prelude::*;
use std::sync::Arc;
use tracing::{debug, info, warn};

// V2 pool contract ABIs (same as Uniswap V2 — all V2 forks share this interface)
abigen!(
    IV2Pair,
    r#"[
        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast)
        function token0() external view returns (address)
        function token1() external view returns (address)
    ]"#
);

// ERC20 decimals query
abigen!(
    IERC20Decimals,
    r#"[
        function decimals() external view returns (uint8)
    ]"#
);

/// V2 pool syncer — fetches reserves for known V2 pool addresses.
/// Designed for the live bot's V2↔V3 cross-protocol arbitrage flow.
pub struct V2PoolSyncer<P> {
    provider: Arc<P>,
}

impl<P: Middleware + 'static> V2PoolSyncer<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }

    /// Initial sync: discover full state for a single V2 pool by address.
    /// Fetches token0, token1, decimals, and reserves.
    /// Called once at startup for each whitelisted V2 pool.
    pub async fn sync_pool_by_address(
        &self,
        pool_address: Address,
        dex_type: DexType,
    ) -> Result<PoolState> {
        let pool = IV2Pair::new(pool_address, Arc::clone(&self.provider));

        // Fetch token addresses from pool contract
        let token0 = pool.token_0().call().await
            .context("V2 sync: failed to get token0")?;
        let token1 = pool.token_1().call().await
            .context("V2 sync: failed to get token1")?;

        // Fetch token decimals (critical for V2↔V3 price comparison)
        let token0_contract = IERC20Decimals::new(token0, Arc::clone(&self.provider));
        let token1_contract = IERC20Decimals::new(token1, Arc::clone(&self.provider));
        let token0_decimals = token0_contract.decimals().call().await
            .context("V2 sync: failed to get token0 decimals")?;
        let token1_decimals = token1_contract.decimals().call().await
            .context("V2 sync: failed to get token1 decimals")?;

        // Fetch reserves
        let (reserve0, reserve1, _timestamp) = pool.get_reserves().call().await
            .context("V2 sync: failed to get reserves")?;

        // Get current block
        let current_block = self.provider.get_block_number().await
            .context("V2 sync: failed to get block number")?
            .as_u64();

        let pair = TradingPair {
            token0,
            token1,
            symbol: String::new(), // Caller sets this from whitelist
        };

        debug!(
            "V2 pool synced: {:?} on {:?} — token0={:?}({}dec) token1={:?}({}dec) reserves=({}, {}) block={}",
            pool_address, dex_type, token0, token0_decimals, token1, token1_decimals,
            reserve0, reserve1, current_block
        );

        Ok(PoolState {
            address: pool_address,
            dex: dex_type,
            pair,
            reserve0: U256::from(reserve0),
            reserve1: U256::from(reserve1),
            last_updated: current_block,
            token0_decimals,
            token1_decimals,
        })
    }

    /// Parallel sync: update reserves for all known V2 pools concurrently.
    /// Only fetches getReserves() (1 RPC call per pool) — tokens/decimals
    /// are preserved from the initial sync.
    ///
    /// Returns updated pool states. On individual pool failure, preserves
    /// the previous state for that pool.
    pub async fn sync_known_pools_parallel(
        &self,
        known_pools: &[PoolState],
    ) -> Vec<PoolState> {
        use futures::future::join_all;

        let tasks: Vec<_> = known_pools.iter().map(|pool| {
            let provider = Arc::clone(&self.provider);
            let pool_address = pool.address;
            let dex = pool.dex;
            let pair = pool.pair.clone();
            let token0_decimals = pool.token0_decimals;
            let token1_decimals = pool.token1_decimals;

            async move {
                let contract = IV2Pair::new(pool_address, provider.clone());
                let reserves = contract.get_reserves().call().await;
                let block = provider.get_block_number().await;

                match (reserves, block) {
                    (Ok((r0, r1, _ts)), Ok(bn)) => {
                        Some(PoolState {
                            address: pool_address,
                            dex,
                            pair,
                            reserve0: U256::from(r0),
                            reserve1: U256::from(r1),
                            last_updated: bn.as_u64(),
                            token0_decimals,
                            token1_decimals,
                        })
                    }
                    (Err(e), _) => {
                        warn!("V2 sync failed for {:?}: reserves error: {}", pool_address, e);
                        None
                    }
                    (_, Err(e)) => {
                        warn!("V2 sync failed for {:?}: block number error: {}", pool_address, e);
                        None
                    }
                }
            }
        }).collect();

        let results = join_all(tasks).await;

        // Merge: use updated state if available, fall back to previous state
        let mut updated: Vec<PoolState> = Vec::with_capacity(known_pools.len());
        for (i, result) in results.into_iter().enumerate() {
            match result {
                Some(pool) => updated.push(pool),
                None => updated.push(known_pools[i].clone()),
            }
        }

        updated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v2_pool_state_price_adjusted() {
        // Simulate USDC(6dec)/WETH(18dec) V2 pool
        // 100 USDC = 100_000_000 raw, 0.042 WETH = 42_000_000_000_000_000 raw
        let pool = PoolState {
            address: Address::zero(),
            dex: DexType::QuickSwapV2,
            pair: TradingPair::new(Address::zero(), Address::zero(), "TEST".to_string()),
            reserve0: U256::from(100_000_000u64),        // 100 USDC (6 dec)
            reserve1: U256::from(42_000_000_000_000_000u64), // 0.042 WETH (18 dec)
            last_updated: 100,
            token0_decimals: 6,
            token1_decimals: 18,
        };

        let price = pool.price_adjusted();
        // Expected: 0.042 WETH / 100 USDC = 0.00042 WETH per USDC
        assert!((price - 0.00042).abs() < 1e-10,
            "V2 price_adjusted should equal 0.00042, got {}", price);
    }

    #[test]
    fn test_v2_pool_state_price_adjusted_stablecoin() {
        // USDT(6dec)/USDC(6dec) — should be ~1.0
        let pool = PoolState {
            address: Address::zero(),
            dex: DexType::QuickSwapV2,
            pair: TradingPair::new(Address::zero(), Address::zero(), "TEST".to_string()),
            reserve0: U256::from(500_000_000_000u64),    // 500K USDT (6 dec)
            reserve1: U256::from(501_000_000_000u64),    // 501K USDC (6 dec)
            last_updated: 100,
            token0_decimals: 6,
            token1_decimals: 6,
        };

        let price = pool.price_adjusted();
        // Expected: 501000/500000 = 1.002
        assert!((price - 1.002).abs() < 1e-6,
            "Stablecoin V2 price should be ~1.002, got {}", price);
    }

    #[test]
    fn test_v2_pool_state_zero_reserves() {
        let pool = PoolState {
            address: Address::zero(),
            dex: DexType::QuickSwapV2,
            pair: TradingPair::new(Address::zero(), Address::zero(), "TEST".to_string()),
            reserve0: U256::zero(),
            reserve1: U256::from(1000u64),
            last_updated: 100,
            token0_decimals: 6,
            token1_decimals: 18,
        };

        assert_eq!(pool.price_adjusted(), 0.0, "Zero reserve0 should return 0 price");
    }
}
