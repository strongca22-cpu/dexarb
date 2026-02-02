//! Opportunity Detector (V3-only)
//!
//! Scans V3 pool states to detect arbitrage opportunities across fee tiers.
//! Key V3 arbitrage: 0.05% ↔ 0.30% fee tiers (0.35% round-trip fee).
//! 1% fee tier excluded (phantom liquidity on Polygon).
//! V2 detection retained but not called from scan_opportunities (V2 sync dropped).
//!
//! Phase 1.1: Whitelist/blacklist filtering — pools must be whitelisted to participate.
//!
//! Author: AI-Generated
//! Created: 2026-01-27
//! Modified: 2026-01-29 - Added V3 pool support
//! Modified: 2026-01-29 - V3-only: drop V2 from scan, exclude 1% fee tier
//! Modified: 2026-01-29 - Phase 1.1: whitelist/blacklist filtering
//! Modified: 2026-01-31 - Multi-chain: quote token + gas cost from config instead of constants

use crate::filters::WhitelistFilter;
use crate::pool::{PoolStateManager, PriceCalculator};
use crate::types::{ArbitrageOpportunity, BotConfig, DexType, PoolState, TradingPair};
use alloy::primitives::{Address, U256};
use tracing::{debug, info, warn};

/// Minimum spread percentage to consider (covers fees)
const MIN_SPREAD_PERCENT: f64 = 0.3;

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
    token0_decimals: u8,
    token1_decimals: u8,
    liquidity: u128,
    /// The actual quote token address this pool uses (USDC.e or native USDC).
    /// Pools with different quote tokens must NOT be compared for arb —
    /// ArbExecutor.sol starts and ends with the same token.
    quote_token: Address,
    /// Per-pool USD trade size cap (from whitelist or global default).
    /// Adaptive sizing: min(buy_pool, sell_pool) determines effective trade size.
    max_trade_size_usd: f64,
}

/// Opportunity detector for cross-DEX arbitrage
pub struct OpportunityDetector {
    config: BotConfig,
    state_manager: PoolStateManager,
    whitelist: WhitelistFilter,
}

impl OpportunityDetector {
    /// Create a new OpportunityDetector.
    /// Loads the whitelist from `config.whitelist_file` if set, otherwise uses defaults.
    pub fn new(config: BotConfig, state_manager: PoolStateManager) -> Self {
        let whitelist = match &config.whitelist_file {
            Some(path) => match WhitelistFilter::load(path) {
                Ok(wl) => wl,
                Err(e) => {
                    warn!("Failed to load whitelist from {}: {}. Using permissive defaults.", path, e);
                    WhitelistFilter::default()
                }
            },
            None => {
                info!("No WHITELIST_FILE configured, using permissive defaults (advisory mode)");
                WhitelistFilter::default()
            }
        };

        Self {
            config,
            state_manager,
            whitelist,
        }
    }

    /// Scan all configured pairs for V3 arbitrage opportunities
    /// Returns opportunities sorted by estimated profit (highest first)
    /// V2 pools dropped (price inversion bug, not synced). V3 0.05%↔0.30% only.
    pub fn scan_opportunities(&self) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();

        for pair_config in &self.config.pairs {
            // Check V3 opportunities (all profitable fee tier combinations)
            // Returns multiple per pair so executor can fall through Quoter rejections
            let v3_opps = self.check_pair_unified(&pair_config.symbol);
            opportunities.extend(v3_opps);
        }

        // Sort by estimated profit descending
        opportunities.sort_by(|a, b| {
            b.estimated_profit
                .partial_cmp(&a.estimated_profit)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        opportunities
    }

    /// Check a pair using V3-only pool comparison.
    /// Returns ALL profitable fee tier combinations (not just the best) so the
    /// executor can fall through Quoter-rejected thin pools to viable ones.
    ///
    /// This is the key method for V3 fee tier arbitrage (0.05% ↔ 0.30% ↔ 1.00%)
    /// NOTE: V2 pools excluded due to price calculation differences
    ///
    /// V3 Price Direction (CRITICAL):
    ///   V3 pools sort token0 < token1 by address. price = token1/token0 (decimal-adjusted).
    ///   Example: UNI/USDC pair → V3 token0=USDC(0x2791), token1=UNI(0xb33E)
    ///   price = UNI per USDC (e.g., 0.2056 means 1 USDC buys 0.2056 UNI)
    ///
    ///   Higher price = more token1 per token0 = BUY token1 here (token0→token1)
    ///   Lower price = fewer token1 per token0 = SELL token1 here (token1→token0)
    ///     (because 1/lower_price = more token0 per token1 = better exit)
    ///
    /// Execute flow: token0 → token1 on buy_pool, then token1 → token0 on sell_pool.
    fn check_pair_unified(&self, pair_symbol: &str) -> Vec<ArbitrageOpportunity> {
        // Collect ALL pools (V3 + V2) into unified format for cross-protocol comparison
        let mut unified_pools: Vec<UnifiedPool> = Vec::new();

        // Add V3 pools with whitelist + liquidity filtering (Phase 1.1)
        for pool in self.state_manager.get_v3_pools_for_pair(pair_symbol) {
            let price = pool.price();
            if price <= 0.0 || price >= 1e15 {
                continue; // Sanity check
            }

            // Phase 1.1: Whitelist/blacklist check (covers fee tier blacklist,
            // pool blacklist, pair blacklist, and strict whitelist enforcement).
            // This supersedes the old `fee >= 10000` check — the 1% tier is
            // blacklisted in the whitelist config.
            if !self.whitelist.is_pool_allowed(&pool.address, pool.fee, pair_symbol) {
                continue;
            }

            let fee_percent = pool.fee as f64 / 10000.0;  // 500 -> 0.05%

            // Phase 1.1: Per-pool / per-tier minimum liquidity
            // Replaces the old flat `< 1000` check with tier-aware thresholds.
            let min_liq = self.whitelist.min_liquidity_for(&pool.address, pool.fee);
            if pool.liquidity < min_liq {
                debug!(
                    "Skipping {} {:?} - liquidity {} below threshold {} (fee tier {})",
                    pair_symbol, pool.dex, pool.liquidity, min_liq, pool.fee
                );
                continue;
            }

            // Determine which quote token this pool uses (USDC.e or native USDC).
            // V3 pools sort token0 < token1 by address. The quote token is whichever
            // of token0/token1 matches a recognized quote token address.
            let qt = if self.config.is_quote_token(&pool.pair.token0) {
                pool.pair.token0
            } else if self.config.is_quote_token(&pool.pair.token1) {
                pool.pair.token1
            } else {
                continue; // Neither token is a known quote token — skip
            };

            // Per-pool trade size cap (adaptive sizing)
            let pool_max = self.whitelist.max_trade_size_for(&pool.address)
                .unwrap_or(self.config.max_trade_size_usd);

            unified_pools.push(UnifiedPool {
                dex: pool.dex,
                price,
                fee_percent,
                address: pool.address,
                pair: pool.pair.clone(),
                token0_decimals: pool.token0_decimals,
                token1_decimals: pool.token1_decimals,
                liquidity: pool.liquidity,
                quote_token: qt,
                max_trade_size_usd: pool_max,
            });
        }

        // Add V2 pools (V2↔V3 cross-protocol arbitrage).
        // V2 price uses decimal-adjusted reserves → same format as V3 tick-based price.
        // V2 fee is always 0.30% (hardcoded in constant-product formula).
        for pool in self.state_manager.get_pools_for_pair(pair_symbol) {
            // Only include new V2 DEX types (QuickSwapV2, SushiSwapV2)
            // Legacy Uniswap/Sushiswap/Quickswap/Apeswap variants are not V2↔V3 aware
            if !matches!(pool.dex, DexType::QuickSwapV2 | DexType::SushiSwapV2) {
                continue;
            }

            // Whitelist check (V2 pools use fee_tier=3000 in whitelist JSON)
            if !self.whitelist.is_pool_allowed(&pool.address, 3000, pair_symbol) {
                continue;
            }

            // Decimal-adjusted price — same format as V3 tick-based price.
            // CRITICAL: Uses price_adjusted(), NOT price() (raw ratio without decimals).
            // This is what makes V2↔V3 cross-protocol comparison correct.
            let price = pool.price_adjusted();
            if price <= 0.0 || price >= 1e15 {
                continue; // Sanity check
            }

            // V2 "liquidity" proxy: smaller raw reserve value.
            // V2 pools are pre-verified by verify_v2_pools.py (whitelist v2_ready),
            // so this is a rough floor check only.
            let liquidity = std::cmp::min(
                pool.reserve0.to::<u128>(),
                pool.reserve1.to::<u128>(),
            );

            // Determine quote token for this V2 pool
            let qt = if self.config.is_quote_token(&pool.pair.token0) {
                pool.pair.token0
            } else if self.config.is_quote_token(&pool.pair.token1) {
                pool.pair.token1
            } else {
                continue; // Neither token is a known quote token — skip
            };

            // Per-pool trade size cap (adaptive sizing)
            let pool_max = self.whitelist.max_trade_size_for(&pool.address)
                .unwrap_or(self.config.max_trade_size_usd);

            unified_pools.push(UnifiedPool {
                dex: pool.dex,
                price,
                fee_percent: V2_FEE_PERCENT, // 0.30 = 0.30% (same format as V3: 500bps → 0.05)
                address: pool.address,
                pair: pool.pair.clone(),
                token0_decimals: pool.token0_decimals,
                token1_decimals: pool.token1_decimals,
                liquidity,
                quote_token: qt,
                max_trade_size_usd: pool_max,
            });
        }

        if unified_pools.len() < 2 {
            return Vec::new();
        }

        // Find ALL profitable combinations (not just the best)
        // The executor will try them in profit order and skip Quoter-rejected ones
        let mut results: Vec<ArbitrageOpportunity> = Vec::new();

        for i in 0..unified_pools.len() {
            for j in (i + 1)..unified_pools.len() {
                let pool_a = &unified_pools[i];
                let pool_b = &unified_pools[j];

                // CRITICAL: Only compare pools that use the SAME quote token variant.
                // ArbExecutor.sol starts and ends with the same token — can't mix
                // USDC.e and native USDC in a single atomic arb.
                if pool_a.quote_token != pool_b.quote_token {
                    continue;
                }

                // Determine if the quote token is V3 token0 for this pool pair.
                // Both pools share the same quote token, so checking pool_a suffices.
                let quote_is_token0 = pool_a.pair.token0 == pool_a.quote_token;

                // V3 price = token1/token0
                //
                // If quote=token0 (USDC=token0, e.g., WETH/USDC):
                //   Higher price = more base per quote = cheaper base → BUY here
                //   Lower price  = less base per quote = expensive base → SELL here
                //
                // If quote=token1 (USDC=token1, e.g., WBTC/USDC, WMATIC/USDC):
                //   Lower price  = less quote per base = cheaper base → BUY here
                //   Higher price = more quote per base = expensive base → SELL here
                let (buy_pool, sell_pool) = if quote_is_token0 {
                    // Current behavior: buy where price is higher
                    if pool_a.price > pool_b.price {
                        (pool_a, pool_b)
                    } else {
                        (pool_b, pool_a)
                    }
                } else {
                    // Reversed: buy where price is lower (cheaper base in quote terms)
                    if pool_a.price < pool_b.price {
                        (pool_a, pool_b)
                    } else {
                        (pool_b, pool_a)
                    }
                };

                // Calculate midmarket spread (before fees)
                // Always positive: distance between buy and sell prices
                let midmarket_spread = if quote_is_token0 {
                    // buy has higher price
                    (buy_pool.price - sell_pool.price) / sell_pool.price
                } else {
                    // sell has higher price
                    (sell_pool.price - buy_pool.price) / buy_pool.price
                };

                // Calculate round-trip fee
                let round_trip_fee = (buy_pool.fee_percent + sell_pool.fee_percent) / 100.0;

                // Calculate executable spread (after fees)
                let executable_spread = midmarket_spread - round_trip_fee;

                if executable_spread <= 0.0 {
                    continue;
                }

                // Per-pool adaptive trade sizing: cap to the thinner pool's safe limit
                let effective_trade_size = buy_pool.max_trade_size_usd
                    .min(sell_pool.max_trade_size_usd);

                // Estimate profit using effective (per-pool-capped) trade size
                let gross = executable_spread * effective_trade_size;
                let slippage_estimate = gross * 0.01;  // 1% slippage estimate (V3 concentrated liquidity has <0.01% at $140-500)
                let net_profit = gross - self.config.estimated_gas_cost_usd - slippage_estimate;

                // Scale min_profit proportionally to trade size, with floor at 2x gas cost.
                // $5K trade @ $5 min_profit → $200 trade needs only $0.20 min_profit.
                // Floor ensures we never accept trades that barely cover gas.
                let scale_factor = effective_trade_size / self.config.max_trade_size_usd;
                let scaled_min_profit = (self.config.min_profit_usd * scale_factor)
                    .max(self.config.estimated_gas_cost_usd * 2.0);

                if net_profit < scaled_min_profit {
                    continue;
                }

                // Liquidity safety check: proportional to effective trade size (not global max).
                // Ensures both pools can absorb the capped trade size.
                // V3 liquidity is in sqrt(token0 * token1) units — this is a rough floor.
                let min_liquidity = (effective_trade_size * 1e6) as u128;
                if buy_pool.liquidity < min_liquidity || sell_pool.liquidity < min_liquidity {
                    debug!(
                        "Skipping {} {:?}<->{:?} - pool liquidity too low for ${:.0} trade: buy_liq={}, sell_liq={}",
                        pair_symbol, buy_pool.dex, sell_pool.dex,
                        effective_trade_size, buy_pool.liquidity, sell_pool.liquidity
                    );
                    continue;
                }

                info!(
                    "🎯 OPPORTUNITY: {} | Buy {:?} ({:.2}%) @ {:.6} | Sell {:?} ({:.2}%) @ {:.6} | Spread {:.2}% | Net ${:.2} | Size ${:.0}",
                    pair_symbol,
                    buy_pool.dex, buy_pool.fee_percent,
                    buy_pool.price,
                    sell_pool.dex, sell_pool.fee_percent,
                    sell_pool.price,
                    executable_spread * 100.0,
                    net_profit,
                    effective_trade_size,
                );

                results.push(ArbitrageOpportunity {
                    pair: buy_pool.pair.clone(),
                    buy_dex: buy_pool.dex,
                    sell_dex: sell_pool.dex,
                    buy_price: buy_pool.price,
                    sell_price: sell_pool.price,
                    spread_percent: executable_spread * 100.0,
                    estimated_profit: net_profit,
                    trade_size: U256::from((effective_trade_size * 1e6) as u64),  // USDC has 6 decimals
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    buy_pool_address: Some(buy_pool.address),
                    sell_pool_address: Some(sell_pool.address),
                    token0_decimals: buy_pool.token0_decimals,
                    token1_decimals: buy_pool.token1_decimals,
                    buy_pool_liquidity: Some(buy_pool.liquidity),
                    quote_token_is_token0: quote_is_token0,
                    min_profit_usd: scaled_min_profit,
                });
            }
        }

        results
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

        // Net profit after gas (chain-specific gas cost from config)
        let net_profit_usd = profit_usd - self.config.estimated_gas_cost_usd;

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
            token0_decimals: 18, // V2 pools don't track decimals, default 18
            token1_decimals: 18,
            buy_pool_liquidity: None,
            quote_token_is_token0: true, // V2 pools: default assumption (USDC is token0)
            min_profit_usd: 0.0, // Legacy V2 path: use global min_profit
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
        let wei_f = wei.to::<u128>() as f64;

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
    use alloy::primitives::Address;

    fn create_test_pool(
        dex: DexType,
        symbol: &str,
        reserve0: u128,
        reserve1: u128,
    ) -> PoolState {
        PoolState {
            address: Address::ZERO,
            dex,
            pair: TradingPair::new(Address::ZERO, Address::ZERO, symbol.to_string()),
            reserve0: U256::from(reserve0),
            reserve1: U256::from(reserve1),
            last_updated: 100,
            token0_decimals: 18,
            token1_decimals: 18,
        }
    }

    fn create_test_config() -> BotConfig {
        use std::str::FromStr;
        BotConfig {
            rpc_url: String::new(),
            chain_id: 137,
            chain_name: "polygon".to_string(),
            quote_token_address: Address::from_str("0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174").unwrap(),
            estimated_gas_cost_usd: 0.05,
            private_key: String::new(),
            min_profit_usd: 5.0,
            max_trade_size_usd: 500.0,
            max_slippage_percent: 0.5,
            uniswap_router: Address::ZERO,
            sushiswap_router: Address::ZERO,
            uniswap_factory: Address::ZERO,
            sushiswap_factory: Address::ZERO,
            apeswap_router: None,
            apeswap_factory: None,
            uniswap_v3_factory: None,
            uniswap_v3_router: None,
            uniswap_v3_quoter: None,
            sushiswap_v3_factory: None,
            sushiswap_v3_router: None,
            sushiswap_v3_quoter: None,
            quickswap_v3_factory: None,
            quickswap_v3_router: None,
            quickswap_v3_quoter: None,
            uniswap_v3_quoter_is_v2: false,
            pairs: vec![],
            poll_interval_ms: 1000,
            max_gas_price_gwei: 100,
            tax_log_dir: None,
            tax_log_enabled: false,
            live_mode: false,
            pool_state_file: None,
            whitelist_file: None,
            price_log_enabled: false,
            price_log_dir: None,
            arb_executor_address: None,
            skip_multicall_prescreen: false,
            route_cooldown_blocks: 10,
            private_rpc_url: None,
            mempool_monitor_mode: "off".to_string(),
            mempool_min_profit_usd: 0.05,
            mempool_gas_limit: 500_000,
            mempool_min_priority_gwei: 1000,
            mempool_gas_profit_cap: 0.50,
            native_token_price_usd: 0.50,
            quote_token_address_native: None,
            ws_rpc_url: None,
            quote_token_address_usdt: None,
        }
    }

    #[test]
    fn test_calculate_spread() {
        let config = create_test_config();

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
        let config = create_test_config();

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
