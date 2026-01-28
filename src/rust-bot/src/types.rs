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
    Uniswap,       // Actually Quickswap on Polygon (Uniswap V2 fork)
    Sushiswap,
    Quickswap,     // Alias for clarity
    Apeswap,       // Phase 1 expansion - third V2 DEX
    UniswapV3_005, // Phase 2 - Uniswap V3 0.05% fee tier
    UniswapV3_030, // Phase 2 - Uniswap V3 0.30% fee tier
    UniswapV3_100, // Phase 2 - Uniswap V3 1.00% fee tier
}

impl DexType {
    /// Returns true if this is a V3 DEX
    pub fn is_v3(&self) -> bool {
        matches!(self, DexType::UniswapV3_005 | DexType::UniswapV3_030 | DexType::UniswapV3_100)
    }

    /// Returns the fee in basis points for V3 pools
    pub fn v3_fee_bps(&self) -> Option<u32> {
        match self {
            DexType::UniswapV3_005 => Some(5),    // 0.05% = 500 / 10000
            DexType::UniswapV3_030 => Some(30),   // 0.30% = 3000 / 10000
            DexType::UniswapV3_100 => Some(100),  // 1.00% = 10000 / 10000
            _ => None,
        }
    }

    /// Get V3 fee tier (for factory queries)
    pub fn v3_fee_tier(&self) -> Option<u32> {
        match self {
            DexType::UniswapV3_005 => Some(500),
            DexType::UniswapV3_030 => Some(3000),
            DexType::UniswapV3_100 => Some(10000),
            _ => None,
        }
    }
}

impl fmt::Display for DexType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DexType::Uniswap => write!(f, "Uniswap"),
            DexType::Sushiswap => write!(f, "Sushiswap"),
            DexType::Quickswap => write!(f, "Quickswap"),
            DexType::Apeswap => write!(f, "Apeswap"),
            DexType::UniswapV3_005 => write!(f, "UniswapV3_0.05%"),
            DexType::UniswapV3_030 => write!(f, "UniswapV3_0.30%"),
            DexType::UniswapV3_100 => write!(f, "UniswapV3_1.00%"),
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

/// Uniswap V3 pool state (Phase 2)
/// V3 uses concentrated liquidity with tick-based pricing
#[derive(Debug, Clone)]
pub struct V3PoolState {
    pub address: Address,
    pub dex: DexType,
    pub pair: TradingPair,
    /// sqrtPriceX96 - the current sqrt(price) as a Q64.96 fixed point number
    pub sqrt_price_x96: U256,
    /// Current tick
    pub tick: i32,
    /// Fee tier (500 = 0.05%, 3000 = 0.30%, 10000 = 1.00%)
    pub fee: u32,
    /// Current in-range liquidity
    pub liquidity: u128,
    /// Token decimals for price calculation
    pub token0_decimals: u8,
    pub token1_decimals: u8,
    /// Last updated block
    pub last_updated: u64,
}

impl V3PoolState {
    /// Calculate price of token0 in terms of token1
    ///
    /// IMPORTANT: Always uses tick-based calculation for reliability.
    /// The sqrtPriceX96 approach is prone to overflow/precision issues with f64.
    /// Tick-based calculation: price = 1.0001^tick is always accurate.
    ///
    /// Price represents: how many token1 for 1 token0 (token1/token0)
    pub fn price(&self) -> f64 {
        // Always use tick-based calculation for reliability
        // The sqrtPriceX96 approach has precision issues with large values
        // even when they fit in u128, due to f64 limitations when squaring
        self.price_from_tick()
    }

    /// Calculate price from tick (always works, used as fallback)
    /// Price = 1.0001^tick * 10^(decimals0 - decimals1)
    pub fn price_from_tick(&self) -> f64 {
        let base: f64 = 1.0001;
        let price = base.powi(self.tick);

        // Adjust for decimals
        let decimal_adjustment =
            10_f64.powi(self.token0_decimals as i32 - self.token1_decimals as i32);

        price * decimal_adjustment
    }

    /// Get price normalized to match pair symbol direction
    ///
    /// V3 pools always have token0 < token1 by address, but the pair symbol
    /// might represent the opposite direction. This method checks the symbol
    /// and inverts the price if needed.
    ///
    /// Example: WETH/USDC pair where USDC < WETH by address
    /// - V3 token0 = USDC, token1 = WETH
    /// - V3 price() returns WETH per USDC (e.g., 0.00042)
    /// - But symbol suggests USDC per WETH (e.g., ~2400)
    /// - This method returns 1/price to match expected direction
    pub fn price_normalized(&self, expected_token0_last_bytes: &str) -> f64 {
        let raw_price = self.price();
        if raw_price == 0.0 {
            return 0.0;
        }

        // Check if actual token0 matches expected direction
        // Compare last few hex chars of token0 address with expected
        let actual_token0_hex = format!("{:?}", self.pair.token0).to_lowercase();

        if actual_token0_hex.ends_with(expected_token0_last_bytes) {
            raw_price
        } else {
            // Token order is inverted relative to symbol, so invert price
            1.0 / raw_price
        }
    }

    /// Get the fee as a percentage
    pub fn fee_percent(&self) -> f64 {
        self.fee as f64 / 10000.0
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

    // ApeSwap addresses (Phase 1 expansion - optional)
    pub apeswap_router: Option<Address>,
    pub apeswap_factory: Option<Address>,

    // Uniswap V3 addresses (Phase 2 expansion - optional)
    pub uniswap_v3_factory: Option<Address>,
    pub uniswap_v3_router: Option<Address>,
    pub uniswap_v3_quoter: Option<Address>,

    // Trading pairs to monitor
    pub pairs: Vec<TradingPairConfig>,

    // Performance
    pub poll_interval_ms: u64,
    pub max_gas_price_gwei: u64,
}
