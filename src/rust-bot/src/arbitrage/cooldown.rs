//! Route-Level Cooldown — Suppress failed arbitrage routes with escalating backoff
//!
//! Purpose:
//!     Prevents the bot from hammering the same failed routes every block.
//!     Structurally dead spreads (e.g., same-DEX fee-tier gaps) quickly reach
//!     max cooldown (~16 min) while legitimate temporary failures recover in ~10s.
//!     Routes that repeatedly reach max cooldown with zero successes are permanently
//!     blacklisted for the session to eliminate structural false positives.
//!
//! Author: AI-Generated
//! Created: 2026-01-31
//! Modified: 2026-02-02 - Permanent blacklist for structural false positives (max_strikes)
//!
//! Design:
//!     - Route key: (pair_symbol, buy_dex, sell_dex)
//!     - Escalating backoff: initial → 5× → 5× → cap (default: 10 → 50 → 250 → 1250 → 1800 blocks)
//!     - On success: entry removed (instant reset), un-blacklisted if applicable
//!     - Permanent blacklist: after N max-cooldown cycles with 0 successes (default N=3)
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
    success_count: u32,
    /// How many times this route has reached max_cooldown cap
    max_cooldown_cycles: u32,
}

/// Route-level cooldown tracker with escalating backoff and permanent blacklist
pub struct RouteCooldown {
    entries: HashMap<RouteKey, CooldownEntry>,
    initial_cooldown: u64,
    max_cooldown: u64,
    /// Routes permanently suppressed this session (structural false positives)
    blacklist: HashMap<RouteKey, bool>,
    /// Max consecutive max-cooldown cycles with 0 successes before blacklisting.
    /// 0 = permanent blacklisting disabled.
    max_strikes: u32,
}

/// Escalation multiplier per failure (5× each step)
const ESCALATION_FACTOR: u64 = 5;

/// Maximum cooldown cap in blocks (~1 hour on Polygon with ~2s blocks)
const DEFAULT_MAX_COOLDOWN: u64 = 1800;

impl RouteCooldown {
    /// Create a new cooldown tracker.
    /// `initial_cooldown` = blocks to suppress after first failure (0 = disabled).
    /// `max_strikes` = max-cooldown cycles with 0 successes before permanent blacklist (0 = disabled).
    pub fn new(initial_cooldown: u64, max_strikes: u32) -> Self {
        Self {
            entries: HashMap::new(),
            initial_cooldown,
            max_cooldown: DEFAULT_MAX_COOLDOWN,
            blacklist: HashMap::new(),
            max_strikes,
        }
    }

    /// Returns true if this route is currently suppressed (in cooldown or blacklisted).
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

        // Check permanent blacklist first
        if self.blacklist.contains_key(&key) {
            return true;
        }

        if let Some(entry) = self.entries.get(&key) {
            let expires_at = entry.last_failed_block + entry.cooldown_blocks;
            current_block < expires_at
        } else {
            false
        }
    }

    /// Record a failure for this route. Creates or escalates the cooldown.
    /// Escalation: initial → initial×5 → initial×25 → ... → max_cooldown
    /// After max_strikes consecutive max-cooldown cycles with 0 successes,
    /// the route is permanently blacklisted for this session.
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

        // Skip if already blacklisted
        if self.blacklist.contains_key(&key) {
            return;
        }

        let entry = self.entries.entry(key.clone()).or_insert_with(|| CooldownEntry {
            last_failed_block: block,
            cooldown_blocks: 0, // Will be set below
            failure_count: 0,
            success_count: 0,
            max_cooldown_cycles: 0,
        });

        entry.failure_count += 1;
        entry.last_failed_block = block;

        // Escalate: initial × 5^(failures-1), capped at max
        let escalated = self.initial_cooldown
            .saturating_mul(ESCALATION_FACTOR.saturating_pow(entry.failure_count.saturating_sub(1)));
        let new_cooldown = escalated.min(self.max_cooldown);

        // Track max-cooldown cycles (when we hit the cap)
        if new_cooldown == self.max_cooldown && entry.cooldown_blocks == self.max_cooldown {
            // Already at max cooldown and hitting it again — increment cycle count
            entry.max_cooldown_cycles += 1;
        } else if new_cooldown == self.max_cooldown && entry.cooldown_blocks < self.max_cooldown {
            // Just reached max cooldown for the first time (or re-reached after expiry)
            entry.max_cooldown_cycles += 1;
        }
        entry.cooldown_blocks = new_cooldown;

        // Permanent blacklist: max_strikes > 0, enough cycles, zero successes
        if self.max_strikes > 0
            && entry.max_cooldown_cycles >= self.max_strikes
            && entry.success_count == 0
        {
            info!(
                "BLACKLISTED: {} {:?}->{:?} | {} max-cooldown cycles with 0 successes | {} total failures",
                pair, buy_dex, sell_dex, entry.max_cooldown_cycles, entry.failure_count
            );
            self.blacklist.insert(key, true);
            return;
        }

        debug!(
            "Route cooldown: {} {:?}->{:?} | fail #{} | {} blocks | max_cd_cycles={}/{}",
            pair, buy_dex, sell_dex, entry.failure_count, entry.cooldown_blocks,
            entry.max_cooldown_cycles, self.max_strikes
        );
    }

    /// Record a success — removes the cooldown entry entirely (instant reset).
    /// Also removes from permanent blacklist if present (recovery safety valve).
    pub fn record_success(
        &mut self,
        pair: &str,
        buy_dex: DexType,
        sell_dex: DexType,
    ) {
        let key = (pair.to_string(), buy_dex, sell_dex);
        // Un-blacklist on success (rare but possible recovery)
        if self.blacklist.remove(&key).is_some() {
            info!("Route UN-BLACKLISTED on success: {} {:?}->{:?}", pair, buy_dex, sell_dex);
        }
        if self.entries.remove(&key).is_some() {
            info!("Route cooldown reset: {} {:?}->{:?} (trade succeeded)", pair, buy_dex, sell_dex);
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

    /// Number of permanently blacklisted routes this session.
    pub fn blacklist_count(&self) -> usize {
        self.blacklist.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_cooldown_initially() {
        let cd = RouteCooldown::new(10, 3);
        assert!(!cd.is_cooled_down("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100));
    }

    #[test]
    fn test_cooldown_after_failure() {
        let mut cd = RouteCooldown::new(10, 3);
        cd.record_failure("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100);

        // Should be cooled down for blocks 100..109
        assert!(cd.is_cooled_down("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100));
        assert!(cd.is_cooled_down("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 109));
        // Block 110 = expired
        assert!(!cd.is_cooled_down("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 110));
    }

    #[test]
    fn test_escalating_backoff() {
        let mut cd = RouteCooldown::new(10, 3);
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

        // Failure 5: 1800 blocks (capped at max, ~1 hour) — 1st max-cooldown cycle
        cd.record_failure(pair, buy, sell, 2000);
        assert!(cd.is_cooled_down(pair, buy, sell, 3799));
        assert!(!cd.is_cooled_down(pair, buy, sell, 3800));

        // Failure 6: still 1800 blocks (cap holds) — 2nd max-cooldown cycle
        cd.record_failure(pair, buy, sell, 4000);
        assert!(cd.is_cooled_down(pair, buy, sell, 5799));
        assert!(!cd.is_cooled_down(pair, buy, sell, 5800));
    }

    #[test]
    fn test_success_resets_cooldown() {
        let mut cd = RouteCooldown::new(10, 3);
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
        let mut cd = RouteCooldown::new(10, 3);
        cd.record_failure("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100);

        // Different pair — not cooled down
        assert!(!cd.is_cooled_down("WBTC/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 101));
        // Same pair, different dexes — not cooled down
        assert!(!cd.is_cooled_down("WETH/USDC", DexType::SushiV3_005, DexType::UniswapV3_030, 101));
    }

    #[test]
    fn test_disabled_when_zero() {
        let mut cd = RouteCooldown::new(0, 3);
        cd.record_failure("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100);
        assert!(!cd.is_cooled_down("WETH/USDC", DexType::UniswapV3_001, DexType::UniswapV3_030, 100));
    }

    #[test]
    fn test_cleanup_removes_expired() {
        let mut cd = RouteCooldown::new(10, 3);
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

    // ── New tests for permanent blacklist ──

    #[test]
    fn test_permanent_blacklist_after_max_strikes() {
        let mut cd = RouteCooldown::new(10, 3); // max_strikes=3
        let pair = "DEAD/USDC";
        let buy = DexType::UniswapV3_001;
        let sell = DexType::UniswapV3_030;

        // Drive to max cooldown: failures 1-5 escalate 10→50→250→1250→1800
        cd.record_failure(pair, buy, sell, 100);     // 10 blocks
        cd.record_failure(pair, buy, sell, 200);     // 50 blocks
        cd.record_failure(pair, buy, sell, 500);     // 250 blocks
        cd.record_failure(pair, buy, sell, 1000);    // 1250 blocks
        cd.record_failure(pair, buy, sell, 3000);    // 1800 blocks — max_cd_cycle 1
        assert_eq!(cd.blacklist_count(), 0);

        // At max cooldown now. More failures at max = more cycles.
        cd.record_failure(pair, buy, sell, 5000);    // 1800 blocks — max_cd_cycle 2
        assert_eq!(cd.blacklist_count(), 0);

        cd.record_failure(pair, buy, sell, 8000);    // 1800 blocks — max_cd_cycle 3 → BLACKLISTED
        assert_eq!(cd.blacklist_count(), 1);

        // Should be permanently suppressed even far in the future
        assert!(cd.is_cooled_down(pair, buy, sell, 999_999));

        // Further failures are no-ops (already blacklisted)
        cd.record_failure(pair, buy, sell, 1_000_000);
        assert_eq!(cd.blacklist_count(), 1);
    }

    #[test]
    fn test_blacklist_disabled_when_zero_strikes() {
        let mut cd = RouteCooldown::new(10, 0); // max_strikes=0 = disabled
        let pair = "DEAD/USDC";
        let buy = DexType::UniswapV3_001;
        let sell = DexType::UniswapV3_030;

        // Drive to max cooldown many times
        for i in 0..20u64 {
            cd.record_failure(pair, buy, sell, i * 2000);
        }
        // Should NOT be blacklisted (feature disabled)
        assert_eq!(cd.blacklist_count(), 0);
    }

    #[test]
    fn test_success_removes_from_blacklist() {
        let mut cd = RouteCooldown::new(10, 1); // very aggressive: 1 strike
        let pair = "REVIVED/USDC";
        let buy = DexType::UniswapV3_001;
        let sell = DexType::UniswapV3_030;

        // Drive to blacklist quickly: 5 failures reach max, then 1 more cycle = blacklisted
        for block in [100, 200, 500, 1000, 3000] {
            cd.record_failure(pair, buy, sell, block);
        }
        // Failure 5 reached max cooldown → cycle 1 ≥ max_strikes(1) → blacklisted
        assert_eq!(cd.blacklist_count(), 1);
        assert!(cd.is_cooled_down(pair, buy, sell, 999_999));

        // Success should un-blacklist
        cd.record_success(pair, buy, sell);
        assert_eq!(cd.blacklist_count(), 0);
        assert!(!cd.is_cooled_down(pair, buy, sell, 999_999));
    }
}
