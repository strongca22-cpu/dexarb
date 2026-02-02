//! A4 Mempool Monitor — Type Definitions
//!
//! Purpose:
//!     Data structures for pending swap observation, cross-reference tracking,
//!     AMM state simulation (Phase 2), and execution signaling (Phase 3).
//!
//! Author: AI-Generated
//! Created: 2026-02-01
//! Modified: 2026-02-01 — Phase 2: simulation types
//! Modified: 2026-02-01 — Phase 3: MempoolSignal for execution pipeline
//!
//! Dependencies:
//!     - alloy (Address, TxHash, U256)
//!     - chrono (timestamps)

use crate::types::DexType;
use alloy::primitives::{Address, TxHash, U256};
use std::collections::HashMap;
use std::time::Instant;

/// Mempool monitor operating mode (set via MEMPOOL_MONITOR env var)
#[derive(Debug, Clone, PartialEq)]
pub enum MempoolMode {
    /// Disabled — no mempool monitoring
    Off,
    /// Observation only — log pending swaps, track confirmation rates
    Observe,
    /// Speculative execution — submit backrun txs (Phase 3, not yet implemented)
    Execute,
}

impl MempoolMode {
    pub fn from_env(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "observe" => Self::Observe,
            "execute" => Self::Execute,
            _ => Self::Off,
        }
    }

    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// Decoded swap calldata — output of the decoder module.
/// Contains only the fields extracted from the transaction input data.
#[derive(Debug, Clone)]
pub struct DecodedSwap {
    /// Function name (e.g., "exactInputSingle", "multicall>exactInputSingle")
    pub function_name: String,
    /// Input token address (None if decoding failed or unknown)
    pub token_in: Option<Address>,
    /// Output token address (None if decoding failed or unknown)
    pub token_out: Option<Address>,
    /// Input amount in raw token units
    pub amount_in: Option<U256>,
    /// Minimum output amount (slippage protection)
    pub amount_out_min: Option<U256>,
    /// V3 fee tier in bps (None for V2 or Algebra)
    pub fee_tier: Option<u32>,
}

/// Full pending swap observation — decoded calldata + transaction metadata.
/// This is what gets written to the CSV log.
#[derive(Debug, Clone)]
pub struct PendingSwap {
    pub timestamp_utc: String,
    pub tx_hash: TxHash,
    pub router: Address,
    pub router_name: String,
    pub function_name: String,
    pub token_in: Option<Address>,
    pub token_out: Option<Address>,
    pub amount_in: Option<U256>,
    pub amount_out_min: Option<U256>,
    pub fee_tier: Option<u32>,
    pub gas_price_gwei: f64,
    pub max_priority_fee_gwei: f64,
}

/// Tracks pending swap observations for cross-reference against confirmed blocks.
/// When a block confirms, we check which of our tracked pending swaps were included.
pub struct ConfirmationTracker {
    /// tx_hash → (time first seen, router_name)
    pending: HashMap<TxHash, (Instant, String)>,
    /// Running stats
    pub total_pending_seen: u64,
    pub total_confirmed: u64,
    pub total_lead_time_ms: u64,
    pub lead_time_samples: Vec<u64>,
}

impl ConfirmationTracker {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            total_pending_seen: 0,
            total_confirmed: 0,
            total_lead_time_ms: 0,
            lead_time_samples: Vec::new(),
        }
    }

    /// Record a pending swap observation
    pub fn track(&mut self, tx_hash: TxHash, router_name: &str) {
        self.total_pending_seen += 1;
        self.pending.insert(tx_hash, (Instant::now(), router_name.to_string()));
    }

    /// Check a set of confirmed tx hashes against our pending tracker.
    /// Returns the number of matches found and their lead times.
    pub fn check_block(&mut self, confirmed_hashes: &[TxHash]) -> Vec<(TxHash, u64, String)> {
        let mut matches = Vec::new();

        for hash in confirmed_hashes {
            if let Some((seen_at, router_name)) = self.pending.remove(hash) {
                let lead_time_ms = seen_at.elapsed().as_millis() as u64;
                self.total_confirmed += 1;
                self.total_lead_time_ms += lead_time_ms;
                self.lead_time_samples.push(lead_time_ms);
                matches.push((*hash, lead_time_ms, router_name));
            }
        }

        matches
    }

    /// Remove entries older than max_age (probably dropped from mempool)
    pub fn cleanup(&mut self, max_age: std::time::Duration) {
        self.pending.retain(|_, (seen_at, _)| seen_at.elapsed() < max_age);
    }

    /// Number of pending txs currently being tracked
    pub fn tracking_count(&self) -> usize {
        self.pending.len()
    }

    /// Confirmation rate as percentage
    pub fn confirmation_rate(&self) -> f64 {
        if self.total_pending_seen == 0 {
            return 0.0;
        }
        self.total_confirmed as f64 / self.total_pending_seen as f64 * 100.0
    }

    /// Median lead time in milliseconds
    pub fn median_lead_time_ms(&self) -> u64 {
        if self.lead_time_samples.is_empty() {
            return 0;
        }
        let mut sorted = self.lead_time_samples.clone();
        sorted.sort();
        sorted[sorted.len() / 2]
    }

    /// Mean lead time in milliseconds
    pub fn mean_lead_time_ms(&self) -> u64 {
        if self.total_confirmed == 0 {
            return 0;
        }
        self.total_lead_time_ms / self.total_confirmed
    }
}

// ── Phase 2: Simulation Types ───────────────────────────────────────────────

/// Result of simulating a pending swap on a pool.
/// Contains the predicted post-swap state for accuracy validation.
#[derive(Debug, Clone)]
pub struct SimulatedPoolState {
    /// Which DEX pool was affected
    pub dex: DexType,
    /// Pair symbol (e.g., "WETH/USDC")
    pub pair_symbol: String,
    /// Whether the source pool is V3 (true) or V2 (false)
    pub is_v3: bool,
    /// Price before the pending swap
    pub pre_swap_price: f64,
    /// Predicted price after the pending swap
    pub post_swap_price: f64,
    /// Post-swap sqrtPriceX96 (V3 only, for accuracy validation)
    pub post_sqrt_price_x96: Option<U256>,
    /// Post-swap reserves (V2 only)
    pub post_reserve0: Option<U256>,
    pub post_reserve1: Option<U256>,
    /// Post-swap tick (V3 only, derived from sqrtPriceX96)
    pub post_tick: Option<i32>,
}

/// A simulated arbitrage opportunity created by a pending swap.
/// Logged to simulated_opportunities CSV.
#[derive(Debug, Clone)]
pub struct SimulatedOpportunity {
    pub timestamp_utc: String,
    pub tx_hash: TxHash,
    /// DEX where the pending swap will land
    pub trigger_dex: DexType,
    /// Function name from decoded calldata
    pub trigger_function: String,
    /// Pair symbol (e.g., "WETH/USDC")
    pub pair_symbol: String,
    /// Swap direction (token0→token1 = true, token1→token0 = false)
    pub zero_for_one: bool,
    /// Raw input amount
    pub amount_in: U256,
    /// Price before and after simulation
    pub pre_swap_price: f64,
    pub post_swap_price: f64,
    /// Price impact of the pending swap (%)
    pub price_impact_pct: f64,
    /// Best cross-DEX arb opportunity
    pub arb_buy_dex: DexType,
    pub arb_sell_dex: DexType,
    pub arb_spread_pct: f64,
    pub arb_est_profit_usd: f64,
}

/// Tracks simulated opportunities for post-confirmation accuracy validation.
/// Mirrors ConfirmationTracker but stores simulation predictions.
pub struct SimulationTracker {
    /// tx_hash → (simulated state, time seen, best opportunity if any)
    pending: HashMap<TxHash, (SimulatedPoolState, Instant, Option<SimulatedOpportunity>)>,
    /// Running stats
    pub total_simulated: u64,
    pub total_opportunities: u64,
    pub total_validated: u64,
    pub price_error_samples: Vec<f64>,
}

impl SimulationTracker {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            total_simulated: 0,
            total_opportunities: 0,
            total_validated: 0,
            price_error_samples: Vec::new(),
        }
    }

    /// Record a simulation result for later accuracy validation
    pub fn track(
        &mut self,
        tx_hash: TxHash,
        state: SimulatedPoolState,
        opportunity: Option<SimulatedOpportunity>,
    ) {
        self.total_simulated += 1;
        if opportunity.is_some() {
            self.total_opportunities += 1;
        }
        self.pending
            .insert(tx_hash, (state, Instant::now(), opportunity));
    }

    /// Check if a confirmed tx has a pending simulation. Returns and removes it.
    pub fn check_confirmation(
        &mut self,
        tx_hash: TxHash,
    ) -> Option<(SimulatedPoolState, Option<SimulatedOpportunity>)> {
        self.pending
            .remove(&tx_hash)
            .map(|(state, _instant, opp)| (state, opp))
    }

    /// Record an accuracy measurement
    pub fn record_accuracy(&mut self, error_pct: f64) {
        self.total_validated += 1;
        self.price_error_samples.push(error_pct);
    }

    /// Remove entries older than max_age
    pub fn cleanup(&mut self, max_age: std::time::Duration) {
        self.pending
            .retain(|_, (_, seen_at, _)| seen_at.elapsed() < max_age);
    }

    /// Median price prediction error (%)
    pub fn median_error_pct(&self) -> f64 {
        if self.price_error_samples.is_empty() {
            return 0.0;
        }
        let mut sorted = self.price_error_samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted[sorted.len() / 2]
    }
}

// ── Phase 3: Execution Signal Types ─────────────────────────────────────────

/// Signal sent from the mempool monitor to the main loop when a simulated
/// opportunity exceeds the execution threshold.
/// Carried over an mpsc channel; the main loop converts it to an
/// ArbitrageOpportunity and calls execute_from_mempool().
#[derive(Debug, Clone)]
pub struct MempoolSignal {
    /// The simulated cross-DEX opportunity
    pub opportunity: SimulatedOpportunity,
    /// Gas price from the trigger transaction (wei)
    pub trigger_gas_price: U256,
    /// EIP-1559 max priority fee from the trigger tx (if available)
    pub trigger_max_priority_fee: Option<U256>,
    /// When the signal was created (for staleness detection)
    pub seen_at: Instant,
}
