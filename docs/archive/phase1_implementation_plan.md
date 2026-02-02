# Phase 1 Implementation Plan: DEX Arbitrage Foundation
## Week 1: Clone, Integrate, Execute First Trade

**Target Objective**: Working Rust bot monitoring Uniswap/Sushiswap, detecting opportunities, executing profitable trades

**Success Criteria**:
- ✅ Bot detects real arbitrage opportunities
- ✅ Executes 2-5 profitable trades/day
- ✅ $5-20 profit per trade
- ✅ <25ms latency (detection → submission)
- ✅ 50%+ win rate

---

## Part 1: Component Architecture & Source Mapping

### High-Level System Design

```
┌─────────────────────────────────────────────────────────────────┐
│                    PHASE 1 ARCHITECTURE                          │
└─────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│ LAYER 1: DATA INGESTION (ethers-rs + custom)                    │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  WebSocket Provider                Blockchain Events             │
│  ├─ Price updates              ├─ New blocks                     │
│  ├─ Pool reserves              ├─ Pending txs (optional Phase 2) │
│  └─ Transaction receipts       └─ Contract events                │
│                                                                   │
└────────────────┬──────────────────────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────────────────────┐
│ LAYER 2: POOL MANAGEMENT (amms-rs + custom sync)                │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Pool Syncer                   State Manager                     │
│  ├─ Fetch all pool addresses   ├─ In-memory pool state           │
│  ├─ Sync reserves              ├─ Update on block events         │
│  ├─ Calculate prices           ├─ Thread-safe access (Arc<RwLock>)│
│  └─ Monitor pool events        └─ Fast lookups (HashMap)         │
│                                                                   │
└────────────────┬──────────────────────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────────────────────┐
│ LAYER 3: OPPORTUNITY DETECTION (custom logic)                   │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Price Comparator              Profitability Calculator          │
│  ├─ Compare across DEXs        ├─ Calculate gross profit         │
│  ├─ Identify spreads >0.3%    ├─ Subtract gas costs             │
│  ├─ Check liquidity depth     ├─ Factor in slippage            │
│  └─ Filter viable pairs       └─ Minimum profit threshold       │
│                                                                   │
└────────────────┬──────────────────────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────────────────────┐
│ LAYER 4: EXECUTION (ethers-rs + custom)                         │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Transaction Builder           Transaction Executor              │
│  ├─ Build swap calldata        ├─ Sign transaction               │
│  ├─ Estimate gas              ├─ Submit to blockchain           │
│  ├─ Set slippage limits       ├─ Monitor for confirmation       │
│  └─ Create atomic sequences   └─ Log results                    │
│                                                                   │
└────────────────┬──────────────────────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────────────────────┐
│ LAYER 5: MONITORING & LOGGING (custom + tracing)                │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Performance Metrics           Trade Logger                      │
│  ├─ Latency tracking          ├─ All opportunities detected     │
│  ├─ Success/failure rates     ├─ Executed trades                │
│  ├─ Profit/loss tracking      ├─ Gas costs                      │
│  └─ System health            └─ Export to CSV/JSON              │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

---

## Part 2: Repository Selection & Cloning Strategy

### Primary Components to Clone

#### 1. **Core Framework Base** 
**Source**: `mev-template-rs` by degatchi
- **Repository**: https://github.com/degatchi/mev-template-rs
- **What to Use**:
  - Project structure
  - Basic monitoring loop
  - Discord alerting system
  - Transaction building patterns
  - Mempool monitoring setup (Phase 2)
- **What to Modify**:
  - Strip out MEV-specific logic
  - Adapt for DEX arbitrage focus
  - Simplify to Phase 1 requirements

**Clone Command**:
```bash
git clone https://github.com/degatchi/mev-template-rs.git phase1-base
cd phase1-base
```

#### 2. **AMM/Pool Interaction**
**Source**: `amms-rs` by darkforestry (or `cfmms-rs` as reference)
- **Repository**: https://github.com/darkforestry/amms-rs
- **What to Use**:
  - Pool syncing logic
  - Price calculation functions
  - DEX protocol interfaces (Uniswap V2/V3)
  - Swap simulation
- **What to Modify**:
  - Focus on Polygon-specific pools
  - Optimize for 2-3 DEXs only (Uniswap, Sushiswap)
  - Add custom caching layer

**Clone Command**:
```bash
git clone https://github.com/darkforestry/amms-rs.git amms-reference
```

#### 3. **Price Monitoring Examples**
**Source**: `crypto-arbitrage-analyzer` by codeesura
- **Repository**: https://github.com/codeesura/crypto-arbitrage-analyzer
- **What to Use**:
  - Real-time price monitoring patterns
  - Async WebSocket handling
  - ETH/USDC pair tracking
  - Basic profitability calculations
- **What to Modify**:
  - Extend to multiple pairs
  - Add Polygon network support
  - Optimize detection speed

**Clone Command**:
```bash
git clone https://github.com/codeesura/crypto-arbitrage-analyzer.git arbitrage-reference
```

#### 4. **Flash Loan Reference** (for Phase 2 preparation)
**Source**: `flashloan-rs` by whitenois3
- **Repository**: https://github.com/whitenois3/flashloan-rs
- **What to Use** (Phase 2):
  - Flash loan builder patterns
  - Aave integration
  - Transaction encoding
- **For Now**: Study only, don't integrate yet

**Clone Command**:
```bash
git clone https://github.com/whitenois3/flashloan-rs.git flashloan-reference
```

### Supporting References (No Direct Cloning, Study Only)

#### 5. **Artemis Framework** (architectural reference)
- **Repository**: https://github.com/paradigmxyz/artemis
- **Purpose**: Understand Collector → Strategy → Executor pattern
- **Application**: Inform our architecture design
- **Don't Clone**: Too heavy for Phase 1, but study structure

#### 6. **solidquant/mev-templates** (Rust version)
- **Repository**: https://github.com/solidquant/mev-templates
- **Purpose**: See complete end-to-end example
- **Application**: Reference for integration patterns

---

## Part 3: Detailed Integration Plan

### Step 1: Project Setup & Foundation (Day 1)

**Objective**: Create clean project structure combining best patterns from sources

**Directory Structure**:
```
phase1-arbitrage-bot/
├── Cargo.toml                    # Project dependencies
├── .env                          # Configuration (private keys, RPC URLs)
├── .gitignore                    # Exclude .env, target/, etc.
│
├── src/
│   ├── main.rs                   # Entry point, monitoring loop
│   ├── config.rs                 # Configuration management
│   ├── types.rs                  # Core data structures
│   │
│   ├── pool/                     # Pool management (from amms-rs)
│   │   ├── mod.rs
│   │   ├── syncer.rs             # Sync pool reserves
│   │   ├── state.rs              # In-memory pool state
│   │   └── calculator.rs         # Price calculations
│   │
│   ├── dex/                      # DEX integrations
│   │   ├── mod.rs
│   │   ├── uniswap.rs            # Uniswap V2 interface
│   │   └── sushiswap.rs          # Sushiswap interface
│   │
│   ├── arbitrage/                # Core arbitrage logic
│   │   ├── mod.rs
│   │   ├── detector.rs           # Opportunity detection
│   │   ├── calculator.rs         # Profitability calculations
│   │   └── executor.rs           # Trade execution
│   │
│   ├── utils/                    # Utilities
│   │   ├── mod.rs
│   │   ├── logger.rs             # Structured logging
│   │   └── metrics.rs            # Performance tracking
│   │
│   └── contracts/                # Smart contract ABIs
│       ├── mod.rs
│       └── bindings/             # Generated with abigen
│
├── contracts/                    # Solidity contracts (Phase 2)
│   └── FlashArbitrage.sol
│
├── tests/
│   ├── integration_tests.rs
│   └── mock_dex.rs
│
└── README.md
```

**Create Base Project**:
```bash
cargo new phase1-arbitrage-bot --bin
cd phase1-arbitrage-bot
```

**Initial Cargo.toml** (combining dependencies from sources):
```toml
[package]
name = "phase1-arbitrage-bot"
version = "0.1.0"
edition = "2021"

[dependencies]
# Core Ethereum interaction (ethers-rs)
ethers = { version = "2.0", features = ["ws", "rustls", "abigen"] }

# Async runtime
tokio = { version = "1.35", features = ["full"] }

# Utilities
anyhow = "1.0"
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Configuration
dotenv = "0.15"
config = "0.14"

# Data structures
dashmap = "5.5"  # Thread-safe HashMap
once_cell = "1.19"

# Numeric computations
num-bigfloat = "1.7"
rust_decimal = "1.33"

# Optional: Discord notifications (from mev-template-rs)
serenity = { version = "0.12", optional = true }

# Optional: Monitoring (add later)
prometheus = { version = "0.13", optional = true }

[dev-dependencies]
tokio-test = "0.4"

[features]
default = []
discord = ["serenity"]
metrics = ["prometheus"]
```

---

### Step 2: Core Data Structures (Day 1)

**File: `src/types.rs`**

Adapted from mev-template-rs and arbitrage-analyzer structures:

```rust
use ethers::types::{Address, U256};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Trading pair configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPair {
    pub token0: Address,
    pub token1: Address,
    pub symbol: String,  // e.g., "ETH/USDC"
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

/// DEX pool state
#[derive(Debug, Clone)]
pub struct PoolState {
    pub address: Address,
    pub dex: DexType,
    pub pair: TradingPair,
    pub reserve0: U256,
    pub reserve1: U256,
    pub last_updated: u64,  // block number
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

/// DEX types we support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DexType {
    Uniswap,
    Sushiswap,
    Quickswap,  // Phase 2
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

/// Arbitrage opportunity detected
#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub pair: TradingPair,
    pub buy_dex: DexType,
    pub sell_dex: DexType,
    pub buy_price: f64,
    pub sell_price: f64,
    pub spread_percent: f64,
    pub estimated_profit: f64,  // in USD
    pub trade_size: U256,       // in wei
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
            estimated_profit: 0.0,  // Calculate separately
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

#[derive(Debug, Clone, Deserialize)]
pub struct TradingPairConfig {
    pub token0: String,  // Address as string
    pub token1: String,
    pub symbol: String,
}
```

---

### Step 3: Configuration Management (Day 1)

**File: `.env`** (not committed to git):
```env
# Network (Polygon Mainnet)
RPC_URL=wss://polygon-mainnet.g.alchemy.com/v2/YOUR_API_KEY
CHAIN_ID=137

# Wallet
PRIVATE_KEY=your_private_key_here

# Trading Parameters
MIN_PROFIT_USD=5.0
MAX_TRADE_SIZE_USD=2000.0
MAX_SLIPPAGE_PERCENT=0.5

# DEX Addresses (Polygon)
UNISWAP_ROUTER=0xE592427A0AEce92De3Edee1F18E0157C05861564
SUSHISWAP_ROUTER=0x1b02dA8Cb0d097eB8D57A175b88c7D8b47997506
UNISWAP_FACTORY=0x1F98431c8aD98523631AE4a59f267346ea31F984
SUSHISWAP_FACTORY=0xc35DADB65012eC5796536bD9864eD8773aBc74C4

# Trading Pairs (comma-separated)
# Format: token0_address:token1_address:symbol
TRADING_PAIRS=\
0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619:0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174:ETH/USDC,\
0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270:0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174:WMATIC/USDC

# Performance
POLL_INTERVAL_MS=100
MAX_GAS_PRICE_GWEI=100
```

**File: `src/config.rs`**:
```rust
use crate::types::{BotConfig, TradingPairConfig};
use anyhow::{Context, Result};
use ethers::types::Address;
use std::str::FromStr;

pub fn load_config() -> Result<BotConfig> {
    dotenv::dotenv().ok();
    
    let trading_pairs_str = std::env::var("TRADING_PAIRS")
        .context("TRADING_PAIRS not set")?;
    
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
        
        uniswap_router: Address::from_str(
            &std::env::var("UNISWAP_ROUTER")?
        )?,
        sushiswap_router: Address::from_str(
            &std::env::var("SUSHISWAP_ROUTER")?
        )?,
        uniswap_factory: Address::from_str(
            &std::env::var("UNISWAP_FACTORY")?
        )?,
        sushiswap_factory: Address::from_str(
            &std::env::var("SUSHISWAP_FACTORY")?
        )?,
        
        pairs,
        
        poll_interval_ms: std::env::var("POLL_INTERVAL_MS")?.parse()?,
        max_gas_price_gwei: std::env::var("MAX_GAS_PRICE_GWEI")?.parse()?,
    })
}
```

---

### Step 4: Pool State Management (Day 2)

**File: `src/pool/mod.rs`**:
```rust
pub mod syncer;
pub mod state;
pub mod calculator;

pub use syncer::PoolSyncer;
pub use state::PoolStateManager;
pub use calculator::PriceCalculator;
```

**File: `src/pool/state.rs`** (adapted from amms-rs patterns):
```rust
use crate::types::{DexType, PoolState, TradingPair};
use dashmap::DashMap;
use ethers::types::Address;
use std::sync::Arc;

/// Thread-safe pool state manager
pub struct PoolStateManager {
    // Key: (DEX, Pair) -> Pool State
    pools: Arc<DashMap<(DexType, String), PoolState>>,
}

impl PoolStateManager {
    pub fn new() -> Self {
        Self {
            pools: Arc::new(DashMap::new()),
        }
    }
    
    /// Add or update pool state
    pub fn update_pool(&self, pool: PoolState) {
        let key = (pool.dex, pool.pair.symbol.clone());
        self.pools.insert(key, pool);
    }
    
    /// Get pool state for a specific DEX and pair
    pub fn get_pool(&self, dex: DexType, pair_symbol: &str) -> Option<PoolState> {
        let key = (dex, pair_symbol.to_string());
        self.pools.get(&key).map(|entry| entry.clone())
    }
    
    /// Get all pools for a specific pair across all DEXs
    pub fn get_pools_for_pair(&self, pair_symbol: &str) -> Vec<PoolState> {
        self.pools
            .iter()
            .filter(|entry| entry.key().1 == pair_symbol)
            .map(|entry| entry.value().clone())
            .collect()
    }
    
    /// Check if we have recent data (< 5 blocks old)
    pub fn is_stale(&self, current_block: u64) -> bool {
        self.pools
            .iter()
            .any(|entry| current_block - entry.value().last_updated > 5)
    }
    
    /// Get statistics
    pub fn stats(&self) -> (usize, u64) {
        let count = self.pools.len();
        let min_block = self.pools
            .iter()
            .map(|entry| entry.value().last_updated)
            .min()
            .unwrap_or(0);
        
        (count, min_block)
    }
}

impl Clone for PoolStateManager {
    fn clone(&self) -> Self {
        Self {
            pools: Arc::clone(&self.pools),
        }
    }
}
```

**File: `src/pool/syncer.rs`** (adapting amms-rs sync logic):
```rust
use crate::pool::PoolStateManager;
use crate::types::{BotConfig, DexType, PoolState, TradingPair};
use anyhow::Result;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::{info, warn};

/// Syncs pool reserves from blockchain
pub struct PoolSyncer {
    provider: Arc<Provider<Ws>>,
    config: BotConfig,
    state_manager: PoolStateManager,
}

impl PoolSyncer {
    pub fn new(
        provider: Arc<Provider<Ws>>,
        config: BotConfig,
        state_manager: PoolStateManager,
    ) -> Self {
        Self {
            provider,
            config,
            state_manager,
        }
    }
    
    /// Initial sync: fetch all pool addresses and reserves
    pub async fn initial_sync(&self) -> Result<()> {
        info!("Starting initial pool sync...");
        
        for pair_config in &self.config.pairs {
            let token0 = pair_config.token0.parse()?;
            let token1 = pair_config.token1.parse()?;
            let pair = TradingPair::new(token0, token1, pair_config.symbol.clone());
            
            // Sync Uniswap pool
            if let Ok(pool) = self.sync_pool(DexType::Uniswap, &pair).await {
                self.state_manager.update_pool(pool);
                info!("Synced Uniswap pool: {}", pair.symbol);
            }
            
            // Sync Sushiswap pool
            if let Ok(pool) = self.sync_pool(DexType::Sushiswap, &pair).await {
                self.state_manager.update_pool(pool);
                info!("Synced Sushiswap pool: {}", pair.symbol);
            }
        }
        
        info!("Initial sync complete: {} pools", self.state_manager.stats().0);
        Ok(())
    }
    
    /// Sync a specific pool's reserves
    async fn sync_pool(&self, dex: DexType, pair: &TradingPair) -> Result<PoolState> {
        // Get pool address from factory
        let pool_address = self.get_pool_address(dex, pair).await?;
        
        // Fetch reserves using getReserves()
        let (reserve0, reserve1, block_timestamp_last) = 
            self.get_reserves(pool_address).await?;
        
        let current_block = self.provider.get_block_number().await?.as_u64();
        
        Ok(PoolState {
            address: pool_address,
            dex,
            pair: pair.clone(),
            reserve0,
            reserve1,
            last_updated: current_block,
        })
    }
    
    /// Get pool address from factory contract
    async fn get_pool_address(
        &self,
        dex: DexType,
        pair: &TradingPair,
    ) -> Result<Address> {
        let factory_address = match dex {
            DexType::Uniswap => self.config.uniswap_factory,
            DexType::Sushiswap => self.config.sushiswap_factory,
            _ => anyhow::bail!("Unsupported DEX"),
        };
        
        // Call factory.getPair(token0, token1)
        // This would use abigen-generated bindings in production
        // For now, using manual call
        
        let factory = IUniswapV2Factory::new(factory_address, Arc::clone(&self.provider));
        let pool_address = factory.get_pair(pair.token0, pair.token1).call().await?;
        
        if pool_address == Address::zero() {
            anyhow::bail!("Pool not found for pair {}", pair.symbol);
        }
        
        Ok(pool_address)
    }
    
    /// Get reserves from pool contract
    async fn get_reserves(
        &self,
        pool_address: Address,
    ) -> Result<(U256, U256, u32)> {
        let pool = IUniswapV2Pair::new(pool_address, Arc::clone(&self.provider));
        let (reserve0, reserve1, block_timestamp_last) = pool.get_reserves().call().await?;
        
        Ok((reserve0, reserve1, block_timestamp_last))
    }
    
    /// Continuous sync: subscribe to Sync events and update pools
    pub async fn start_continuous_sync(&self) -> Result<()> {
        info!("Starting continuous sync...");
        
        // Subscribe to Sync events from all pools
        // Event: Sync(uint112 reserve0, uint112 reserve1)
        
        let mut stream = self.provider.subscribe_logs(&Filter::new()).await?;
        
        while let Some(log) = stream.next().await {
            // Parse log and update pool state
            // This would use event parsing in production
            // For Phase 1, we'll use polling instead (simpler)
        }
        
        Ok(())
    }
}

// Contract interfaces (would be generated with abigen)
abigen!(
    IUniswapV2Factory,
    r#"[
        function getPair(address tokenA, address tokenB) external view returns (address pair)
    ]"#
);

abigen!(
    IUniswapV2Pair,
    r#"[
        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast)
        function token0() external view returns (address)
        function token1() external view returns (address)
    ]"#
);
```

---

### Step 5: Opportunity Detection (Day 2-3)

**File: `src/arbitrage/detector.rs`**:
```rust
use crate::pool::PoolStateManager;
use crate::types::{ArbitrageOpportunity, BotConfig, DexType, TradingPair};
use anyhow::Result;
use tracing::{debug, info};

pub struct OpportunityDetector {
    config: BotConfig,
    state_manager: PoolStateManager,
}

impl OpportunityDetector {
    pub fn new(config: BotConfig, state_manager: PoolStateManager) -> Self {
        Self {
            config,
            state_manager,
        }
    }
    
    /// Scan all pairs for arbitrage opportunities
    pub fn scan_opportunities(&self) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();
        
        for pair_config in &self.config.pairs {
            if let Some(opp) = self.check_pair(&pair_config.symbol) {
                opportunities.push(opp);
            }
        }
        
        opportunities
    }
    
    /// Check a specific pair for arbitrage opportunity
    fn check_pair(&self, pair_symbol: &str) -> Option<ArbitrageOpportunity> {
        // Get all pools for this pair
        let pools = self.state_manager.get_pools_for_pair(pair_symbol);
        
        if pools.len() < 2 {
            debug!("Not enough pools for pair: {}", pair_symbol);
            return None;
        }
        
        // Find best buy and sell prices
        let mut best_buy: Option<(DexType, f64)> = None;
        let mut best_sell: Option<(DexType, f64)> = None;
        
        for pool in &pools {
            let price = pool.price();
            
            match best_buy {
                None => best_buy = Some((pool.dex, price)),
                Some((_, current_price)) if price < current_price => {
                    best_buy = Some((pool.dex, price));
                }
                _ => {}
            }
            
            match best_sell {
                None => best_sell = Some((pool.dex, price)),
                Some((_, current_price)) if price > current_price => {
                    best_sell = Some((pool.dex, price));
                }
                _ => {}
            }
        }
        
        let (buy_dex, buy_price) = best_buy?;
        let (sell_dex, sell_price) = best_sell?;
        
        // Calculate spread
        let spread_percent = ((sell_price - buy_price) / buy_price) * 100.0;
        
        // Filter: need at least 0.3% spread to be viable (covers gas + fees)
        if spread_percent < 0.3 {
            return None;
        }
        
        // Get the pair info
        let pair = pools[0].pair.clone();
        
        // Determine trade size (for now, use fixed size)
        let trade_size = ethers::utils::parse_ether("0.1")?;  // 0.1 ETH or equivalent
        
        let mut opportunity = ArbitrageOpportunity::new(
            pair,
            buy_dex,
            sell_dex,
            buy_price,
            sell_price,
            trade_size,
        );
        
        // Calculate estimated profit
        opportunity.estimated_profit = self.estimate_profit(&opportunity, &pools);
        
        if opportunity.is_profitable(self.config.min_profit_usd) {
            info!(
                "Opportunity found: {} - Buy {} @ {:.4}, Sell {} @ {:.4}, Spread: {:.2}%, Est Profit: ${:.2}",
                opportunity.pair.symbol,
                buy_dex,
                buy_price,
                sell_dex,
                sell_price,
                spread_percent,
                opportunity.estimated_profit
            );
            Some(opportunity)
        } else {
            None
        }
    }
    
    /// Estimate profit considering gas and fees
    fn estimate_profit(
        &self,
        opportunity: &ArbitrageOpportunity,
        pools: &[PoolState],
    ) -> f64 {
        // Find the actual pools
        let buy_pool = pools
            .iter()
            .find(|p| p.dex == opportunity.buy_dex)
            .unwrap();
        
        let sell_pool = pools
            .iter()
            .find(|p| p.dex == opportunity.sell_dex)
            .unwrap();
        
        // Calculate actual output amounts considering slippage
        let amount_in = opportunity.trade_size;
        let amount_mid = buy_pool.get_amount_out(amount_in, buy_pool.pair.token0);
        let amount_out = sell_pool.get_amount_out(amount_mid, sell_pool.pair.token1);
        
        // Convert to USD (simplified - would use real price feeds)
        let amount_in_usd = (amount_in.as_u128() as f64) / 1e18 * opportunity.buy_price;
        let amount_out_usd = (amount_out.as_u128() as f64) / 1e18 * opportunity.sell_price;
        
        // Gross profit
        let gross_profit = amount_out_usd - amount_in_usd;
        
        // Subtract DEX fees (0.3% * 2 = 0.6%)
        let dex_fees = amount_in_usd * 0.006;
        
        // Subtract gas cost (estimate ~$0.50 on Polygon)
        let gas_cost = 0.50;
        
        // Net profit
        gross_profit - dex_fees - gas_cost
    }
}
```

---

### Step 6: Trade Execution (Day 3-4)

**File: `src/arbitrage/executor.rs`**:
```rust
use crate::types::{ArbitrageOpportunity, BotConfig, DexType, TradeResult};
use anyhow::Result;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};

pub struct TradeExecutor {
    provider: Arc<Provider<Ws>>,
    wallet: LocalWallet,
    config: BotConfig,
}

impl TradeExecutor {
    pub fn new(
        provider: Arc<Provider<Ws>>,
        wallet: LocalWallet,
        config: BotConfig,
    ) -> Self {
        Self {
            provider,
            wallet,
            config,
        }
    }
    
    /// Execute arbitrage trade
    pub async fn execute(&self, opportunity: &ArbitrageOpportunity) -> Result<TradeResult> {
        let start = Instant::now();
        
        info!(
            "Executing arbitrage: {} - {} -> {}",
            opportunity.pair.symbol, opportunity.buy_dex, opportunity.sell_dex
        );
        
        // Step 1: Buy on cheaper DEX
        let buy_tx = self.swap(
            opportunity.buy_dex,
            opportunity.pair.token0,
            opportunity.pair.token1,
            opportunity.trade_size,
            true,  // is_buy
        ).await;
        
        match buy_tx {
            Ok(receipt) => {
                info!("Buy executed: {:?}", receipt.transaction_hash);
                
                // Step 2: Sell on more expensive DEX
                // (For Phase 1, we'll do manual two-step; Phase 2 will be atomic)
                let sell_tx = self.swap(
                    opportunity.sell_dex,
                    opportunity.pair.token1,
                    opportunity.pair.token0,
                    opportunity.trade_size,  // Amount received from buy
                    false,  // is_sell
                ).await;
                
                match sell_tx {
                    Ok(receipt) => {
                        let execution_time = start.elapsed().as_millis() as u64;
                        
                        Ok(TradeResult {
                            opportunity: opportunity.pair.symbol.clone(),
                            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
                            success: true,
                            profit_usd: opportunity.estimated_profit,
                            gas_cost_usd: 0.5,  // Estimate
                            net_profit_usd: opportunity.estimated_profit - 0.5,
                            execution_time_ms: execution_time,
                            error: None,
                        })
                    }
                    Err(e) => {
                        error!("Sell failed: {}", e);
                        // We're stuck with token1 now - this is leg risk in Phase 1
                        // Phase 2 will eliminate this with atomic execution
                        
                        Ok(TradeResult {
                            opportunity: opportunity.pair.symbol.clone(),
                            tx_hash: None,
                            success: false,
                            profit_usd: 0.0,
                            gas_cost_usd: 0.5,
                            net_profit_usd: -0.5,
                            execution_time_ms: start.elapsed().as_millis() as u64,
                            error: Some(format!("Sell failed: {}", e)),
                        })
                    }
                }
            }
            Err(e) => {
                error!("Buy failed: {}", e);
                
                Ok(TradeResult {
                    opportunity: opportunity.pair.symbol.clone(),
                    tx_hash: None,
                    success: false,
                    profit_usd: 0.0,
                    gas_cost_usd: 0.0,  // No gas spent if buy fails
                    net_profit_usd: 0.0,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("Buy failed: {}", e)),
                })
            }
        }
    }
    
    /// Execute a swap on a specific DEX
    async fn swap(
        &self,
        dex: DexType,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        is_buy: bool,
    ) -> Result<TransactionReceipt> {
        let router_address = match dex {
            DexType::Uniswap => self.config.uniswap_router,
            DexType::Sushiswap => self.config.sushiswap_router,
            _ => anyhow::bail!("Unsupported DEX"),
        };
        
        let router = IUniswapV2Router02::new(router_address, Arc::clone(&self.provider));
        
        // Build path
        let path = vec![token_in, token_out];
        
        // Calculate minimum output with slippage
        let min_out = self.calculate_min_out(amount_in, is_buy).await?;
        
        // Get deadline (current timestamp + 5 minutes)
        let deadline = chrono::Utc::now().timestamp() as U256 + U256::from(300);
        
        // Build transaction
        let tx = router.swap_exact_tokens_for_tokens(
            amount_in,
            min_out,
            path,
            self.wallet.address(),
            deadline,
        );
        
        // Send transaction
        let pending_tx = tx.send().await?;
        
        // Wait for confirmation
        let receipt = pending_tx
            .await?
            .ok_or_else(|| anyhow::anyhow!("Transaction dropped"))?;
        
        Ok(receipt)
    }
    
    /// Calculate minimum output amount with slippage tolerance
    async fn calculate_min_out(&self, amount_in: U256, is_buy: bool) -> Result<U256> {
        // For Phase 1, use simple percentage
        // Phase 2 will use getAmountsOut() from router
        
        let slippage_factor = 1.0 - (self.config.max_slippage_percent / 100.0);
        let min_out = amount_in.as_u128() as f64 * slippage_factor;
        
        Ok(U256::from(min_out as u128))
    }
}

// Router interface
abigen!(
    IUniswapV2Router02,
    r#"[
        function swapExactTokensForTokens(
            uint amountIn,
            uint amountOutMin,
            address[] calldata path,
            address to,
            uint deadline
        ) external returns (uint[] memory amounts)
    ]"#
);
```

---

### Step 7: Main Event Loop (Day 4-5)

**File: `src/main.rs`**:
```rust
mod config;
mod types;
mod pool;
mod dex;
mod arbitrage;
mod utils;

use anyhow::Result;
use arbitrage::{OpportunityDetector, TradeExecutor};
use config::load_config;
use ethers::prelude::*;
use pool::{PoolSyncer, PoolStateManager};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("phase1_arbitrage_bot=info,warn")
        .init();
    
    info!("Starting Phase 1 Arbitrage Bot");
    
    // Load configuration
    let config = load_config()?;
    info!("Configuration loaded");
    
    // Initialize provider (WebSocket for low latency)
    let provider = Provider::<Ws>::connect(&config.rpc_url).await?;
    let provider = Arc::new(provider);
    info!("Connected to Polygon via WebSocket");
    
    // Initialize wallet
    let wallet = config.private_key.parse::<LocalWallet>()?
        .with_chain_id(config.chain_id);
    info!("Wallet initialized: {}", wallet.address());
    
    // Initialize components
    let state_manager = PoolStateManager::new();
    let syncer = PoolSyncer::new(
        Arc::clone(&provider),
        config.clone(),
        state_manager.clone(),
    );
    let detector = OpportunityDetector::new(config.clone(), state_manager.clone());
    let executor = TradeExecutor::new(
        Arc::clone(&provider),
        wallet,
        config.clone(),
    );
    
    // Initial pool sync
    syncer.initial_sync().await?;
    info!("Initial sync complete");
    
    // Main monitoring loop
    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    let mut iteration = 0u64;
    
    loop {
        iteration += 1;
        
        // Update pool states (every iteration)
        if let Err(e) = update_pools(&syncer).await {
            error!("Failed to update pools: {}", e);
            continue;
        }
        
        // Detect opportunities
        let opportunities = detector.scan_opportunities();
        
        if opportunities.is_empty() {
            if iteration % 100 == 0 {
                info!("No opportunities found (iteration {})", iteration);
            }
        } else {
            info!("Found {} opportunities", opportunities.len());
            
            // Execute the best opportunity
            if let Some(best) = opportunities.into_iter().max_by(|a, b| {
                a.estimated_profit.partial_cmp(&b.estimated_profit).unwrap()
            }) {
                match executor.execute(&best).await {
                    Ok(result) => {
                        if result.success {
                            info!(
                                "✅ Trade successful! Profit: ${:.2} (gas: ${:.2})",
                                result.net_profit_usd,
                                result.gas_cost_usd
                            );
                        } else {
                            warn!(
                                "❌ Trade failed: {}",
                                result.error.unwrap_or_else(|| "Unknown error".to_string())
                            );
                        }
                    }
                    Err(e) => {
                        error!("Execution error: {}", e);
                    }
                }
            }
        }
        
        // Sleep before next iteration
        sleep(poll_interval).await;
    }
}

async fn update_pools(syncer: &PoolSyncer) -> Result<()> {
    // For Phase 1, we'll do a full sync every iteration
    // Phase 2 will use event subscriptions for efficiency
    syncer.initial_sync().await
}
```

---

## Part 4: Testing & Deployment (Day 5-7)

### Testing on Mumbai Testnet

**File: `scripts/test_on_testnet.sh`**:
```bash
#!/bin/bash

echo "Testing on Mumbai testnet..."

# Set testnet environment
export RPC_URL="wss://polygon-mumbai.g.alchemy.com/v2/YOUR_API_KEY"
export CHAIN_ID=80001
export MIN_PROFIT_USD=1.0

# Run bot in test mode
cargo run

# Monitor logs
tail -f bot.log
```

### Deployment Checklist

```
☐ Test on Mumbai testnet (2-3 days)
☐ Verify opportunity detection works
☐ Execute 10+ test trades
☐ Validate profitability calculations
☐ Check gas cost estimates
☐ Deploy to Polygon mainnet with $500
☐ Monitor for 1-2 days before increasing capital
☐ Scale to $2,000 once proven
```

---

## Part 5: Code Clone & Integration Timeline

### Day 1: Foundation
```bash
# Clone references
git clone https://github.com/degatchi/mev-template-rs.git references/mev-template
git clone https://github.com/darkforestry/amms-rs.git references/amms
git clone https://github.com/codeesura/crypto-arbitrage-analyzer.git references/analyzer

# Create our project
cargo new phase1-arbitrage-bot --bin
cd phase1-arbitrage-bot

# Copy over useful patterns
# - Project structure from mev-template-rs
# - Pool syncing from amms-rs
# - Price monitoring from crypto-arbitrage-analyzer

# Implement:
# - types.rs (core data structures)
# - config.rs (configuration management)
```

### Day 2: Data Pipeline
```
# Implement:
# - pool/state.rs (state management)
# - pool/syncer.rs (pool synchronization)
# - pool/calculator.rs (price calculations)

# Test:
# - Connect to Polygon
# - Fetch pool addresses
# - Sync reserves
# - Calculate prices
```

### Day 3: Opportunity Detection
```
# Implement:
# - arbitrage/detector.rs (opportunity scanner)
# - arbitrage/calculator.rs (profitability)

# Test:
# - Detect real opportunities
# - Calculate profits accurately
# - Filter unprofitable trades
```

### Day 4: Execution
```
# Implement:
# - arbitrage/executor.rs (trade execution)
# - main.rs (event loop)

# Test:
# - Build transactions
# - Submit to testnet
# - Handle errors
```

### Day 5: Integration Testing
```
# Test end-to-end on Mumbai:
# - Full monitoring loop
# - Opportunity detection
# - Trade execution
# - Logging and monitoring
```

### Day 6-7: Deployment
```
# Deploy to Polygon mainnet:
# - Start with $500 capital
# - Monitor closely
# - Scale gradually
```

---

## Part 6: Success Metrics

### Phase 1 Goals (Week 1)
```
✅ Bot detects 10+ opportunities per day
✅ Executes 2-5 trades per day
✅ Win rate >30% (accounting for competition)
✅ Average profit $5-20 per trade
✅ Latency <100ms (Phase 1 acceptable)
✅ Zero losses due to bugs/errors
```

### Ready for Phase 2 When:
```
✅ 20+ successful trades executed
✅ Win rate >50%
✅ Profitability calculations accurate
✅ Gas cost estimates within 10%
✅ No unexpected errors or bugs
✅ Comfortable with Rust + ethers-rs
```

---

## Summary: Clone & Build Strategy

1. **Clone** mev-template-rs as architectural reference
2. **Clone** amms-rs for pool syncing patterns
3. **Clone** crypto-arbitrage-analyzer for price monitoring
4. **Build** custom integration combining best parts
5. **Test** thoroughly on testnet
6. **Deploy** gradually to mainnet
7. **Iterate** based on real-world results
8. **Prepare** for Phase 2 flash loan integration

**This approach gives you**:
- Proven patterns from production systems
- Rust speed advantages
- Custom integration for your specific needs
- Clear path from MVP to production
- Foundation for Phase 2 flash loans

Next step: Start Day 1 setup and begin cloning repositories!
