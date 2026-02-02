//! Static Pool Whitelist/Blacklist (Phase 1.1)
//!
//! Validates V3 pools against a JSON config before they enter detection.
//! Pools not in the whitelist are rejected (strict mode) or allowed (advisory mode).
//! Blacklisted pools and fee tiers are always rejected.
//!
//! Config file: config/pools_whitelist.json
//!
//! Author: AI-Generated
//! Created: 2026-01-29

use anyhow::{Context, Result};
use alloy::primitives::Address;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// JSON structures
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PoolWhitelist {
    pub version: String,
    pub last_updated: String,
    pub config: WhitelistConfig,
    pub whitelist: WhitelistSection,
    pub blacklist: BlacklistSection,
    #[serde(default)]
    pub observation: Option<ObservationSection>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WhitelistConfig {
    pub default_min_liquidity: u128,
    /// "strict" = only whitelisted pools allowed; "advisory" = only blacklisted rejected
    pub whitelist_enforcement: String,
    #[serde(default)]
    pub liquidity_thresholds: Option<LiquidityThresholds>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LiquidityThresholds {
    /// 0.01% tier minimum
    #[serde(default)]
    pub v3_100: Option<u128>,
    /// 0.05% tier minimum
    #[serde(default)]
    pub v3_500: Option<u128>,
    /// 0.30% tier minimum
    #[serde(default)]
    pub v3_3000: Option<u128>,
    /// 1.00% tier minimum (0 = disabled)
    #[serde(default)]
    pub v3_10000: Option<u128>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WhitelistSection {
    pub pools: Vec<WhitelistPool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WhitelistPool {
    pub address: String,
    pub pair: String,
    pub dex: String,
    pub fee_tier: u32,
    pub status: String,
    #[serde(default)]
    pub min_liquidity: Option<u128>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub added: Option<String>,
    #[serde(default)]
    pub last_verified: Option<String>,
    /// Per-pool maximum trade size in USD. When set, the detector caps
    /// trade_size to this amount for any opportunity involving this pool.
    /// Pools without this field use config.max_trade_size_usd (global default).
    #[serde(default)]
    pub max_trade_size_usd: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BlacklistSection {
    pub pools: Vec<BlacklistPool>,
    pub fee_tiers: Vec<BlacklistTier>,
    #[serde(default)]
    pub pairs: Vec<BlacklistPair>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BlacklistPool {
    pub address: String,
    pub pair: String,
    pub dex: String,
    pub fee_tier: u32,
    pub reason: String,
    #[serde(default)]
    pub phantom_spread: Option<String>,
    pub date_added: String,
    #[serde(default)]
    pub discovered_by: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BlacklistTier {
    pub tier: u32,
    pub reason: String,
    pub applies_to: String,
    pub date_added: String,
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BlacklistPair {
    pub pair: String,
    pub reason: String,
    pub date_added: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ObservationSection {
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub pools: Vec<ObservationPool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ObservationPool {
    pub address: String,
    pub pair: String,
    pub fee_tier: u32,
    pub concern: String,
    pub status: String,
    pub added: String,
}

// ---------------------------------------------------------------------------
// Precomputed lookup sets (built once at load time)
// ---------------------------------------------------------------------------

/// Fast-lookup wrapper built from the JSON config.
/// All address comparisons are lowercase hex without 0x prefix.
pub struct WhitelistFilter {
    /// Lowercase hex addresses of active whitelisted pools
    whitelisted_addrs: HashSet<String>,
    /// Lowercase hex addresses of blacklisted pools
    blacklisted_addrs: HashSet<String>,
    /// Blacklisted fee tiers (e.g., 10000 for 1%)
    blacklisted_tiers: HashSet<u32>,
    /// Blacklisted pair symbols (uppercased)
    blacklisted_pairs: HashSet<String>,
    /// Per-pool minimum liquidity overrides (lowercase hex → u128)
    pool_min_liquidity: std::collections::HashMap<String, u128>,
    /// Per-pool maximum trade size in USD (lowercase hex → f64)
    pool_max_trade_size: std::collections::HashMap<String, f64>,
    /// Per-tier minimum liquidity defaults
    tier_min_liquidity: std::collections::HashMap<u32, u128>,
    /// Default minimum liquidity
    default_min_liquidity: u128,
    /// "strict" or "advisory"
    enforcement: String,
    /// Raw config (retained for logging / debug)
    pub raw: PoolWhitelist,
}

impl WhitelistFilter {
    // ------------------------------------------------------------------
    // Construction
    // ------------------------------------------------------------------

    /// Load from a JSON file path. Returns an error if the file is missing
    /// or unparseable (caller decides whether to fall back to permissive).
    pub fn load(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read whitelist file: {}", path))?;

        let raw: PoolWhitelist = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse whitelist JSON: {}", path))?;

        Ok(Self::from_config(raw))
    }

    /// Build from an already-parsed config.
    pub fn from_config(raw: PoolWhitelist) -> Self {
        // Whitelisted addresses (both "active" V3 pools and "v2_ready" V2 pools)
        let whitelisted_addrs: HashSet<String> = raw
            .whitelist
            .pools
            .iter()
            .filter(|p| p.status == "active" || p.status == "v2_ready")
            .map(|p| normalize_addr(&p.address))
            .collect();

        // Blacklisted addresses
        let blacklisted_addrs: HashSet<String> = raw
            .blacklist
            .pools
            .iter()
            .map(|p| normalize_addr(&p.address))
            .collect();

        // Blacklisted fee tiers
        let blacklisted_tiers: HashSet<u32> =
            raw.blacklist.fee_tiers.iter().map(|t| t.tier).collect();

        // Blacklisted pairs
        let blacklisted_pairs: HashSet<String> = raw
            .blacklist
            .pairs
            .iter()
            .map(|p| p.pair.to_uppercase())
            .collect();

        // Per-pool min liquidity
        let pool_min_liquidity: std::collections::HashMap<String, u128> = raw
            .whitelist
            .pools
            .iter()
            .filter_map(|p| {
                p.min_liquidity
                    .map(|liq| (normalize_addr(&p.address), liq))
            })
            .collect();

        // Per-pool max trade size (USD)
        let pool_max_trade_size: std::collections::HashMap<String, f64> = raw
            .whitelist
            .pools
            .iter()
            .filter_map(|p| {
                p.max_trade_size_usd
                    .map(|size| (normalize_addr(&p.address), size))
            })
            .collect();

        // Per-tier min liquidity
        let mut tier_min_liquidity = std::collections::HashMap::new();
        if let Some(ref thresholds) = raw.config.liquidity_thresholds {
            if let Some(v) = thresholds.v3_100 {
                tier_min_liquidity.insert(100u32, v);
            }
            if let Some(v) = thresholds.v3_500 {
                tier_min_liquidity.insert(500u32, v);
            }
            if let Some(v) = thresholds.v3_3000 {
                tier_min_liquidity.insert(3000u32, v);
            }
            if let Some(v) = thresholds.v3_10000 {
                tier_min_liquidity.insert(10000u32, v);
            }
        }

        let default_min_liquidity = raw.config.default_min_liquidity;
        let enforcement = raw.config.whitelist_enforcement.clone();

        info!(
            "Whitelist loaded: {} active pools, {} blacklisted pools, {} blacklisted tiers, mode={}",
            whitelisted_addrs.len(),
            blacklisted_addrs.len(),
            blacklisted_tiers.len(),
            enforcement,
        );

        Self {
            whitelisted_addrs,
            blacklisted_addrs,
            blacklisted_tiers,
            blacklisted_pairs,
            pool_min_liquidity,
            pool_max_trade_size,
            tier_min_liquidity,
            default_min_liquidity,
            enforcement,
            raw,
        }
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// Main entry point: is this pool allowed to participate in detection?
    pub fn is_pool_allowed(&self, address: &Address, fee_tier: u32, pair: &str) -> bool {
        let addr = format!("{:?}", address).to_lowercase();

        // 1. Fee tier blacklist (fastest check)
        if self.blacklisted_tiers.contains(&fee_tier) {
            debug!("Whitelist: {:?} rejected — fee tier {} blacklisted", address, fee_tier);
            return false;
        }

        // 2. Pool blacklist
        if self.blacklisted_addrs.contains(&addr) {
            debug!("Whitelist: {:?} rejected — pool blacklisted", address);
            return false;
        }

        // 3. Pair blacklist
        if self.blacklisted_pairs.contains(&pair.to_uppercase()) {
            debug!("Whitelist: {:?} rejected — pair {} blacklisted", address, pair);
            return false;
        }

        // 4. Whitelist enforcement
        if self.enforcement == "strict" {
            let allowed = self.whitelisted_addrs.contains(&addr);
            if !allowed {
                debug!("Whitelist: {:?} rejected — not whitelisted (strict mode)", address);
            }
            return allowed;
        }

        // Advisory mode: anything not blacklisted is allowed
        true
    }

    /// Get the minimum liquidity for a specific pool.
    /// Priority: per-pool override > per-tier default > global default.
    pub fn min_liquidity_for(&self, address: &Address, fee_tier: u32) -> u128 {
        let addr = format!("{:?}", address).to_lowercase();

        // Per-pool override
        if let Some(&liq) = self.pool_min_liquidity.get(&addr) {
            return liq;
        }

        // Per-tier default
        if let Some(&liq) = self.tier_min_liquidity.get(&fee_tier) {
            return liq;
        }

        self.default_min_liquidity
    }

    /// Get the maximum trade size (in USD) for a specific pool.
    /// Returns Some(cap) if pool has a per-pool cap, None to use global default.
    pub fn max_trade_size_for(&self, address: &Address) -> Option<f64> {
        let addr = format!("{:?}", address).to_lowercase();
        self.pool_max_trade_size.get(&addr).copied()
    }

    /// Number of active whitelisted pools.
    pub fn active_pool_count(&self) -> usize {
        self.whitelisted_addrs.len()
    }

    /// Is strict enforcement enabled?
    pub fn is_strict(&self) -> bool {
        self.enforcement == "strict"
    }
}

// ---------------------------------------------------------------------------
// Default (empty, advisory — used when no config file exists)
// ---------------------------------------------------------------------------

impl Default for WhitelistFilter {
    fn default() -> Self {
        warn!("Whitelist: no config loaded, using permissive defaults (advisory mode, no blacklists)");
        let raw = PoolWhitelist {
            version: "1.0".to_string(),
            last_updated: String::new(),
            config: WhitelistConfig {
                default_min_liquidity: 1_000_000_000,
                whitelist_enforcement: "advisory".to_string(),
                liquidity_thresholds: None,
            },
            whitelist: WhitelistSection { pools: Vec::new() },
            blacklist: BlacklistSection {
                pools: Vec::new(),
                fee_tiers: vec![BlacklistTier {
                    tier: 10000,
                    reason: "Systematic phantom liquidity on Polygon".to_string(),
                    applies_to: "all_v3".to_string(),
                    date_added: "2026-01-28".to_string(),
                    evidence: Some("phantom_spread_analysis.md".to_string()),
                }],
                pairs: Vec::new(),
            },
            observation: None,
        };
        Self::from_config(raw)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Normalize an address string to lowercase with 0x prefix.
fn normalize_addr(s: &str) -> String {
    let s = s.trim().to_lowercase();
    if s.starts_with("0x") {
        s
    } else {
        format!("0x{}", s)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn test_filter() -> WhitelistFilter {
        let json = r#"{
            "version": "1.0",
            "last_updated": "2026-01-29T00:00:00Z",
            "config": {
                "default_min_liquidity": 1000000000,
                "whitelist_enforcement": "strict"
            },
            "whitelist": {
                "pools": [
                    {
                        "address": "0x45dda9cb7c25131df268515131f647d726f50608",
                        "pair": "WETH/USDC",
                        "dex": "UniswapV3",
                        "fee_tier": 500,
                        "status": "active",
                        "min_liquidity": 5000000000
                    }
                ]
            },
            "blacklist": {
                "pools": [
                    {
                        "address": "0x04537f43f6add7b1b60cab199c7a910024ee0594",
                        "pair": "WETH/USDC",
                        "dex": "UniswapV3",
                        "fee_tier": 100,
                        "reason": "Phantom",
                        "date_added": "2026-01-29"
                    }
                ],
                "fee_tiers": [
                    {
                        "tier": 10000,
                        "reason": "Phantom",
                        "applies_to": "all_v3",
                        "date_added": "2026-01-28"
                    }
                ],
                "pairs": []
            }
        }"#;
        let raw: PoolWhitelist = serde_json::from_str(json).unwrap();
        WhitelistFilter::from_config(raw)
    }

    #[test]
    fn test_tier_blacklist() {
        let f = test_filter();
        let addr = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        assert!(!f.is_pool_allowed(&addr, 10000, "TEST/USDC"));
    }

    #[test]
    fn test_pool_blacklist() {
        let f = test_filter();
        let addr = Address::from_str("0x04537f43f6add7b1b60cab199c7a910024ee0594").unwrap();
        assert!(!f.is_pool_allowed(&addr, 100, "WETH/USDC"));
    }

    #[test]
    fn test_whitelisted_pool_allowed() {
        let f = test_filter();
        let addr = Address::from_str("0x45dda9cb7c25131df268515131f647d726f50608").unwrap();
        assert!(f.is_pool_allowed(&addr, 500, "WETH/USDC"));
    }

    #[test]
    fn test_strict_rejects_unknown() {
        let f = test_filter();
        let addr = Address::from_str("0x0000000000000000000000000000000000000099").unwrap();
        assert!(!f.is_pool_allowed(&addr, 500, "FOO/USDC"));
    }

    #[test]
    fn test_min_liquidity_override() {
        let f = test_filter();
        let addr = Address::from_str("0x45dda9cb7c25131df268515131f647d726f50608").unwrap();
        assert_eq!(f.min_liquidity_for(&addr, 500), 5_000_000_000);
    }

    #[test]
    fn test_min_liquidity_default() {
        let f = test_filter();
        let addr = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        assert_eq!(f.min_liquidity_for(&addr, 500), 1_000_000_000);
    }

    fn test_filter_with_trade_caps() -> WhitelistFilter {
        let json = r#"{
            "version": "1.7",
            "last_updated": "2026-02-02T00:00:00Z",
            "config": {
                "default_min_liquidity": 1000000000,
                "whitelist_enforcement": "strict"
            },
            "whitelist": {
                "pools": [
                    {
                        "address": "0x45dda9cb7c25131df268515131f647d726f50608",
                        "pair": "WETH/USDC",
                        "dex": "UniswapV3",
                        "fee_tier": 500,
                        "status": "active",
                        "min_liquidity": 5000000000
                    },
                    {
                        "address": "0x74d3c85df4dbd03c7c12f7649faa6457610e7604",
                        "pair": "UNI/USDC",
                        "dex": "UniswapV3",
                        "fee_tier": 3000,
                        "status": "active",
                        "min_liquidity": 3000000000,
                        "max_trade_size_usd": 200.0
                    }
                ]
            },
            "blacklist": {
                "pools": [],
                "fee_tiers": [],
                "pairs": []
            }
        }"#;
        let raw: PoolWhitelist = serde_json::from_str(json).unwrap();
        WhitelistFilter::from_config(raw)
    }

    #[test]
    fn test_max_trade_size_with_cap() {
        let f = test_filter_with_trade_caps();
        let addr = Address::from_str("0x74d3c85df4dbd03c7c12f7649faa6457610e7604").unwrap();
        assert_eq!(f.max_trade_size_for(&addr), Some(200.0));
    }

    #[test]
    fn test_max_trade_size_without_cap() {
        let f = test_filter_with_trade_caps();
        let addr = Address::from_str("0x45dda9cb7c25131df268515131f647d726f50608").unwrap();
        assert_eq!(f.max_trade_size_for(&addr), None);
    }

    #[test]
    fn test_max_trade_size_unknown_pool() {
        let f = test_filter_with_trade_caps();
        let addr = Address::from_str("0x0000000000000000000000000000000000000099").unwrap();
        assert_eq!(f.max_trade_size_for(&addr), None);
    }
}
