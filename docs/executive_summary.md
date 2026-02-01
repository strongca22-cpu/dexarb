# A4 Mempool Monitor: Executive Summary & Integration Plan

**Date:** February 1, 2026
**Status:** Phase 1 implementation ready. A0-A3 diagnostic complete.
**Predecessor:** `docs/a4_mempool_monitor_plan.md` (technical detail), `docs/next_steps.md` (full roadmap)

---

## Why A4 — The Diagnostic Evidence

A0-A3 latency optimizations were deployed and measured (2h 45m live session, 34 execution attempts):

| Metric | Pre-A0-A3 | Post-A0-A3 | Interpretation |
|--------|-----------|------------|----------------|
| Revert rate | 99.1% | **97.1%** | Marginal — speed was not the bottleneck |
| Fill latency | ~200ms | **11ms** | Execution is fast; pool state is stale |
| On-chain submissions | 0 | 0 | Never passed estimateGas |
| Opportunities/hr | 170.8 | 84.8 | Real spreads, cooldown filtering working |

**Conclusion:** Competitors react to pending transactions in the mempool, not confirmed blocks. By the time a block confirms and we read its events, the arb opportunity has already been captured by someone who saw the triggering swap as a pending tx and submitted a backrun.

**What's NOT the bottleneck:**
- Capital ($25k+ available, spreads $0.14-$0.95 on $500 trades)
- Latency (5ms RTT to Alchemy, 11ms fill latency)
- Compute (Rust on 1 vCPU, microsecond math)

**What IS the bottleneck:** Information advantage — we need to see pending txs before block confirmation.

---

## Architecture: Current vs Target

**Current (block-reactive, A0-A3):**
```
Block confirms (WS) → base_fee cached (A1)
  → eth_getLogs: V3 Swap + V2 Sync events (~50ms, A3)
  → Detect spread from confirmed state
  → Submit arb tx (estimateGas only — gas/nonce pre-set, A0+A2)
  → 97.1% revert (opportunity already captured)
```

**Target (mempool-reactive, A4):**
```
Pending DEX swap appears in mempool
  → Decode calldata (pool, direction, amount)
  → Simulate post-swap pool state
  → Check cross-DEX spread against simulated state
  → If profitable: build + submit backrun tx immediately
  → Tx lands in same block, right after target swap
```

Both flows run simultaneously. The block-reactive path catches anything the mempool monitor misses.

---

## Implementation Phases

### Phase 1: Observation Mode (current priority)

**Goal:** Measure Alchemy's mempool visibility on Polygon before committing to execution logic.

**What it builds:**
1. WS subscription to `alchemy_pendingTransactions` filtered to V3 router addresses
2. Calldata decoder for V3 swap functions (`exactInputSingle`, `multicall`, Algebra variant)
3. CSV logger: every decoded pending swap with timestamp, pair, direction, amount, router
4. Cross-reference: when block confirms, did we see the triggering swap in advance?

**Output:** Answers to:
- What % of confirmed V3 swaps did we see as pending?
- What's the median lead time (pending seen → block confirmed)?
- How many of our detected opportunities had a visible pending swap trigger?

**Decision gate:** >30% visibility + >500ms lead time → proceed to Phase 2. Otherwise → evaluate own Bor node ($80-100/mo).

**New files:**
```
src/rust-bot/src/mempool/
  mod.rs          — module root
  monitor.rs      — WS subscription + event loop
  decoder.rs      — calldata parsing (V2/V3 router functions)
  types.rs        — PendingSwap struct
```

**Modified files:**
- `src/rust-bot/src/lib.rs` — add `pub mod mempool;`
- `src/rust-bot/src/main.rs` — spawn mempool monitor as async task
- `.env.polygon` — `MEMPOOL_MONITOR=observe` (observe/execute/off)

**Dependencies:** None new. ethers-rs has built-in ABI decoding.
**Estimated LOC:** ~170 (50 subscription + 80 decoder + 40 logger)
**CU cost:** ~3.5M CU/month (V3 routers, full tx objects). Fits free tier.

### Phase 2: AMM State Simulation

**Goal:** Given a pending swap's calldata, compute the post-swap pool state mathematically.

**What it builds:**
- V2: constant product simulation (`x * y = k`, apply 0.3% fee)
- V3: sqrt-price movement within current tick range
- Cross-DEX spread check against simulated (not current) state

**Key decision: alloy migration** (see "Technical Blockers" below)

### Phase 3: Speculative Execution (Backrunning)

**Goal:** Submit arb txs targeting the simulated post-swap state.

**What it builds:**
- Backrun tx builder using simulated state
- Gas bidding: match target tx's `maxPriorityFeePerGas` + small delta
- Skip estimateGas (A5): fixed 500K gas limit, save ~150ms
- Capital risk: zero (ArbExecutor.sol reverts on loss, only gas at risk ~$0.01/revert)

### Phase 4: Own Bor Node (conditional)

**When:** Only if Phase 1 shows <30% pending tx visibility through Alchemy.

**What:** Polygon Bor full node on dedicated VPS ($80-100/mo). 100% mempool visibility via p2p gossip. Also serves as zero-latency local RPC.

---

## Technical Blocker: amms-rs Requires alloy

**Issue:** `amms-rs` (the best Rust AMM simulation library, 491 stars, V2+V3 math) migrated from ethers-rs to alloy in May 2024. Our bot uses ethers-rs.

**Impact:** Phase 1 is unaffected (no simulation needed). Phase 2 requires either:

| Option | Pros | Cons |
|--------|------|------|
| **A: Migrate to alloy + use amms-rs** | Battle-tested math, future-proof, 35-60% faster | Migration effort, new API |
| **B: Build V3 math in ethers-rs** | No migration | Complex tick math, technical debt (ethers-rs EOL) |
| **C: Two-phase (recommended)** | Ship Phase 1 now, migrate later with real data | Two implementation passes on ~170 LOC |

**Recommendation: Option C.** Ship Phase 1 in ethers-rs immediately. If Phase 1 data validates the approach (>30% visibility, >500ms lead time), migrate to alloy for Phase 2. The 170 LOC of Phase 1 code is trivial to rewrite. Don't invest in migration before knowing if Alchemy's mempool view is sufficient.

---

## Cross-Chain Portability

A4 architecture is fully reusable across all EVM chains. Building for Polygon (hardest case — partial mempool, no MEV auction) means every other chain is easier.

**Reusable components (chain-agnostic):**

| Component | Reuse | Notes |
|-----------|-------|-------|
| Calldata decoder | 100% | Uniswap V2/V3 ABIs identical on all chains |
| AMM state simulator | 100% | Constant-product (V2) and sqrt-price (V3) math is universal |
| Backrun tx builder | 100% | Same ArbExecutor.sol pattern, same fee sentinel routing |
| Opportunity detection | 100% | Cross-DEX spread math doesn't change |
| Gas bidding logic | ~90% | EIP-1559 everywhere, priority strategies vary |

**Chain-specific adapters (thin layer):**

| Chain | Mempool Access | Priority Mechanism | Viability |
|-------|---------------|-------------------|-----------|
| **Polygon** | Alchemy partial (Bor mempools) | Gas bidding (PGA only) | Medium — partial mempool, no auction |
| **Base** | Sequencer feed (full visibility) | Bundle submission (Flashbots on OP Stack) | High — sequencer is transparent |
| **Arbitrum** | Sequencer FIFO | Speed (first-come-first-served) | High — pure speed game, our latency is good |
| **Ethereum** | Full (Flashbots) | Bundle auction (MEV-Boost/MEV-Share) | High — democratized access |

**Key insight:** Base is the best A4 target after Polygon. The sequencer feed gives full mempool visibility (vs Alchemy's partial view on Polygon). ArbExecutor already deployed on Base (`0x9054...`). Port is minimal — same ABIs, same math, different router addresses.

---

## RPC Budget (with A4)

Alchemy free tier: 30M CU/month. Growth/Scale tiers deprecated Feb 2025.

| Component | CU/month | Notes |
|-----------|----------|-------|
| eth_getLogs (A3, block sync) | ~3.2M | 75 CU/block x ~43K blocks/month |
| estimateGas (attempts) | ~0.5M | ~12/hr x 250 CU x 720 hrs |
| Pending V3 txs (full objects) | ~3.5M | ~2/min x 40 CU x 43K min/month |
| Pending V2 txs (hashesOnly) | ~5M | ~198/min x 2.6 CU x 43K min/month |
| Selective V2 tx fetch | ~2M | ~10% of V2 hashes x 17 CU |
| **Total** | **~14.2M** | **Within 30M free tier** |

No paid plan needed. If usage exceeds free tier later, Pay As You Go is $0.45/M CU.

---

## External Repo Research (completed Feb 1)

**Phase 1 — no clones needed:**

| Component | Build Cost | Clone Value | Verdict |
|-----------|------------|-------------|---------|
| Mempool subscription | ~50 LOC | Near zero (`alchemy-rs` archived) | **Build** |
| Calldata decoder | ~80 LOC | Low (ethers-rs `decode_function_data` built in) | **Build** |
| CSV logger | ~40 LOC | Zero | **Build** |

**Phase 2 — external libraries worth evaluating:**

| Repo | Stars | Use Case | Blocker |
|------|-------|----------|---------|
| [darkforestry/amms-rs](https://github.com/darkforestry/amms-rs) | 491 | V2+V3 swap simulation | Requires alloy (not ethers-rs) |
| [shuhuiluo/uniswap-v3-sdk-rs](https://github.com/shuhuiluo/uniswap-v3-sdk-rs) | - | V3 tick/sqrt-price math | crates.io: `uniswap-v3-sdk` |
| [pawurb/univ3-revm-arbitrage](https://github.com/pawurb/univ3-revm-arbitrage) | 111 | REVM-based V3 arb simulation | Reference only |

**Not using:**

| Repo | Why |
|------|-----|
| [paradigmxyz/artemis](https://github.com/paradigmxyz/artemis) | Full MEV framework — too heavyweight for our monolithic architecture |
| [refcell/alchemy-rs](https://github.com/refcell/alchemy-rs) | Archived Aug 2023. Pattern is useful reference but not a dependency. |
| [mouseless0x/rusty-sando](https://github.com/mouseless0x/rusty-sando) | Sandwich bot. Reference for mempool monitoring patterns. |

---

## Polygon-Specific Considerations

**Mempool characteristics (measured Feb 1):**
- ~200 pending DEX txs/min to 6 monitored routers
- 99% V2, 1% V3 (V3 is our target — lower volume but higher value)
- Alchemy sees only its own Bor node's mempool (partial view of network)
- Block time: ~2s

**QuickSwap uses Algebra, NOT standard Uniswap V3:**
- Dynamic fees (0.01%-0.5%, adjusts with volatility)
- Single pool per pair (no fee tiers)
- Different router selector: `0xbc651188` (vs standard `0x414bf389`)
- Already handled in bot: `fee=0` sentinel routes to Algebra SwapRouter

**Private RPC:** 1RPC (metadata privacy only). FastLane is dead on Polygon (pivoted to Monad 2025, Chainlink acquired Atlas IP Jan 2026). No MEV auction infrastructure exists on Polygon — it's pure PGA (priority gas auction).

---

## Cost-Benefit Summary

| Investment | Cost | Expected Return |
|-----------|------|-----------------|
| Phase 1 (observation) | ~170 LOC, ethers-rs | Data: mempool visibility %, lead time |
| Phase 2 (simulation) | ~300 LOC + possible alloy migration | Simulated post-swap state for backrun targeting |
| Phase 3 (execution) | ~100 LOC | On-chain submissions (currently 0%) |
| Phase 4 (Bor node) | $80-100/mo | Full mempool visibility (only if Alchemy insufficient) |
| alloy migration | ~2-3 sessions | Future-proof, unlocks amms-rs, 35-60% faster |

**Capital at risk:** Zero. ArbExecutor.sol reverts on loss. Only gas at risk (~$0.01/revert on Polygon).

---

## Strategic Questions (for owner)

These are resolved or deferred — updating status:

| Question | Status | Answer |
|----------|--------|--------|
| RPC budget | **Resolved** | Free tier sufficient (14.2M / 30M CU). No paid plan needed. |
| Opportunity size | **Known** | Spreads $0.14-$0.95 on $500 trades. MIN_PROFIT = $0.10. |
| Risk tolerance on alloy migration | **Deferred** | Phase 1 first. Migrate only if Phase 1 data validates approach. |
| Timeline pressure | **Phased** | Ship Phase 1, collect 24h+ data, then decide Phase 2. |
| Own Bor node | **Deferred** | Only if Phase 1 shows <30% Alchemy visibility. $80-100/mo. |
| Cross-chain priority | **Decided** | Build for Polygon first (hardest). Port to Base next (best mempool access). |

---

## Immediate Next Steps

1. Create `src/rust-bot/src/mempool/` module (mod.rs, monitor.rs, decoder.rs, types.rs)
2. Implement V3 calldata decoder (selectors: `0x414bf389`, `0x5ae401dc`, `0xbc651188`)
3. Implement observation monitor (WS subscription to `alchemy_pendingTransactions`, toAddress filter)
4. Wire into main.rs as async task, ENV toggle: `MEMPOOL_MONITOR=observe`
5. Build + deploy observation mode alongside existing live bot
6. Run 24h+, analyze with cross-reference script
7. Decision gate: visibility >30% + lead time >500ms → Phase 2

---

**Decision point:** After Phase 1 observation run (24h+)
**Full technical plan:** `docs/a4_mempool_monitor_plan.md`
**Session history:** `docs/session_summaries/2026-02-01_a0_a3_deploy_and_diagnostic.md`

*Last updated: 2026-02-01 — Rewritten from MEV research summary to A4 integration plan. Incorporates A0-A3 diagnostic results, external repo research, CU budget, cross-chain portability analysis.*
