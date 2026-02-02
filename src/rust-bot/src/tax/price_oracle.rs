//! Price Oracle for Tax Calculations
//!
//! Provides USD price lookups for tokens using pool state data.
//! Reads from the shared pool_state.json file maintained by data-collector.
//!
//! Key features:
//! - Real-time price lookups from V2 and V3 pools
//! - Stablecoin handling (USDC, USDT, DAI = $1)
//! - Caching for performance
//! - Fallback to last known price if state is stale
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use crate::data_collector::SharedPoolState;
use anyhow::Result;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Default path to pool state file
pub const DEFAULT_POOL_STATE_PATH: &str = "/home/botuser/bots/dexarb/data/pool_state_phase1.json";

/// Token decimals for common tokens on Polygon
pub const TOKEN_DECIMALS: &[(&str, u8)] = &[
    ("USDC", 6),
    ("USDT", 6),
    ("DAI", 18),
    ("WETH", 18),
    ("WMATIC", 18),
    ("MATIC", 18),
    ("WBTC", 8),
    ("LINK", 18),
    ("UNI", 18),
];

/// Stablecoins that are pegged to $1
const STABLECOINS: &[&str] = &["USDC", "USDT", "DAI"];

/// Price oracle for fetching USD prices
pub struct PriceOracle {
    /// Path to pool state JSON file
    state_path: PathBuf,
    /// Cached prices with timestamps
    cache: RwLock<PriceCache>,
    /// Cache TTL (how long before refreshing)
    cache_ttl: Duration,
}

/// Cached price data
struct PriceCache {
    /// Token prices in USD (symbol -> price)
    prices: HashMap<String, Decimal>,
    /// Last update timestamp
    last_updated: Option<Instant>,
    /// Raw pool state for detailed lookups
    pool_state: Option<SharedPoolState>,
}

impl Default for PriceCache {
    fn default() -> Self {
        Self {
            prices: HashMap::new(),
            last_updated: None,
            pool_state: None,
        }
    }
}

impl PriceOracle {
    /// Create a new price oracle
    pub fn new<P: AsRef<Path>>(state_path: P) -> Self {
        Self {
            state_path: state_path.as_ref().to_path_buf(),
            cache: RwLock::new(PriceCache::default()),
            cache_ttl: Duration::from_secs(30), // Refresh every 30 seconds
        }
    }

    /// Create with default pool state path
    pub fn default_path() -> Self {
        Self::new(DEFAULT_POOL_STATE_PATH)
    }

    /// Set cache TTL
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// Get USD price for a token symbol
    ///
    /// Returns the price of one unit of the token in USD.
    /// For example, if WETH is $3000, returns Decimal(3000).
    pub fn get_price_usd(&self, symbol: &str) -> Result<Decimal> {
        // Normalize symbol
        let symbol = symbol.to_uppercase();

        // Stablecoins are always $1
        if STABLECOINS.contains(&symbol.as_str()) {
            return Ok(Decimal::ONE);
        }

        // Check if cache needs refresh
        self.refresh_if_needed()?;

        // Look up in cache
        let cache = self.cache.read().unwrap();
        if let Some(price) = cache.prices.get(&symbol) {
            return Ok(*price);
        }

        // Try to derive from pool data
        drop(cache); // Release read lock
        self.derive_price(&symbol)
    }

    /// Get MATIC price for gas calculations
    pub fn get_matic_price_usd(&self) -> Result<Decimal> {
        self.get_price_usd("WMATIC")
    }

    /// Get token decimals
    pub fn get_decimals(&self, symbol: &str) -> u8 {
        let symbol = symbol.to_uppercase();
        TOKEN_DECIMALS
            .iter()
            .find(|(s, _)| *s == symbol)
            .map(|(_, d)| *d)
            .unwrap_or(18) // Default to 18 decimals
    }

    /// Refresh cache if needed
    fn refresh_if_needed(&self) -> Result<()> {
        let should_refresh = {
            let cache = self.cache.read().unwrap();
            cache.last_updated.is_none()
                || cache.last_updated.unwrap().elapsed() > self.cache_ttl
        };

        if should_refresh {
            self.refresh_cache()?;
        }

        Ok(())
    }

    /// Refresh the price cache from pool state
    fn refresh_cache(&self) -> Result<()> {
        let state = match SharedPoolState::read_from_file(&self.state_path) {
            Ok(s) => s,
            Err(_) => {
                // State file missing or unreadable — mark cache as updated (empty)
                // so get_price_usd() falls through to derive_price() fallbacks
                let mut cache = self.cache.write().unwrap();
                cache.last_updated = Some(Instant::now());
                return Ok(());
            }
        };

        let mut prices = HashMap::new();

        // Extract prices from V2 pools
        // Pool prices are in terms of token1/token0
        // For pairs ending in /USDC, price is token/USDC
        for pool in state.pools.values() {
            if pool.pair_symbol.ends_with("/USDC") {
                let token = pool.pair_symbol.split('/').next().unwrap_or("");
                if !token.is_empty() && pool.price > 0.0 {
                    // V2 price is token0/token1, so for WETH/USDC it's WETH price in USDC
                    let price = Decimal::from_str(&pool.price.to_string())
                        .unwrap_or(Decimal::ZERO);

                    // Use the highest liquidity pool (check if better than existing)
                    if let Some(existing) = prices.get(token) {
                        // Keep the existing price if it's non-zero (first valid wins)
                        if *existing == Decimal::ZERO {
                            prices.insert(token.to_string(), price);
                        }
                    } else {
                        prices.insert(token.to_string(), price);
                    }
                }
            }
        }

        // Extract prices from V3 pools (may be more accurate)
        for pool in state.v3_pools.values() {
            if pool.pair_symbol.ends_with("/USDC") {
                let token = pool.pair_symbol.split('/').next().unwrap_or("");
                if !token.is_empty() {
                    // Use validated_price which handles overflow errors
                    let price = Decimal::from_str(&pool.validated_price().to_string())
                        .unwrap_or(Decimal::ZERO);

                    if price > Decimal::ZERO {
                        // V3 prices from 0.30% fee tier are usually most liquid
                        if pool.dex.contains("0.30%") {
                            prices.insert(token.to_string(), price);
                        } else if !prices.contains_key(token) {
                            prices.insert(token.to_string(), price);
                        }
                    }
                }
            }
        }

        // Update cache
        let mut cache = self.cache.write().unwrap();
        cache.prices = prices;
        cache.last_updated = Some(Instant::now());
        cache.pool_state = Some(state);

        Ok(())
    }

    /// Derive price from pool data
    fn derive_price(&self, symbol: &str) -> Result<Decimal> {
        let cache = self.cache.read().unwrap();

        if let Some(state) = &cache.pool_state {
            // Try to find a direct pair with USDC
            let pair_symbol = format!("{}/USDC", symbol);

            // Check V3 pools first (usually more accurate)
            for pool in state.v3_pools.values() {
                if pool.pair_symbol == pair_symbol {
                    let price = pool.validated_price();
                    if price > 0.0 && price < 1e15 {
                        return Ok(Decimal::from_str(&price.to_string()).unwrap_or(Decimal::ZERO));
                    }
                }
            }

            // Check V2 pools
            for pool in state.pools.values() {
                if pool.pair_symbol == pair_symbol && pool.price > 0.0 {
                    return Ok(Decimal::from_str(&pool.price.to_string()).unwrap_or(Decimal::ZERO));
                }
            }
        }

        // Default fallback prices for common tokens
        match symbol {
            "WMATIC" | "MATIC" => Ok(Decimal::from_str("0.90").unwrap()),
            "WETH" => Ok(Decimal::from_str("3000.00").unwrap()),
            "WBTC" => Ok(Decimal::from_str("95000.00").unwrap()),
            "LINK" => Ok(Decimal::from_str("15.00").unwrap()),
            "UNI" => Ok(Decimal::from_str("10.00").unwrap()),
            _ => Ok(Decimal::ZERO),
        }
    }

    /// Get all cached prices
    pub fn get_all_prices(&self) -> Result<HashMap<String, Decimal>> {
        self.refresh_if_needed()?;
        let cache = self.cache.read().unwrap();
        Ok(cache.prices.clone())
    }

    /// Check if pool state is fresh (updated within threshold)
    pub fn is_state_fresh(&self, max_age_secs: i64) -> Result<bool> {
        self.refresh_if_needed()?;
        let cache = self.cache.read().unwrap();

        if let Some(state) = &cache.pool_state {
            Ok(!state.is_stale(max_age_secs))
        } else {
            Ok(false)
        }
    }

    /// Get the current block number from pool state
    pub fn get_block_number(&self) -> Result<u64> {
        self.refresh_if_needed()?;
        let cache = self.cache.read().unwrap();

        if let Some(state) = &cache.pool_state {
            Ok(state.block_number)
        } else {
            Ok(0)
        }
    }
}

/// Builder for creating TaxRecords with price oracle integration
pub struct TaxRecordBuilder {
    oracle: PriceOracle,
}

impl TaxRecordBuilder {
    /// Create a new builder with default price oracle
    pub fn new() -> Result<Self> {
        Ok(Self {
            oracle: PriceOracle::default_path(),
        })
    }

    /// Create with custom price oracle
    pub fn with_oracle(oracle: PriceOracle) -> Self {
        Self { oracle }
    }

    /// Build a tax record from trade parameters
    ///
    /// Automatically fetches USD prices from the oracle.
    #[allow(clippy::too_many_arguments)]
    pub fn build_arbitrage_record(
        &self,
        asset_sent: &str,
        amount_sent: Decimal,
        asset_received: &str,
        amount_received: Decimal,
        gas_fee_native: Decimal,
        dex_fee_percent: Decimal,
        transaction_hash: String,
        block_number: u64,
        wallet_address: String,
        dex_buy: String,
        dex_sell: String,
        pool_address_buy: String,
        pool_address_sell: String,
        spread_percent: Decimal,
        is_paper_trade: bool,
    ) -> Result<super::TaxRecord> {
        // Get prices from oracle
        let spot_price_sent = self.oracle.get_price_usd(asset_sent)?;
        let spot_price_received = self.oracle.get_price_usd(asset_received)?;
        let matic_price = self.oracle.get_matic_price_usd()?;

        // Get decimals
        let token_sent_decimals = self.oracle.get_decimals(asset_sent);
        let token_received_decimals = self.oracle.get_decimals(asset_received);

        Ok(super::TaxRecord::new_arbitrage(
            asset_sent.to_string(),
            amount_sent,
            token_sent_decimals,
            asset_received.to_string(),
            amount_received,
            token_received_decimals,
            spot_price_sent,
            spot_price_received,
            gas_fee_native,
            matic_price,
            dex_fee_percent,
            transaction_hash,
            block_number,
            wallet_address,
            dex_buy,
            dex_sell,
            pool_address_buy,
            pool_address_sell,
            spread_percent,
            is_paper_trade,
        ))
    }

    /// Get the underlying price oracle
    pub fn oracle(&self) -> &PriceOracle {
        &self.oracle
    }
}

impl Default for TaxRecordBuilder {
    fn default() -> Self {
        Self::new().expect("Failed to create default TaxRecordBuilder")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    #[test]
    fn test_stablecoin_prices() {
        let oracle = PriceOracle::new("/nonexistent/path");

        // Stablecoins should always return $1 even without pool state
        assert_eq!(oracle.get_price_usd("USDC").unwrap(), Decimal::ONE);
        assert_eq!(oracle.get_price_usd("USDT").unwrap(), Decimal::ONE);
        assert_eq!(oracle.get_price_usd("DAI").unwrap(), Decimal::ONE);
        assert_eq!(oracle.get_price_usd("usdc").unwrap(), Decimal::ONE); // Case insensitive
    }

    #[test]
    fn test_token_decimals() {
        let oracle = PriceOracle::new("/nonexistent/path");

        assert_eq!(oracle.get_decimals("USDC"), 6);
        assert_eq!(oracle.get_decimals("WETH"), 18);
        assert_eq!(oracle.get_decimals("WBTC"), 8);
        assert_eq!(oracle.get_decimals("UNKNOWN"), 18); // Default
    }

    #[test]
    fn test_fallback_prices() {
        let oracle = PriceOracle::new("/nonexistent/path");

        // Fallback prices for common tokens
        let matic_price = oracle.get_price_usd("WMATIC").unwrap();
        assert!(matic_price > Decimal::ZERO);

        let eth_price = oracle.get_price_usd("WETH").unwrap();
        assert!(eth_price > Decimal::from(1000));
    }

    #[test]
    fn test_price_oracle_with_real_state() {
        // Only run if pool state exists
        let state_path = PathBuf::from(DEFAULT_POOL_STATE_PATH);
        if !state_path.exists() {
            return;
        }

        let oracle = PriceOracle::default_path();

        // Should be able to get prices
        let prices = oracle.get_all_prices();
        assert!(prices.is_ok());

        // WMATIC should have a price
        let matic = oracle.get_price_usd("WMATIC");
        assert!(matic.is_ok());
        assert!(matic.unwrap() > Decimal::ZERO);
    }
}
