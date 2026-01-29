//! Opportunity Detector (V2 + V3)
//!
//! Scans pool states to detect arbitrage opportunities between DEXs.
//! Supports both V2 (constant product) and V3 (concentrated liquidity) pools.
//! Key V3 arbitrage: 0.05% ↔ 1.00% fee tiers have ~1.05% round-trip but 2%+ spreads.
//!
//! Author: AI-Generated
//! Created: 2026-01-27
//! Modified: 2026-01-29 - Added V3 pool support

use crate::pool::{PoolStateManager, PriceCalculator};
use crate::types::{ArbitrageOpportunity, BotConfig, DexType, PoolState, TradingPair, V3PoolState};
use ethers::types::{Address, U256};
use tracing::{debug, info};

/// Minimum spread percentage to consider (covers fees)
const MIN_SPREAD_PERCENT: f64 = 0.3;

/// Estimated gas cost in USD for two swaps on Polygon
const ESTIMATED_GAS_COST_USD: f64 = 0.50;

/// V2 DEX fee percentage (Quickswap, Sushiswap, Apeswap)
const V2_FEE_PERCENT: f64 = 0.30;

/// Unified pool representation for comparing V2 and V3
#[derive(Debug, Clone)]
struct UnifiedPool {
    dex: DexType,
    price: f64,
    fee_percent: f64,  // Single swap fee
    address: Address,
    pair: TradingPair,
}

/// Opportunity detector for cross-DEX arbitrage
pub struct OpportunityDetector {
    config: BotConfig,
    state_manager: PoolStateManager,
}

impl OpportunityDetector {
    /// Create a new OpportunityDetector
    pub fn new(config: BotConfig, state_manager: PoolStateManager) -> Self {
        Self {
            config,
            state_manager,
        }
    }

    /// Scan all configured pairs for arbitrage opportunities (V2 + V3)
    /// Returns opportunities sorted by estimated profit (highest first)
    pub fn scan_opportunities(&self) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();

        for pair_config in &self.config.pairs {
            // Check V2-only opportunities (legacy)
            if let Some(opp) = self.check_pair(&pair_config.symbol) {
                opportunities.push(opp);
            }

            // Check V3 opportunities (including V3↔V3 and V3↔V2)
            if let Some(opp) = self.check_pair_unified(&pair_config.symbol) {
                // Only add if better than existing for same pair
                let dominated = opportunities.iter().any(|existing| {
                    existing.pair.symbol == pair_config.symbol
                        && existing.estimated_profit >= opp.estimated_profit
                });
                if !dominated {
                    // Remove dominated existing opportunity for same pair
                    opportunities.retain(|existing| {
                        existing.pair.symbol != pair_config.symbol
                            || existing.estimated_profit > opp.estimated_profit
                    });
                    opportunities.push(opp);
                }
            }
        }

        // Sort by estimated profit descending
        opportunities.sort_by(|a, b| {
            b.estimated_profit
                .partial_cmp(&a.estimated_profit)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        opportunities
    }

    /// Check a pair using V3-only pool comparison
    /// This is the key method for V3 fee tier arbitrage (0.05% ↔ 0.30% ↔ 1.00%)
    /// NOTE: V2 pools excluded due to price calculation differences
    fn check_pair_unified(&self, pair_symbol: &str) -> Option<ArbitrageOpportunity> {
        // Collect V3 pools only (V2 excluded - price format incompatible)
        let mut unified_pools: Vec<UnifiedPool> = Vec::new();

        // Add V3 pools only
        for pool in self.state_manager.get_v3_pools_for_pair(pair_symbol) {
            let price = pool.price();
            if price > 0.0 && price < 1e15 {  // Sanity check
                let fee_percent = pool.fee as f64 / 10000.0;  // 500 -> 0.05%
                unified_pools.push(UnifiedPool {
                    dex: pool.dex,
                    price,
                    fee_percent,
                    address: pool.address,
                    pair: pool.pair.clone(),
                });
            }
        }

        if unified_pools.len() < 2 {
            return None;
        }

        // Find best opportunity by comparing all pairs
        let mut best_opportunity: Option<ArbitrageOpportunity> = None;
        let mut best_profit: f64 = 0.0;

        for i in 0..unified_pools.len() {
            for j in (i + 1)..unified_pools.len() {
                let pool_a = &unified_pools[i];
                let pool_b = &unified_pools[j];

                // Determine buy (lower price) and sell (higher price)
                let (buy_pool, sell_pool) = if pool_b.price > pool_a.price {
                    (pool_a, pool_b)
                } else {
                    (pool_b, pool_a)
                };

                // Calculate midmarket spread (before fees)
                let midmarket_spread = (sell_pool.price - buy_pool.price) / buy_pool.price;

                // Calculate round-trip fee
                let round_trip_fee = (buy_pool.fee_percent + sell_pool.fee_percent) / 100.0;

                // Calculate executable spread (after fees)
                let executable_spread = midmarket_spread - round_trip_fee;

                if executable_spread <= 0.0 {
                    continue;
                }

                // Estimate profit
                let gross = executable_spread * self.config.max_trade_size_usd;
                let slippage_estimate = gross * 0.10;  // 10% slippage estimate
                let net_profit = gross - ESTIMATED_GAS_COST_USD - slippage_estimate;

                if net_profit < self.config.min_profit_usd {
                    continue;
                }

                // Check if this is the best opportunity
                if net_profit > best_profit {
                    best_profit = net_profit;

                    info!(
                        "🎯 V3 OPPORTUNITY: {} | Buy {:?} ({:.2}%) @ {:.6} | Sell {:?} ({:.2}%) @ {:.6} | Spread {:.2}% | Net ${:.2}",
                        pair_symbol,
                        buy_pool.dex, buy_pool.fee_percent,
                        buy_pool.price,
                        sell_pool.dex, sell_pool.fee_percent,
                        sell_pool.price,
                        executable_spread * 100.0,
                        net_profit
                    );

                    best_opportunity = Some(ArbitrageOpportunity {
                        pair: buy_pool.pair.clone(),
                        buy_dex: buy_pool.dex,
                        sell_dex: sell_pool.dex,
                        buy_price: buy_pool.price,
                        sell_price: sell_pool.price,
                        spread_percent: executable_spread * 100.0,
                        estimated_profit: net_profit,
                        trade_size: U256::from((self.config.max_trade_size_usd * 1e6) as u64),  // USDC has 6 decimals
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        buy_pool_address: Some(buy_pool.address),
                        sell_pool_address: Some(sell_pool.address),
                    });
                }
            }
        }

        best_opportunity
    }

    /// Check a specific pair for arbitrage opportunity
    /// Returns Some(opportunity) if profitable, None otherwise
    pub fn check_pair(&self, pair_symbol: &str) -> Option<ArbitrageOpportunity> {
        let pools = self.state_manager.get_pools_for_pair(pair_symbol);

        if pools.len() < 2 {
            debug!("Pair {} has < 2 pools, skipping", pair_symbol);
            return None;
        }

        // Find best buy (lowest price) and best sell (highest price)
        let (buy_pool, sell_pool) = self.find_best_pools(&pools)?;

        // Calculate spread
        let spread_percent = self.calculate_spread(buy_pool.price(), sell_pool.price());

        // Early filter: spread must cover DEX fees
        if spread_percent < MIN_SPREAD_PERCENT {
            debug!(
                "{}: spread {:.4}% < {:.1}% minimum",
                pair_symbol, spread_percent, MIN_SPREAD_PERCENT
            );
            return None;
        }

        debug!(
            "{}: Found spread {:.4}% - Buy on {:?} @ {:.6}, Sell on {:?} @ {:.6}",
            pair_symbol,
            spread_percent,
            buy_pool.dex,
            buy_pool.price(),
            sell_pool.dex,
            sell_pool.price()
        );

        // Calculate optimal trade size and actual profit
        let (trade_size, profit_usd) =
            self.calculate_profit(&buy_pool, &sell_pool, pair_symbol)?;

        // Net profit after gas
        let net_profit_usd = profit_usd - ESTIMATED_GAS_COST_USD;

        // Filter by minimum profit threshold
        if net_profit_usd < self.config.min_profit_usd {
            debug!(
                "{}: net profit ${:.2} < ${:.2} minimum",
                pair_symbol, net_profit_usd, self.config.min_profit_usd
            );
            return None;
        }

        info!(
            "🎯 OPPORTUNITY: {} | Buy {:?} @ {:.6} | Sell {:?} @ {:.6} | Spread {:.2}% | Profit ${:.2}",
            pair_symbol,
            buy_pool.dex,
            buy_pool.price(),
            sell_pool.dex,
            sell_pool.price(),
            spread_percent,
            net_profit_usd
        );

        Some(ArbitrageOpportunity {
            pair: buy_pool.pair.clone(),
            buy_dex: buy_pool.dex,
            sell_dex: sell_pool.dex,
            buy_price: buy_pool.price(),
            sell_price: sell_pool.price(),
            spread_percent,
            estimated_profit: net_profit_usd,
            trade_size,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            buy_pool_address: Some(buy_pool.address),
            sell_pool_address: Some(sell_pool.address),
        })
    }

    /// Find the best buy pool (lowest price) and sell pool (highest price)
    fn find_best_pools<'a>(&self, pools: &'a [PoolState]) -> Option<(&'a PoolState, &'a PoolState)> {
        let valid_pools: Vec<&PoolState> = pools
            .iter()
            .filter(|p| p.price() > 0.0 && !p.reserve0.is_zero() && !p.reserve1.is_zero())
            .collect();

        if valid_pools.len() < 2 {
            return None;
        }

        let buy_pool = valid_pools
            .iter()
            .min_by(|a, b| a.price().partial_cmp(&b.price()).unwrap())?;

        let sell_pool = valid_pools
            .iter()
            .max_by(|a, b| a.price().partial_cmp(&b.price()).unwrap())?;

        // Must be different DEXs
        if buy_pool.dex == sell_pool.dex {
            return None;
        }

        Some((buy_pool, sell_pool))
    }

    /// Calculate spread percentage between buy and sell prices
    fn calculate_spread(&self, buy_price: f64, sell_price: f64) -> f64 {
        if buy_price == 0.0 {
            return 0.0;
        }
        ((sell_price - buy_price) / buy_price) * 100.0
    }

    /// Calculate optimal trade size and expected profit in USD
    fn calculate_profit(
        &self,
        buy_pool: &PoolState,
        sell_pool: &PoolState,
        pair_symbol: &str,
    ) -> Option<(U256, f64)> {
        // Get token addresses
        let token_in = buy_pool.pair.token0;

        // Calculate optimal trade size (1% of smaller pool's liquidity)
        let trade_size = PriceCalculator::optimal_trade_size(buy_pool, sell_pool, token_in);

        // Enforce max trade size from config
        let max_trade_size = self.max_trade_size_wei(pair_symbol);
        let trade_size = std::cmp::min(trade_size, max_trade_size);

        if trade_size.is_zero() {
            return None;
        }

        // Simulate the arbitrage
        let (amount_out, profit_wei) =
            PriceCalculator::simulate_arbitrage(buy_pool, sell_pool, trade_size, token_in);

        if profit_wei.is_zero() {
            return None;
        }

        // Convert profit to USD
        let profit_usd = self.wei_to_usd(profit_wei, pair_symbol);

        debug!(
            "{}: trade_size={}, amount_out={}, profit_wei={}, profit_usd=${:.2}",
            pair_symbol, trade_size, amount_out, profit_wei, profit_usd
        );

        Some((trade_size, profit_usd))
    }

    /// Convert trade size to Wei based on pair
    fn max_trade_size_wei(&self, pair_symbol: &str) -> U256 {
        // Max trade size in USD from config
        let max_usd = self.config.max_trade_size_usd;

        // Rough conversion based on pair (can be improved with price oracle)
        if pair_symbol.starts_with("WETH") {
            // Assume ETH ~$3300
            let eth_amount = max_usd / 3300.0;
            U256::from((eth_amount * 1e18) as u128)
        } else if pair_symbol.starts_with("WMATIC") {
            // Assume MATIC ~$0.50
            let matic_amount = max_usd / 0.50;
            U256::from((matic_amount * 1e18) as u128)
        } else {
            // Default: assume 18 decimals, $1 = 1 token
            U256::from((max_usd * 1e18) as u128)
        }
    }

    /// Convert Wei profit to USD based on pair
    fn wei_to_usd(&self, wei: U256, pair_symbol: &str) -> f64 {
        let wei_f = wei.as_u128() as f64;

        // Convert based on token (can be improved with price oracle)
        if pair_symbol.starts_with("WETH") {
            // ETH: 18 decimals, ~$3300
            (wei_f / 1e18) * 3300.0
        } else if pair_symbol.starts_with("WMATIC") {
            // MATIC: 18 decimals, ~$0.50
            (wei_f / 1e18) * 0.50
        } else {
            // Default: assume 18 decimals, $1 = 1 token
            wei_f / 1e18
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DexType, TradingPair};
    use ethers::types::Address;

    fn create_test_pool(
        dex: DexType,
        symbol: &str,
        reserve0: u128,
        reserve1: u128,
    ) -> PoolState {
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
    fn test_calculate_spread() {
        let config = BotConfig {
            rpc_url: String::new(),
            chain_id: 137,
            private_key: String::new(),
            min_profit_usd: 5.0,
            max_trade_size_usd: 500.0,
            max_slippage_percent: 0.5,
            uniswap_router: Address::zero(),
            sushiswap_router: Address::zero(),
            uniswap_factory: Address::zero(),
            sushiswap_factory: Address::zero(),
            pairs: vec![],
            poll_interval_ms: 1000,
            max_gas_price_gwei: 100,
        };

        let state_manager = PoolStateManager::new();
        let detector = OpportunityDetector::new(config, state_manager);

        // 1% spread
        let spread = detector.calculate_spread(100.0, 101.0);
        assert!((spread - 1.0).abs() < 0.001);

        // 0.5% spread
        let spread = detector.calculate_spread(200.0, 201.0);
        assert!((spread - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_find_best_pools() {
        let config = BotConfig {
            rpc_url: String::new(),
            chain_id: 137,
            private_key: String::new(),
            min_profit_usd: 5.0,
            max_trade_size_usd: 500.0,
            max_slippage_percent: 0.5,
            uniswap_router: Address::zero(),
            sushiswap_router: Address::zero(),
            uniswap_factory: Address::zero(),
            sushiswap_factory: Address::zero(),
            pairs: vec![],
            poll_interval_ms: 1000,
            max_gas_price_gwei: 100,
        };

        let state_manager = PoolStateManager::new();
        let detector = OpportunityDetector::new(config, state_manager);

        // Pool A: price = 2000/1000 = 2.0
        // Pool B: price = 2100/1000 = 2.1 (5% higher)
        let pools = vec![
            create_test_pool(DexType::Uniswap, "ETH/USDC", 1000, 2000),
            create_test_pool(DexType::Sushiswap, "ETH/USDC", 1000, 2100),
        ];

        let result = detector.find_best_pools(&pools);
        assert!(result.is_some());

        let (buy, sell) = result.unwrap();
        assert_eq!(buy.dex, DexType::Uniswap); // Lower price = buy here
        assert_eq!(sell.dex, DexType::Sushiswap); // Higher price = sell here
    }
}
