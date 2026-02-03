// Core data structures for Phase 1
// Expand these based on the implementation plan

use alloy::primitives::{Address, U256};
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

/// Fee sentinel for V2 routers in ArbExecutor.sol.
/// type(uint24).max = 16777215. Signals swapExactTokensForTokens instead of V3 exactInputSingle.
/// fee=0 → Algebra (QuickSwap V3), fee=1..65535 → standard V3, fee=16777215 → V2.
pub const V2_FEE_SENTINEL: u32 = 16_777_215;

/// DEX types we support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DexType {
    Uniswap,       // Actually Quickswap on Polygon (Uniswap V2 fork)
    Sushiswap,
    Quickswap,     // Alias for clarity
    Apeswap,       // Phase 1 expansion - third V2 DEX
    UniswapV3_001, // Uniswap V3 0.01% fee tier (stablecoins)
    UniswapV3_005, // Phase 2 - Uniswap V3 0.05% fee tier
    UniswapV3_030, // Phase 2 - Uniswap V3 0.30% fee tier
    UniswapV3_100, // Phase 2 - Uniswap V3 1.00% fee tier
    SushiV3_001,   // SushiSwap V3 0.01% fee tier (cross-DEX arb)
    SushiV3_005,   // SushiSwap V3 0.05% fee tier (cross-DEX arb)
    SushiV3_030,   // SushiSwap V3 0.30% fee tier (cross-DEX arb)
    QuickswapV3,   // QuickSwap V3 (Algebra) — dynamic fees, single pool per pair
    QuickSwapV2,   // QuickSwap V2 (constant product, 0.30% fee) — V2↔V3 cross-protocol arb
    SushiSwapV2,   // SushiSwap V2 (constant product, 0.30% fee) — V2↔V3 cross-protocol arb
}

impl DexType {
    /// Returns true if this is a V3 DEX (Uniswap, SushiSwap, or QuickSwap V3)
    pub fn is_v3(&self) -> bool {
        matches!(self,
            DexType::UniswapV3_001 | DexType::UniswapV3_005 | DexType::UniswapV3_030 | DexType::UniswapV3_100 |
            DexType::SushiV3_001 | DexType::SushiV3_005 | DexType::SushiV3_030 |
            DexType::QuickswapV3
        )
    }

    /// Returns true if this is a V2 DEX (constant product AMM, 0.30% fee)
    pub fn is_v2(&self) -> bool {
        matches!(self,
            DexType::Uniswap | DexType::Sushiswap | DexType::Quickswap | DexType::Apeswap |
            DexType::QuickSwapV2 | DexType::SushiSwapV2
        )
    }

    /// Returns the fee percentage for any DEX type.
    /// V2: always 0.30%. V3: from fee tier. Algebra: dynamic (returns None).
    pub fn fee_percent(&self) -> Option<f64> {
        if self.is_v2() {
            Some(0.30)
        } else if self.is_quickswap_v3() {
            None // Dynamic fee — read from pool state
        } else {
            self.v3_fee_bps().map(|bps| bps as f64 / 100.0)
        }
    }

    /// Returns true if this is a QuickSwap V3 (Algebra) DEX
    /// Algebra uses different ABIs: globalState() not slot0(), no fee parameter in quoter/router
    pub fn is_quickswap_v3(&self) -> bool {
        matches!(self, DexType::QuickswapV3)
    }

    /// Returns true if this is a SushiSwap V3 DEX (for quoter/router routing)
    pub fn is_sushi_v3(&self) -> bool {
        matches!(self, DexType::SushiV3_001 | DexType::SushiV3_005 | DexType::SushiV3_030)
    }

    /// Returns the fee in basis points for V3 pools
    /// QuickswapV3 returns None (dynamic fee — read from pool state)
    pub fn v3_fee_bps(&self) -> Option<u32> {
        match self {
            DexType::UniswapV3_001 | DexType::SushiV3_001 => Some(1),    // 0.01%
            DexType::UniswapV3_005 | DexType::SushiV3_005 => Some(5),    // 0.05%
            DexType::UniswapV3_030 | DexType::SushiV3_030 => Some(30),   // 0.30%
            DexType::UniswapV3_100 => Some(100),  // 1.00%
            _ => None,
        }
    }

    /// Get V3 fee tier (for factory queries and router calls)
    /// QuickswapV3 returns Some(0) — sentinel value meaning "Algebra, no fee param"
    /// Returns None for V2 dex types — use atomic_fee() for ArbExecutor routing.
    pub fn v3_fee_tier(&self) -> Option<u32> {
        match self {
            DexType::UniswapV3_001 | DexType::SushiV3_001 => Some(100),
            DexType::UniswapV3_005 | DexType::SushiV3_005 => Some(500),
            DexType::UniswapV3_030 | DexType::SushiV3_030 => Some(3000),
            DexType::UniswapV3_100 => Some(10000),
            DexType::QuickswapV3 => Some(0), // Sentinel: Algebra has no fixed fee tier
            _ => None,
        }
    }

    /// Fee value for ArbExecutor.sol atomic execution.
    /// Routes each leg to the correct on-chain swap path:
    ///   V2 → V2_FEE_SENTINEL (type(uint24).max = 16777215) → swapExactTokensForTokens
    ///   Algebra → 0 → Algebra exactInputSingle (no fee param)
    ///   V3 → actual fee tier (100, 500, 3000, 10000) → standard exactInputSingle
    pub fn atomic_fee(&self) -> u32 {
        if self.is_v2() {
            V2_FEE_SENTINEL
        } else {
            self.v3_fee_tier().unwrap_or(0)
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
            DexType::UniswapV3_001 => write!(f, "UniswapV3_0.01%"),
            DexType::UniswapV3_005 => write!(f, "UniswapV3_0.05%"),
            DexType::UniswapV3_030 => write!(f, "UniswapV3_0.30%"),
            DexType::UniswapV3_100 => write!(f, "UniswapV3_1.00%"),
            DexType::SushiV3_001 => write!(f, "SushiV3_0.01%"),
            DexType::SushiV3_005 => write!(f, "SushiV3_0.05%"),
            DexType::SushiV3_030 => write!(f, "SushiV3_0.30%"),
            DexType::QuickswapV3 => write!(f, "QuickswapV3"),
            DexType::QuickSwapV2 => write!(f, "QuickSwapV2"),
            DexType::SushiSwapV2 => write!(f, "SushiSwapV2"),
        }
    }
}

/// DEX pool state (V2 constant-product AMM)
#[derive(Debug, Clone)]
pub struct PoolState {
    pub address: Address,
    pub dex: DexType,
    pub pair: TradingPair,
    pub reserve0: U256,
    pub reserve1: U256,
    pub last_updated: u64, // block number
    /// Token0 decimals (required for cross-protocol V2↔V3 price comparison)
    pub token0_decimals: u8,
    /// Token1 decimals (required for cross-protocol V2↔V3 price comparison)
    pub token1_decimals: u8,
}

impl PoolState {
    /// Calculate raw price of token0 in terms of token1 (NO decimal adjustment).
    /// WARNING: This is the raw reserve ratio. For cross-protocol comparison
    /// with V3 prices, use `price_adjusted()` instead.
    pub fn price(&self) -> f64 {
        let reserve0_f = self.reserve0.to::<u128>() as f64;
        let reserve1_f = self.reserve1.to::<u128>() as f64;

        if reserve0_f == 0.0 {
            return 0.0;
        }

        reserve1_f / reserve0_f
    }

    /// Calculate decimal-adjusted price: token1 per token0 in human-readable units.
    /// This produces the SAME format as V3PoolState::price() (tick-based),
    /// allowing direct comparison between V2 and V3 pool prices.
    ///
    /// Formula: (reserve1 / reserve0) * 10^(decimals0 - decimals1)
    ///
    /// Example: USDC(6)/WETH(18) pool with 100 USDC and 0.042 WETH:
    ///   raw reserves: 100_000_000 / 42_000_000_000_000_000
    ///   raw ratio: 4.2e8
    ///   * 10^(6-18) = * 10^(-12)
    ///   = 0.00042 WETH per USDC ✓ (matches V3 tick-based price)
    pub fn price_adjusted(&self) -> f64 {
        let reserve0_f = self.reserve0.to::<u128>() as f64;
        let reserve1_f = self.reserve1.to::<u128>() as f64;

        if reserve0_f == 0.0 {
            return 0.0;
        }

        let raw_ratio = reserve1_f / reserve0_f;
        let decimal_adjustment =
            10_f64.powi(self.token0_decimals as i32 - self.token1_decimals as i32);

        raw_ratio * decimal_adjustment
    }

    /// Calculate output amount for given input (constant product formula)
    /// amountOut = (amountIn * 997 * reserveOut) / (reserveIn * 1000 + amountIn * 997)
    /// The 997/1000 factor is the 0.3% V2 swap fee.
    pub fn get_amount_out(&self, amount_in: U256, token_in: Address) -> U256 {
        let (reserve_in, reserve_out) = if token_in == self.pair.token0 {
            (self.reserve0, self.reserve1)
        } else {
            (self.reserve1, self.reserve0)
        };

        if reserve_in.is_zero() || reserve_out.is_zero() {
            return U256::ZERO;
        }

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
    /// Fee tier (100 = 0.01%, 500 = 0.05%, 3000 = 0.30%, 10000 = 1.00%)
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
///
/// Buy/Sell semantics (V3 price = token1/token0, token0 sorted by address):
///   The trade always starts and ends with the QUOTE token (USDC).
///   quote_token_is_token0 determines the swap direction:
///
///   If quote=token0 (e.g., WETH/USDC where USDC=token0):
///     buy_dex  = pool with HIGHER V3 price (more token1 per quote = cheap base → buy here)
///     sell_dex = pool with LOWER V3 price (less token1 per quote = expensive base → sell here)
///     Execute: token0(quote)→token1(base) on buy, token1(base)→token0(quote) on sell
///
///   If quote=token1 (e.g., WBTC/USDC where USDC=token1):
///     buy_dex  = pool with LOWER V3 price (less quote per base = cheap base → buy here)
///     sell_dex = pool with HIGHER V3 price (more quote per base = expensive base → sell here)
///     Execute: token1(quote)→token0(base) on buy, token0(base)→token1(quote) on sell
#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub pair: TradingPair,
    pub buy_dex: DexType,
    pub sell_dex: DexType,
    pub buy_price: f64,
    pub sell_price: f64,
    pub spread_percent: f64,
    pub estimated_profit: f64, // in USD
    pub trade_size: U256,      // in quote token raw units (dynamic decimals based on quote token)
    pub timestamp: u64,
    /// Pool address where we buy (optional for tax logging)
    pub buy_pool_address: Option<Address>,
    /// Pool address where we sell (optional for tax logging)
    pub sell_pool_address: Option<Address>,
    /// Token0 decimals (for correct min_out calculation)
    pub token0_decimals: u8,
    /// Token1 decimals (for correct min_out calculation)
    pub token1_decimals: u8,
    /// Pool liquidity at buy pool (V3 only, for safety checks)
    pub buy_pool_liquidity: Option<u128>,
    /// Whether the quote token (USDC) is V3 token0.
    /// Determines swap direction in quoter and executor.
    /// true:  trade goes token0→token1→token0 (USDC is token0)
    /// false: trade goes token1→token0→token1 (USDC is token1)
    pub quote_token_is_token0: bool,
    /// Per-opportunity minimum profit threshold (scaled to trade size).
    /// Used by executor for on-chain revert threshold (ArbExecutor minProfit).
    /// 0.0 means "use config.min_profit_usd" (backwards compat).
    pub min_profit_usd: f64,
    /// Pre-computed minimum profit in raw quote token units (dynamic decimals).
    /// Computed by detector: (scaled_min_profit_usd / quote_usd_price) * 10^quote_decimals.
    /// Executor reads this directly — no USD→token conversion in hot path.
    /// U256::ZERO means "not pre-computed" → executor falls back to legacy 1e6 path.
    pub min_profit_raw: U256,
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
        let spread_percent = ((sell_price - buy_price) / buy_price).abs() * 100.0;

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
            buy_pool_address: None,
            sell_pool_address: None,
            token0_decimals: 18, // Default to 18, override for specific tokens
            token1_decimals: 18,
            buy_pool_liquidity: None,
            quote_token_is_token0: true,
            min_profit_usd: 0.0,
            min_profit_raw: U256::ZERO,
        }
    }

    /// Create with pool addresses (for tax logging)
    pub fn with_pool_addresses(
        pair: TradingPair,
        buy_dex: DexType,
        sell_dex: DexType,
        buy_price: f64,
        sell_price: f64,
        trade_size: U256,
        buy_pool: Address,
        sell_pool: Address,
    ) -> Self {
        let mut opp = Self::new(pair, buy_dex, sell_dex, buy_price, sell_price, trade_size);
        opp.buy_pool_address = Some(buy_pool);
        opp.sell_pool_address = Some(sell_pool);
        opp
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
    pub block_number: Option<u64>,
    pub success: bool,
    pub profit_usd: f64,
    pub gas_cost_usd: f64,
    pub gas_used_native: f64,
    pub net_profit_usd: f64,
    pub execution_time_ms: u64,
    pub error: Option<String>,
    /// Amount sent in raw token units
    pub amount_in: Option<String>,
    /// Amount received in raw token units
    pub amount_out: Option<String>,
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

    // Chain identification (multi-chain support)
    // "polygon", "base", etc. Used for log messages and data path defaults.
    pub chain_name: String,

    // Quote token address (USDC or equivalent per chain)
    // Polygon: USDC.e (0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174)
    // Base: native USDC (0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913)
    pub quote_token_address: Address,

    // Estimated gas cost per trade in USD (chain-specific)
    // Polygon: ~$0.05, Base: ~$0.02
    pub estimated_gas_cost_usd: f64,

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

    // SushiSwap V3 addresses (cross-DEX arb - optional)
    pub sushiswap_v3_factory: Option<Address>,
    pub sushiswap_v3_router: Option<Address>,
    pub sushiswap_v3_quoter: Option<Address>,

    // QuickSwap V3 (Algebra) addresses (cross-DEX arb - optional)
    // Algebra uses dynamic fees, single pool per pair, different ABI
    pub quickswap_v3_factory: Option<Address>,
    pub quickswap_v3_router: Option<Address>,
    pub quickswap_v3_quoter: Option<Address>,

    // Uniswap V3 Quoter version flag (multi-chain compatibility)
    // Polygon deploys QuoterV1 (flat params), Base deploys QuoterV2 (struct params).
    // When true, Uniswap V3 quoter calls use QuoterV2 ABI in both
    // multicall_quoter.rs (batch pre-screen) and executor.rs (per-leg safety check).
    // Default: false (QuoterV1 — Polygon backwards compat)
    pub uniswap_v3_quoter_is_v2: bool,

    // Trading pairs to monitor
    pub pairs: Vec<TradingPairConfig>,

    // Performance
    pub poll_interval_ms: u64,
    pub max_gas_price_gwei: u64,

    // Tax Logging (IRS Compliance)
    pub tax_log_dir: Option<String>,
    pub tax_log_enabled: bool,

    // Live trading mode (false = dry run/paper trading)
    pub live_mode: bool,

    // Shared pool state file (written by data collector)
    // If set, bot reads pool data from this file instead of syncing via RPC
    pub pool_state_file: Option<String>,

    // Pool whitelist/blacklist config file (Phase 1.1)
    // If set, only whitelisted pools participate in detection
    pub whitelist_file: Option<String>,

    // Historical price logging (research)
    // Logs V3 pool prices to CSV per block for offline analysis
    pub price_log_enabled: bool,
    pub price_log_dir: Option<String>,

    // Atomic arbitrage executor contract (Phase: Atomic Execution)
    // When set, the bot executes both swap legs in a single atomic transaction
    // via the deployed ArbExecutor.sol contract. Reverts on loss.
    pub arb_executor_address: Option<Address>,

    // Skip Multicall3 batch Quoter pre-screen (default false)
    // When true, detected opportunities bypass batch_verify() and go straight
    // to the executor (which still has its own Quoter + eth_estimateGas checks).
    // Saves ~12ms per scan cycle by eliminating redundant on-chain verification.
    pub skip_multicall_prescreen: bool,

    // Route cooldown: suppress failed routes for N blocks (escalating backoff).
    // After a route fails, it is suppressed for N blocks. On repeated failures,
    // cooldown escalates 5× per failure up to ~1800 blocks (~1 hr on Polygon).
    // Eliminates hammering of structurally dead spreads. Set to 0 to disable.
    // Default: 10 blocks (~20s on Polygon).
    pub route_cooldown_blocks: u64,
    // Max consecutive max-cooldown cycles with 0 successes before permanent blacklist.
    // Eliminates structural false positives (fee combos that can never be profitable).
    // Default: 3. Set to 0 to disable permanent blacklisting.
    pub cooldown_max_strikes: u32,

    // Private RPC URL for transaction submission (Polygon Fastlane).
    // When set, atomic arb transactions are sent through this endpoint instead
    // of the main WS provider. Transactions are invisible to other MEV bots
    // until block inclusion. All reads stay on the Alchemy WS connection.
    // Rollback: remove this env var to fall back to public mempool.
    pub private_rpc_url: Option<String>,

    // A4 Mempool Monitor mode: "off", "observe", "execute"
    // observe: log pending DEX swaps to CSV, measure visibility + lead time
    // execute: submit backrun txs (Phase 3)
    // off: mempool monitoring disabled (default)
    pub mempool_monitor_mode: String,

    // A4 Phase 3: Mempool execution parameters
    // Minimum estimated profit to send a mempool signal to the executor (default $0.05)
    pub mempool_min_profit_usd: f64,
    // Fixed gas limit for mempool-sourced txs (skip estimateGas for speed) (default 500000)
    pub mempool_gas_limit: u64,
    // Minimum priority fee floor in gwei (Polygon competitive floor) (default 1000)
    pub mempool_min_priority_gwei: u64,
    // Max fraction of estimated profit to spend on gas (default 0.50 = 50%)
    pub mempool_gas_profit_cap: f64,

    // Native token price in USD (MATIC on Polygon, ETH on Base/Ethereum)
    // Used for gas cost calculations everywhere. Default 0.50 (MATIC).
    // Set via NATIVE_TOKEN_PRICE_USD env var.
    pub native_token_price_usd: f64,

    // Secondary quote token address (native USDC on Polygon).
    // When set, pools using either USDC variant are eligible for arbitrage.
    // Pools with different quote tokens are never compared against each other.
    // Polygon: USDC.e (primary) + native USDC (0x3c499c...) (secondary)
    pub quote_token_address_native: Option<Address>,

    // Separate WebSocket RPC URL for mempool monitor.
    // When RPC_URL is an IPC path (local Bor node), the mempool monitor still
    // needs a WS endpoint (Alchemy's alchemy_pendingTransactions requires WS,
    // local Bor WS at ws://127.0.0.1:8546).
    // Falls back to rpc_url if not set.
    pub ws_rpc_url: Option<String>,

    // Tertiary quote token address (USDT on Polygon).
    // When set, pools using USDT are eligible for arbitrage.
    // USDT arbs are isolated: USDT pools only compare against other USDT pools.
    // Pools with different quote tokens are never compared against each other.
    // Polygon USDT: 0xc2132D05D31c914a87C6611C10748AEb04B58e8F (6 decimals)
    pub quote_token_address_usdt: Option<Address>,

    // Quaternary quote token address (WETH on Polygon).
    // When set, pools using WETH are eligible for arbitrage.
    // WETH arbs are isolated: WETH pools only compare against other WETH pools.
    // WETH has 18 decimals — requires dynamic decimal handling in detector/executor.
    // Polygon WETH: 0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619
    pub quote_token_address_weth: Option<Address>,

    // WETH price in USD for USD→WETH conversion (min_profit, trade_size).
    // Used by quote_token_usd_price() — returns 1.0 for stablecoins, this for WETH.
    // A 10% WETH price move changes min_profit by ~$0.01 — irrelevant for safety threshold.
    // Default: 3300.0 (updated periodically, not latency-sensitive).
    pub weth_price_usd: f64,

    // Quinary quote token address (WMATIC on Polygon).
    // When set, pools using WMATIC are eligible for arbitrage.
    // WMATIC arbs are isolated: WMATIC pools only compare against other WMATIC pools.
    // WMATIC has 18 decimals (same as WETH).
    // Polygon WMATIC: 0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270
    // IMPORTANT: WMATIC is also a base token in WMATIC/USDC, WMATIC/WETH pairs.
    // The preferred_quote_token() method ensures WMATIC is only selected as quote
    // when the other token is NOT a higher-priority quote (stablecoins > WETH > WMATIC).
    pub quote_token_address_wmatic: Option<Address>,
}

impl BotConfig {
    /// Check if an address is any recognized quote token (primary, native, USDT, WETH, or WMATIC).
    /// Used by detector, simulator, and mempool handler to determine swap direction.
    pub fn is_quote_token(&self, addr: &Address) -> bool {
        *addr == self.quote_token_address
            || self.quote_token_address_native.map_or(false, |a| a == *addr)
            || self.quote_token_address_usdt.map_or(false, |a| a == *addr)
            || self.quote_token_address_weth.map_or(false, |a| a == *addr)
            || self.quote_token_address_wmatic.map_or(false, |a| a == *addr)
    }

    /// Returns the priority of a quote token (higher = preferred as quote).
    /// When both tokens in a pool are recognized quote tokens (e.g., WMATIC/USDC.e),
    /// the one with higher priority is selected as the quote.
    /// This prevents WMATIC from being picked as quote in WMATIC/USDC or WMATIC/WETH pools.
    fn quote_priority(&self, addr: &Address) -> u8 {
        if *addr == self.quote_token_address { 5 }                                              // USDC.e
        else if self.quote_token_address_native.map_or(false, |a| a == *addr) { 4 }             // nUSDC
        else if self.quote_token_address_usdt.map_or(false, |a| a == *addr) { 3 }               // USDT
        else if self.quote_token_address_weth.map_or(false, |a| a == *addr) { 2 }               // WETH
        else if self.quote_token_address_wmatic.map_or(false, |a| a == *addr) { 1 }             // WMATIC
        else { 0 }
    }

    /// Select the preferred quote token from two pool tokens.
    /// Returns None if neither token is a recognized quote token.
    /// When both tokens are quote tokens, returns the one with higher priority:
    ///   USDC.e (5) > nUSDC (4) > USDT (3) > WETH (2) > WMATIC (1)
    pub fn preferred_quote_token(&self, a: &Address, b: &Address) -> Option<Address> {
        let a_qt = self.is_quote_token(a);
        let b_qt = self.is_quote_token(b);
        match (a_qt, b_qt) {
            (true, true) => {
                if self.quote_priority(a) >= self.quote_priority(b) {
                    Some(*a)
                } else {
                    Some(*b)
                }
            }
            (true, false) => Some(*a),
            (false, true) => Some(*b),
            (false, false) => None,
        }
    }

    /// Returns the USD price of a quote token.
    /// Stablecoins (USDC.e, native USDC, USDT) → 1.0
    /// WETH → weth_price_usd (from env, default 3300.0)
    /// WMATIC → native_token_price_usd (from env, default 0.50)
    /// Used for USD→raw-token conversion (trade_size, min_profit_raw).
    pub fn quote_token_usd_price(&self, addr: &Address) -> f64 {
        if self.quote_token_address_weth.map_or(false, |a| a == *addr) {
            self.weth_price_usd
        } else if self.quote_token_address_wmatic.map_or(false, |a| a == *addr) {
            self.native_token_price_usd
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v2_fee_sentinel() {
        assert_eq!(V2_FEE_SENTINEL, 16_777_215);
        assert_eq!(V2_FEE_SENTINEL, (1u32 << 24) - 1); // type(uint24).max
    }

    #[test]
    fn test_atomic_fee_v2() {
        assert_eq!(DexType::QuickSwapV2.atomic_fee(), V2_FEE_SENTINEL);
        assert_eq!(DexType::SushiSwapV2.atomic_fee(), V2_FEE_SENTINEL);
        assert_eq!(DexType::Uniswap.atomic_fee(), V2_FEE_SENTINEL);
        assert_eq!(DexType::Sushiswap.atomic_fee(), V2_FEE_SENTINEL);
    }

    #[test]
    fn test_atomic_fee_v3() {
        assert_eq!(DexType::UniswapV3_001.atomic_fee(), 100);
        assert_eq!(DexType::UniswapV3_005.atomic_fee(), 500);
        assert_eq!(DexType::UniswapV3_030.atomic_fee(), 3000);
        assert_eq!(DexType::UniswapV3_100.atomic_fee(), 10000);
        assert_eq!(DexType::SushiV3_001.atomic_fee(), 100);
        assert_eq!(DexType::SushiV3_005.atomic_fee(), 500);
        assert_eq!(DexType::SushiV3_030.atomic_fee(), 3000);
        assert_eq!(DexType::QuickswapV3.atomic_fee(), 0); // Algebra sentinel
    }

    #[test]
    fn test_v3_fee_tier_unchanged() {
        // Verify v3_fee_tier() still returns None for V2 types (legacy path compatibility)
        assert_eq!(DexType::QuickSwapV2.v3_fee_tier(), None);
        assert_eq!(DexType::SushiSwapV2.v3_fee_tier(), None);
        // And correct values for V3 types
        assert_eq!(DexType::UniswapV3_005.v3_fee_tier(), Some(500));
        assert_eq!(DexType::QuickswapV3.v3_fee_tier(), Some(0));
    }
}
