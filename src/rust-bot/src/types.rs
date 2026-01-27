// Core data structures for Phase 1
// Expand these based on the implementation plan

use ethers::types::{Address, U256};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Trading pair configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPair {
    pub token0: Address,
    pub token1: Address,
    pub symbol: String,
}

impl TradingPair {
    pub fn new(token0: Address, token1: Address, symbol: String) -> Self {
        Self {
            token0,
            token1,
            symbol,
        }
    }
}

/// DEX types we support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DexType {
    Uniswap,
    Sushiswap,
    Quickswap, // Phase 2
}

impl fmt::Display for DexType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DexType::Uniswap => write!(f, "Uniswap"),
            DexType::Sushiswap => write!(f, "Sushiswap"),
            DexType::Quickswap => write!(f, "Quickswap"),
        }
    }
}

/// DEX pool state
#[derive(Debug, Clone)]
pub struct PoolState {
    pub address: Address,
    pub dex: DexType,
    pub pair: TradingPair,
    pub reserve0: U256,
    pub reserve1: U256,
    pub last_updated: u64, // block number
}

impl PoolState {
    /// Calculate price of token0 in terms of token1
    pub fn price(&self) -> f64 {
        let reserve0_f = self.reserve0.as_u128() as f64;
        let reserve1_f = self.reserve1.as_u128() as f64;

        if reserve0_f == 0.0 {
            return 0.0;
        }

        reserve1_f / reserve0_f
    }

    /// Calculate output amount for given input (constant product formula)
    pub fn get_amount_out(&self, amount_in: U256, token_in: Address) -> U256 {
        let (reserve_in, reserve_out) = if token_in == self.pair.token0 {
            (self.reserve0, self.reserve1)
        } else {
            (self.reserve1, self.reserve0)
        };

        // x * y = k formula with 0.3% fee
        let amount_in_with_fee = amount_in * U256::from(997);
        let numerator = amount_in_with_fee * reserve_out;
        let denominator = (reserve_in * U256::from(1000)) + amount_in_with_fee;

        numerator / denominator
    }
}

/// Arbitrage opportunity detected
#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub pair: TradingPair,
    pub buy_dex: DexType,
    pub sell_dex: DexType,
    pub buy_price: f64,
    pub sell_price: f64,
    pub spread_percent: f64,
    pub estimated_profit: f64, // in USD
    pub trade_size: U256,      // in wei
    pub timestamp: u64,
}

impl ArbitrageOpportunity {
    pub fn new(
        pair: TradingPair,
        buy_dex: DexType,
        sell_dex: DexType,
        buy_price: f64,
        sell_price: f64,
        trade_size: U256,
    ) -> Self {
        let spread_percent = ((sell_price - buy_price) / buy_price) * 100.0;

        Self {
            pair,
            buy_dex,
            sell_dex,
            buy_price,
            sell_price,
            spread_percent,
            estimated_profit: 0.0, // Calculate separately
            trade_size,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    pub fn is_profitable(&self, min_profit_usd: f64) -> bool {
        self.estimated_profit > min_profit_usd
    }
}

/// Trade execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeResult {
    pub opportunity: String,
    pub tx_hash: Option<String>,
    pub success: bool,
    pub profit_usd: f64,
    pub gas_cost_usd: f64,
    pub net_profit_usd: f64,
    pub execution_time_ms: u64,
    pub error: Option<String>,
}

/// Trading pair configuration (from env)
#[derive(Debug, Clone, Deserialize)]
pub struct TradingPairConfig {
    pub token0: String, // Address as string
    pub token1: String,
    pub symbol: String,
}

/// Bot configuration
#[derive(Debug, Clone, Deserialize)]
pub struct BotConfig {
    // Network
    pub rpc_url: String,
    pub chain_id: u64,

    // Wallet
    pub private_key: String,

    // Trading parameters
    pub min_profit_usd: f64,
    pub max_trade_size_usd: f64,
    pub max_slippage_percent: f64,

    // DEX addresses (Polygon mainnet)
    pub uniswap_router: Address,
    pub sushiswap_router: Address,
    pub uniswap_factory: Address,
    pub sushiswap_factory: Address,

    // Trading pairs to monitor
    pub pairs: Vec<TradingPairConfig>,

    // Performance
    pub poll_interval_ms: u64,
    pub max_gas_price_gwei: u64,
}
