//! Price Calculator
//!
//! Utilities for calculating prices, slippage, and trade amounts
//! from pool reserves using constant product formula (x * y = k).
//!
//! Author: AI-Generated
//! Created: 2026-01-27

use crate::pool::PoolStateManager;
use crate::types::{DexType, PoolState};
use ethers::types::{Address, U256};
use tracing::debug;

/// Price calculator for DEX pools
pub struct PriceCalculator {
    state_manager: PoolStateManager,
}

impl PriceCalculator {
    /// Create a new PriceCalculator
    pub fn new(state_manager: PoolStateManager) -> Self {
        Self { state_manager }
    }

    /// Calculate the spot price of token0 in terms of token1 for a pool
    pub fn spot_price(pool: &PoolState) -> f64 {
        pool.price()
    }

    /// Calculate amount out for a given input using constant product formula
    /// Includes 0.3% fee (997/1000)
    ///
    /// Formula: amount_out = (amount_in * 997 * reserve_out) / (reserve_in * 1000 + amount_in * 997)
    pub fn get_amount_out(
        amount_in: U256,
        reserve_in: U256,
        reserve_out: U256,
    ) -> U256 {
        if amount_in.is_zero() || reserve_in.is_zero() || reserve_out.is_zero() {
            return U256::zero();
        }

        let amount_in_with_fee = amount_in * U256::from(997);
        let numerator = amount_in_with_fee * reserve_out;
        let denominator = (reserve_in * U256::from(1000)) + amount_in_with_fee;

        numerator / denominator
    }

    /// Calculate amount in required to get a specific output
    /// Inverse of get_amount_out
    ///
    /// Formula: amount_in = (reserve_in * amount_out * 1000) / ((reserve_out - amount_out) * 997) + 1
    pub fn get_amount_in(
        amount_out: U256,
        reserve_in: U256,
        reserve_out: U256,
    ) -> U256 {
        if amount_out.is_zero() || reserve_in.is_zero() || reserve_out.is_zero() {
            return U256::zero();
        }

        if amount_out >= reserve_out {
            return U256::MAX; // Not enough liquidity
        }

        let numerator = reserve_in * amount_out * U256::from(1000);
        let denominator = (reserve_out - amount_out) * U256::from(997);

        (numerator / denominator) + U256::from(1)
    }

    /// Calculate price impact of a trade (as percentage)
    pub fn price_impact(
        amount_in: U256,
        reserve_in: U256,
        reserve_out: U256,
    ) -> f64 {
        if reserve_in.is_zero() || reserve_out.is_zero() {
            return 100.0;
        }

        // Spot price before trade
        let spot_price = reserve_out.low_u128() as f64 / reserve_in.low_u128() as f64;

        // Actual execution price with trade
        let amount_out = Self::get_amount_out(amount_in, reserve_in, reserve_out);
        if amount_out.is_zero() {
            return 100.0;
        }

        let execution_price = amount_out.low_u128() as f64 / amount_in.low_u128() as f64;

        // Price impact as percentage
        ((spot_price - execution_price) / spot_price) * 100.0
    }

    /// Calculate the optimal trade size that maximizes profit
    /// Given two pools with different prices
    pub fn optimal_trade_size(
        pool_buy: &PoolState,
        pool_sell: &PoolState,
        token_in: Address,
    ) -> U256 {
        // Simplified: use 10% of smaller pool's reserve as max
        let buy_reserve = if token_in == pool_buy.pair.token0 {
            pool_buy.reserve0
        } else {
            pool_buy.reserve1
        };

        let sell_reserve = if token_in == pool_sell.pair.token0 {
            pool_sell.reserve0
        } else {
            pool_sell.reserve1
        };

        let smaller_reserve = std::cmp::min(buy_reserve, sell_reserve);

        // Start with 1% of liquidity as a safe trade size
        smaller_reserve / U256::from(100)
    }

    /// Simulate a complete arbitrage trade path
    /// Returns (amount_out, net_profit_in_token0)
    pub fn simulate_arbitrage(
        pool_buy: &PoolState,
        pool_sell: &PoolState,
        amount_in: U256,
        token_in: Address,
    ) -> (U256, U256) {
        // Step 1: Buy on cheaper DEX (token_in -> intermediate)
        let (buy_reserve_in, buy_reserve_out) = if token_in == pool_buy.pair.token0 {
            (pool_buy.reserve0, pool_buy.reserve1)
        } else {
            (pool_buy.reserve1, pool_buy.reserve0)
        };

        let amount_mid = Self::get_amount_out(amount_in, buy_reserve_in, buy_reserve_out);

        // Step 2: Sell on expensive DEX (intermediate -> token_in)
        let (sell_reserve_in, sell_reserve_out) = if token_in == pool_sell.pair.token0 {
            (pool_sell.reserve1, pool_sell.reserve0)
        } else {
            (pool_sell.reserve0, pool_sell.reserve1)
        };

        let amount_out = Self::get_amount_out(amount_mid, sell_reserve_in, sell_reserve_out);

        // Profit = amount_out - amount_in
        let profit = if amount_out > amount_in {
            amount_out - amount_in
        } else {
            U256::zero()
        };

        debug!(
            "Arbitrage simulation: in={}, mid={}, out={}, profit={}",
            amount_in, amount_mid, amount_out, profit
        );

        (amount_out, profit)
    }

    /// Get the best price across all DEXs for a pair
    pub fn best_price_for_pair(&self, pair_symbol: &str, is_buy: bool) -> Option<(DexType, f64)> {
        let pools = self.state_manager.get_pools_for_pair(pair_symbol);

        if pools.is_empty() {
            return None;
        }

        if is_buy {
            // Looking for lowest price to buy
            pools
                .iter()
                .filter(|p| p.price() > 0.0)
                .min_by(|a, b| a.price().partial_cmp(&b.price()).unwrap())
                .map(|p| (p.dex, p.price()))
        } else {
            // Looking for highest price to sell
            pools
                .iter()
                .filter(|p| p.price() > 0.0)
                .max_by(|a, b| a.price().partial_cmp(&b.price()).unwrap())
                .map(|p| (p.dex, p.price()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_amount_out() {
        // Test with typical reserves
        let amount_in = U256::from(1_000_000_000_000_000_000u64); // 1 ETH
        let reserve_in = U256::from(100_000_000_000_000_000_000u128); // 100 ETH
        let reserve_out = U256::from(200_000_000_000u64); // 200,000 USDC (6 decimals)

        let amount_out = PriceCalculator::get_amount_out(amount_in, reserve_in, reserve_out);

        // Should get approximately 1976 USDC (with fee and slippage)
        assert!(amount_out > U256::from(1_970_000_000u64));
        assert!(amount_out < U256::from(2_000_000_000u64));
    }

    #[test]
    fn test_get_amount_out_zero_inputs() {
        assert_eq!(
            PriceCalculator::get_amount_out(U256::zero(), U256::from(100), U256::from(100)),
            U256::zero()
        );
        assert_eq!(
            PriceCalculator::get_amount_out(U256::from(100), U256::zero(), U256::from(100)),
            U256::zero()
        );
        assert_eq!(
            PriceCalculator::get_amount_out(U256::from(100), U256::from(100), U256::zero()),
            U256::zero()
        );
    }

    #[test]
    fn test_price_impact() {
        let amount_in = U256::from(10_000_000_000_000_000_000u128); // 10 ETH
        let reserve_in = U256::from(100_000_000_000_000_000_000u128); // 100 ETH
        let reserve_out = U256::from(200_000_000_000u64); // 200,000 USDC

        let impact = PriceCalculator::price_impact(amount_in, reserve_in, reserve_out);

        // 10% of pool should have meaningful price impact
        assert!(impact > 5.0);
        assert!(impact < 15.0);
    }

    #[test]
    fn test_get_amount_in() {
        let amount_out = U256::from(1_000_000_000u64); // 1000 USDC
        let reserve_in = U256::from(100_000_000_000_000_000_000u128); // 100 ETH
        let reserve_out = U256::from(200_000_000_000u64); // 200,000 USDC

        let amount_in = PriceCalculator::get_amount_in(amount_out, reserve_in, reserve_out);

        // Verify by using get_amount_out (should be close to amount_out)
        let verified_out =
            PriceCalculator::get_amount_out(amount_in, reserve_in, reserve_out);
        assert!(verified_out >= amount_out);
    }
}
