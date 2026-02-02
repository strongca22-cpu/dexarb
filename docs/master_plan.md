# DEX Arbitrage Bot — Master Plan

## Mission

Profitable cross-DEX atomic arbitrage on Polygon PoS, running on a dedicated Hetzner server with a co-located Bor full node. The bot detects price dislocations between decentralized exchanges, executes atomic swaps via a custom smart contract (revert-on-loss = zero capital risk), and targets long-tail pool pairs where competition is thinner.

---

## Current State (2026-02-02)

| Item | Status |
|------|--------|
| **Codebase** | Rust, alloy 1.5.2 (zero ethers-rs deps), 73/73 tests, 0 warnings |
| **Branch** | `feature/alloy-migration` — ready to merge to main |
| **Polygon** | 32 active pools (25 V3 + 7 V2), ArbExecutorV3 deployed |
| **Base** | 5 active V3 pools, ArbExecutor deployed, dry-run only |
| **Contract** | `ArbExecutorV3` at `0x7761f012a0EFa05eac3e717f93ad39cC4e2474F7` |
| **Infrastructure** | Vultr VPS (1 vCPU, 2GB RAM) + Alchemy free tier (30M CU/mo) |
| **Bot mode** | Offline (validated, stopped for VPS resource conservation) |

---

## Historical Performance Data

All data collected 2026-01-30 through 2026-02-02 on Polygon mainnet via Alchemy.

### Price Data Collected

| File | Records | Date |
|------|---------|------|
| `data/polygon/price_history/prices_20260130.csv` | 297,195 | Jan 30 |
| `data/polygon/price_history/prices_20260131.csv` | 361,857 | Jan 31 |
| `data/polygon/price_history/prices_20260201.csv` | 761,181 | Feb 01 |
| `data/polygon/price_history/prices_20260202.csv` | 293,376 | Feb 02 |
| **Total** | **1,713,609** | **4 days** |

Columns: `timestamp, block, pair, dex, fee, price, tick, liquidity, sqrt_price_x96, address`
Pairs: WETH/USDC, WMATIC/USDC, WBTC/USDC, USDT/USDC, DAI/USDC, LINK/USDC
DEXes: UniswapV3, SushiSwapV3, QuickSwapV3, QuickSwapV2, SushiSwapV2

### Cross-DEX Spread Frequency (from 360K price rows, 4hr session analysis)

| Pair | Route | Spread >0.10% | Spread >0.20% |
|------|-------|--------------|--------------|
| WETH/USDC | SushiV3 0.30% vs UniV3 0.05% | 71.8% | 37.2% |
| WETH/USDC | QuickswapV3 vs UniV3 0.30% | 67.9% | 20.5% |
| WMATIC/USDC | QuickswapV3 vs UniV3 0.05% | 10.4% | 1.3% |
| WBTC/USDC | QuickswapV3 vs UniV3 0.05% | 24.3% | 3.0% |

**Key takeaway:** Spreads are real and persistent. The problem is capturing them, not detecting them.

### Simulated Opportunities (mempool, 24hr sample)

Source: `data/polygon/mempool/simulated_opportunities_20260201.csv` (438 records)

| Metric | Value |
|--------|-------|
| Total opportunities | 438 |
| Rate | ~18/hr |
| Sum estimated profit | $175.37 |
| Average | $0.40 |
| Median | ~$0.25 |
| Max single | **$4.13** |

**Profit distribution:**

| Bucket | Count | % |
|--------|-------|---|
| >= $1.00 | 36 | 8.2% |
| $0.50 – $0.99 | 77 | 17.6% |
| $0.10 – $0.49 | 210 | 47.9% |
| < $0.10 | 115 | 26.3% |

### Execution Attempts (mempool pipeline, 24hr)

Source: `data/polygon/mempool/mempool_executions_20260201.csv` (167 records)

| Metric | Value |
|--------|-------|
| Total attempts | 167 |
| Estimated value | $70.94 |
| Average per attempt | $0.42 |
| Max single | $2.73 |
| **Success rate** | **0%** |
| Failure mode | All FAIL pre-send (tx ordering problem) |

### Simulation Accuracy

Source: `data/polygon/mempool/simulation_accuracy_20260201.csv` (1,246 predictions)

| Metric | Value |
|--------|-------|
| Avg prediction error | **0.07%** |
| Max error | 1.12% |
| Median error | < 0.1% |

The AMM simulator (V2 constant product + V3 sqrtPriceX96 math) is highly accurate. The detection and simulation pipeline works — only execution capture is broken.

### Block-Reactive Performance (A0–A3, 2h 45m session)

| Metric | Pre-A0-A3 | Post-A0-A3 |
|--------|-----------|------------|
| Revert rate | 99.1% | 97.1% |
| Fill latency | ~200ms | 11ms |
| On-chain submissions | 0 | 0 |
| Opportunities/hr | 170.8 | 84.8 |

---

## PnL Projections (Hetzner Model)

Based on observed data: **~18 opportunities/hr, $0.40 avg estimated profit, $500 trade size.**

| Scenario | Capture Rate | $/hr | $/day | $/month | Assumption |
|----------|-------------|------|-------|---------|------------|
| **Conservative** | 10% | $0.72 | $17 | **$520** | Own node, basic hybrid pipeline, 32 pools |
| **Moderate** | 20% | $1.44 | $35 | **$1,040** | Hybrid pipeline + validator peering |
| **Optimistic** | 30% | $2.16 | $52 | **$1,560** | Above + micro-latency optimizations |

**With 200+ pool expansion (3-5x opportunity multiplier):**

| Scenario | Capture Rate | $/hr | $/day | $/month |
|----------|-------------|------|-------|---------|
| Conservative | 10% | $2.16 | $52 | **$1,560** |
| Moderate | 20% | $4.32 | $104 | **$3,120** |
| Optimistic | 30% | $6.48 | $156 | **$4,680** |

**Breakeven:** Server costs ~$140/mo. Need >4% capture rate on 32 pools to break even.

**Caveat:** These projections assume competitors don't adapt. Real capture rates will depend on how many other bots run local nodes on Polygon. The long-tail pool expansion is the more defensible edge — fewer competitors monitor LINK/USDC or AAVE/USDC than WETH/USDC.

---

## Key Findings & Lessons

### What We Proved

1. **Spreads are real and frequent.** 71.8% of WETH/USDC observations show >0.10% cross-DEX spread. Not phantom — confirmed by quoter contracts and on-chain execution.

2. **Detection pipeline works.** Event-driven sync (eth_getLogs), AMM simulator (0.07% error), and mempool decoder (11 selectors) all function correctly.

3. **Execution pipeline works.** 11ms fill latency. Atomic revert-on-loss means zero capital risk from failed trades.

4. **Block-reactive cannot capture.** 97-99% revert rate. By the time we see a confirmed block, the spread is already captured by mempool-aware competitors.

5. **Mempool-reactive cannot capture on Alchemy.** 0% success rate on 167 attempts. Our tx lands at block position 25, but the trigger swap is at position 106 — we arrive before the opportunity exists. Polygon has no Flashbots-equivalent for ordered bundles.

6. **The bottleneck is infrastructure, not code.** 250ms Alchemy RPC round-trip dominates the latency budget. An own Bor node with IPC eliminates this entirely (target: 3-7ms total).

### What Doesn't Work

| Approach | Why |
|----------|-----|
| Lower MIN_PROFIT | Smaller spreads are even more contested |
| Better private RPC | No MEV auction exists on Polygon (FastLane dead) |
| More pairs on Alchemy | Rate-limited at 30M CU/month |
| Block-reactive only | 97-99% revert rate — competitors use mempool |

### What Will Work (the thesis)

| Approach | Expected Impact |
|----------|----------------|
| **Own Bor node** (A6) | 250ms → <1ms block/mempool access |
| **Validator peering** (A6) | 50-200ms faster block arrival |
| **IPC transport** (A7 Phase 7) | Sub-millisecond RPC calls |
| **Hybrid pipeline** (A8) | Pre-built txs, 5-10ms total execution |
| **200+ pool coverage** (A9) | Access long-tail uncaptured opportunities |
| **Micro-latency stack** | CPU pinning, huge pages, arena alloc |

---

## Strategy: Polygon-Only Dedicated Server

### Why Single-Chain

- A Bor full node demands 64GB+ RAM and 1.5TB+ NVMe. Adding a second chain's node (Base/Arbitrum) would compete for I/O and memory — directly degrading the latency we're paying for.
- Base uses a sequencer (not P2P mempool), so a local node gives less edge there.
- If chain #2 is worth pursuing, it gets its own server. No resource contention.

### Multi-Chain Code Is Preserved

The `--chain` CLI flag and per-chain `.env` configuration are already built. The codebase supports multiple chains with zero additional work. Only the infrastructure is single-chain.

### Future Decision Point

After Hetzner is validated and profitable, choose between:
1. **Add observer nodes** — lightweight read-only nodes for data collection on other chains, running on cheaper VPS instances
2. **Replicate the model** — dedicated server per chain (e.g., Arbitrum node + bot on a second Hetzner)

This decision can wait until the Polygon server is generating consistent revenue.

---

## Roadmap

| Phase | Work Item | Description | Reference |
|-------|-----------|-------------|-----------|
| **Now** | Merge to main | Merge `feature/alloy-migration` branch | `next_steps.md` |
| **A6** | Hetzner + Bor | Dedicated server, Bor node, IPC | `hetzner_bor_architecture.md` |
| **A7.7** | IPC transport | Unix socket to local Bor (alloy already supports it) | `next_steps.md` |
| **A8** | Hybrid pipeline | Mempool pre-build + block-confirmed execution | `next_steps.md` |
| **A9** | Pool expansion | 32 → 200+ pools, long-tail pairs | `pool_expansion_catalog.md` |
| **A10** | Parallel submissions | `tokio::join!` on top N opportunities | `next_steps.md` |
| **A11** | Dynamic trade sizing | Size per-pool based on depth and spread width | `next_steps.md` |
| **A12** | Pre-built tx templates | Pre-sign tx skeletons, fill amounts at execution time | `next_steps.md` |
| **Future** | Triangular arb | USDC→WETH→WMATIC→USDC multi-hop | — |
| **Future** | Flash loans | Aave/Balancer for $50K+ trades, zero capital | — |
| **Future** | Chain #2 | Replicate model on Arbitrum or Base | — |

---

## Completed Work (A0–A7)

| Phase | Date | Summary |
|-------|------|---------|
| V3 swap routing | 01-28 | `exactInputSingle`, V3 SwapRouter |
| Whitelist v1.1 | 01-29 | Strict enforcement, per-tier liquidity thresholds |
| Multicall3 quoter | 01-29 | Batch pre-screen, 1 RPC call |
| Monolithic live bot | 01-30 | Direct RPC sync, ~1s cycle, in-memory pools |
| SushiSwap V3 | 01-30 | Cross-DEX, dual-quoter |
| Atomic executor | 01-30 | `ArbExecutor.sol`, single-tx V3 <> V3 |
| QuickSwap V3 (Algebra) | 01-30 | Tri-DEX, fee=0 sentinel |
| WS block subscription | 01-30 | `subscribe_blocks()`, ~100ms notification |
| V2 <> V3 atomic execution | 01-31 | V2 syncer, V2 DexType, ArbExecutorV3 deployed |
| Multi-chain architecture | 01-31 | `--chain` CLI, Base support, QuoterV2 fix |
| Route cooldown | 01-31 | Escalating backoff (10→1800 blocks) |
| Private RPC (1RPC) | 01-31 | Metadata privacy, send_raw via 1RPC |
| **A0: Gas priority bump** | 02-01 | 5000 gwei maxPriorityFee |
| **A1: Cache base fee** | 02-01 | From block header, eliminates RPC call |
| **A2: Pre-cache nonce** | 02-01 | AtomicU64, eliminates nonce lookup |
| **A3: Event-driven sync** | 02-01 | eth_getLogs replaces 21 per-pool calls (~350ms saved) |
| **A4: Mempool monitor** | 02-01 | Phase 1-3: observe, simulate, execute. 2,500+ LOC, 14 tests |
| **A5: Dynamic gas** | 02-01 | Merged into A4 Phase 3. Profit-capped gas pricing |
| **A7: alloy migration** | 02-02 | Full ethers-rs → alloy 1.5. 27 files, 73/73 tests, 2hr live validation |

---

## Capital & Risk

### Wallets

| Wallet | Address | Purpose | USDC.e | MATIC |
|--------|---------|---------|--------|-------|
| **Live** | `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2` | Trading | 516.70 | 165.57 |
| **Backup** | `0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb` | Deep storage | 0 | 0.07 |
| **Base** | `0x48091E0ee0427A7369c7732f779a09A0988144fa` | Base chain | — | 0.0057 ETH |

### Settings

`MAX_TRADE_SIZE_USD=500, MIN_PROFIT_USD=0.10, MAX_SLIPPAGE_PERCENT=0.5`

### Contracts

| Contract | Address | Status |
|----------|---------|--------|
| ArbExecutorV3 (Polygon) | `0x7761f012a0EFa05eac3e717f93ad39cC4e2474F7` | LIVE |
| ArbExecutor (Base) | `0x90545f20fd9877667Ce3a7c80D5f1C63CF6AE079` | Dry-run |

### Incident History

| Date | Loss | Root Cause |
|------|------|-----------|
| 01-29 | $500 | Decimal mismatch + no liquidity check + inverted direction (all fixed) |
| 01-29 | $3.35 | Thin pool + buy-then-continue bug (0.01% blacklisted) |
| 01-31 | $0.11 | 1 on-chain atomic revert (gas cost only, position 122/153) |
| **Total losses** | **$503.46** | All root causes fixed. Atomic revert = zero trade risk. |

---

## Architecture

```
Monolithic bot (alloy 1.5): WS subscribe_blocks() → Header
  → eth_getLogs: V3 Swap + V2 Sync events
  → Detector (reserve/tick prices) → min_profit gate ($0.10)
  → ArbExecutor.sol (fee sentinel routing: V2/Algebra/V3)
  → Pre-set: EIP-1559 gas (base_fee + 5000 gwei priority), nonce (AtomicU64)
  → estimateGas → Sign → send_raw via 1RPC
  → Revert on loss (zero capital risk)
```

**Fee sentinel routing (ArbExecutor.sol):**
- `fee = 0` → Algebra SwapRouter (QuickSwap V3)
- `fee = 1..65535` → Standard V3 SwapRouter (Uniswap/SushiSwap V3)
- `fee = 16777215` → V2 Router (`swapExactTokensForTokens`)

**Target architecture (Hetzner):** See `hetzner_bor_architecture.md`

---

## Data Files Reference

| Location | Contents |
|----------|----------|
| `data/polygon/price_history/prices_2026*.csv` | 1.7M price records (4 days) |
| `data/polygon/mempool/simulated_opportunities_*.csv` | 438 simulated arb opportunities with profit estimates |
| `data/polygon/mempool/mempool_executions_*.csv` | 167 execution attempts (all FAIL — tx ordering) |
| `data/polygon/mempool/simulation_accuracy_*.csv` | 1,246 AMM prediction accuracy records |
| `data/polygon/logs/livebot_alloy_*.log` | 2hr alloy validation session (4200 blocks) |
| `data/polygon/logs/livebot_ws.log` | Historical ethers-rs bot logs |
| `data/base/logs/livebot_*.log` | Base chain dry-run sessions |
| `config/polygon/pools_whitelist.json` | Whitelist v1.6: 32 active + 22 blacklisted |
| `config/base/pools_whitelist.json` | Base v1.1: 5 active + 2 blacklisted |

**Analysis scripts:**
- `scripts/analyze_bot_session.py` — Session analysis (log + price CSV parsing)
- `scripts/analyze_price_logs.py` — Cross-DEX spreads, volatility, frequency
- `scripts/analyze_mempool.py` — Mempool visibility, lead time, decoder analysis
- `scripts/analyze_mempool_executions.py` — Phase 3 execution PnL tracking

---

## Key Codebase Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Bot entry point: WS sub + sync + detect + execute loop |
| `src/arbitrage/detector.rs` | Unified V2+V3 opportunity detection |
| `src/arbitrage/executor.rs` | Atomic execution via ArbExecutor (~1700 LOC) |
| `src/arbitrage/multicall_quoter.rs` | Batch V3 quoter, V2 passthrough |
| `src/pool/v3_syncer.rs` | V3 event-driven sync (eth_getLogs) |
| `src/pool/v2_syncer.rs` | V2 reserve sync |
| `src/pool/syncer.rs` | Pool state management (DashMap) |
| `src/mempool/monitor.rs` | Alchemy pendingTx subscription, simulation pipeline |
| `src/mempool/decoder.rs` | Calldata decoder (11 selectors: V3, Algebra, V2) |
| `src/mempool/simulator.rs` | V2/V3 AMM math (871 LOC, 11 tests) |
| `src/types.rs` | DexType, V2_FEE_SENTINEL, atomic_fee() |
| `contracts/src/ArbExecutor.sol` | Atomic arb contract (V2+Algebra+V3 routing) |
| `.env.polygon` | Polygon config (loaded via --chain polygon) |
| `.env.base` | Base config |

---

## Archived Documentation

All historical session summaries, completed plans, verification reports, and superseded strategy docs are in `docs/archive/`. See `docs/archive/` for the full list.

---

*Last updated: 2026-02-02. Next: merge alloy branch, order Hetzner, deploy Bor node.*
