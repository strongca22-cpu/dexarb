//! Configuration management
//! Load settings from .env file
//!
//! Two entry points:
//! - load_config(): loads from .env (data collector, dev/paper workflows)
//! - load_config_from_file(): loads from a specific env file (live bot uses .env.live)
//!
//! Modified: 2026-01-29 - Added load_config_from_file() for live/dev config separation
//! Modified: 2026-01-31 - Multi-chain: chain_name, quote_token_address, estimated_gas_cost_usd

use crate::types::TradingPairConfig;
use anyhow::{Context, Result};

// Re-export BotConfig for external access
pub use crate::types::BotConfig;
use alloy::primitives::Address;
use std::str::FromStr;

/// Load config from default .env file (used by data collector, dev tools)
pub fn load_config() -> Result<BotConfig> {
    dotenv::dotenv().ok();
    load_config_inner()
}

/// Load config from a specific env file (used by live bot with .env.live)
pub fn load_config_from_file(filename: &str) -> Result<BotConfig> {
    dotenv::from_filename(filename).ok();
    load_config_inner()
}

fn load_config_inner() -> Result<BotConfig> {
    let trading_pairs_str =
        std::env::var("TRADING_PAIRS").context("TRADING_PAIRS not set")?;

    let pairs: Vec<TradingPairConfig> = trading_pairs_str
        .split(',')
        .map(|pair_str| {
            let parts: Vec<&str> = pair_str.trim().split(':').collect();
            if parts.len() != 3 {
                panic!("Invalid trading pair format: {}", pair_str);
            }

            TradingPairConfig {
                token0: parts[0].to_string(),
                token1: parts[1].to_string(),
                symbol: parts[2].to_string(),
            }
        })
        .collect();

    // Load optional ApeSwap addresses
    let apeswap_router = std::env::var("APESWAP_ROUTER")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());
    let apeswap_factory = std::env::var("APESWAP_FACTORY")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());

    // Load optional Uniswap V3 addresses (Phase 2)
    let uniswap_v3_factory = std::env::var("UNISWAP_V3_FACTORY")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());
    let uniswap_v3_router = std::env::var("UNISWAP_V3_ROUTER")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());
    let uniswap_v3_quoter = std::env::var("UNISWAP_V3_QUOTER")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());

    // Load optional SushiSwap V3 addresses (cross-DEX arb)
    let sushiswap_v3_factory = std::env::var("SUSHISWAP_V3_FACTORY")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());
    let sushiswap_v3_router = std::env::var("SUSHISWAP_V3_ROUTER")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());
    let sushiswap_v3_quoter = std::env::var("SUSHISWAP_V3_QUOTER")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());

    // Load optional QuickSwap V3 (Algebra) addresses (cross-DEX arb)
    let quickswap_v3_factory = std::env::var("QUICKSWAP_V3_FACTORY")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());
    let quickswap_v3_router = std::env::var("QUICKSWAP_V3_ROUTER")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());
    let quickswap_v3_quoter = std::env::var("QUICKSWAP_V3_QUOTER")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());

    // Multi-chain fields with backwards-compatible defaults (Polygon)
    let chain_name = std::env::var("CHAIN_NAME")
        .unwrap_or_else(|_| "polygon".to_string());

    // Default: Polygon USDC.e (0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174)
    let quote_token_address = std::env::var("QUOTE_TOKEN_ADDRESS")
        .ok()
        .and_then(|s| Address::from_str(&s).ok())
        .unwrap_or_else(|| Address::from_str("0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174").unwrap());

    let estimated_gas_cost_usd: f64 = std::env::var("ESTIMATED_GAS_COST_USD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.05);

    // Secondary quote token (native USDC on Polygon)
    let quote_token_address_native = std::env::var("QUOTE_TOKEN_ADDRESS_NATIVE")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());

    // Tertiary quote token (USDT on Polygon — 6 decimals, same as USDC)
    let quote_token_address_usdt = std::env::var("QUOTE_TOKEN_ADDRESS_USDT")
        .ok()
        .and_then(|s| Address::from_str(&s).ok());

    // Native token price — must resolve before chain_name is moved into BotConfig
    let native_token_price_usd: f64 = std::env::var("NATIVE_TOKEN_PRICE_USD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| {
            match chain_name.as_str() {
                "polygon" => 0.50,
                "base" | "ethereum" => 3300.0,
                _ => 1.0,
            }
        });

    Ok(BotConfig {
        rpc_url: std::env::var("RPC_URL")?,
        chain_id: std::env::var("CHAIN_ID")?.parse()?,
        chain_name,
        quote_token_address,
        estimated_gas_cost_usd,
        private_key: std::env::var("PRIVATE_KEY")?,

        min_profit_usd: std::env::var("MIN_PROFIT_USD")?.parse()?,
        max_trade_size_usd: std::env::var("MAX_TRADE_SIZE_USD")?.parse()?,
        max_slippage_percent: std::env::var("MAX_SLIPPAGE_PERCENT")?.parse()?,

        uniswap_router: Address::from_str(&std::env::var("UNISWAP_ROUTER")?)?,
        sushiswap_router: Address::from_str(&std::env::var("SUSHISWAP_ROUTER")?)?,
        uniswap_factory: Address::from_str(&std::env::var("UNISWAP_FACTORY")?)?,
        sushiswap_factory: Address::from_str(&std::env::var("SUSHISWAP_FACTORY")?)?,

        apeswap_router,
        apeswap_factory,

        uniswap_v3_factory,
        uniswap_v3_router,
        uniswap_v3_quoter,

        sushiswap_v3_factory,
        sushiswap_v3_router,
        sushiswap_v3_quoter,

        quickswap_v3_factory,
        quickswap_v3_router,
        quickswap_v3_quoter,

        // Base uses QuoterV2 for Uniswap V3; Polygon uses QuoterV1
        uniswap_v3_quoter_is_v2: std::env::var("UNISWAP_V3_QUOTER_IS_V2")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false),

        pairs,

        poll_interval_ms: std::env::var("POLL_INTERVAL_MS")?.parse()?,
        // Gas cap no longer enforced in executor — kept for config compatibility
        max_gas_price_gwei: std::env::var("MAX_GAS_PRICE_GWEI")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),

        // Tax logging configuration
        tax_log_dir: std::env::var("TAX_LOG_DIR").ok(),
        tax_log_enabled: std::env::var("TAX_LOG_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true), // Default to enabled for safety

        // Live trading mode (default to false for safety)
        live_mode: std::env::var("LIVE_MODE")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false),

        // Shared pool state file (data collector writes, live bot reads)
        pool_state_file: std::env::var("POOL_STATE_FILE").ok(),

        // Pool whitelist/blacklist config (Phase 1.1)
        whitelist_file: std::env::var("WHITELIST_FILE").ok(),

        // Historical price logging (research)
        price_log_enabled: std::env::var("PRICE_LOG_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false),
        price_log_dir: std::env::var("PRICE_LOG_DIR").ok(),

        // Atomic arbitrage executor contract
        arb_executor_address: std::env::var("ARB_EXECUTOR_ADDRESS")
            .ok()
            .and_then(|s| Address::from_str(&s).ok()),

        // Skip Multicall3 batch pre-screen (default false — existing behavior preserved)
        skip_multicall_prescreen: std::env::var("SKIP_MULTICALL_PRESCREEN")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false),

        // Route cooldown: suppress failed routes for N blocks (default 10, 0 = disabled)
        route_cooldown_blocks: std::env::var("ROUTE_COOLDOWN_BLOCKS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10),

        // Private RPC for tx submission (Polygon Fastlane — optional)
        private_rpc_url: std::env::var("PRIVATE_RPC_URL").ok(),

        // A4 Mempool Monitor mode (default: off)
        mempool_monitor_mode: std::env::var("MEMPOOL_MONITOR")
            .unwrap_or_else(|_| "off".to_string()),

        // A4 Phase 3: Mempool execution parameters
        mempool_min_profit_usd: std::env::var("MEMPOOL_MIN_PROFIT_USD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.05),
        mempool_gas_limit: std::env::var("MEMPOOL_GAS_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500_000),
        mempool_min_priority_gwei: std::env::var("MEMPOOL_MIN_PRIORITY_GWEI")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000),
        mempool_gas_profit_cap: std::env::var("MEMPOOL_GAS_PROFIT_CAP")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.50),

        native_token_price_usd,
        quote_token_address_native,

        // Separate WS RPC URL for mempool monitor (optional).
        // When RPC_URL is IPC, mempool monitor needs WS. Falls back to RPC_URL.
        ws_rpc_url: std::env::var("WS_RPC_URL").ok(),

        quote_token_address_usdt,
    })
}
