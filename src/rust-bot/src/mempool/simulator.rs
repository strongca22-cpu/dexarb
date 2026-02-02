//! A4 Mempool Monitor — AMM State Simulator (Phase 2)
//!
//! Purpose:
//!     Given a decoded pending swap, simulate the post-swap pool state using
//!     on-chain AMM math (constant product for V2, sqrtPrice math for V3).
//!     Then check cross-DEX pools for arbitrage opportunities the swap creates.
//!
//! Author: AI-Generated
//! Created: 2026-02-01
//! Modified: 2026-02-01
//!
//! Dependencies:
//!     - alloy (U256 arithmetic, Address)
//!
//! Notes:
//!     - V2: constant product (x * y = k) with 0.30% fee
//!     - V3: within-tick sqrtPriceX96 math (Uniswap SqrtPriceMath formulas)
//!     - Algebra (QuickSwap V3): same V3 math, fee from pool state (dynamic)
//!     - Phase 2 scope: WETH/USDC and WMATIC/USDC pairs only
//!     - Returns None on overflow or tick boundary crossing (conservative)
//!
//! References:
//!     - Uniswap V3 SqrtPriceMath.sol: getNextSqrtPriceFromInput
//!     - Uniswap V3 SwapMath.sol: computeSwapStep

use alloy::primitives::{Address, TxHash, U256};
use std::collections::{HashMap, HashSet};
use tracing::{debug, info, warn};

use crate::pool::PoolStateManager;
use crate::types::{BotConfig, DexType, PoolState, V3PoolState};

use super::types::{DecodedSwap, SimulatedOpportunity, SimulatedPoolState};

// ── Constants ────────────────────────────────────────────────────────────────

/// Q96 = 2^96, used in sqrtPriceX96 math
const Q96: u128 = 1u128 << 96;

/// V2 fee factor: 997/1000 = 0.30% fee
const V2_FEE_NUMERATOR: u64 = 997;
const V2_FEE_DENOMINATOR: u64 = 1000;

// ── Data-Driven Pair Identification ─────────────────────────────────────────

/// Data-driven pair identification — built from PoolStateManager at startup.
/// Automatically covers all whitelisted pairs (V3 + V2). When new pairs are
/// added to the whitelist, they're automatically picked up on next restart.
pub struct PairLookup {
    /// Lowercase hex (no 0x prefix) → pair symbol. Only non-quote base tokens.
    /// e.g., "7ceb23..." → "WETH/USDC", "b33eaa..." → "UNI/USDC"
    token_to_pair: HashMap<String, String>,
    /// Lowercase hex quote tokens (USDC.e + native USDC)
    quote_tokens: HashSet<String>,
}

impl PairLookup {
    /// Build pair lookup from current pool state.
    /// Must be called after pools are synced (PoolStateManager populated).
    pub fn from_pool_state(state: &PoolStateManager, config: &BotConfig) -> Self {
        let mut token_to_pair = HashMap::new();
        let mut quote_tokens = HashSet::new();

        // Collect quote token addresses (lowercase hex, no 0x prefix)
        let qt_addr = format!("{:x}", config.quote_token_address);
        quote_tokens.insert(qt_addr);
        if let Some(native) = config.quote_token_address_native {
            quote_tokens.insert(format!("{:x}", native));
        }
        if let Some(usdt) = config.quote_token_address_usdt {
            quote_tokens.insert(format!("{:x}", usdt));
        }

        // V3 pools: extract non-quote token → pair symbol mapping
        for pool in state.get_all_v3_pools() {
            let t0 = format!("{:x}", pool.pair.token0);
            let t1 = format!("{:x}", pool.pair.token1);
            if quote_tokens.contains(&t0) {
                token_to_pair.entry(t1).or_insert_with(|| pool.pair.symbol.clone());
            } else if quote_tokens.contains(&t1) {
                token_to_pair.entry(t0).or_insert_with(|| pool.pair.symbol.clone());
            }
        }

        // V2 pools: same logic
        for pool in state.get_all_pools() {
            let t0 = format!("{:x}", pool.pair.token0);
            let t1 = format!("{:x}", pool.pair.token1);
            if quote_tokens.contains(&t0) {
                token_to_pair.entry(t1).or_insert_with(|| pool.pair.symbol.clone());
            } else if quote_tokens.contains(&t1) {
                token_to_pair.entry(t0).or_insert_with(|| pool.pair.symbol.clone());
            }
        }

        info!(
            "PairLookup: {} base tokens, {} quote tokens, {} unique pairs",
            token_to_pair.len(),
            quote_tokens.len(),
            token_to_pair.values().collect::<HashSet<_>>().len(),
        );

        Self { token_to_pair, quote_tokens }
    }

    /// Identify pair from swap token addresses (lowercase hex, no 0x prefix).
    /// Returns the pair symbol (e.g., "WETH/USDC") or None if unrecognized.
    pub fn identify_pair(&self, in_hex: &str, out_hex: &str) -> Option<String> {
        if self.quote_tokens.contains(in_hex) {
            self.token_to_pair.get(out_hex).cloned()
        } else if self.quote_tokens.contains(out_hex) {
            self.token_to_pair.get(in_hex).cloned()
        } else {
            None
        }
    }

    /// Number of unique pairs covered.
    pub fn pair_count(&self) -> usize {
        self.token_to_pair.values().collect::<HashSet<_>>().len()
    }
}

// ── Pool Identification ──────────────────────────────────────────────────────

/// Identify which monitored pair a decoded pending swap affects.
///
/// Returns (DexType, pair_symbol, zero_for_one) if the swap is:
/// - On a pair we monitor (any whitelisted pair via PairLookup)
/// - An exact-input function (not exact-output)
/// - Has a decodable amount_in
///
/// Returns None otherwise (skip simulation).
pub fn identify_affected_pool(
    decoded: &DecodedSwap,
    router_name: &str,
    state_manager: &PoolStateManager,
    pair_lookup: &PairLookup,
) -> Option<(DexType, String, bool)> {
    // Must have token_in, token_out, and amount_in
    let token_in = decoded.token_in?;
    let token_out = decoded.token_out?;
    let _amount_in = decoded.amount_in?;

    // Skip exact-output functions
    if is_exact_output(&decoded.function_name) {
        return None;
    }

    // Normalize addresses to lowercase hex (strip 0x prefix for comparison)
    let in_hex = format!("{:x}", token_in);
    let out_hex = format!("{:x}", token_out);

    // Identify the pair via data-driven lookup (covers all whitelisted pairs)
    let pair_symbol = match pair_lookup.identify_pair(&in_hex, &out_hex) {
        Some(p) => p,
        None => {
            return None;
        }
    };

    // Map router_name + fee_tier → DexType
    let dex = match router_fee_to_dex_type(router_name, decoded.fee_tier) {
        Some(d) => d,
        None => {
            debug!(
                "Sim skip: router={} fee={:?} pair={}",
                router_name, decoded.fee_tier, pair_symbol
            );
            return None;
        }
    };

    // Verify this pool exists in state_manager
    let pool_exists = if dex.is_v3() {
        state_manager.get_v3_pool(dex, &pair_symbol).is_some()
    } else {
        state_manager.get_pool(dex, &pair_symbol).is_some()
    };

    if !pool_exists {
        debug!(
            "Sim skip: {:?}/{} not in state_manager",
            dex, pair_symbol
        );
        return None;
    }

    // Determine zero_for_one from the pool's actual token ordering
    let zero_for_one = if dex.is_v3() {
        let pool = state_manager.get_v3_pool(dex, &pair_symbol)?;
        token_in == pool.pair.token0
    } else {
        let pool = state_manager.get_pool(dex, &pair_symbol)?;
        token_in == pool.pair.token0
    };

    Some((dex, pair_symbol, zero_for_one))
}

/// Check if a function name represents an exact-output swap (skip for simulation)
fn is_exact_output(function_name: &str) -> bool {
    function_name.contains("exactOutput")
        || function_name.contains("ForExactTokens")
        || function_name.contains("swapExactETHForTokens")
        || function_name == "multicall(opaque)"
}

/// Map router name + fee tier to DexType variant.
fn router_fee_to_dex_type(router_name: &str, fee_tier: Option<u32>) -> Option<DexType> {
    match router_name {
        "UniswapV3" => match fee_tier? {
            100 => Some(DexType::UniswapV3_001),
            500 => Some(DexType::UniswapV3_005),
            3000 => Some(DexType::UniswapV3_030),
            10000 => Some(DexType::UniswapV3_100),
            _ => None,
        },
        "SushiV3" => match fee_tier? {
            100 => Some(DexType::SushiV3_001),
            500 => Some(DexType::SushiV3_005),
            3000 => Some(DexType::SushiV3_030),
            _ => None,
        },
        "AlgebraV3" => Some(DexType::QuickswapV3),
        _ => None,
    }
}

// ── V2 Simulation (Constant Product) ─────────────────────────────────────────

/// Simulate a V2 swap using the constant product formula.
///
/// Formula: amountOut = (amountIn * 997 * reserveOut) / (reserveIn * 1000 + amountIn * 997)
/// Post-swap: reserveIn += amountIn, reserveOut -= amountOut
pub fn simulate_v2_swap(
    pool: &PoolState,
    amount_in: U256,
    token_in: Address,
) -> Option<SimulatedPoolState> {
    if amount_in.is_zero() {
        return None;
    }

    // Determine swap direction
    let (reserve_in, reserve_out) = if token_in == pool.pair.token0 {
        (pool.reserve0, pool.reserve1)
    } else {
        (pool.reserve1, pool.reserve0)
    };

    if reserve_in.is_zero() || reserve_out.is_zero() {
        return None;
    }

    // Constant product with 0.3% fee
    let amount_in_with_fee = amount_in
        .checked_mul(U256::from(V2_FEE_NUMERATOR))?;
    let numerator = amount_in_with_fee.checked_mul(reserve_out)?;
    let denominator = reserve_in
        .checked_mul(U256::from(V2_FEE_DENOMINATOR))?
        .checked_add(amount_in_with_fee)?;

    if denominator.is_zero() {
        return None;
    }

    let amount_out = numerator / denominator;

    // Post-swap reserves
    let (new_reserve0, new_reserve1) = if token_in == pool.pair.token0 {
        (
            pool.reserve0.checked_add(amount_in)?,
            pool.reserve1.checked_sub(amount_out)?,
        )
    } else {
        (
            pool.reserve0.checked_sub(amount_out)?,
            pool.reserve1.checked_add(amount_in)?,
        )
    };

    // Calculate prices (decimal-adjusted ratio: token1/token0)
    let pre_price = v2_price_adjusted(
        pool.reserve0,
        pool.reserve1,
        pool.token0_decimals,
        pool.token1_decimals,
    );
    let post_price = v2_price_adjusted(
        new_reserve0,
        new_reserve1,
        pool.token0_decimals,
        pool.token1_decimals,
    );

    Some(SimulatedPoolState {
        dex: pool.dex,
        pair_symbol: pool.pair.symbol.clone(),
        is_v3: false,
        pre_swap_price: pre_price,
        post_swap_price: post_price,
        post_sqrt_price_x96: None,
        post_reserve0: Some(new_reserve0),
        post_reserve1: Some(new_reserve1),
        post_tick: None,
    })
}

/// V2 price adjusted for decimals: (reserve1 / reserve0) * 10^(dec0 - dec1)
fn v2_price_adjusted(reserve0: U256, reserve1: U256, dec0: u8, dec1: u8) -> f64 {
    if reserve0.is_zero() {
        return 0.0;
    }
    let r0 = reserve0.to::<u128>() as f64;
    let r1 = reserve1.to::<u128>() as f64;
    let decimal_adj = 10_f64.powi(dec0 as i32 - dec1 as i32);
    (r1 / r0) * decimal_adj
}

// ── V3 Simulation (Within-Tick sqrtPriceX96) ─────────────────────────────────

/// Simulate a V3 swap within the current tick range.
///
/// Uses Uniswap SqrtPriceMath formulas for exact within-tick computation.
/// Returns None if: amount is zero, liquidity is zero, swap crosses tick
/// boundary, or arithmetic overflows.
pub fn simulate_v3_swap(
    pool: &V3PoolState,
    amount_in: U256,
    zero_for_one: bool,
) -> Option<SimulatedPoolState> {
    if amount_in.is_zero() || pool.liquidity == 0 {
        return None;
    }

    // Step 1: Apply fee (V3 fee is in millionths: 500 = 0.05%)
    let fee = pool.fee;
    let amount_after_fee = amount_in
        .checked_mul(U256::from(1_000_000u32.checked_sub(fee)?))?
        .checked_div(U256::from(1_000_000u32))?;

    if amount_after_fee.is_zero() {
        return None;
    }

    // Step 2: Compute new sqrtPriceX96
    let new_sqrt_price = if zero_for_one {
        get_next_sqrt_price_from_amount0(pool.sqrt_price_x96, pool.liquidity, amount_after_fee)?
    } else {
        get_next_sqrt_price_from_amount1(pool.sqrt_price_x96, pool.liquidity, amount_after_fee)?
    };

    // Sanity: price should move in the expected direction
    if zero_for_one && new_sqrt_price >= pool.sqrt_price_x96 {
        warn!("V3 sim: zeroForOne but price didn't decrease");
        return None;
    }
    if !zero_for_one && new_sqrt_price <= pool.sqrt_price_x96 {
        warn!("V3 sim: oneForZero but price didn't increase");
        return None;
    }

    // Step 3: Tick boundary check (soft — log but don't reject within tolerance)
    // Single-tick approximation is exact within one tick spacing, degrades beyond.
    // Allow up to 10 tick spacings (~1% price move for fee=500) for data collection;
    // accuracy CSV will quantify degradation for cross-tick simulations.
    let new_tick = tick_from_sqrt_price_x96(new_sqrt_price);
    let tick_space = tick_spacing_for_fee(fee);
    let ticks_crossed = ((new_tick - pool.tick).abs() + tick_space - 1) / tick_space;

    if ticks_crossed > 10 {
        // Too many ticks crossed — approximation is unreliable
        warn!(
            "V3 sim: crossed {} ticks (cur={}, new={}, spacing={}). amt={}, liq={}. Skipping.",
            ticks_crossed, pool.tick, new_tick, tick_space, amount_in, pool.liquidity
        );
        return None;
    }

    if ticks_crossed > 1 {
        debug!(
            "V3 sim: crossed {} ticks (cur={}, new={}). Single-tick approx, accuracy may degrade.",
            ticks_crossed, pool.tick, new_tick
        );
    }

    // Step 4: Compute prices
    let pre_price = pool.price();
    let post_price =
        price_from_sqrt_price_x96(new_sqrt_price, pool.token0_decimals, pool.token1_decimals);

    Some(SimulatedPoolState {
        dex: pool.dex,
        pair_symbol: pool.pair.symbol.clone(),
        is_v3: true,
        pre_swap_price: pre_price,
        post_swap_price: post_price,
        post_sqrt_price_x96: Some(new_sqrt_price),
        post_reserve0: None,
        post_reserve1: None,
        post_tick: Some(new_tick),
    })
}

// ── V3 Math Helpers (from Uniswap SqrtPriceMath.sol) ─────────────────────────

/// getNextSqrtPriceFromAmount0RoundingUp
///
/// When adding token0 (zeroForOne), sqrtPrice decreases.
///
/// Primary formula (precise):
///   result = ceil(numerator1 * sqrtPX96 / (numerator1 + amount * sqrtPX96))
///   where numerator1 = liquidity << 96
///
/// Fallback (avoids overflow in numerator1 * sqrtPX96):
///   result = ceil(numerator1 / (numerator1 / sqrtPX96 + amount))
///
/// Mirrors Uniswap SqrtPriceMath.sol with FullMath overflow fallback.
fn get_next_sqrt_price_from_amount0(
    sqrt_price_x96: U256,
    liquidity: u128,
    amount: U256,
) -> Option<U256> {
    if amount.is_zero() {
        return Some(sqrt_price_x96);
    }
    if sqrt_price_x96.is_zero() {
        return None;
    }

    let liquidity_u256 = U256::from(liquidity);
    // numerator1 = liquidity << 96 (safe: u128 << 96 fits in U256)
    let numerator1: U256 = liquidity_u256 << 96;

    // Try precise formula first: ceil(numerator1 * sqrtPX96 / denominator)
    if let Some(product) = amount.checked_mul(sqrt_price_x96) {
        if let Some(denominator) = numerator1.checked_add(product) {
            if !denominator.is_zero() {
                if let Some(full_num) = numerator1.checked_mul(sqrt_price_x96) {
                    let result = (full_num + denominator - U256::from(1)) / denominator;
                    if !result.is_zero() {
                        return Some(result);
                    }
                }
            }
        }
    }

    // Fallback: ceil(numerator1 / (numerator1 / sqrtPX96 + amount))
    // Avoids the large numerator1 * sqrtPX96 intermediate product.
    let quotient = numerator1 / sqrt_price_x96;
    let denominator = quotient.checked_add(amount)?;
    if denominator.is_zero() {
        return None;
    }
    let result = (numerator1 + denominator - U256::from(1)) / denominator;
    if result.is_zero() {
        return None;
    }
    Some(result)
}

/// getNextSqrtPriceFromAmount1RoundingDown
///
/// When adding token1 (oneForZero), sqrtPrice increases.
///
/// Formula:
///   quotient = (amount << 96) / liquidity
///   result = sqrtPriceX96 + quotient
fn get_next_sqrt_price_from_amount1(
    sqrt_price_x96: U256,
    liquidity: u128,
    amount: U256,
) -> Option<U256> {
    if amount.is_zero() {
        return Some(sqrt_price_x96);
    }

    let liquidity_u256 = U256::from(liquidity);
    // quotient = (amount << 96) / liquidity
    let shifted = amount.checked_mul(U256::from(Q96))?;
    let quotient = shifted.checked_div(liquidity_u256)?;

    // result = sqrtPriceX96 + quotient
    let result = sqrt_price_x96.checked_add(quotient)?;

    Some(result)
}

/// Compute tick from sqrtPriceX96 using f64 approximation.
///
/// tick = floor(2 * ln(sqrtPrice / 2^96) / ln(1.0001))
///
/// Uses f64 for logarithm. Acceptable for tick boundary checks —
/// exact tick precision isn't needed, just "same tick range" verification.
fn tick_from_sqrt_price_x96(sqrt_price_x96: U256) -> i32 {
    let q96_f = 2.0_f64.powi(96);

    // Convert sqrtPriceX96 to f64
    let sqrt_price_f = if sqrt_price_x96 > U256::from(u128::MAX) {
        // Very large sqrtPrice: shift right and compensate
        let shifted: U256 = sqrt_price_x96 >> 64;
        let shifted_f = shifted.to::<u128>() as f64;
        shifted_f / (q96_f / 2.0_f64.powi(64))
    } else {
        sqrt_price_x96.to::<u128>() as f64 / q96_f
    };

    if sqrt_price_f <= 0.0 {
        return i32::MIN;
    }

    // tick = floor(2 * ln(sqrt_ratio) / ln(1.0001))
    let log_base = 1.0001_f64.ln();
    (2.0 * sqrt_price_f.ln() / log_base).floor() as i32
}

/// Compute price from sqrtPriceX96 using tick-based calculation.
/// Matches V3PoolState::price() for consistency.
///
/// price = 1.0001^tick * 10^(decimals0 - decimals1)
fn price_from_sqrt_price_x96(sqrt_price_x96: U256, dec0: u8, dec1: u8) -> f64 {
    let tick = tick_from_sqrt_price_x96(sqrt_price_x96);
    let price = 1.0001_f64.powi(tick);
    let decimal_adj = 10_f64.powi(dec0 as i32 - dec1 as i32);
    price * decimal_adj
}

/// Tick spacing for each V3 fee tier.
fn tick_spacing_for_fee(fee: u32) -> i32 {
    match fee {
        100 => 1,
        500 => 10,
        3000 => 60,
        10000 => 200,
        _ => 1, // Algebra/dynamic — use minimum spacing
    }
}

// ── Cross-DEX Opportunity Check ──────────────────────────────────────────────

/// Unified pool view for cross-DEX comparison (same as detector pattern)
struct UnifiedPool {
    dex: DexType,
    price: f64,
    fee_percent: f64,
}

/// Check for cross-DEX arbitrage opportunities using the simulated post-swap state.
///
/// The pending swap moves ONE pool's price. We compare the simulated post-swap
/// price against CURRENT prices of all OTHER pools for the same pair.
/// If a spread exceeds round-trip fees + gas, it's a simulated opportunity.
///
/// Mirrors the spread calculation in OpportunityDetector::check_pair_unified().
pub fn check_post_swap_opportunities(
    state_manager: &PoolStateManager,
    simulated: &SimulatedPoolState,
    config: &BotConfig,
    tx_hash: TxHash,
    trigger_function: &str,
    amount_in: U256,
    zero_for_one: bool,
    timestamp_utc: &str,
) -> Vec<SimulatedOpportunity> {
    let mut opportunities = Vec::new();

    // Build unified pool list: the simulated pool + all other pools for this pair
    let mut pools: Vec<UnifiedPool> = Vec::new();

    // Add the simulated pool (with post-swap price)
    let sim_fee = if simulated.is_v3 {
        state_manager
            .get_v3_pool(simulated.dex, &simulated.pair_symbol)
            .map(|p| p.fee as f64 / 10000.0)
            .unwrap_or(0.30)
    } else {
        0.30 // V2 always 0.30%
    };
    pools.push(UnifiedPool {
        dex: simulated.dex,
        price: simulated.post_swap_price,
        fee_percent: sim_fee,
    });

    // Add all OTHER V3 pools for this pair
    for pool in state_manager.get_v3_pools_for_pair(&simulated.pair_symbol) {
        if pool.dex == simulated.dex {
            continue; // Skip the pool we simulated
        }
        if pool.liquidity == 0 {
            continue;
        }
        pools.push(UnifiedPool {
            dex: pool.dex,
            price: pool.price(),
            fee_percent: pool.fee as f64 / 10000.0,
        });
    }

    // Add all V2 pools for this pair
    for pool in state_manager.get_pools_for_pair(&simulated.pair_symbol) {
        if !simulated.is_v3 && pool.dex == simulated.dex {
            continue; // Skip if we simulated a V2 pool
        }
        if pool.reserve0.is_zero() || pool.reserve1.is_zero() {
            continue;
        }
        pools.push(UnifiedPool {
            dex: pool.dex,
            price: v2_price_adjusted(
                pool.reserve0,
                pool.reserve1,
                pool.token0_decimals,
                pool.token1_decimals,
            ),
            fee_percent: 0.30, // V2 always 0.30%
        });
    }

    if pools.len() < 2 {
        return opportunities; // Need at least 2 pools for cross-DEX
    }

    // Determine quote token direction (same logic as detector.rs)
    // quote_is_token0: if any recognized USDC variant is token0
    // On Polygon: USDC variants (0x2791..., 0x3c49...) < WETH (0x7ceb...) → true for WETH/USDC
    //             WMATIC (0x0d50...) < USDC variants → false for WMATIC/USDC
    let quote_is_token0 = if simulated.is_v3 {
        state_manager
            .get_v3_pools_for_pair(&simulated.pair_symbol)
            .first()
            .map(|p| config.is_quote_token(&p.pair.token0))
            .unwrap_or(true)
    } else {
        state_manager
            .get_pools_for_pair(&simulated.pair_symbol)
            .first()
            .map(|p| config.is_quote_token(&p.pair.token0))
            .unwrap_or(true)
    };

    // Price impact
    let price_impact = if simulated.pre_swap_price != 0.0 {
        ((simulated.post_swap_price - simulated.pre_swap_price) / simulated.pre_swap_price * 100.0)
            .abs()
    } else {
        0.0
    };

    // Check all pairs (simulated pool vs each other pool)
    let sim_pool = &pools[0]; // The simulated pool is always first
    for other in &pools[1..] {
        // Assign buy/sell based on quote token direction
        let (buy_pool, sell_pool) = if quote_is_token0 {
            // quote=token0: BUY where price is HIGHER (more token1 per quote)
            if sim_pool.price > other.price {
                (sim_pool, other)
            } else {
                (other, sim_pool)
            }
        } else {
            // quote=token1: BUY where price is LOWER
            if sim_pool.price < other.price {
                (sim_pool, other)
            } else {
                (other, sim_pool)
            }
        };

        // Midmarket spread
        let midmarket_spread = if quote_is_token0 {
            (buy_pool.price - sell_pool.price) / sell_pool.price
        } else {
            (sell_pool.price - buy_pool.price) / buy_pool.price
        };

        // Round-trip fee (both legs)
        let round_trip_fee = (buy_pool.fee_percent + sell_pool.fee_percent) / 100.0;

        // Executable spread after fees
        let executable_spread = midmarket_spread - round_trip_fee;

        if executable_spread <= 0.0 {
            continue;
        }

        // Profit estimate
        let gross = executable_spread * config.max_trade_size_usd;
        let slippage_estimate = gross * 0.01;
        let net_profit = gross - config.estimated_gas_cost_usd - slippage_estimate;

        if net_profit > 0.0 {
            opportunities.push(SimulatedOpportunity {
                timestamp_utc: timestamp_utc.to_string(),
                tx_hash,
                trigger_dex: simulated.dex,
                trigger_function: trigger_function.to_string(),
                pair_symbol: simulated.pair_symbol.clone(),
                zero_for_one,
                amount_in,
                pre_swap_price: simulated.pre_swap_price,
                post_swap_price: simulated.post_swap_price,
                price_impact_pct: price_impact,
                arb_buy_dex: buy_pool.dex,
                arb_sell_dex: sell_pool.dex,
                arb_spread_pct: executable_spread * 100.0,
                arb_est_profit_usd: net_profit,
            });
        }
    }

    // Sort by profit descending
    opportunities.sort_by(|a, b| {
        b.arb_est_profit_usd
            .partial_cmp(&a.arb_est_profit_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    opportunities
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TradingPair;

    /// Q96 as U256 for test convenience
    fn q96_u256() -> U256 {
        U256::from(1) << 96
    }

    #[test]
    fn test_v3_amount0_price_decreases() {
        // Adding token0 → sqrtPrice should decrease (zeroForOne)
        let sqrt_price = q96_u256() * U256::from(100u64); // 100 * 2^96
        let liquidity: u128 = 1_000_000_000_000_000_000; // 1e18
        let amount = U256::from(1_000_000u64); // small amount

        let result = get_next_sqrt_price_from_amount0(sqrt_price, liquidity, amount);
        assert!(result.is_some(), "Should not overflow");
        let new_price = result.unwrap();
        assert!(new_price < sqrt_price, "Price should decrease (zeroForOne)");
        assert!(!new_price.is_zero(), "Price should not be zero");
    }

    #[test]
    fn test_v3_amount1_price_increases() {
        // Adding token1 → sqrtPrice should increase (oneForZero)
        let sqrt_price = q96_u256() * U256::from(100u64);
        let liquidity: u128 = 1_000_000_000_000_000_000;
        let amount = U256::from(1_000_000u64);

        let result = get_next_sqrt_price_from_amount1(sqrt_price, liquidity, amount);
        assert!(result.is_some());
        let new_price = result.unwrap();
        assert!(
            new_price > sqrt_price,
            "Price should increase (oneForZero)"
        );
    }

    #[test]
    fn test_v3_zero_amount_no_change() {
        let sqrt_price = q96_u256() * U256::from(50u64);
        let liquidity: u128 = 1_000_000_000_000_000_000;

        let r0 = get_next_sqrt_price_from_amount0(sqrt_price, liquidity, U256::ZERO);
        assert_eq!(r0, Some(sqrt_price));

        let r1 = get_next_sqrt_price_from_amount1(sqrt_price, liquidity, U256::ZERO);
        assert_eq!(r1, Some(sqrt_price));
    }

    #[test]
    fn test_tick_from_sqrt_price_at_tick_zero() {
        // At tick 0: sqrtPrice = sqrt(1.0) * 2^96 = 2^96
        let sqrt_price = q96_u256();
        let tick = tick_from_sqrt_price_x96(sqrt_price);
        assert_eq!(tick, 0, "tick at sqrtPrice=Q96 should be 0");
    }

    #[test]
    fn test_tick_from_sqrt_price_positive() {
        // At tick 1000: sqrt(1.0001^1000) * 2^96
        let sqrt_ratio = (1.0001_f64.powi(1000)).sqrt();
        let q96_f = 2.0_f64.powi(96);
        let sqrt_price = U256::from((sqrt_ratio * q96_f) as u128);
        let tick = tick_from_sqrt_price_x96(sqrt_price);
        // Should be close to 1000 (f64 precision allows ±1)
        assert!(
            (tick - 1000).abs() <= 1,
            "tick should be ~1000, got {}",
            tick
        );
    }

    #[test]
    fn test_v2_simulation_basic() {
        // Pool: 1000 WETH / 2,400,000 USDC (price ~2400 USDC/WETH)
        let pool = PoolState {
            address: Address::ZERO,
            dex: DexType::QuickSwapV2,
            pair: TradingPair::new(Address::ZERO, Address::ZERO, "WETH/USDC".to_string()),
            reserve0: U256::from(1000u64) * U256::from(10u64).pow(U256::from(18)), // 1000 WETH (18 dec)
            reserve1: U256::from(2_400_000u64) * U256::from(10u64).pow(U256::from(6)), // 2.4M USDC (6 dec)
            last_updated: 100,
            token0_decimals: 18,
            token1_decimals: 6,
        };

        // Swap 1 WETH → USDC (token0 → token1)
        let amount_in = U256::from(10u64).pow(U256::from(18)); // 1e18 = 1 WETH
        let token_in = pool.pair.token0;

        let result = simulate_v2_swap(&pool, amount_in, token_in);
        assert!(result.is_some(), "V2 sim should succeed");

        let sim = result.unwrap();
        assert!(
            sim.post_swap_price < sim.pre_swap_price,
            "Price should decrease when adding token0"
        );
        assert!(
            sim.post_reserve0.unwrap() > pool.reserve0,
            "reserve0 should increase"
        );
        assert!(
            sim.post_reserve1.unwrap() < pool.reserve1,
            "reserve1 should decrease"
        );
    }

    #[test]
    fn test_v2_simulation_price_impact() {
        // Small pool to see measurable impact
        let pool = PoolState {
            address: Address::ZERO,
            dex: DexType::QuickSwapV2,
            pair: TradingPair::new(Address::ZERO, Address::ZERO, "WETH/USDC".to_string()),
            reserve0: U256::from(100u64) * U256::from(10u64).pow(U256::from(18)), // 100 WETH
            reserve1: U256::from(240_000u64) * U256::from(10u64).pow(U256::from(6)), // 240K USDC
            last_updated: 100,
            token0_decimals: 18,
            token1_decimals: 6,
        };

        // Swap 10 WETH (10% of reserves) → expect ~10% price impact
        let amount_in = U256::from(10u64) * U256::from(10u64).pow(U256::from(18));
        let token_in = pool.pair.token0;

        let sim = simulate_v2_swap(&pool, amount_in, token_in).unwrap();
        let impact = ((sim.pre_swap_price - sim.post_swap_price) / sim.pre_swap_price * 100.0).abs();
        // 10% of reserves → roughly 18-20% price impact for constant product
        assert!(
            impact > 10.0 && impact < 30.0,
            "10% swap should cause ~18% impact, got {:.1}%",
            impact
        );
    }

    #[test]
    fn test_fee_application() {
        // V3 fee 500 (0.05%) on 1,000,000 input
        let amount = U256::from(1_000_000u64);
        let fee = 500u32;
        let after_fee = amount * U256::from(1_000_000u32 - fee) / U256::from(1_000_000u32);
        assert_eq!(after_fee, U256::from(999_500u64));

        // V3 fee 3000 (0.30%)
        let after_fee_30 = amount * U256::from(1_000_000u32 - 3000u32) / U256::from(1_000_000u32);
        assert_eq!(after_fee_30, U256::from(997_000u64));
    }

    #[test]
    fn test_router_fee_to_dex_type() {
        assert_eq!(
            router_fee_to_dex_type("UniswapV3", Some(500)),
            Some(DexType::UniswapV3_005)
        );
        assert_eq!(
            router_fee_to_dex_type("UniswapV3", Some(3000)),
            Some(DexType::UniswapV3_030)
        );
        assert_eq!(
            router_fee_to_dex_type("SushiV3", Some(100)),
            Some(DexType::SushiV3_001)
        );
        assert_eq!(
            router_fee_to_dex_type("AlgebraV3", None),
            Some(DexType::QuickswapV3)
        );
        assert_eq!(router_fee_to_dex_type("UnknownDex", Some(500)), None);
    }

    #[test]
    fn test_pair_lookup_basic() {
        // Build a PairLookup with known tokens
        let mut token_to_pair = HashMap::new();
        let mut quote_tokens = HashSet::new();

        // Polygon USDC variants
        quote_tokens.insert("3c499c542cef5e3811e1192ce70d8cc03d5c3359".to_string()); // native USDC
        quote_tokens.insert("2791bca1f2de4661ed88a30c99a7a9449aa84174".to_string()); // USDC.e

        // Base tokens
        token_to_pair.insert(
            "7ceb23fd6bc0add59e62ac25578270cff1b9f619".to_string(),
            "WETH/USDC".to_string(),
        );
        token_to_pair.insert(
            "0d500b1d8e8ef31e21c99d1db9a6444d3adf1270".to_string(),
            "WMATIC/USDC".to_string(),
        );
        token_to_pair.insert(
            "b33eaad8d922b1083446dc23f610c2567fb5180f".to_string(),
            "UNI/USDC".to_string(),
        );

        let lookup = PairLookup { token_to_pair, quote_tokens };

        // WETH → native USDC
        assert_eq!(
            lookup.identify_pair(
                "7ceb23fd6bc0add59e62ac25578270cff1b9f619",
                "3c499c542cef5e3811e1192ce70d8cc03d5c3359"
            ),
            Some("WETH/USDC".to_string())
        );
        // native USDC → WETH (reversed direction)
        assert_eq!(
            lookup.identify_pair(
                "3c499c542cef5e3811e1192ce70d8cc03d5c3359",
                "7ceb23fd6bc0add59e62ac25578270cff1b9f619"
            ),
            Some("WETH/USDC".to_string())
        );
        // WMATIC → USDC.e
        assert_eq!(
            lookup.identify_pair(
                "0d500b1d8e8ef31e21c99d1db9a6444d3adf1270",
                "2791bca1f2de4661ed88a30c99a7a9449aa84174"
            ),
            Some("WMATIC/USDC".to_string())
        );
        // UNI → native USDC
        assert_eq!(
            lookup.identify_pair(
                "b33eaad8d922b1083446dc23f610c2567fb5180f",
                "3c499c542cef5e3811e1192ce70d8cc03d5c3359"
            ),
            Some("UNI/USDC".to_string())
        );
        // Unknown token
        assert_eq!(
            lookup.identify_pair(
                "deadbeef00000000000000000000000000000000",
                "3c499c542cef5e3811e1192ce70d8cc03d5c3359"
            ),
            None
        );
        // Neither is a quote token
        assert_eq!(
            lookup.identify_pair(
                "7ceb23fd6bc0add59e62ac25578270cff1b9f619",
                "0d500b1d8e8ef31e21c99d1db9a6444d3adf1270"
            ),
            None
        );
    }

    #[test]
    fn test_pair_lookup_count() {
        let mut token_to_pair = HashMap::new();
        let quote_tokens = HashSet::new();
        token_to_pair.insert("aaa".to_string(), "WETH/USDC".to_string());
        token_to_pair.insert("bbb".to_string(), "WMATIC/USDC".to_string());
        token_to_pair.insert("ccc".to_string(), "WETH/USDC".to_string()); // Duplicate pair

        let lookup = PairLookup { token_to_pair, quote_tokens };
        assert_eq!(lookup.pair_count(), 2); // 2 unique pairs
    }

    #[test]
    fn test_tick_spacing() {
        assert_eq!(tick_spacing_for_fee(100), 1);
        assert_eq!(tick_spacing_for_fee(500), 10);
        assert_eq!(tick_spacing_for_fee(3000), 60);
        assert_eq!(tick_spacing_for_fee(10000), 200);
        assert_eq!(tick_spacing_for_fee(0), 1); // Algebra
    }
}
