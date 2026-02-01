//! A4 Mempool Monitor — Type Definitions
//!
//! Purpose:
//!     Data structures for pending swap observation and cross-reference tracking.
//!
//! Author: AI-Generated
//! Created: 2026-02-01
//! Modified: 2026-02-01
//!
//! Dependencies:
//!     - ethers (Address, TxHash, U256)
//!     - chrono (timestamps)

use ethers::types::{Address, TxHash, U256};
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
