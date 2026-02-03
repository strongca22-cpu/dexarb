# Node Sync Deadtime Preparation Guide
## Maximize Productivity During 4-6 Hour Polygon Node Sync

**Context:** While your Polygon Bor node syncs from snapshot (4-6 hours), you have a perfect window for high-value preparation work that will dramatically improve your bot's performance once the node is live.

**Goal:** By the time the node is synced, you'll have everything ready to go from "node live" to "improved bot in production" in under 1 hour.

---

## Overview: What You're Doing

**Current State:**
- Server is provisioned and running
- Heimdall and Bor are syncing
- Bot is still using Alchemy RPC
- You have 4-6 hours of sync time

**Optimal Use of This Time:**
1. Build critical features that need the local node to function
2. Backfill historical data while you still have Alchemy access
3. Research and document strategy improvements
4. Set up monitoring infrastructure
5. Prepare migration checklist

---

## TIER 1: Critical Bot Improvements (Do These First)

### 1. Backfill Historical Data for Strategy Optimization
**Time Required:** 2-3 hours  
**Priority:** HIGHEST ROI

#### Why This Matters
- Identify which pools are most profitable
- Find optimal gas price strategies  
- Detect patterns in failed trades
- Build baseline for measuring improvement
- **This data becomes harder to get once you switch to local node**

#### Implementation

**Create backfill script:**

```bash
cd ~/your-bot-repo
mkdir -p scripts
nano scripts/backfill_historical_data.rs
```

**Script content:**

```rust
// scripts/backfill_historical_data.rs
use ethers::prelude::*;
use std::fs::File;
use std::io::Write;
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use Alchemy while you still have it
    let provider = Provider::<Http>::try_from(
        "https://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY"
    )?;
    
    let current_block = provider.get_block_number().await?.as_u64();
    let lookback_blocks = 100_000; // ~2 days at 2sec blocks (adjust as needed)
    
    println!("Starting backfill from block {} to {}", 
             current_block - lookback_blocks, 
             current_block);
    
    // Prepare CSV files
    let mut pool_states = File::create("data/pool_states.csv")?;
    let mut gas_prices = File::create("data/gas_prices.csv")?;
    let mut bot_txs = File::create("data/bot_performance.csv")?;
    
    // Write headers
    writeln!(pool_states, "block,timestamp,pool_address,token0,token1,reserve0,reserve1")?;
    writeln!(gas_prices, "block,timestamp,base_fee,priority_fee,gas_used")?;
    writeln!(bot_txs, "block,timestamp,tx_hash,profit,gas_cost,success")?;
    
    let start_block = current_block - lookback_blocks;
    
    for block_num in start_block..=current_block {
        // Fetch block with transactions
        if let Some(block) = provider.get_block_with_txs(block_num).await? {
            let timestamp = block.timestamp.as_u64();
            
            // Extract gas price data
            if let Some(base_fee) = block.base_fee_per_gas {
                writeln!(
                    gas_prices,
                    "{},{},{},{},{}",
                    block_num,
                    timestamp,
                    base_fee,
                    0, // Calculate avg priority fee from txs if needed
                    block.gas_used
                )?;
            }
            
            // Analyze transactions
            for tx in block.transactions {
                // Check if tx is to a DEX router
                if let Some(to) = tx.to {
                    // Add your DEX router addresses here
                    let dex_routers = vec![
                        "0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff", // QuickSwap
                        "0x1b02dA8Cb0d097eB8D57A175b88c7D8b47997506", // SushiSwap
                        // Add more...
                    ];
                    
                    let router_str = format!("{:?}", to);
                    if dex_routers.iter().any(|r| router_str.contains(r)) {
                        // This is a DEX trade - analyze it
                        // Decode swap parameters, calculate implied prices, etc.
                        // Store to pool_states.csv
                    }
                }
                
                // Check if tx is from your bot's wallet
                // if tx.from == YOUR_BOT_ADDRESS {
                //     // Analyze your own trades
                //     // Calculate profit/loss, gas costs
                //     // Store to bot_txs.csv
                // }
            }
        }
        
        // Progress indicator
        if block_num % 1000 == 0 {
            let progress = ((block_num - start_block) as f64 / lookback_blocks as f64) * 100.0;
            println!("Progress: {:.2}% (block {})", progress, block_num);
        }
        
        // Rate limiting (be nice to Alchemy)
        if block_num % 100 == 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
    
    println!("Backfill complete! Data saved to data/ directory");
    Ok(())
}
```

**Add to Cargo.toml:**

```toml
[[bin]]
name = "backfill"
path = "scripts/backfill_historical_data.rs"
```

**Run it:**

```bash
mkdir -p data
cargo run --release --bin backfill
```

**What You Get:**
- `data/pool_states.csv` - Historical DEX pool states
- `data/gas_prices.csv` - Gas price trends
- `data/bot_performance.csv` - Your bot's historical performance

**Analyze Later:**
```python
# Quick Python analysis
import pandas as pd
import matplotlib.pyplot as plt

gas = pd.read_csv('data/gas_prices.csv')
gas['base_fee_gwei'] = gas['base_fee'] / 1e9

plt.plot(gas['block'], gas['base_fee_gwei'])
plt.xlabel('Block Number')
plt.ylabel('Base Fee (Gwei)')
plt.title('Gas Price History')
plt.savefig('gas_analysis.png')
```

---

### 2. Build Mempool Transaction Parser
**Time Required:** 1-2 hours  
**Priority:** CRITICAL (enables new capabilities)

#### Why This Matters
- Core feature for utilizing `txpool_content` API
- Lets you see pending transactions before they're mined
- Competitive advantage: react to opportunities faster
- Foundation for advanced strategies (frontrunning detection, gas bidding)

#### Implementation

**Create mempool module:**

```bash
mkdir -p src/mempool
nano src/mempool/mod.rs
```

**Module code:**

```rust
// src/mempool/mod.rs

use ethers::prelude::*;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct MempoolMonitor {
    provider: Provider<Http>,
    target_routers: Vec<Address>,
}

impl MempoolMonitor {
    pub fn new(rpc_url: &str, routers: Vec<Address>) -> Result<Self, Box<dyn std::error::Error>> {
        let provider = Provider::<Http>::try_from(rpc_url)?;
        Ok(Self {
            provider,
            target_routers: routers,
        })
    }
    
    /// Fetch all pending transactions from txpool
    pub async fn fetch_pending_txs(&self) -> Result<HashMap<Address, Vec<Transaction>>, Box<dyn std::error::Error>> {
        // Call txpool_content RPC method
        let txpool_content: serde_json::Value = self.provider
            .request("txpool_content", ())
            .await?;
        
        // Parse pending transactions
        let pending = txpool_content["pending"]
            .as_object()
            .ok_or("Invalid txpool format")?;
        
        let mut dex_txs = HashMap::new();
        
        for (_address, txs) in pending.iter() {
            let tx_obj = txs.as_object().ok_or("Invalid tx format")?;
            
            for (_nonce, tx) in tx_obj.iter() {
                // Parse transaction
                let tx: Transaction = serde_json::from_value(tx.clone())?;
                
                // Filter: only DEX router interactions
                if let Some(to) = tx.to {
                    if self.target_routers.contains(&to) {
                        dex_txs
                            .entry(to)
                            .or_insert_with(Vec::new)
                            .push(tx);
                    }
                }
            }
        }
        
        Ok(dex_txs)
    }
    
    /// Get count of pending transactions (lighter weight)
    pub async fn get_mempool_status(&self) -> Result<MempoolStatus, Box<dyn std::error::Error>> {
        let status: serde_json::Value = self.provider
            .request("txpool_status", ())
            .await?;
        
        Ok(MempoolStatus {
            pending: status["pending"].as_u64().unwrap_or(0),
            queued: status["queued"].as_u64().unwrap_or(0),
        })
    }
    
    /// Extract swap parameters from transaction calldata
    pub fn extract_swap_params(&self, tx: &Transaction) -> Option<SwapParams> {
        // Decode calldata for common DEX methods:
        // - swapExactTokensForTokens
        // - swapTokensForExactTokens
        // - swapExactETHForTokens
        // etc.
        
        let input = &tx.input;
        if input.len() < 4 {
            return None;
        }
        
        // Method selector is first 4 bytes
        let selector = &input[0..4];
        
        // swapExactTokensForTokens selector: 0x38ed1739
        if selector == [0x38, 0xed, 0x17, 0x39] {
            // Decode parameters using ethers-rs ABI decoder
            // This is simplified - implement full ABI decoding
            return Some(SwapParams {
                method: "swapExactTokensForTokens".to_string(),
                token_in: Address::zero(), // Decode from calldata
                token_out: Address::zero(), // Decode from calldata
                amount_in: U256::zero(),    // Decode from calldata
                min_amount_out: U256::zero(), // Decode from calldata
                deadline: U256::zero(),     // Decode from calldata
            });
        }
        
        None
    }
    
    /// Detect potential arbitrage opportunities in mempool
    pub async fn scan_for_opportunities(&self) -> Result<Vec<ArbitrageOpportunity>, Box<dyn std::error::Error>> {
        let pending_txs = self.fetch_pending_txs().await?;
        let mut opportunities = Vec::new();
        
        for (router, txs) in pending_txs.iter() {
            for tx in txs {
                if let Some(swap_params) = self.extract_swap_params(tx) {
                    // Check if this swap creates an arbitrage opportunity
                    // Compare against current pool states
                    // If profitable, add to opportunities list
                    
                    // Placeholder logic:
                    if self.is_arbitrage_opportunity(&swap_params).await? {
                        opportunities.push(ArbitrageOpportunity {
                            triggering_tx: tx.hash,
                            router: *router,
                            estimated_profit: U256::from(1000000000000000u64), // Placeholder
                            gas_price_to_beat: tx.gas_price.unwrap_or_default(),
                        });
                    }
                }
            }
        }
        
        Ok(opportunities)
    }
    
    async fn is_arbitrage_opportunity(&self, _swap: &SwapParams) -> Result<bool, Box<dyn std::error::Error>> {
        // Implement your arbitrage detection logic
        // This would check pool states, calculate profitability, etc.
        todo!("Implement based on your strategy")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolStatus {
    pub pending: u64,
    pub queued: u64,
}

#[derive(Debug, Clone)]
pub struct SwapParams {
    pub method: String,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub min_amount_out: U256,
    pub deadline: U256,
}

#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub triggering_tx: H256,
    pub router: Address,
    pub estimated_profit: U256,
    pub gas_price_to_beat: U256,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    #[ignore] // Only run when local node is available
    async fn test_mempool_fetch() {
        let routers = vec![
            "0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff".parse().unwrap(), // QuickSwap
        ];
        
        let monitor = MempoolMonitor::new("http://localhost:8545", routers).unwrap();
        let status = monitor.get_mempool_status().await.unwrap();
        
        println!("Mempool status: {:?}", status);
        assert!(status.pending > 0 || status.queued >= 0);
    }
    
    #[tokio::test]
    #[ignore]
    async fn test_scan_opportunities() {
        let routers = vec![
            "0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff".parse().unwrap(),
        ];
        
        let monitor = MempoolMonitor::new("http://localhost:8545", routers).unwrap();
        let opportunities = monitor.scan_for_opportunities().await.unwrap();
        
        println!("Found {} opportunities", opportunities.len());
    }
}
```

**Add to your main lib.rs:**

```rust
// src/lib.rs
pub mod mempool;
```

**Test structure ready** - will work once local node is synced.

---

### 3. Gas Price Optimization Strategy
**Time Required:** 1 hour  
**Priority:** HIGH (direct profit impact)

#### Why This Matters
- Better gas strategies = higher win rate
- Local node gives you visibility into competitor gas prices
- Can implement dynamic bidding instead of static prices

#### Create Strategy Document

```bash
mkdir -p docs
nano docs/gas_optimization.md
```

**Document content:**

```markdown
# Gas Price Optimization Strategy for Local Node

## Current Approach (Alchemy-Based)

**Method:**
- Use `eth_gasPrice` RPC (returns network average)
- Add fixed 20% buffer
- Submit transaction with buffered price

**Problems:**
- Network average is too low for competitive arb
- Fixed buffer doesn't account for competition level
- No visibility into what other bots are bidding
- Success rate: ~60% (40% of txs fail or get frontrun)

**Example:**
```
Network avg: 100 Gwei
Our bid: 120 Gwei (100 * 1.2)
Competitor bid: 150 Gwei
Result: We lose the opportunity
```

---

## New Approach (Local Node + Mempool)

### Strategy 1: Dynamic Gas Bumping (Implement First)

**Concept:** Monitor mempool for competing transactions targeting the same opportunity.

**Implementation:**

```rust
// Pseudocode
pub async fn submit_with_dynamic_gas(
    &self,
    opportunity: ArbitrageOpportunity,
) -> Result<TxHash> {
    // 1. Calculate base gas price (current network conditions)
    let base_gas = self.get_current_base_fee().await?;
    let priority_fee = U256::from(30_000_000_000u64); // 30 Gwei starting point
    
    // 2. Check mempool for competing transactions
    let competitors = self.find_competing_txs(&opportunity).await?;
    
    // 3. If competitors exist, outbid them
    let final_gas = if !competitors.is_empty() {
        let max_competitor_gas = competitors
            .iter()
            .map(|tx| tx.gas_price)
            .max()
            .unwrap_or(base_gas);
        
        // Bid 50% higher than highest competitor
        max_competitor_gas * 150 / 100
    } else {
        base_gas + priority_fee
    };
    
    // 4. Safety checks
    if final_gas > MAX_GAS_PRICE {
        return Err("Gas price too high, opportunity not profitable");
    }
    
    // 5. Submit transaction
    self.send_transaction_with_gas(opportunity, final_gas).await
}
```

**Advantages:**
- Only pay high gas when necessary (competition exists)
- Save on gas costs when no competition
- Higher win rate in contested scenarios

**Expected Improvement:**
- Success rate: 60% → 75-85%
- Average gas savings: 15-20%

---

### Strategy 2: Priority Fee Targeting (Add After 1 Week)

**Concept:** Target specific position in block (e.g., top 10%) using EIP-1559 priority fees.

**How Polygon Validators Prioritize:**
1. Transactions sorted by `maxPriorityFeePerGas` (tip to validator)
2. Higher tips → earlier in block
3. First 10-20 transactions in block have highest success rate

**Implementation:**

```rust
pub fn calculate_priority_fee(
    &self,
    target_position: BlockPosition,
    current_mempool: &MempoolStatus,
) -> U256 {
    match target_position {
        BlockPosition::Top10Percent => {
            // Analyze recent blocks to find 90th percentile priority fee
            let historical_fees = self.get_recent_priority_fees(100).await?;
            historical_fees.percentile(90) * 110 / 100 // +10% buffer
        }
        BlockPosition::Top25Percent => {
            let historical_fees = self.get_recent_priority_fees(100).await?;
            historical_fees.percentile(75) * 105 / 100
        }
        BlockPosition::Median => {
            self.get_current_median_priority_fee().await?
        }
    }
}

pub enum BlockPosition {
    Top10Percent,  // Use for high-value arb (>0.01 MATIC profit)
    Top25Percent,  // Use for medium arb (>0.005 MATIC)
    Median,        // Use for small arb (>0.002 MATIC)
}
```

**When to Use:**
- High-value opportunities (>$5 profit): Top 10%
- Medium opportunities ($2-5): Top 25%
- Small opportunities (<$2): Median

---

### Strategy 3: Validator Bundles (Research Phase)

**Status:** Investigate if available on Polygon

**Concept:** Submit transaction bundles directly to validators via:
- Flashbots Protect (if available on Polygon)
- Eden Network (if supports Polygon)
- Direct validator RPC endpoints

**Advantages:**
- Guarantee execution or full revert (no wasted gas)
- No public mempool exposure
- Can include complex multi-transaction strategies

**Research Questions:**
1. Does Polygon support Flashbots/MEV-Boost?
2. Which validators accept bundle submissions?
3. What's the API format?
4. Is there a builder marketplace?

**Action Items:**
- [ ] Check Polygon docs for MEV infrastructure
- [ ] Ask in Polygon Discord #validators channel
- [ ] Test if Flashbots RPC works on Polygon
- [ ] Contact major validators directly

**If Available:**
- Implement bundle submission as Strategy 3
- Use for high-value multi-step arbitrage
- Expected success rate: 90%+

---

## Gas Price Thresholds & Safety Limits

```rust
pub struct GasLimits {
    // Minimum profit to attempt trade (after gas costs)
    pub min_profit_wei: U256,
    
    // Maximum gas price willing to pay
    pub max_gas_gwei: U256,
    
    // Emergency cutoff (network congestion)
    pub emergency_cutoff_gwei: U256,
}

impl Default for GasLimits {
    fn default() -> Self {
        Self {
            min_profit_wei: U256::from(5_000_000_000_000_000u64), // 0.005 MATIC
            max_gas_gwei: U256::from(500_000_000_000u64),         // 500 Gwei
            emergency_cutoff_gwei: U256::from(1000_000_000_000u64), // 1000 Gwei
        }
    }
}

pub fn is_trade_profitable(
    estimated_profit: U256,
    gas_price: U256,
    estimated_gas_units: U256,
) -> bool {
    let gas_cost = gas_price * estimated_gas_units;
    let net_profit = estimated_profit.saturating_sub(gas_cost);
    
    net_profit >= GasLimits::default().min_profit_wei
}
```

**Adjust based on market conditions:**
- High volatility: Increase min_profit (more opportunities, be selective)
- Low volatility: Decrease min_profit (fewer opportunities, take what's available)
- Gas spike: Increase emergency_cutoff (protect from network attacks)

---

## Implementation Timeline

### Week 1: Dynamic Gas Bumping
- [x] Document strategy
- [ ] Implement mempool competitor detection
- [ ] Implement dynamic bidding logic
- [ ] Test in dry-run mode (1-2 days)
- [ ] Enable for live trading
- [ ] Monitor success rate improvement

**Success Metrics:**
- Success rate increases from 60% to 75%+
- Average gas cost decreases by 10-15%
- No unprofitable trades due to high gas

### Week 2-3: Priority Fee Targeting
- [ ] Collect historical priority fee data
- [ ] Implement percentile calculations
- [ ] Add position-based bidding
- [ ] Backtest against historical data
- [ ] Deploy to production

**Success Metrics:**
- 80-85% success rate on high-value opportunities
- Optimal gas spending (not overpaying for small arbs)

### Week 4+: Validator Bundles (If Available)
- [ ] Complete research phase
- [ ] Test bundle submission
- [ ] Implement bundle builder
- [ ] Compare results vs mempool strategy

**Success Metrics:**
- 90%+ success rate on bundled transactions
- Zero wasted gas on failed transactions
- Ability to execute complex multi-hop arb

---

## Monitoring & Adjustment

**Create dashboard to track:**

```
Gas Strategy Performance
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Total Trades: 1,234
Successful: 987 (80%)
Failed: 247 (20%)

Gas Spending
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Total Gas Spent: 12.5 MATIC
Average per Trade: 0.01 MATIC
Vs. Baseline (fixed 120 Gwei): -15% (saved 1.8 MATIC)

Success by Strategy
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Dynamic Bumping: 82% (723/882)
No Competition: 95% (264/278)
High Competition: 65% (459/604)

Profitability
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Gross Profit: 45.2 MATIC
Gas Costs: 12.5 MATIC
Net Profit: 32.7 MATIC
ROI: 261%
```

**Weekly review questions:**
1. Is success rate improving?
2. Are we overpaying for gas?
3. Which strategy works best for which opportunities?
4. Should we adjust thresholds?

---

## Emergency Fallback

**If gas optimization causes issues:**

```rust
// Revert to simple strategy
pub fn fallback_gas_strategy(&self) -> U256 {
    let base_fee = self.get_current_base_fee().await?;
    base_fee * 120 / 100 // Simple 20% buffer
}
```

**When to use fallback:**
- Network anomalies (unusual gas spikes)
- Mempool parsing errors
- Testing new features
- First day after migration to local node

---

## Key Takeaways

1. **Start simple** - Dynamic bumping is easiest, biggest impact
2. **Measure everything** - Track success rates and gas costs
3. **Be patient** - Collect 1 week of data before adding Strategy 2
4. **Safety first** - Always enforce max gas limits
5. **Stay flexible** - Market conditions change, strategy should adapt
```

---

### 4. Expand Pool Whitelist Research
**Time Required:** 30 minutes  
**Priority:** MEDIUM (prepares for capacity increase)

**Current limitation:** Small whitelist due to Alchemy rate limits  
**With local node:** Can monitor ALL major pools simultaneously

**Create research doc:**

```bash
nano docs/pool_expansion.md
```

```markdown
# Pool Whitelist Expansion Plan

## Current Whitelist (Alchemy-Limited)
- QuickSwap V2: 0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff
- QuickSwap V3: 0xf5b509bB0909a69B1c207E495f687a596C168E12

**Reason for limited list:** Alchemy rate limits (300 req/s max)

---

## Expanded Whitelist (Local Node)

### Tier 1: High Volume DEXs (Add First)

**QuickSwap (Already monitoring)**
- V2 Router: 0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff
- V3 Router: 0xf5b509bB0909a69B1c207E495f687a596C168E12
- Daily Volume: $50-100M
- Fee Tiers: 0.01%, 0.05%, 0.3%, 1%

**Uniswap V3**
- Router: 0xE592427A0AEce92De3Edee1F18E0157C05861564
- Daily Volume: $30-50M
- Fee Tiers: 0.01%, 0.05%, 0.3%, 1%
- Status: Add immediately

**SushiSwap**
- Router: 0x1b02dA8Cb0d097eB8D57A175b88c7D8b47997506
- Daily Volume: $10-20M
- Fee: 0.3%
- Status: Add immediately

### Tier 2: Medium Volume DEXs (Add After 1 Week)

**Balancer V2**
- Vault: 0xBA12222222228d8Ba445958a75a0704d566BF2C8
- Daily Volume: $5-15M
- Pool Types: Weighted, Stable, MetaStable
- Status: Add after validating Tier 1 performance

**Curve**
- Router: Various (pool-specific)
- Daily Volume: $5-10M
- Focus: Stablecoin pools (USDC/USDT/DAI)
- Status: Add for stablecoin arb strategies

**DODO**
- Router: 0xa222e6a71D1A1Dd5F279805fbe38d5329C1d0e70
- Daily Volume: $2-5M
- Unique: PMM (Proactive Market Maker) algorithm
- Status: Research further, may have unique opportunities

### Tier 3: Specialized DEXs (Research Phase)

**Algebra (QuickSwap's AMM)**
- Concentrated liquidity
- Dynamic fees
- Volume: Included in QuickSwap stats
- Status: May already be covered by QuickSwap monitoring

**Dystopia**
- ve(3,3) model
- Low fees (0.01-0.05%)
- Daily Volume: $1-3M
- Status: Monitor for stable pair arb

**Retro**
- Fork of Velodrome
- Vote-escrowed model
- Daily Volume: $500K-2M
- Status: Low priority, revisit if volume increases

---

## Pool Selection Criteria

**Include pool if:**
- ✅ Daily volume >$1M
- ✅ Sufficient liquidity (>$100K TVL)
- ✅ Actively traded (>100 swaps/day)
- ✅ Supports common token pairs (MATIC, ETH, USDC, USDT)
- ✅ Reliable on-chain data (no oracle dependencies)

**Exclude pool if:**
- ❌ Low volume (<$500K daily)
- ❌ High slippage (>2% for typical trade sizes)
- ❌ Exotic tokenomics (rebase tokens, fee-on-transfer)
- ❌ Unaudited contracts
- ❌ Centralization risks (admin keys, upgradeable)

---

## Implementation Plan

### Phase 1: Double Current Coverage (Week 1)
```rust
const POOL_WHITELIST: &[&str] = &[
    // Existing
    "0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff", // QuickSwap V2
    "0xf5b509bB0909a69B1c207E495f687a596C168E12", // QuickSwap V3
    
    // New Tier 1
    "0xE592427A0AEce92De3Edee1F18E0157C05861564", // Uniswap V3
    "0x1b02dA8Cb0d097eB8D57A175b88c7D8b47997506", // SushiSwap
];
```

**Expected Impact:**
- Opportunities detected: +50-100%
- Profitable trades: +30-50%
- No additional costs (local node = unlimited queries)

### Phase 2: Add Tier 2 (Week 2)
```rust
    // Tier 2
    "0xBA12222222228d8Ba445958a75a0704d566BF2C8", // Balancer V2
    // Curve pools (add specific pools as identified)
```

### Phase 3: Specialized Strategies (Week 3+)
- Stablecoin arbitrage (Curve focus)
- Multi-hop routing (A→B→C→A)
- Flash loan opportunities

---

## Monitoring Per-Pool Performance

**Track for each pool:**
```
Pool: QuickSwap V2 MATIC/USDC
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Opportunities Detected: 1,234
Attempts: 456
Successful: 398 (87%)
Total Profit: 12.5 MATIC
Avg Profit/Trade: 0.031 MATIC
Best Trade: 0.45 MATIC
Worst Trade: -0.02 MATIC (gas loss)
```

**Remove pool if:**
- Success rate <50% for 7 days
- Average profit <gas costs
- Volume drops below $500K/day
- Contract upgraded/changed

---

## Token Pair Priority

**Focus on liquid pairs first:**

**Tier 1 Pairs (Highest Priority):**
- MATIC/USDC
- MATIC/USDT
- MATIC/ETH
- USDC/USDT
- ETH/USDC

**Tier 2 Pairs:**
- MATIC/DAI
- WBTC/ETH
- WBTC/USDC
- Popular governance tokens (AAVE, LINK, etc.)

**Avoid (Initially):**
- Low-liquidity pairs (<$50K TVL)
- Exotic/meme tokens
- Tokens with transfer fees
- Pairs with <10 swaps/day

---

## Risk Management

**Per-pool limits:**
```rust
pub struct PoolLimits {
    max_trade_size: U256,        // Max amount to trade in single tx
    max_exposure: U256,           // Max total capital at risk in pool
    min_liquidity: U256,          // Don't trade if pool liquidity below this
    max_price_impact: f64,        // Don't trade if price impact >X%
}

impl Default for PoolLimits {
    fn default() -> Self {
        Self {
            max_trade_size: U256::from(1_000_000_000_000_000_000u64), // 1 MATIC
            max_exposure: U256::from(10_000_000_000_000_000_000u64),   // 10 MATIC
            min_liquidity: U256::from(100_000_000_000_000_000_000u64), // 100 MATIC
            max_price_impact: 0.01, // 1%
        }
    }
}
```

**Adjust based on pool characteristics:**
- High volume pools: Increase limits
- New/untested pools: Decrease limits
- Stablecoin pools: Higher max_trade_size (lower price impact)

---

## Next Steps

1. **Week 1:** Add Uniswap V3 + SushiSwap
2. **Monitor:** Collect 1 week of performance data
3. **Week 2:** Add Balancer if Week 1 successful
4. **Week 3:** Consider Curve for stablecoin strategies
5. **Month 2:** Evaluate Tier 3 / specialized DEXs

**Goal:** 10-15 high-quality pools within 1 month
**Expected result:** 3-5x increase in profitable opportunities
```

Save this for reference when configuring post-sync.

---

## TIER 2: Infrastructure & Monitoring

### 5. Set Up Alerting System
**Time Required:** 30 minutes  
**Priority:** HIGH (prevents costly downtime)

**Create alert script:**

```bash
mkdir -p ~/alerts
nano ~/alerts/node-health-monitor.sh
```

**Script content:**

```bash
#!/bin/bash

# Configuration
EMAIL="your-email@example.com"
TELEGRAM_BOT_TOKEN=""  # Optional: Add if you set up Telegram bot
TELEGRAM_CHAT_ID=""    # Your Telegram chat ID

# Function to send alerts
send_alert() {
    local message=$1
    local severity=$2  # INFO, WARNING, CRITICAL
    
    # Email alert
    echo "[${severity}] ${message}" | mail -s "Polygon Node Alert - ${severity}" $EMAIL
    
    # Telegram alert (if configured)
    if [ -n "$TELEGRAM_BOT_TOKEN" ] && [ -n "$TELEGRAM_CHAT_ID" ]; then
        curl -s -X POST "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/sendMessage" \
            -d chat_id=$TELEGRAM_CHAT_ID \
            -d text="🚨 [${severity}] ${message}"
    fi
    
    # Log to file
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] [${severity}] ${message}" >> ~/alerts/alert.log
}

# Check if node is syncing (should be false when fully synced)
SYNCING=$(curl -s -X POST -H "Content-Type: application/json" \
    --data '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' \
    http://localhost:8545 | jq -r '.result')

if [ "$SYNCING" != "false" ]; then
    send_alert "Node is out of sync! Currently syncing." "CRITICAL"
fi

# Check peer count
PEERS=$(curl -s -X POST -H "Content-Type: application/json" \
    --data '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' \
    http://localhost:8545 | jq -r '.result')

if [ -n "$PEERS" ]; then
    PEERS_DEC=$((16#${PEERS#0x}))
    
    if [ $PEERS_DEC -lt 10 ]; then
        send_alert "CRITICAL: Very low peer count: $PEERS_DEC (should be 40-80)" "CRITICAL"
    elif [ $PEERS_DEC -lt 20 ]; then
        send_alert "WARNING: Low peer count: $PEERS_DEC (should be 40-80)" "WARNING"
    fi
fi

# Check block freshness
BLOCK_HEX=$(curl -s -X POST -H "Content-Type: application/json" \
    --data '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["latest", false],"id":1}' \
    http://localhost:8545 | jq -r '.result.timestamp')

if [ -n "$BLOCK_HEX" ] && [ "$BLOCK_HEX" != "null" ]; then
    BLOCK_TIME=$((16#${BLOCK_HEX#0x}))
    NOW=$(date +%s)
    AGE=$((NOW - BLOCK_TIME))
    
    if [ $AGE -gt 60 ]; then
        send_alert "CRITICAL: Latest block is ${AGE}s old. Node may be stalled!" "CRITICAL"
    elif [ $AGE -gt 30 ]; then
        send_alert "WARNING: Latest block is ${AGE}s old (should be <10s)" "WARNING"
    fi
fi

# Check disk usage
DISK_USAGE=$(df -h /mnt/polygon-data 2>/dev/null | tail -1 | awk '{print $5}' | sed 's/%//')
if [ -n "$DISK_USAGE" ]; then
    if [ $DISK_USAGE -gt 90 ]; then
        send_alert "CRITICAL: Disk usage at ${DISK_USAGE}%! Add storage immediately." "CRITICAL"
    elif [ $DISK_USAGE -gt 85 ]; then
        send_alert "WARNING: Disk usage at ${DISK_USAGE}%. Plan for expansion." "WARNING"
    fi
fi

# Check if Bor process is running
if ! pgrep -f "bor server" > /dev/null; then
    send_alert "CRITICAL: Bor process is not running!" "CRITICAL"
fi

# Check if Heimdall process is running
if ! pgrep -f "heimdall start" > /dev/null; then
    send_alert "CRITICAL: Heimdall process is not running!" "CRITICAL"
fi

# Check if bot process is running (adjust process name)
if ! pgrep -f "your-bot-name" > /dev/null; then
    send_alert "WARNING: Bot process is not running!" "WARNING"
fi

# Check system resources
CPU_USAGE=$(top -bn1 | grep "Cpu(s)" | sed "s/.*, *\([0-9.]*\)%* id.*/\1/" | awk '{print 100 - $1}')
CPU_INT=${CPU_USAGE%.*}

if [ $CPU_INT -gt 95 ]; then
    send_alert "WARNING: High CPU usage: ${CPU_INT}%" "WARNING"
fi

MEM_USAGE=$(free | grep Mem | awk '{printf "%.0f", $3/$2 * 100}')
if [ $MEM_USAGE -gt 90 ]; then
    send_alert "WARNING: High memory usage: ${MEM_USAGE}%" "WARNING"
fi

# All checks passed
if [ $? -eq 0 ]; then
    # Only log success, don't send alert (avoid spam)
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] [INFO] All health checks passed" >> ~/alerts/alert.log
fi
```

**Make executable:**

```bash
chmod +x ~/alerts/node-health-monitor.sh
```

**Set up email (if not already installed):**

```bash
sudo apt install -y mailutils
```

**Test the script:**

```bash
./alerts/node-health-monitor.sh
# Check ~/alerts/alert.log for output
```

**Add to cron (run every 5 minutes):**

```bash
crontab -e

# Add this line:
*/5 * * * * /home/polygon/alerts/node-health-monitor.sh
```

**Optional: Set up Telegram alerts**

1. Create Telegram bot via @BotFather
2. Get your bot token
3. Get your chat ID (message your bot, then visit: `https://api.telegram.org/bot<YOUR_BOT_TOKEN>/getUpdates`)
4. Add token and chat ID to the script

**You now have 24/7 monitoring with instant alerts!**

---

### 6. Create System Architecture Documentation
**Time Required:** 30 minutes  
**Priority:** MEDIUM (helps future debugging)

```bash
nano docs/ARCHITECTURE.md
```

**Content:**

```markdown
# System Architecture Documentation

## Overview

This document describes the complete architecture of our Polygon arbitrage trading system running on a Hetzner dedicated server.

---

## Component Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Hetzner AX102 Server                         │
│                    (Falkenstein, Germany - FSN1)                     │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │                  Heimdall (Consensus Layer)                 │    │
│  │                                                              │    │
│  │  Port 26657: RPC (localhost only)                          │    │
│  │  Port 26656: P2P (public - validator communication)        │    │
│  │  Port 26660: Prometheus metrics                            │    │
│  │                                                              │    │
│  │  Purpose: Syncs checkpoints from Ethereum L1                │    │
│  │  Data: /mnt/polygon-data/heimdall                          │    │
│  └──────────────────────┬───────────────────────────────────────┘    │
│                         │ (provides consensus)                        │
│                         ▼                                             │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │              Bor (Execution Layer - Full Node)              │    │
│  │                                                              │    │
│  │  Port 8545: HTTP RPC (localhost:8545)                      │    │
│  │  Port 8546: WebSocket RPC (localhost:8546)                 │    │
│  │  Port 30303: P2P (public - peer network)                   │    │
│  │  Port 7071: Prometheus metrics                              │    │
│  │                                                              │    │
│  │  Mode: Full node (archive mode)                            │    │
│  │  APIs: eth, net, web3, txpool, bor                         │    │
│  │  Data: /mnt/polygon-data/bor (~600GB, growing)             │    │
│  │                                                              │    │
│  │  Key Features:                                              │    │
│  │  - Unfiltered mempool access (txpool_content)              │    │
│  │  - Historical state (archive mode)                          │    │
│  │  - 0ms RPC latency (localhost)                             │    │
│  │  - Unlimited request rate                                   │    │
│  └──────────────────────┬───────────────────────────────────────┘    │
│                         │ (provides blockchain data)                  │
│                         ▼                                             │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │              Rust Arbitrage Bot (Your Code)                 │    │
│  │                                                              │    │
│  │  Connects to: localhost:8545 (HTTP), localhost:8546 (WS)   │    │
│  │  Port 9090: Prometheus metrics (optional)                   │    │
│  │                                                              │    │
│  │  Core Functions:                                            │    │
│  │  1. Monitor mempool for DEX trades (txpool_content)        │    │
│  │  2. Subscribe to new blocks (eth_subscribe)                │    │
│  │  3. Calculate arbitrage opportunities                       │    │
│  │  4. Submit transactions (eth_sendRawTransaction)           │    │
│  │                                                              │    │
│  │  Working Directory: /home/polygon/your-bot-repo            │    │
│  └────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │           Monitoring & Alerting (Optional)                  │    │
│  │                                                              │    │
│  │  - Prometheus: Scrapes metrics from Bor, Heimdall, Bot     │    │
│  │  - Grafana: Visualizes dashboards (port 3000)              │    │
│  │  - Alert Script: ~/alerts/node-health-monitor.sh           │    │
│  │    Runs every 5 min via cron                                │    │
│  │                                                              │    │
│  └────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Data Flow: Block Production to Arb Execution

```
1. Polygon Validator Network
   ↓ (produces new block every 2 seconds)
   
2. Block propagated via P2P network (port 30303)
   ↓ (~50-200ms to reach our node in Falkenstein)
   
3. Bor receives block
   ↓ (validates, adds to chain, updates state)
   
4. Bot receives notification via WebSocket (eth_subscribe)
   ↓ (<1ms localhost latency)
   
5. Bot queries pool states (eth_call to pool contracts)
   ↓ (<1ms per call)
   
6. Bot calculates arbitrage opportunity
   ↓ (computational time: 1-5ms)
   
7. Bot submits transaction (eth_sendRawTransaction)
   ↓ (enters local mempool)
   
8. Bor propagates tx to network
   ↓ (P2P gossip, 50-500ms)
   
9. Validator includes tx in next block
   ↓ (2-4 seconds from submission)
   
10. Profit extracted ✅
```

**Total latency: Block arrival → Transaction submitted: 5-15ms**  
*(Compare to Alchemy: 200-300ms)*

---

## Network Topology

```
┌──────────────────────────────────────────────────────────┐
│                  Internet / WAN                           │
└────────┬─────────────────────────────────────────┬────────┘
         │                                           │
         │ (P2P connections)                         │ (SSH access)
         │                                           │
    ┌────▼────────────────────────────────────────┐ │
    │   Polygon Validator Network                 │ │
    │                                              │ │
    │  - Heimdall validators (consensus)          │ │
    │  - Bor validators (block production)        │ │
    │  - Peer nodes (propagation)                 │ │
    │                                              │ │
    │  Location: Primarily Hetzner DE, AWS EU     │ │
    │  Latency to our node: 5-15ms                │ │
    └────┬──────────────────────────────────────────┘ │
         │                                             │
         │ (ports 26656, 30303)                        │
         │                                             │
    ┌────▼──────────────────────────────────────────┐ │
    │         Hetzner Firewall (ufw)                │ │
    │                                                │ │
    │  ALLOW: 22 (SSH), 26656 (Heimdall P2P),      │ │
    │         30303 (Bor P2P)                       │ │
    │  DENY:  8545, 8546 (RPC - localhost only)    │ │
    └────┬───────────────────────────────────────────┘ │
         │                                             │
         ▼                                             ▼
    [Heimdall] ←→ [Bor] ←→ [Bot]            [SSH from your machine]
    
    All RPC traffic: localhost only (127.0.0.1)
```

---

## Security Architecture

### Firewall Rules

```bash
# SSH (adjust port if changed)
22/tcp: ALLOW from anywhere

# Heimdall P2P
26656/tcp: ALLOW from anywhere (validator communication)

# Bor P2P
30303/tcp: ALLOW from anywhere (peer synchronization)
30303/udp: ALLOW from anywhere (peer discovery)

# RPC Endpoints - LOCALHOST ONLY
8545/tcp: BIND to 127.0.0.1 only (HTTP RPC)
8546/tcp: BIND to 127.0.0.1 only (WebSocket RPC)

# Metrics (if exposed)
7071/tcp: BIND to 127.0.0.1 only (Bor metrics)
26660/tcp: BIND to 127.0.0.1 only (Heimdall metrics)
```

### Access Control

**SSH:**
- ✅ Public key authentication only
- ❌ Password authentication disabled
- ❌ Root login disabled
- ✅ Non-root user (polygon) with sudo

**RPC:**
- ✅ Only accessible from localhost
- ❌ No public exposure (prevents abuse, DDoS, unauthorized access)
- ✅ Bot connects via localhost:8545/8546

**Private Keys:**
- ✅ Bot wallet private key stored in `.env` (chmod 600)
- ❌ Never committed to git
- ✅ Separate hot wallet with limited funds

---

## Storage Layout

```
/mnt/polygon-data/        (1.92TB NVMe - dedicated partition)
├── bor/                  (~600GB, growing ~50GB/month)
│   ├── bor/
│   │   ├── chaindata/    (LevelDB - blockchain state)
│   │   ├── nodes/        (peer discovery data)
│   │   └── transactions.rlp (txpool journal)
│   └── config.toml
│
└── heimdall/             (~50GB)
    ├── data/
    │   └── checkpoints/  (consensus checkpoints)
    └── config/
        ├── genesis.json
        └── config.toml

/home/polygon/            (OS partition)
├── your-bot-repo/        (Rust bot code)
│   ├── src/
│   ├── .env              (private keys, config)
│   └── target/release/   (compiled binary)
│
├── alerts/               (monitoring scripts)
│   ├── node-health-monitor.sh
│   └── alert.log
│
└── data/                 (historical data, backtests)
    ├── pool_states.csv
    ├── gas_prices.csv
    └── bot_performance.csv
```

---

## Process Management (systemd)

All critical services managed by systemd for auto-restart:

```
heimdall.service
├── Starts: Heimdall consensus node
├── Depends on: network.target
├── Restart: on-failure (10 sec delay)
└── Logs: sudo journalctl -u heimdall -f

bor.service
├── Starts: Bor execution node
├── Depends on: heimdall.service
├── Restart: on-failure (10 sec delay)
└── Logs: sudo journalctl -u bor -f

polygon-bot.service (optional)
├── Starts: Your Rust arbitrage bot
├── Depends on: bor.service
├── Restart: on-failure (10 sec delay)
└── Logs: sudo journalctl -u polygon-bot -f
```

**Why systemd?**
- Auto-restart on crash or server reboot
- Dependency management (Bor won't start without Heimdall)
- Centralized logging (journalctl)
- Easy start/stop/restart commands

---

## Resource Usage (Typical)

```
Component         CPU (avg)    RAM        Disk I/O      Network
─────────────────────────────────────────────────────────────────
Heimdall          5-10%        4-8 GB     Low           5-10 Mbps
Bor               10-30%       32-64 GB   Medium-High   20-50 Mbps
Bot               1-5%         <1 GB      Low           <1 Mbps
OS + Monitoring   2-5%         2-4 GB     Low           <1 Mbps
─────────────────────────────────────────────────────────────────
TOTAL             18-50%       40-75 GB   Variable      25-60 Mbps

Available Headroom:
- CPU: 50-82% free (16 cores, asymmetric - 3D V-Cache helps)
- RAM: 53-88 GB free (128 GB total)
- Disk: 1.1-1.3 TB free (1.92 TB total)
- Network: 940+ Mbps free (1 Gbit/s total)
```

**Performance Notes:**
- CPU spikes to 60-80% during:
  - Initial sync
  - High network activity (many concurrent swaps)
  - Bot backtesting
- RAM usage grows over time (caching), but stable
- Disk grows ~50GB/month (archive mode)

---

## Failover & Recovery Strategy

### Scenario 1: Bor Node Crashes

**Detection:** Alert script detects process not running  
**Auto-recovery:** systemd restarts Bor within 10 seconds  
**Bot behavior:** Waits for RPC to come back online  
**Manual action:** None (unless repeated crashes)

### Scenario 2: Node Falls Out of Sync

**Detection:** Alert script detects `eth_syncing` returns true  
**Auto-recovery:** Bor resyncs automatically (usually <10 min)  
**Bot behavior:** Pauses trading until sync complete  
**Manual action:** Investigate if sync takes >30 min

### Scenario 3: Local Node Complete Failure

**Fallback plan:**

1. Bot detects RPC connection failure (timeout >5 sec)
2. Bot switches to Alchemy backup endpoint (in `.env.backup`)
3. Alert sent to operator
4. Manual intervention to fix node
5. Bot switches back to local node once restored

**Implementation:**

```rust
// In bot config
pub struct RpcConfig {
    primary: String,    // "http://localhost:8545"
    fallback: String,   // "https://polygon-mainnet.g.alchemy.com/v2/..."
    current: String,
}

impl RpcConfig {
    pub async fn get_provider(&mut self) -> Provider<Http> {
        // Try primary
        match Provider::try_from(&self.primary) {
            Ok(provider) => {
                self.current = self.primary.clone();
                provider
            }
            Err(_) => {
                // Fall back to Alchemy
                warn!("Primary RPC failed, using fallback");
                self.current = self.fallback.clone();
                Provider::try_from(&self.fallback).unwrap()
            }
        }
    }
}
```

### Scenario 4: Disk Full

**Detection:** Alert when disk >85% full  
**Prevention:**
- Monitoring every 5 min
- Alerts at 85% (warning) and 90% (critical)
- Weekly review of disk growth rate

**Action plan:**
1. If >90% full: Stop Bor, take snapshot, delete old data, restart
2. If trend shows full within 30 days: Plan upgrade or switch to full mode
3. Emergency: Add external NVMe via Hetzner

---

## Monitoring & Metrics

### Health Checks (Every 5 Minutes)

```bash
~/alerts/node-health-monitor.sh checks:

✓ Sync status (eth_syncing should be false)
✓ Peer count (should be 40-80)
✓ Block freshness (<10 seconds old)
✓ Disk usage (<85%)
✓ Process status (Bor, Heimdall, Bot running)
✓ System resources (CPU <95%, RAM <90%)
```

### Prometheus Metrics (Optional)

**Bor metrics (port 7071):**
- `chain_head_block` - Current block height
- `txpool_pending` - Pending transactions
- `txpool_queued` - Queued transactions
- `p2p_peers` - Connected peers
- `rpc_requests_total` - RPC call count

**Heimdall metrics (port 26660):**
- `heimdall_latest_block_number`
- `heimdall_validators_count`
- `heimdall_sync_catching_up`

**Bot metrics (port 9090 - if you implement):**
- `bot_trades_total{status="success|failed"}`
- `bot_profit_total_wei`
- `bot_gas_spent_total_wei`
- `bot_opportunities_detected`

---

## Deployment Checklist

### Initial Setup (Once)
- [x] Order Hetzner AX102 server
- [x] Configure SSH key authentication
- [x] Harden SSH (disable password auth, change port if desired)
- [x] Set up firewall (ufw)
- [x] Create polygon user
- [x] Install dependencies (Go, Rust, etc.)

### Node Setup (Once)
- [x] Install Heimdall & Bor
- [x] Download snapshots
- [x] Configure Heimdall (seeds, config.toml)
- [x] Configure Bor (RPC, P2P, txpool)
- [x] Create systemd services
- [x] Start Heimdall, wait for sync
- [x] Start Bor, wait for sync

### Bot Deployment (Once)
- [x] Clone bot repository
- [x] Build in release mode
- [x] Configure .env (local RPC endpoints)
- [x] Test in dry-run mode
- [x] Create systemd service (optional)
- [x] Enable live trading

### Monitoring Setup (Once)
- [x] Create alert script
- [x] Configure email alerts
- [x] Set up cron job
- [x] (Optional) Install Prometheus + Grafana
- [x] Test alerts

### Ongoing Maintenance (Weekly/Monthly)
- [ ] Check disk usage trends
- [ ] Review alert logs
- [ ] Update Bor/Heimdall if new versions released
- [ ] Analyze bot performance
- [ ] Rotate logs (journalctl --vacuum-time=30d)

---

## Troubleshooting Guide

### Node won't sync
**Symptoms:** `eth_syncing` stuck at same block  
**Causes:** Bad peers, network issues, disk I/O bottleneck  
**Fix:**
1. Restart Bor: `sudo systemctl restart bor`
2. Check peers: Should have 40-80 connected
3. Check logs: `sudo journalctl -u bor -n 100`
4. Last resort: Resync from snapshot

### Low peer count
**Symptoms:** <10 peers connected  
**Causes:** Firewall blocking P2P, bad bootnodes  
**Fix:**
1. Verify firewall: `sudo ufw status | grep 30303`
2. Add static peers to config.toml
3. Check logs for connection errors

### Bot can't connect to RPC
**Symptoms:** Connection refused errors  
**Causes:** Bor not running, wrong endpoint  
**Fix:**
1. Check Bor is running: `sudo systemctl status bor`
2. Verify endpoint: `curl http://localhost:8545` should respond
3. Check bot .env file has correct URL

### High CPU usage
**Symptoms:** CPU at 100% constantly  
**Causes:** Sync in progress, or bot in tight loop  
**Fix:**
1. If syncing: Normal, wait for sync to complete
2. If synced: Check bot isn't hammering RPC
3. Add rate limiting to bot queries if needed

---

## Maintenance Schedule

### Daily (Automated)
- ✅ Health checks every 5 min (cron + alert script)
- ✅ Log rotation (automatic via systemd)

### Weekly (Manual)
- 📊 Review alert logs: `cat ~/alerts/alert.log | grep WARNING`
- 📊 Check disk usage: `df -h /mnt/polygon-data`
- 📊 Review bot performance: Profit, gas costs, success rate
- 📊 Check for Bor/Heimdall updates

### Monthly (Manual)
- 🔧 Update Bor/Heimdall if new versions available
- 🔧 Clean old logs: `sudo journalctl --vacuum-time=30d`
- 🔧 Review and optimize bot strategies
- 🔧 Evaluate disk projections (plan for expansion if needed)

### Every 6 Months
- 🔍 Review server costs vs alternatives
- 🔍 Consider switching from archive to full mode (if disk limited)
- 🔍 Audit security (SSH keys, firewall rules)
- 🔍 Backup important data (bot config, historical data)

---

## Cost Analysis

### Monthly Costs
```
Hetzner AX102:         €129 (~$140)
Electricity:           €0 (included)
Bandwidth:             €0 (unlimited)
────────────────────────────────────
TOTAL:                 ~$140/month
```

### Cost Comparison

| Service | Cost/Month | Request Limit | Latency |
|---------|------------|---------------|---------|
| **Local Node** | $140 | Unlimited | 1-5ms |
| Alchemy Growth | $199 | 300 req/s | 200-300ms |
| QuickNode Pro | $250+ | Variable | 150-250ms |
| AWS Equivalent | $400+ | Unlimited | 1-5ms |

**ROI:** If bot generates >$150/month profit, local node pays for itself and provides unlimited scalability.

---

## Future Enhancements

### Short-term (1-3 months)
- [ ] Implement mempool scanner (txpool_content)
- [ ] Add dynamic gas bidding
- [ ] Expand pool whitelist
- [ ] Set up Grafana dashboards

### Medium-term (3-6 months)
- [ ] Research MEV bundles on Polygon
- [ ] Implement flash loan arbitrage
- [ ] Add multi-hop routing (A→B→C→A)
- [ ] Build trade simulator for backtesting

### Long-term (6-12 months)
- [ ] Switch to full mode (if disk becomes constraint)
- [ ] Explore validator staking (if profitable)
- [ ] Build custom transaction routing
- [ ] Integrate with other L2s (Arbitrum, Optimism)

---

## Support & Resources

**Polygon Documentation:**
- https://docs.polygon.technology/
- https://wiki.polygon.technology/

**GitHub Repositories:**
- Bor: https://github.com/maticnetwork/bor
- Heimdall: https://github.com/maticnetwork/heimdall

**Community:**
- Discord: https://discord.gg/polygon (#node-runners channel)
- Forum: https://forum.polygon.technology/

**Hetzner:**
- Robot Panel: https://robot.hetzner.com
- Support: Open ticket via Robot panel

---

**Last Updated:** [Current Date]  
**System Version:** Bor v1.4.0, Heimdall v1.0.7  
**Maintainer:** [Your Name/Team]
```

Save for reference and onboarding.

---

## TIER 3: Research & Strategy (If Time Permits)

### 7. Research MEV on Polygon
**Time Required:** 1 hour  
**Priority:** LOW (but high potential value)

Create research document:

```bash
nano docs/mev_research.md
```

```markdown
# MEV Research on Polygon

## Research Questions

1. **Does Polygon support Flashbots/MEV-Boost?**
   - Status: Research in progress
   - Initial findings: [To be filled]

2. **Which validators accept priority tips?**
   - List of validator endpoints: [To be filled]
   - Average priority fee for inclusion: [To be filled]

3. **Private mempools / Dark pools?**
   - Available services: [To be filled]
   - Costs and requirements: [To be filled]

4. **Time-to-inclusion for high gas transactions?**
   - Average: [To be filled]
   - 99th percentile: [To be filled]

## Resources to Check

- [ ] Polygon Wiki MEV section
- [ ] Discord #validators channel
- [ ] BloXroute Polygon integration
- [ ] Eden Network Polygon support
- [ ] Contact top 10 validators directly

## Findings

[Document findings here as you research]
```

---

## Summary: 4-6 Hour Checklist

**If you have 6 hours:**

### Hours 1-2: Backfill Data ✅ HIGHEST PRIORITY
- [ ] Create backfill script
- [ ] Run historical data extraction
- [ ] Save to CSV files for later analysis

### Hour 3: Mempool Parser ✅ CRITICAL
- [ ] Build mempool module
- [ ] Implement txpool_content parsing
- [ ] Create swap parameter extraction
- [ ] Write tests (can't run yet, but ready)

### Hour 4: Gas Strategy ✅ HIGH IMPACT
- [ ] Document gas optimization approaches
- [ ] Create implementation plan
- [ ] Define success metrics

### Hour 5: Infrastructure ✅ PREVENTS DOWNTIME
- [ ] Set up alert script
- [ ] Configure email/Telegram
- [ ] Test alerts
- [ ] Add to cron

### Hour 6: Documentation ✅ HELPS FUTURE YOU
- [ ] Create architecture doc
- [ ] Document system layout
- [ ] Write troubleshooting guide

**Bonus if time:**
- [ ] Research pool expansion
- [ ] MEV research
- [ ] Prometheus/Grafana setup

---

## What NOT to Do

❌ Don't modify your live `.env` yet (bot still uses Alchemy)  
❌ Don't remove rate limiting code yet (will break current bot)  
❌ Don't rebuild the bot yet (changes not ready)  
❌ Don't add new pools yet (would hit Alchemy limits)  

---

## After Node Sync Completes

**You'll be ready to:**

1. ✅ Switch `.env` to local endpoints (5 min)
2. ✅ Deploy mempool scanner (already built)
3. ✅ Enable dynamic gas bidding (documented)
4. ✅ Expand pool whitelist (researched)
5. ✅ Monitor with alerts (already configured)

**Time from "node synced" to "improved bot live": ~1 hour**

vs.

**Without this prep: 1-2 weeks of trial and error**

---

**The sync time is your opportunity to build the foundation for a 10x better bot. Use it wisely!** 🚀
