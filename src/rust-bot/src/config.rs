//! Configuration management
//! Load settings from .env file

use crate::types::TradingPairConfig;
use anyhow::{Context, Result};

// Re-export BotConfig for external access
pub use crate::types::BotConfig;
use ethers::types::Address;
use std::str::FromStr;

pub fn load_config() -> Result<BotConfig> {
    dotenv::dotenv().ok();

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

    Ok(BotConfig {
        rpc_url: std::env::var("RPC_URL")?,
        chain_id: std::env::var("CHAIN_ID")?.parse()?,
        private_key: std::env::var("PRIVATE_KEY")?,

        min_profit_usd: std::env::var("MIN_PROFIT_USD")?.parse()?,
        max_trade_size_usd: std::env::var("MAX_TRADE_SIZE_USD")?.parse()?,
        max_slippage_percent: std::env::var("MAX_SLIPPAGE_PERCENT")?.parse()?,

        uniswap_router: Address::from_str(&std::env::var("UNISWAP_ROUTER")?)?,
        sushiswap_router: Address::from_str(&std::env::var("SUSHISWAP_ROUTER")?)?,
        uniswap_factory: Address::from_str(&std::env::var("UNISWAP_FACTORY")?)?,
        sushiswap_factory: Address::from_str(&std::env::var("SUSHISWAP_FACTORY")?)?,

        pairs,

        poll_interval_ms: std::env::var("POLL_INTERVAL_MS")?.parse()?,
        max_gas_price_gwei: std::env::var("MAX_GAS_PRICE_GWEI")?.parse()?,
    })
}
