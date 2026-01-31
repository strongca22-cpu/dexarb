//! Route-Level Cooldown — Suppress failed arbitrage routes with escalating backoff
//!
//! Purpose:
//!     Prevents the bot from hammering the same failed routes every block.
//!     Structurally dead spreads (e.g., same-DEX fee-tier gaps) quickly reach
//!     max cooldown (~16 min) while legitimate temporary failures recover in ~10s.
//!
//! Author: AI-Generated
//! Created: 2026-01-31
//! Modified: 2026-01-31
//!
//! Design:
//!     - Route key: (pair_symbol, buy_dex, sell_dex)
//!     - Escalating backoff: initial → 5× → 5× → cap (default: 10 → 50 → 250 → 1250 → 1800 blocks)
//!     - On success: entry removed (instant reset)
//!     - Periodic cleanup removes expired entries to bound memory

use std::collections::HashMap;
use tracing::{info, debug};

use crate::types::DexType;

/// Unique identifier for a route: (pair_symbol, buy_dex, sell_dex)
type RouteKey = (String, DexType, DexType);

/// Tracks cooldown state for a single route
struct CooldownEntry {
    last_failed_block: u64,
    cooldown_blocks: u64,
    failure_count: u32,
}

/// Route-level cooldown tracker with escalating backoff
pub struct RouteCooldown {
    entries: HashMap<RouteKey, CooldownEntry>,
    initial_cooldown: u64,
    max_cooldown: u64,
}

/// Escalation multiplier per failure (5× each step)
const ESCALATION_FACTOR: u64 = 5;

/// Maximum cooldown cap in blocks (~1 hour on Polygon with ~2s blocks)
const DEFAULT_MAX_COOLDOWN: u64 = 1800;

impl RouteCooldown {
    /// Create a new cooldown tracker.
    /// `initial_cooldown` = blocks to suppress after first failure (0 = disabled).
    pub fn new(initial_cooldown: u64) -> Self {
        Self {
            entries: HashMap::new(),
            initial_cooldown,
            max_cooldown: DEFAULT_MAX_COOLDOWN,
        }
    }

    /// Returns true if this route is currently suppressed (in cooldown).
    /// Returns false if no entry exists or cooldown has expired.
    pub fn is_cooled_down(
        &self,
        pair: &str,
        buy_dex: DexType,
        sell_dex: DexType,
        current_block: u64,
    ) -> bool {
        if self.initial_cooldown == 0 {
            return false; // Cooldown disabled
        }

        let key = (pair.to_string(), buy_dex, sell_dex);
        if let Some(entry) = self.entries.get(&key) {
            let expires_at = entry.last_failed_block + entry.cooldown_blocks;
            current_block < expires_at
        } else {
            false
        }
    }

    /// Record a failure for this route. Creates or escalates the cooldown.
    /// Escalation: initial → initial×5 → initial×25 → ... → max_cooldown
    pub fn record_failure(
        &mut self,
        pair: &str,
        buy_dex: DexType,
        sell_dex: DexType,
        block: u64,
    ) {
        if self.initial_cooldown == 0 {
            return; // Cooldown disabled
        }

        let key = (pair.to_string(), buy_dex, sell_dex);
        let entry = self.entries.entry(key).or_insert_with(|| CooldownEntry {
            last_failed_block: block,
            cooldown_blocks: 0, // Will be set below
            failure_count: 0,
        });

        entry.failure_count += 1;
        entry.last_failed_block = block;

        // Escalate: initial × 5^(failures-1), capped at max
        let escalated = self.initial_cooldown
            .saturating_mul(ESCALATION_FACTOR.saturating_pow(entry.failure_count.saturating_sub(1)));
        entry.cooldown_blocks = escalated.min(self.max_cooldown);

        debug!(
            "Route cooldown: {} {:?}→{:?} | fail #{} | suppressed for {} blocks",
            pair, buy_dex, sell_dex, entry.failure_count, entry.cooldown_blocks
        );
    }

    /// Record a success — removes the cooldown entry entirely (instant reset).
    pub fn record_success(
        &mut self,
        pair: &str,
        buy_dex: DexType,
        sell_dex: DexType,
    ) {
        let key = (pair.to_string(), buy_dex, sell_dex);
        if self.entries.remove(&key).is_some() {
            info!("Route cooldown reset: {} {:?}→{:?} (trade succeeded)", pair, buy_dex, sell_dex);
        }
    }

    /// Remove expired entries to bound memory usage.
    /// Call periodically (e.g., every ~100 blocks).
    pub fn cleanup(&mut self, current_block: u64) {
        let before = self.entries.len();
        self.entries.retain(|_key, entry| {
            let expires_at = entry.last_failed_block + entry.cooldown_blocks;
            current_block < expires_at
        });
        let removed = before - self.entries.len();
        if removed > 0 {
            debug!("Route cooldown cleanup: removed {} expired entries", removed);
        }
    }

    /// Number of currently active (non-expired) cooldown entries.
    pub fn active_count(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_cooldown_initially() {
        let cd = RouteCooldown::new(10);
        assert!(!cd.is_cooled_down("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100));
    }

    #[test]
    fn test_cooldown_after_failure() {
        let mut cd = RouteCooldown::new(10);
        cd.record_failure("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100);

        // Should be cooled down for blocks 100..109
        assert!(cd.is_cooled_down("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100));
        assert!(cd.is_cooled_down("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 109));
        // Block 110 = expired
        assert!(!cd.is_cooled_down("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 110));
    }

    #[test]
    fn test_escalating_backoff() {
        let mut cd = RouteCooldown::new(10);
        let pair = "WBTC/USDC";
        let buy = DexType::UniswapV3_001;
        let sell = DexType::UniswapV3_030;

        // Failure 1: 10 blocks (~20s)
        cd.record_failure(pair, buy, sell, 100);
        assert!(cd.is_cooled_down(pair, buy, sell, 109));
        assert!(!cd.is_cooled_down(pair, buy, sell, 110));

        // Failure 2: 50 blocks (~1.7 min)
        cd.record_failure(pair, buy, sell, 200);
        assert!(cd.is_cooled_down(pair, buy, sell, 249));
        assert!(!cd.is_cooled_down(pair, buy, sell, 250));

        // Failure 3: 250 blocks (~8.3 min)
        cd.record_failure(pair, buy, sell, 300);
        assert!(cd.is_cooled_down(pair, buy, sell, 549));
        assert!(!cd.is_cooled_down(pair, buy, sell, 550));

        // Failure 4: 1250 blocks (~42 min)
        cd.record_failure(pair, buy, sell, 600);
        assert!(cd.is_cooled_down(pair, buy, sell, 1849));
        assert!(!cd.is_cooled_down(pair, buy, sell, 1850));

        // Failure 5: 1800 blocks (capped at max, ~1 hour)
        cd.record_failure(pair, buy, sell, 2000);
        assert!(cd.is_cooled_down(pair, buy, sell, 3799));
        assert!(!cd.is_cooled_down(pair, buy, sell, 3800));

        // Failure 6: still 1800 blocks (cap holds)
        cd.record_failure(pair, buy, sell, 4000);
        assert!(cd.is_cooled_down(pair, buy, sell, 5799));
        assert!(!cd.is_cooled_down(pair, buy, sell, 5800));
    }

    #[test]
    fn test_success_resets_cooldown() {
        let mut cd = RouteCooldown::new(10);
        let pair = "WETH/USDC";
        let buy = DexType::UniswapV3_005;
        let sell = DexType::SushiV3_005;

        cd.record_failure(pair, buy, sell, 100);
        assert!(cd.is_cooled_down(pair, buy, sell, 101));

        cd.record_success(pair, buy, sell);
        assert!(!cd.is_cooled_down(pair, buy, sell, 101));
        assert_eq!(cd.active_count(), 0);
    }

    #[test]
    fn test_different_routes_independent() {
        let mut cd = RouteCooldown::new(10);
        cd.record_failure("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100);

        // Different pair — not cooled down
        assert!(!cd.is_cooled_down("WBTC/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 101));
        // Same pair, different dexes — not cooled down
        assert!(!cd.is_cooled_down("WETH/USDC", DexType::SushiV3_005, DexType::UniswapV3_030, 101));
    }

    #[test]
    fn test_disabled_when_zero() {
        let mut cd = RouteCooldown::new(0);
        cd.record_failure("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100);
        assert!(!cd.is_cooled_down("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100));
    }

    #[test]
    fn test_cleanup_removes_expired() {
        let mut cd = RouteCooldown::new(10);
        cd.record_failure("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100);
        cd.record_failure("WBTC/USDC", DexType::UniswapV3_005, DexType::SushiV3_005, 200);

        assert_eq!(cd.active_count(), 2);

        // Cleanup at block 111: first entry expired (100+10=110), second still active (200+10=210)
        cd.cleanup(111);
        assert_eq!(cd.active_count(), 1);

        // Cleanup at block 211: both expired
        cd.cleanup(211);
        assert_eq!(cd.active_count(), 0);
    }
}
