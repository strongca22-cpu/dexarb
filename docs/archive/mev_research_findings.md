# MEV Bot Research: Complete Findings & Implementation Strategy

**Research Date:** January 31, 2026  
**Target:** Phase 1 (Mempool Observation) + Phase 2 (AMM Simulation) for Polygon  
**Searches Conducted:** 7 deep-dive searches  

---

## Executive Summary

### Critical Discovery: ethers-rs → alloy Migration

**BLOCKER IDENTIFIED:** `amms-rs` migrated from `ethers-rs` to `alloy` in May 2024. Your bot currently uses `ethers-rs`, creating a dependency incompatibility.

**Impact:**
- ✅ **Phase 1** (mempool subscription + calldata decoding): **No blocker** — can be built entirely with ethers-rs
- ⚠️ **Phase 2** (AMM simulation): **Requires decision** — either migrate to alloy or build V3 math from scratch

---

## Search 1: amms-rs Compatibility Analysis

### Key Finding: Alloy Migration (May 2024)

**Source:** [@0xKitsune on X, May 11, 2024](https://x.com/0xKitsune/status/1789347102318887091)
> "`amms-rs` has officially been upgraded from `ethers-rs` to `alloy`! Enjoy the performance gains."

**Implications:**
- Current `amms-rs` version is **alloy-only**
- Old ethers-rs version exists but is **unmaintained**
- Alloy offers 35-60% faster arithmetic for MEV math (per Paradigm benchmarks)

### Why Alloy?

**Performance Gains (from Paradigm's Alloy v1.0 announcement):**
- UniswapV2 `get_amount_in`: **35% faster** than ethers-rs
- UniswapV2 `get_amount_out`: **60% faster** than ethers-rs
- Faster primitives (U256, Address, Bytes) used across stack
- Used by: Reth, Foundry, REVM, SP1 zkVM

**Migration Complexity:**
- Alloy is **not a drop-in replacement** — different API design
- Provider pattern changed: `ProviderBuilder` vs ethers `Provider`
- Middleware → Fillers (different abstraction)
- Type conversions needed (see [migration guide](https://alloy.rs/migrating-from-ethers/reference.html))

**Recommendation for Phase 2:**
1. **Option A (Recommended):** Migrate entire bot to alloy + use amms-rs
   - Pro: Access to maintained library with V2+V3 simulation
   - Pro: Future-proof (ethers-rs deprecated since Nov 2023)
   - Con: Rewrite mempool subscription + calldata decoder
   - Estimated effort: 2-3 days migration + testing

2. **Option B:** Build V3 math from scratch in ethers-rs
   - Pro: No migration needed
   - Con: Complex V3 tick math (sqrt price, liquidity, fees)
   - Con: Technical debt (ethers-rs end-of-life)
   - Estimated effort: 4-6 days for correct V3 implementation

3. **Option C (Phase 1 Only):** Defer Phase 2, ship observation mode in ethers-rs
   - Pro: Fastest to production
   - Con: Delays simulation capability

---

## Search 2: QuickSwap Algebra V3 on Polygon

### Key Finding: Algebra is NOT Standard Uniswap V3

**Critical Difference:**
- QuickSwap V3 uses **Algebra Integral** (licensed, modified V3)
- **Dynamic fees** (0.1%-0.15% average, adjusts with volatility)
- **Single pool per pair** (unlike Uniswap V3's multiple fee tiers)
- **Different pool deployer** contract

**Contract Addresses (Polygon Mainnet):**
```
AlgebraFactory:     0x411b0fAcC3489691f28ad58c47006AF5E3Ab3A28
PoolDeployer:       0x2D98E2FA9da15aa6dC9581AB097Ced7af697CB92
SwapRouter:         0xf5b509bB0909a69B1c207E495f687a596C168E12
Quoter:             0xa15F0D7377B2A0C0c10db057f641beD21028FC89
POOL_INIT_CODE_HASH: 0x6ec6c9c8091d160c0aa74b2b14ba9c1717e95093bd3ac085cee99a49aab294a4
```

**Implications for Phase 2:**
- Standard `amms-rs` V3 code **may not work** with Algebra
- Need to verify if `amms-rs` supports Algebra variant
- May need custom pool sync logic for dynamic fees
- Tick math should be similar, but fee calculation differs

**Action Item:** Test if `amms-rs` Algebra support exists or clone + modify

---

## Search 3: Artemis Framework Collectors

### Finding: No Extractable PendingTransactionCollector

**Artemis Architecture:**
```
Collectors (external events) 
    → Strategies (MEV logic) 
        → Executors (tx submission)
```

**Reality Check:**
- Artemis is a **framework**, not a library
- Collectors are tightly coupled to the Event enum
- Extracting just the collector = rewriting half of Artemis
- Current Artemis uses alloy (not ethers-rs)

**Conclusion:** Don't extract Artemis components. The collector pattern is simple enough to implement directly:

```rust
// Pseudocode - 50 lines with ethers-rs
let ws = Provider::<Ws>::connect(ws_url).await?;
let stream = ws.subscribe_pending_txs().await?;

while let Some(tx_hash) = stream.next().await {
    let tx = ws.get_transaction(tx_hash).await?;
    // Process tx
}
```

---

## Search 4: Polygon Mempool Characteristics

### Key Finding: Polygon Mempool Works Like Ethereum

**Differences from Ethereum:**
- **Block time:** 2.3s (vs 13.2s on ETH)
- **Txs per block:** ~48 (vs ~188 on ETH)
- **Throughput:** 271 txs/13.2s window (44% more than ETH)
- **Consensus:** Proof-of-Stake (Bor client)

**Mempool Access Methods:**
1. **WebSocket subscription** (standard):
   ```javascript
   eth_subscribe("newPendingTransactions")
   ```
   - Works identically to Ethereum
   - Supported by all major RPC providers (Alchemy, Chainstack, Infura)

2. **Alchemy `pendingTransactions`** (premium):
   - Filters by `toAddress` (DEX router contracts)
   - Reduces noise significantly
   - Requires Alchemy Growth plan

**Polygon-Specific Concerns:**
- **Gas fee volatility:** Dynamic fees can spike during peak usage
- **Mempool drops:** Transactions can disappear from public mempool after ~20 min if underpriced
- **Reorgs:** More frequent than Ethereum (faster blocks)

**Recommendation:** Use Alchemy's `alchemy_pendingTransactions` with `toAddress` filter for QuickSwap routers to reduce mempool spam.

---

## Detailed Breakdown: Phase 1 Implementation (Ethers-rs)

### Component 1: Mempool Subscription

**Code Path:** `~50 lines`
```rust
use ethers::providers::{Provider, Ws, StreamExt};

let ws_url = "wss://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY";
let ws = Provider::<Ws>::connect(ws_url).await?;
let stream = ws.subscribe_pending_txs().await?;

while let Some(tx_hash) = stream.next().await {
    if let Some(tx) = ws.get_transaction(tx_hash).await? {
        // Process tx
    }
}
```

**No external dependencies needed** — ethers-rs handles this natively.

---

### Component 2: Calldata Decoder

**Required Function Selectors:**
```solidity
// Uniswap V2 Router
swapExactTokensForTokens(uint256,uint256,address[],address,uint256)
  → 0x38ed1739

swapTokensForExactTokens(uint256,uint256,address[],address,uint256)
  → 0x8803dbee

// Uniswap V3 Router / QuickSwap Algebra Router
exactInputSingle((address,address,address,uint256,uint256,uint256,uint160))
  → 0x414bf389

exactInput((bytes,address,uint256,uint256,uint256))
  → 0xc04b8d59

// Universal Router (if needed)
execute(bytes,bytes[],uint256)
  → 0x3593564c
```

**Code Path:** `~80 lines`
```rust
use ethers::abi::{decode, ParamType};

fn decode_v2_swap(calldata: &[u8]) -> Option<V2Swap> {
    let params = vec![
        ParamType::Uint(256),  // amountIn
        ParamType::Uint(256),  // amountOutMin
        ParamType::Array(Box::new(ParamType::Address)), // path
        ParamType::Address,    // to
        ParamType::Uint(256),  // deadline
    ];
    
    let decoded = decode(&params, &calldata[4..]).ok()?;
    // Extract fields
}
```

**Alternative:** Use `ethers::contract::Abigen` if you have the router ABI JSON.

**No external dependencies needed** — ethers-rs `abi` module is sufficient.

---

### Component 3: CSV Logger

**Required:** `csv` crate (lightweight, 0 dependencies)

**Code Path:** `~40 lines`
```rust
use csv::Writer;

struct LogEntry {
    timestamp: u64,
    tx_hash: String,
    dex: String,        // "quickswap_v2" | "quickswap_v3_algebra"
    token_in: String,
    token_out: String,
    amount_in: String,
}

let mut wtr = Writer::from_path("mempool_log.csv")?;
wtr.write_record(&["timestamp", "tx_hash", "dex", ...])?;
wtr.serialize(log_entry)?;
wtr.flush()?;
```

**Total Phase 1 LOC:** ~170 lines (excluding boilerplate)

---

## Phase 2: AMM State Simulation

### Option A: Use amms-rs (Requires Alloy Migration)

**Pros:**
- V2 constant product math: **ready-made**
- V3 tick math: **ready-made**
- Pool syncing: **handles ERC20 balances, reserves, slot0**
- Swap simulation: **gas-optimized, battle-tested**

**Cons:**
- Requires full migration to alloy
- May not support Algebra variant (needs testing)

**Migration Checklist:**
```
[ ] Replace ethers::providers::Provider → alloy::providers::ProviderBuilder
[ ] Replace ethers::types::Address → alloy::primitives::Address
[ ] Replace ethers::types::U256 → alloy::primitives::U256
[ ] Replace ethers::abi → alloy::sol! macro
[ ] Rewrite mempool subscription (alloy streams)
[ ] Rewrite calldata decoder (alloy ABI)
[ ] Test amms-rs with Polygon Algebra pools
```

**Estimated Effort:** 2-3 days

---

### Option B: Build V3 Math from Scratch (Stay on Ethers-rs)

**Required Knowledge:**
- V3 concentrated liquidity model
- Tick math (sqrt price → price, tick → sqrt price)
- Liquidity calculations per tick
- Fee tier handling (Algebra: dynamic fees)

**Reference Implementation:**
- Uniswap V3 Core: `TickMath.sol`, `SqrtPriceMath.sol`
- `uniswap-v3-sdk-rs` (Rust port, but may be outdated)

**Estimated Effort:** 4-6 days (high complexity)

**Risk:** Incorrect V3 math = incorrect simulations = bad trades

---

### Option C: Hybrid Approach

**Phase 1:** Ship observation mode in ethers-rs (this week)
**Phase 2:** Migrate to alloy + amms-rs (next sprint)

**Pros:**
- Fastest time-to-value (Phase 1 deployed)
- Technical debt limited to Phase 1 code (small surface area)
- Learn from real mempool data before building simulation

**Cons:**
- Deferred simulation capability
- Future migration work

---

## Recommended Implementation Path

### Immediate Action (Next 2 Days): Phase 1 MVP

**Tech Stack:** ethers-rs (existing dependency)

**Deliverables:**
1. WebSocket mempool subscription to Polygon
2. Calldata decoder for V2/V3 swaps (QuickSwap routers)
3. CSV logger with timestamp, tx_hash, dex, token_in, token_out, amount_in
4. Filter: only log swaps > $100 equivalent (reduce noise)

**Files to Create:**
```
src/
  main.rs              (orchestration)
  mempool.rs           (subscription, stream handling)
  decoder.rs           (calldata → swap struct)
  logger.rs            (CSV writer)
  types.rs             (SwapEvent, DexType enums)
```

**Estimated Effort:** 1-2 days

---

### Next Sprint (Week 2): Migration + Simulation

**Decision Point:** Evaluate Phase 1 mempool data quality
- If swap volume is high → proceed to Phase 2
- If too much noise → refine filters first

**Path Forward:**
1. **Migrate to alloy:** 2 days
2. **Integrate amms-rs:** 1 day
3. **Test Algebra compatibility:** 1 day
4. **Build simulation loop:** 1 day
5. **Backtest on Phase 1 logs:** 1 day

**Total:** ~6 days to production simulation

---

## Technical Debt Assessment

### If You Stay on ethers-rs for Phase 1:

**Pros:**
- Zero migration cost
- Proven, stable library (even if deprecated)
- Familiar API

**Cons:**
- ethers-rs is **end-of-life** (no new features, no security patches)
- Cannot use amms-rs (alloy-only)
- Phase 2 will require migration anyway

**Risk Level:** **Low** for Phase 1 (mempool observation is simple)
**Risk Level:** **Medium** for Phase 2 (simulation benefits from amms-rs)

---

### If You Migrate to alloy Now:

**Pros:**
- Future-proof (industry-standard library)
- Access to amms-rs (proven AMM simulation)
- Performance gains (35-60% faster math)

**Cons:**
- 2-3 day migration cost upfront
- Learning curve (new API patterns)
- May encounter bugs (alloy v1.0 released May 2025, still relatively new)

**Risk Level:** **Low** (alloy is production-ready, used by Reth/Foundry)

---

## Polygon-Specific Recommendations

### 1. RPC Provider Selection

**Alchemy (Recommended):**
- `alchemy_pendingTransactions` with `toAddress` filter
- Reduces mempool spam by 90%+
- Growth plan required ($199/mo)

**Chainstack (Alternative):**
- Standard `eth_subscribe("newPendingTransactions")`
- Higher throughput (5000 txs/sec capability)
- Lower cost for high-volume use

### 2. Target Contracts

**QuickSwap V2 Router:**
```
Address: 0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff
Pairs: WMATIC/USDC, WMATIC/WETH, etc.
```

**QuickSwap V3 (Algebra) SwapRouter:**
```
Address: 0xf5b509bB0909a69B1c207E495f687a596C168E12
```

**Filter Strategy:**
```rust
let quickswap_routers = vec![
    "0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff", // V2
    "0xf5b509bB0909a69B1c207E495f687a596C168E12", // V3
];

if !quickswap_routers.contains(&tx.to) {
    continue; // Skip non-DEX txs
}
```

### 3. Gas Price Handling

**Polygon Gas Quirks:**
- Dynamic baseFee (EIP-1559)
- Can spike 10x during peak usage (airdrops, mints)
- Mempool drops underpriced txs after ~20 min

**Mitigation:**
```rust
let base_fee = latest_block.base_fee_per_gas;
let priority_fee = U256::from(30_000_000_000u64); // 30 gwei
let max_fee = base_fee * 2 + priority_fee; // 2x buffer

if tx.max_fee_per_gas < max_fee {
    // Likely to drop from mempool
    log_warning("Underpriced tx");
}
```

---

## Testing Strategy

### Phase 1 Testing

**Unit Tests:**
- Calldata decoder against known swap txs
- CSV logger writes correct format

**Integration Tests:**
- Connect to Polygon testnet (Amoy)
- Subscribe to mempool for 1 hour
- Verify logs contain expected swaps

**Mainnet Smoke Test:**
- Run in dry-run mode for 24 hours
- Analyze logs: expected swap rate, false positives

### Phase 2 Testing (Post-Migration)

**Pool Sync Test:**
- Fetch QuickSwap V2 WMATIC/USDC pool
- Verify reserves match PolygonScan

**Simulation Test:**
- Replay historical swap from logs
- Compare simulated output to actual on-chain result
- Acceptable error: <0.1% (slippage + rounding)

**Algebra V3 Test:**
- Sync Algebra pool
- Verify dynamic fee calculation
- Simulate swap with different price ranges

---

## Repository References

### Core Libraries

**ethers-rs** (if staying on ethers):
```
https://github.com/gakonst/ethers-rs
⚠️ Deprecated Nov 2023, but stable
```

**alloy** (if migrating):
```
https://github.com/alloy-rs/alloy
✅ v1.0 released May 2025
Migration guide: https://alloy.rs/migrating-from-ethers/
```

**amms-rs** (AMM simulation):
```
https://github.com/darkforestry/amms-rs
⚠️ Requires alloy
491 stars, active development
```

### Alternative Libraries (If Building V3 from Scratch)

**uniswap-v3-sdk-rs:**
```
https://crates.io/crates/uniswap-v3-sdk
⚠️ May be outdated, check last update
```

**pawurb/univ3-revm-arbitrage:**
```
https://github.com/pawurb/univ3-revm-arbitrage
Reference: REVM-based V3 simulation
```

### Mempool Listeners (Reference Code)

**0xpanoramix/eth-mempool-listener-rs:**
```
https://github.com/0xpanoramix/eth-mempool-listener-rs
Simple pending tx listener (~100 LOC)
```

**mouseless0x/rusty-sando:**
```
https://github.com/mouseless0x/rusty-sando
Full sandwich bot with V2/V3 decoding
⚠️ Uses Artemis framework
```

---

## Cost-Benefit Analysis

### Build vs. Clone Tradeoff

**Mempool Subscription:**
- **Clone value:** Near zero (50 lines of code)
- **Build cost:** 2 hours
- **Verdict:** Build it yourself

**Calldata Decoder:**
- **Clone value:** Low (ethers-rs has built-in ABI decoding)
- **Build cost:** 4 hours (handle edge cases)
- **Verdict:** Build it yourself

**AMM Simulation (V3):**
- **Clone value:** High (complex tick math, battle-tested)
- **Build cost:** 4-6 days + debugging
- **Verdict:** Use amms-rs (requires alloy migration)

**CSV Logger:**
- **Clone value:** Zero (trivial)
- **Build cost:** 1 hour
- **Verdict:** Build it yourself

---

## Final Recommendation

### Two-Phase Approach

**Phase 1: Observation Mode (This Week)**
- **Stack:** ethers-rs (current dependency)
- **Scope:** Mempool subscription + calldata decoding + CSV logging
- **Effort:** 1-2 days
- **Output:** CSV logs of all QuickSwap swaps
- **Value:** Understand mempool patterns, validate filters

**Phase 2: Simulation Mode (Next Sprint)**
- **Stack:** Migrate to alloy + integrate amms-rs
- **Scope:** Pool syncing + swap simulation
- **Effort:** 6 days (2 migration + 4 simulation)
- **Output:** Simulated swap outcomes for mempool txs
- **Value:** Identify profitable MEV opportunities

**Why This Works:**
- Phase 1 is **low-risk, high-value** (immediate data collection)
- Phase 2 migration is **inevitable** (ethers-rs is EOL)
- Splitting phases allows **learning from data** before simulation investment
- Technical debt limited to 170 LOC (easy to rewrite in alloy)

---

## Appendix: QuickSwap Algebra Specifics

### Dynamic Fee Model

**Fee Calculation:**
```
base_fee = 0.1%  (1000 in basis points)
fee = base_fee + volatility_premium

volatility_premium = f(price_movement, time_since_last_swap)
```

**Fee Range:**
- Stablecoin swaps: ~0.01% - 0.05%
- Volatile pairs: ~0.1% - 0.3%
- Extreme volatility: up to 0.5%

**Implications for Simulation:**
- Cannot use fixed 0.3% fee like V2
- Must fetch `globalState.fee` from pool contract
- Fee changes between swaps (dynamic)

**Pool State Query:**
```solidity
interface IAlgebraPool {
    struct GlobalState {
        uint160 price;           // Current sqrt price
        int24 tick;              // Current tick
        uint16 fee;              // Current dynamic fee
        uint16 timepointIndex;   // Oracle index
        uint8 communityFeeToken0;
        uint8 communityFeeToken1;
        bool unlocked;
    }
    
    function globalState() external view returns (GlobalState);
}
```

### Tick Spacing

**Algebra uses tick spacing = 60** (vs Uniswap V3's 10/60/200)

**Impact:**
- Fewer ticks to track
- Wider price ranges per tick
- Easier simulation (less state)

---

## Next Steps

1. **Immediate (Today):**
   - [ ] Verify disk space: ✅ 9.9GB available
   - [ ] Choose RPC provider (Alchemy Growth vs Chainstack)
   - [ ] Set up Polygon mainnet WS endpoint
   - [ ] Create project structure (`src/mempool.rs`, `src/decoder.rs`, etc.)

2. **Day 1 (Tomorrow):**
   - [ ] Implement mempool subscription
   - [ ] Implement V2 swap decoder
   - [ ] Test against 10 historical QuickSwap txs

3. **Day 2:**
   - [ ] Implement V3/Algebra swap decoder
   - [ ] Implement CSV logger
   - [ ] Run 24-hour dry-run on mainnet

4. **Day 3 (Decision Point):**
   - [ ] Review mempool data quality
   - [ ] Decide: proceed to Phase 2 or refine filters
   - [ ] If proceeding: start alloy migration

---

**Research Completed:** January 31, 2026  
**Researcher:** Claude (Anthropic)  
**Total Searches:** 7  
**Document Status:** Final  
