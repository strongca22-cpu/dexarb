# Component Mapping & Integration Guide
## Detailed Source-to-Implementation Mapping

This document shows exactly which files to study from each source repository and how to adapt them for our Phase 1 bot.

---

## Repository 1: mev-template-rs (degatchi)
**Repository**: https://github.com/degatchi/mev-template-rs

### Files to Study & Adapt

#### 1. Project Structure
```
mev-template-rs/
├── src/
│   ├── main.rs              → Study: Event loop pattern
│   ├── collector/           → Study: Data collection architecture
│   ├── executor/            → Adapt: Transaction execution patterns
│   ├── strategies/          → Study: Strategy pattern
│   └── utils/               → Adapt: Logging, metrics
└── Cargo.toml               → Copy: Dependencies structure
```

**Specific Adaptations**:

**From**: `mev-template-rs/src/main.rs`
**Pattern**: Tokio async runtime with graceful shutdown
```rust
// Their pattern:
#[tokio::main]
async fn main() {
    // Setup
    let config = Config::load();
    let provider = setup_provider(&config).await;
    
    // Main loop
    loop {
        // Collect data
        // Analyze
        // Execute
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
```

**Adapt to**:
```rust
// Our implementation:
#[tokio::main]
async fn main() -> Result<()> {
    // Setup components
    let config = load_config()?;
    let state_manager = PoolStateManager::new();
    let syncer = PoolSyncer::new(...);
    let detector = OpportunityDetector::new(...);
    let executor = TradeExecutor::new(...);
    
    // Initial sync
    syncer.initial_sync().await?;
    
    // Main loop
    loop {
        // Update pool states
        syncer.update_pools().await?;
        
        // Detect opportunities
        let opportunities = detector.scan_opportunities();
        
        // Execute best opportunity
        if let Some(best) = select_best(opportunities) {
            executor.execute(&best).await?;
        }
        
        tokio::time::sleep(poll_interval).await;
    }
}
```

**From**: `mev-template-rs/src/utils/logger.rs`
**Pattern**: Structured logging with tracing
```rust
// Copy their logging setup
use tracing::{info, warn, error};
use tracing_subscriber::fmt;

pub fn init_logger() {
    fmt()
        .with_env_filter("bot=debug,warn")
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();
}
```

**From**: `mev-template-rs/src/executor/tx_builder.rs`
**Pattern**: Transaction building with gas estimation
```rust
// Their pattern (simplified):
async fn build_transaction(
    &self,
    to: Address,
    data: Bytes,
) -> Result<TypedTransaction> {
    let mut tx = TransactionRequest::new()
        .to(to)
        .data(data)
        .from(self.wallet.address());
    
    // Estimate gas
    let gas_estimate = self.provider.estimate_gas(&tx, None).await?;
    tx = tx.gas(gas_estimate * 110 / 100); // 10% buffer
    
    // Get gas price
    let gas_price = self.provider.get_gas_price().await?;
    tx = tx.gas_price(gas_price);
    
    Ok(tx.into())
}
```

---

## Repository 2: amms-rs (darkforestry)
**Repository**: https://github.com/darkforestry/amms-rs

### Files to Study & Adapt

#### 1. Pool Syncing
```
amms-rs/
├── src/
│   ├── amm/
│   │   ├── uniswap_v2/      → Adapt: V2 pool logic
│   │   │   ├── pool.rs      → CRITICAL: Pool state management
│   │   │   └── factory.rs   → Use: Factory interaction
│   │   └── uniswap_v3/      → Study: V3 for Phase 3
│   ├── sync/
│   │   ├── sync.rs          → Adapt: Syncing strategies
│   │   └── checkpoint.rs    → Optional: Fast startup
│   └── state_space/         → Study: State management patterns
```

**From**: `amms-rs/src/amm/uniswap_v2/pool.rs`
**Critical Functions**:
```rust
// Their Pool struct
pub struct UniswapV2Pool {
    pub address: Address,
    pub token_a: Address,
    pub token_b: Address,
    pub reserve_0: u128,
    pub reserve_1: u128,
    pub fee: u32,
}

impl UniswapV2Pool {
    // Copy this function exactly - it's battle-tested
    pub fn calculate_price(&self, base_token: Address) -> f64 {
        if base_token == self.token_a {
            self.reserve_1 as f64 / self.reserve_0 as f64
        } else {
            self.reserve_0 as f64 / self.reserve_1 as f64
        }
    }
    
    // Copy this - constant product formula with fee
    pub fn simulate_swap(&self, token_in: Address, amount_in: U256) -> U256 {
        let (reserve_in, reserve_out) = if token_in == self.token_a {
            (self.reserve_0, self.reserve_1)
        } else {
            (self.reserve_1, self.reserve_0)
        };
        
        // Apply 0.3% fee
        let amount_in_with_fee = amount_in * 997;
        let numerator = amount_in_with_fee * reserve_out;
        let denominator = (reserve_in * 1000) + amount_in_with_fee;
        
        numerator / denominator
    }
    
    // Copy this - fetches reserves from chain
    pub async fn sync<M: Middleware>(
        &mut self,
        middleware: Arc<M>,
    ) -> Result<(), AMMError> {
        let pair = IUniswapV2Pair::new(self.address, middleware);
        
        let (reserve_0, reserve_1, _) = pair.get_reserves().call().await?;
        
        self.reserve_0 = reserve_0.as_u128();
        self.reserve_1 = reserve_1.as_u128();
        
        Ok(())
    }
}
```

**Adaptation for Our Bot**:
```rust
// Our PoolState (simpler, focused on Phase 1)
pub struct PoolState {
    pub address: Address,
    pub dex: DexType,
    pub pair: TradingPair,
    pub reserve0: U256,
    pub reserve1: U256,
    pub last_updated: u64,
}

impl PoolState {
    // Use their price calculation logic
    pub fn price(&self) -> f64 {
        let reserve0_f = self.reserve0.as_u128() as f64;
        let reserve1_f = self.reserve1.as_u128() as f64;
        
        if reserve0_f == 0.0 {
            return 0.0;
        }
        
        reserve1_f / reserve0_f
    }
    
    // Use their swap simulation (exact copy)
    pub fn get_amount_out(&self, amount_in: U256, token_in: Address) -> U256 {
        let (reserve_in, reserve_out) = if token_in == self.pair.token0 {
            (self.reserve0, self.reserve1)
        } else {
            (self.reserve1, self.reserve0)
        };
        
        let amount_in_with_fee = amount_in * U256::from(997);
        let numerator = amount_in_with_fee * reserve_out;
        let denominator = (reserve_in * U256::from(1000)) + amount_in_with_fee;
        
        numerator / denominator
    }
}
```

**From**: `amms-rs/src/sync/sync.rs`
**Pattern**: Batch syncing multiple pools
```rust
// Their approach (simplified):
pub async fn sync_pairs<M: Middleware>(
    pairs: Vec<UniswapV2Pool>,
    middleware: Arc<M>,
) -> Result<Vec<UniswapV2Pool>> {
    let multicall = Multicall::new(middleware.clone(), None).await?;
    
    // Batch all getReserves calls
    for pair in &pairs {
        multicall.add_call(pair.get_reserves_call(), false);
    }
    
    let results: Vec<(U256, U256, u32)> = multicall.call_array().await?;
    
    // Update all pairs
    for (i, (r0, r1, _)) in results.iter().enumerate() {
        pairs[i].reserve_0 = r0.as_u128();
        pairs[i].reserve_1 = r1.as_u128();
    }
    
    Ok(pairs)
}
```

**Adapt to**:
```rust
// Our implementation with multicall for efficiency
pub async fn batch_sync_pools(&self) -> Result<()> {
    // Get all pool addresses
    let pools: Vec<_> = self.pairs
        .iter()
        .flat_map(|pair| {
            vec![
                (DexType::Uniswap, pair),
                (DexType::Sushiswap, pair),
            ]
        })
        .collect();
    
    // Create multicall contract
    let multicall = Multicall::new(
        Arc::clone(&self.provider),
        Some(self.config.multicall_address),
    ).await?;
    
    // Add all getReserves() calls
    for (dex, pair) in &pools {
        let pool_address = self.get_pool_address(*dex, pair).await?;
        multicall.add_call(
            IUniswapV2Pair::new(pool_address, Arc::clone(&self.provider))
                .get_reserves(),
            false,
        );
    }
    
    // Execute all calls in single RPC request
    let results: Vec<(U256, U256, u32)> = multicall.call_array().await?;
    
    // Update state manager
    for ((dex, pair), (r0, r1, _)) in pools.iter().zip(results.iter()) {
        let pool = PoolState {
            address: self.get_pool_address(*dex, pair).await?,
            dex: *dex,
            pair: (*pair).clone(),
            reserve0: *r0,
            reserve1: *r1,
            last_updated: self.provider.get_block_number().await?.as_u64(),
        };
        
        self.state_manager.update_pool(pool);
    }
    
    Ok(())
}
```

---

## Repository 3: crypto-arbitrage-analyzer (codeesura)
**Repository**: https://github.com/codeesura/crypto-arbitrage-analyzer

### Files to Study & Adapt

```
crypto-arbitrage-analyzer/
└── src/
    ├── main.rs              → Study: Basic arbitrage logic
    ├── dex_fetcher.rs       → Adapt: Price fetching patterns
    └── arbitrage.rs         → Study: Opportunity detection
```

**From**: `crypto-arbitrage-analyzer/src/arbitrage.rs`
**Pattern**: Simple arbitrage detection
```rust
// Their approach (simplified):
pub fn detect_arbitrage(
    uniswap_price: f64,
    sushiswap_price: f64,
    threshold: f64,
) -> Option<ArbitrageOpportunity> {
    let price_diff = (uniswap_price - sushiswap_price).abs();
    let price_diff_percent = (price_diff / uniswap_price.min(sushiswap_price)) * 100.0;
    
    if price_diff_percent > threshold {
        Some(ArbitrageOpportunity {
            buy_exchange: if uniswap_price < sushiswap_price { "Uniswap" } else { "Sushiswap" },
            sell_exchange: if uniswap_price > sushiswap_price { "Uniswap" } else { "Sushiswap" },
            buy_price: uniswap_price.min(sushiswap_price),
            sell_price: uniswap_price.max(sushiswap_price),
            spread: price_diff_percent,
        })
    } else {
        None
    }
}
```

**Enhance for Our Bot**:
```rust
// Our enhanced detection with profitability check
pub fn detect_arbitrage(
    &self,
    pair_symbol: &str,
) -> Option<ArbitrageOpportunity> {
    let pools = self.state_manager.get_pools_for_pair(pair_symbol);
    
    if pools.len() < 2 {
        return None;
    }
    
    // Find best buy and sell
    let (buy_dex, buy_price, buy_pool) = pools.iter()
        .map(|p| (p.dex, p.price(), p))
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())?;
    
    let (sell_dex, sell_price, sell_pool) = pools.iter()
        .map(|p| (p.dex, p.price(), p))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())?;
    
    let spread_percent = ((sell_price - buy_price) / buy_price) * 100.0;
    
    // Early filter: need >0.3% spread
    if spread_percent < 0.3 {
        return None;
    }
    
    // Calculate actual profit accounting for:
    // - Slippage (using constant product formula)
    // - DEX fees (0.3% * 2)
    // - Gas costs (~$0.50 on Polygon)
    
    let trade_size = self.calculate_optimal_size(buy_pool, sell_pool);
    let amount_out = buy_pool.get_amount_out(trade_size, buy_pool.pair.token0);
    let final_amount = sell_pool.get_amount_out(amount_out, sell_pool.pair.token1);
    
    let gross_profit = self.calculate_usd_value(final_amount) 
        - self.calculate_usd_value(trade_size);
    
    let dex_fees = self.calculate_usd_value(trade_size) * 0.006; // 0.6% total
    let gas_cost = 0.50;
    
    let net_profit = gross_profit - dex_fees - gas_cost;
    
    if net_profit < self.config.min_profit_usd {
        return None;
    }
    
    Some(ArbitrageOpportunity {
        pair: buy_pool.pair.clone(),
        buy_dex,
        sell_dex,
        buy_price,
        sell_price,
        spread_percent,
        estimated_profit: net_profit,
        trade_size,
        timestamp: chrono::Utc::now().timestamp() as u64,
    })
}
```

**From**: `crypto-arbitrage-analyzer/src/dex_fetcher.rs`
**Pattern**: Async price fetching
```rust
// Their WebSocket approach
pub async fn subscribe_to_prices(
    provider: &Provider<Ws>,
    pair_address: Address,
) -> Result<impl Stream<Item = (U256, U256)>> {
    // Subscribe to Sync events
    let filter = Filter::new()
        .address(pair_address)
        .event("Sync(uint112,uint112)");
    
    let stream = provider.subscribe_logs(&filter).await?;
    
    // Parse events into reserve updates
    let reserve_stream = stream.map(|log| {
        // Parse log to get reserve0, reserve1
        (reserve0, reserve1)
    });
    
    Ok(reserve_stream)
}
```

**Adapt to Our State Manager**:
```rust
// Our event-driven update (for Phase 2 optimization)
pub async fn subscribe_to_pool_updates(&self) -> Result<()> {
    // Subscribe to Sync events from all pools
    let mut streams = Vec::new();
    
    for (dex, pair_symbol) in self.get_all_pools() {
        let pool_address = self.get_pool_address(dex, &pair_symbol)?;
        
        let filter = Filter::new()
            .address(pool_address)
            .event("Sync(uint112,uint112)");
        
        let stream = self.provider.subscribe_logs(&filter).await?;
        streams.push((dex, pair_symbol, stream));
    }
    
    // Process all streams concurrently
    tokio::spawn(async move {
        // Use select_all to wait on all streams
        // Update state when events arrive
    });
    
    Ok(())
}
```

---

## Repository 4: flashloan-rs (whitenois3)
**Repository**: https://github.com/whitenois3/flashloan-rs
**(Phase 2 preparation - study only for now)**

### Files to Study for Phase 2

```
flashloan-rs/
├── src/
│   ├── builder.rs           → Phase 2: Flash loan builder pattern
│   └── contract.rs          → Phase 2: Contract bindings
├── contracts/
│   └── FlashBorrower.sol    → Phase 2: Reference contract
└── examples/
    └── pure_arb.rs          → Phase 2: Complete example
```

**Key Pattern to Understand**:
```rust
// Their FlashLoanBuilder pattern (for Phase 2)
let mut builder = FlashloanBuilder::new(
    Arc::clone(&provider),
    chain_id,
    Some(wallet.address()),
    Some(lender_address),
    Some(token_address),
    Some(amount),
);

// Add custom logic to execute during callback
builder.with_params(abi::encode(&params));

// Execute flash loan
let tx = builder.build()?;
let pending = provider.send_transaction(tx, None).await?;
let receipt = pending.await?;
```

---

## Repository 5: Artemis (paradigmxyz)
**Repository**: https://github.com/paradigmxyz/artemis
**(Architectural reference - don't clone, just study)**

### Patterns to Learn

**Collector → Strategy → Executor Architecture**:
```
┌─────────────┐
│  Collectors │ (Gather data: blocks, prices, mempool)
└──────┬──────┘
       │ Events
       ▼
┌─────────────┐
│ Strategies  │ (Detect opportunities)
└──────┬──────┘
       │ Actions
       ▼
┌─────────────┐
│  Executors  │ (Execute trades)
└─────────────┘
```

**Key Insight**: This architecture scales well as you add:
- More collectors (new data sources)
- More strategies (liquidations, multi-hop arb)
- More executors (Flashbots, different networks)

**For Phase 1**: We use a simplified version:
```
Monitor Loop → Detect → Execute
```

**For Phase 3+**: Consider refactoring to Artemis pattern.

---

## Integration Workflow

### Week 1 Implementation

**Day 1 - Setup**:
```bash
# Clone references
mkdir references
cd references
git clone https://github.com/degatchi/mev-template-rs.git
git clone https://github.com/darkforestry/amms-rs.git
git clone https://github.com/codeesura/crypto-arbitrage-analyzer.git

# Create project
cd ..
cargo new phase1-arbitrage-bot --bin
cd phase1-arbitrage-bot

# Study references
# - mev-template-rs: Main structure
# - amms-rs: Pool logic
# - crypto-arbitrage-analyzer: Detection logic
```

**Day 2 - Foundation**:
```bash
# Implement from mev-template-rs:
# - src/main.rs (event loop structure)
# - src/config.rs (configuration)
# - src/utils/logger.rs (logging)

# Implement from amms-rs:
# - src/pool/state.rs (state management)
# - src/pool/syncer.rs (with their pool logic)

# Test:
cargo test --lib pool
```

**Day 3 - Detection**:
```bash
# Implement from crypto-arbitrage-analyzer:
# - src/arbitrage/detector.rs (enhanced)

# Implement from amms-rs:
# - Price calculations
# - Slippage simulation

# Test:
cargo test --lib arbitrage::detector
```

**Day 4 - Execution**:
```bash
# Implement from mev-template-rs:
# - src/arbitrage/executor.rs (tx building)

# Test on Mumbai testnet:
cargo run --release
```

**Day 5 - Integration**:
```bash
# Full end-to-end testing
# Deploy to Mumbai
# Execute test trades
```

---

## Component Checklist

### ✅ From mev-template-rs:
- [ ] Event loop structure
- [ ] Logging setup
- [ ] Transaction building patterns
- [ ] Error handling
- [ ] Configuration management

### ✅ From amms-rs:
- [ ] Pool state structure
- [ ] Price calculation (exact formula)
- [ ] Slippage simulation
- [ ] Batch syncing with multicall
- [ ] Factory interaction

### ✅ From crypto-arbitrage-analyzer:
- [ ] Basic detection logic
- [ ] Price comparison
- [ ] Opportunity structure

### ✅ Custom Implementation:
- [ ] DexType enum
- [ ] State manager with DashMap
- [ ] Enhanced profitability calculator
- [ ] Trade size optimization
- [ ] Gas cost estimation
- [ ] Main event loop

### 📝 Phase 2 Preparation (study only):
- [ ] flashloan-rs builder pattern
- [ ] Flash loan contract structure
- [ ] Atomic execution pattern

---

## Next Steps

1. **Clone all reference repositories**
2. **Study key files identified above**
3. **Start with types.rs and config.rs**
4. **Implement pool management (copying amms-rs logic)**
5. **Add detection logic (enhancing crypto-arbitrage-analyzer)**
6. **Build executor (using mev-template-rs patterns)**
7. **Test end-to-end on testnet**
8. **Deploy to mainnet with small capital**

Each component builds on proven patterns from production systems, ensuring reliability while allowing customization for your specific arbitrage strategy.
