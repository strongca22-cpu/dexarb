# A4: Mempool Monitor — Integration Plan

## Purpose
React to pending DEX swaps (mempool) instead of confirmed blocks. Submit backrun arbitrage transactions that land in the same block, immediately after the target swap.

**Date:** 2026-02-01
**Status:** Planning — observation mode first, then speculative execution
**Prerequisite:** A0-A3 deployed. Diagnostic confirms revert rate >95% (97.1% measured), proving competitors operate from mempool.

---

## Why A4

The A0-A3 diagnostic answered the question definitively:

| Metric | Pre-A0-A3 | Post-A0-A3 | Interpretation |
|--------|-----------|------------|----------------|
| Revert rate | 99.1% | 97.1% | Marginal improvement — speed was not the bottleneck |
| Fill latency | ~200ms | **11ms** | Execution path is fast; state is stale |
| On-chain submissions | 0 | 0 | Never even got past estimateGas |
| Opportunities/hr | 170.8 | 84.8 | Real spreads exist, they're just captured before us |

**Conclusion:** Competitors react to pending transactions, not confirmed blocks. By the time a block confirms and we read its events, the arb opportunity created by a swap in that block has already been captured by someone who saw the swap in the mempool and submitted a backrun.

---

## Cross-Chain Portability

This architecture is **fully portable to all EVM chains**. The investment pays off across every chain the bot deploys to.

### Reusable Components (chain-agnostic)

| Component | Reuse | Notes |
|-----------|-------|-------|
| **Calldata decoder** | 100% | Uniswap V2/V3 ABIs identical on all chains |
| **AMM state simulator** | 100% | Constant-product (V2) and sqrt-price (V3) math is universal |
| **Backrun tx builder** | 100% | Same ArbExecutor.sol pattern, same fee sentinel routing |
| **Opportunity detection** | 100% | Cross-DEX spread math doesn't change |
| **Gas bidding logic** | ~90% | EIP-1559 everywhere, but priority strategies vary |

### Chain-Specific Adapters (thin layer)

| Component | Varies By | Notes |
|-----------|-----------|-------|
| **Mempool subscription** | Provider API | Alchemy: `alchemy_pendingTransactions`. Standard: `eth_subscribe("newPendingTransactions")` |
| **Router addresses** | Chain | Different deployment addresses per chain, same ABIs |
| **Block timing** | Chain | Polygon ~2s, Base ~2s, Arbitrum ~0.25s, Ethereum ~12s |
| **MEV infrastructure** | Chain | Polygon: PGA only. Base: sequencer. Ethereum: Flashbots/MEV-Boost |

### Chain-Specific Dynamics

| Chain | Mempool Access | MEV Infra | Priority Mechanism | Viability |
|-------|---------------|-----------|-------------------|-----------|
| **Polygon** | Alchemy partial (Bor mempools) | None (PGA only) | Gas bidding | Medium — partial mempool, no auction |
| **Base** | Sequencer feed | Flashbots on OP Stack | Bundle submission | High — sequencer is transparent |
| **Arbitrum** | Sequencer FIFO | None (FCFS) | Speed (first-come) | High — pure speed game, our latency is good |
| **Ethereum** | Full (Flashbots) | MEV-Boost/MEV-Share | Bundle auction | High — democratized access |
| **BSC** | Full (public mempool) | bloXroute, 48 Club | Gas bidding + private relay | Medium |
| **Avalanche** | Full (public mempool) | None | Gas bidding | Medium |

**Key insight:** Building A4 for Polygon (hardest case — partial mempool) means every other chain is easier. Ethereum and Base have better infrastructure. Arbitrum rewards speed which we already have.

---

## Architecture

### Current Flow (block-reactive, A0-A3)
```
Block confirms (WS)
  → eth_getLogs: read confirmed events (~50ms)
  → Detect spread from confirmed state
  → Submit arb tx
  → 97.1% revert (opportunity already captured)
```

### Target Flow (mempool-reactive, A4)
```
Pending DEX swap appears in mempool
  → Decode calldata (identify pool, direction, amount)
  → Simulate post-swap pool state
  → Check cross-DEX spread against simulated state
  → If profitable: build + submit backrun tx immediately
  → Tx lands in same block, right after target swap
```

### Dual-Mode Operation
The bot runs BOTH flows simultaneously:
- **Mempool monitor** (A4): async task, reacts to pending txs in real-time
- **Block-reactive** (existing): catches anything the mempool monitor missed

---

## Implementation Phases

### Phase 1: Observation Mode (low risk, high information)

**Goal:** Measure mempool visibility and validate the approach before executing.

**What it does:**
1. Subscribe to `alchemy_pendingTransactions` filtered to DEX router addresses
2. Decode swap calldata (function selector + parameters)
3. Log every decoded swap: pair, direction, amount, router, timestamp
4. When a block confirms, cross-reference: "Did we see this swap pending before it confirmed?"
5. Compute: what % of confirmed swaps did we see in advance? How much lead time?

**Output:** A CSV log + analysis script answering:
- What % of WETH/USDC V3 swaps does Alchemy show us pending?
- What's the median lead time (pending seen → block confirmed)?
- How many of our detected opportunities had a visible pending swap as the trigger?

**New files:**
- `src/rust-bot/src/mempool/mod.rs` — module root
- `src/rust-bot/src/mempool/monitor.rs` — WS subscription + event loop
- `src/rust-bot/src/mempool/decoder.rs` — calldata parsing (V2/V3 router functions)
- `src/rust-bot/src/mempool/types.rs` — PendingSwap struct
- `data/polygon/mempool/pending_swaps_YYYYMMDD.csv` — observation log

**Modified files:**
- `src/rust-bot/src/lib.rs` — add `pub mod mempool;`
- `src/rust-bot/src/main.rs` — spawn mempool monitor as async task
- `.env.polygon` — `MEMPOOL_MONITOR=observe` (observe/execute/off)

**CU budget:** V3 routers only (full tx objects): ~3.5M CU/month. Fits in free tier alongside eth_getLogs.

**Decision gate:** If <20% of confirmed swaps are visible pending, Alchemy's view is too partial → consider own Bor node ($80-100/mo) or different chain. If >50% visible with >500ms lead time → proceed to Phase 2.

### Phase 2: AMM State Simulation

**Goal:** Given a pending swap's calldata, compute the post-swap pool state.

**What it does:**
1. Parse swap parameters: tokenIn, tokenOut, amountIn, fee, sqrtPriceLimitX96
2. Apply the swap to current pool state mathematically:
   - V2: constant product (`x * y = k`), compute new reserves
   - V3: tick-range swap simulation (sqrtPrice movement within current tick)
3. Check cross-DEX spreads against the *simulated* post-swap state
4. Log: "Pending swap on UniV3_0.05% WETH/USDC for 5 ETH would create 0.15% spread vs QuickswapV3"

**New files:**
- `src/rust-bot/src/mempool/simulator.rs` — AMM math (apply swap to state)

**Complexity notes:**
- V2 simulation is straightforward (constant product + 0.3% fee)
- V3 simulation within a single tick range is tractable
- V3 cross-tick simulation (large swaps that cross tick boundaries) is complex but rarely needed for the swaps that create arb opportunities (those tend to be small-to-medium)

### Phase 3: Speculative Execution (Backrunning)

**Goal:** Submit arb transactions targeting the post-swap state.

**What it does:**
1. When Phase 2 detects a profitable post-swap spread:
2. Build the arb tx targeting the simulated post-swap state
3. Set gas price to match or slightly exceed the target tx's gas
4. Submit immediately via 1RPC (or direct to Bor node)
5. Skip estimateGas (A5) — we have mempool conviction, save 150ms

**Key design decisions:**
- **Gas bidding:** Match target tx's `maxPriorityFeePerGas` + small delta (100-500 gwei). Ensures our tx lands right after theirs.
- **Skip estimateGas (A5):** Fixed gas limit (500K). On-chain revert costs ~$0.01 on Polygon. Worth it for speed.
- **Nonce management:** Pre-cached (A2). No additional RPC.
- **Capital risk:** Zero. ArbExecutor.sol reverts on loss. Only gas at risk ($0.01/revert).

**Modified files:**
- `src/rust-bot/src/mempool/monitor.rs` — add execution path
- `src/rust-bot/src/arbitrage/executor.rs` — add `execute_speculative()` (skip estimateGas)

### Phase 4: Own Bor Node (conditional)

**Goal:** Full mempool visibility instead of Alchemy's partial view.

**When:** Only if Phase 1 shows <30% pending tx visibility through Alchemy.

**What:**
- Run a Polygon Bor full node on a dedicated VPS ($80-100/mo, 4 CPU, 16GB RAM, 2TB SSD)
- Subscribe to `eth_subscribe("newPendingTransactions")` on local node
- 100% visibility of txs that reach our node via p2p gossip
- Also serves as a local RPC endpoint (zero-latency reads)

**Infrastructure cost:** ~$80-100/mo for a Hetzner/OVH dedicated server with sufficient disk.

---

## Calldata Decoding Reference

### Uniswap V3 SwapRouter Functions

```
exactInputSingle(ExactInputSingleParams)    → 0x414bf389
exactInput(ExactInputParams)                 → 0xc04b8d59
exactOutputSingle(ExactOutputSingleParams)   → 0xdb3e2198
exactOutput(ExactOutputParams)               → 0xf28c0498
multicall(uint256,bytes[])                   → 0x5ae401dc
multicall(bytes[])                           → 0xac9650d8
```

**ExactInputSingleParams:**
```
tokenIn:            address (bytes 16-35 of params)
tokenOut:           address (bytes 48-67)
fee:                uint24  (bytes 68-91)
recipient:          address
deadline:           uint256
amountIn:           uint256
amountOutMinimum:   uint256
sqrtPriceLimitX96:  uint160
```

### Uniswap V2 Router Functions
```
swapExactTokensForTokens(uint,uint,address[],address,uint)  → 0x38ed1739
swapTokensForExactTokens(uint,uint,address[],address,uint)  → 0x8803dbee
swapExactETHForTokens(uint,address[],address,uint)           → 0x7ff36ab5
swapExactTokensForETH(uint,uint,address[],address,uint)      → 0x18cbafe5
```

### QuickSwap V3 (Algebra) Router Functions
```
exactInputSingle(ExactInputSingleParams)     → 0xbc651188  (different from Uni V3!)
exactInput(ExactInputParams)                  → 0xc04b8d59  (same selector)
```
Note: Algebra's `ExactInputSingleParams` has no `fee` field (dynamic fees).

---

## CU Budget (with A4)

| Component | CU/month | Notes |
|-----------|----------|-------|
| eth_getLogs (A3, block sync) | ~3.2M | 75 CU/block × ~43K blocks/month |
| estimateGas (attempts) | ~0.5M | ~12/hr × 250 CU × 720 hrs |
| Pending V3 txs (full objects) | ~3.5M | ~2/min × 40 CU × 43K min/month |
| Pending V2 txs (hashesOnly) | ~5M | ~198/min × 2.6 CU × 43K min/month |
| Selective V2 tx fetch | ~2M | ~10% of V2 hashes × 17 CU |
| **Total** | **~14.2M** | **Within 30M free tier** |

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Alchemy mempool too partial | Medium | High | Phase 1 observation validates before building Phase 2-3 |
| Calldata decoding bugs | Low | Medium | Unit tests against known tx hashes (cast tx) |
| Simulated state inaccurate | Medium | Low | ArbExecutor.sol reverts on loss — $0.01 gas cost |
| Competition from other backrunners | High | Medium | Gas bidding + speed. Some opportunities will be contested. |
| Validator-level MEV extraction | Unknown | High | Unfixable from outside. Accept as structural ceiling. |

---

## Success Criteria

| Phase | Gate Metric | Threshold |
|-------|------------|-----------|
| Phase 1 (Observation) | % of confirmed swaps seen pending | >30% to proceed |
| Phase 1 (Observation) | Median lead time (pending → confirmed) | >500ms to proceed |
| Phase 2 (Simulation) | Simulation accuracy vs on-chain result | >90% match |
| Phase 3 (Execution) | On-chain submission rate | >10% (vs current 0%) |
| Phase 3 (Execution) | Net profitable trades | >0 within first 24h |

---

## Immediate Next Steps

1. **Phase 1 scaffolding:** Create `mempool/` module with monitor, decoder, types
2. **V3 calldata decoder:** Parse `exactInputSingle` and `multicall` for Uniswap/Sushi/QuickSwap V3 routers
3. **Observation loop:** Subscribe to pending V3 txs, log decoded swaps to CSV
4. **Cross-reference script:** Compare pending swap log against confirmed block events
5. **Run observation for 24h**, analyze results, decide on Phase 2

---

*Created: 2026-02-01 — Post A0-A3 diagnostic. Revert rate 97.1% confirms mempool-based competition.*
