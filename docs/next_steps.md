# Next Steps - DEX Arbitrage Bot

## Status: A4 Phase 2 DEPLOYED — Simulation Collecting Data, Phase 3 Next

**Date:** 2026-02-01
**Polygon:** 23 active pools (16 V3 + 7 V2), atomic via `ArbExecutorV3` (`0x7761...`), WS block sub, private RPC (1RPC)
**Base:** 5 active V3 pools (whitelist v1.1), ArbExecutor deployed (`0x9054...`), EVENT_SYNC=true, dry-run + mempool observe
**Build:** 72/72 Rust tests, clean release build. A4 mempool module (2,100+ LOC, 14 tests).
**Mode:** 4 tmux sessions (livebot_polygon, dexarb-base, botstatus, botwatch)
**A0-A3:** Deployed 2026-02-01. Diagnostic: 97.1% revert rate → mempool competition confirmed.
**A4 Phase 1:** DEPLOYED. 100% confirmation rate, 6.0s median lead time.
**A4 Phase 2:** DEPLOYED. AMM simulator: 0.04% median prediction error, 12 cross-DEX opps in 45min, median $0.45 est. profit.
**Base mempool:** 0 pending txs — sequencer model has no public mempool via Alchemy.
**Next:** Collect 24h+ simulation data, analyze, build Phase 3 execution pipeline + A5 dynamic gas.

---

## Key Finding: 4-Hour Live Session Analysis (2026-01-31)

**Data source:** `scripts/analyze_bot_session.py` — rerunnable on any log.

### Execution Funnel

| Stage | Count | Rate |
|-------|------:|------|
| Opportunities detected | 686 | 170.8/hr |
| Cooldown-suppressed routes | 539 | — |
| Execution attempts (TRY #) | 113 | 28.1/hr |
| Reverted at estimateGas | 112 | **99.1% of attempts** |
| Submitted on-chain | 0 | 0% |
| Successful trades | 0 | 0% |

### Root Cause: Stale-State Execution

The bot detects spreads from **confirmed block state**. By the time `fill_transaction` (estimateGas) executes ~50ms later, the spread has already been captured by faster actors. All 112 failures returned "Too little received" — the spread no longer exists at execution time.

### The Spreads Are Real

Cross-DEX spread analysis (360K price rows) confirms persistent, genuine spreads:

| Pair | Route | Spread >0.10% | Spread >0.20% |
|------|-------|--------------|--------------|
| WETH/USDC | SushiV3_0.30% vs UniV3_0.05% | 71.8% of observations | 37.2% |
| WETH/USDC | QuickswapV3 vs UniV3_0.30% | 67.9% | 20.5% |
| WMATIC/USDC | QuickswapV3 vs UniV3_0.05% | 10.4% | 1.3% |
| WBTC/USDC | QuickswapV3 vs UniV3_0.05% | 24.3% | 3.0% |

No phantom spreads detected. Opportunities are real but uncapturable at current architecture.

### Timing Budget (Block → estimateGas Revert)

**Pre-A0-A3 (measured):**
```
T=0ms         Block arrives via WS
T=0-400ms     V3 pool sync (21 parallel RPC calls)        ← BIGGEST chunk
T=400-500ms   V2 pool sync (7 parallel RPC calls)
T=500-505ms   Opportunity scan (CPU, no RPC)
T=505-555ms   get_gas_price() (redundant RPC)              ← WASTED
T=555-705ms   fill_transaction / estimateGas                ← 99.1% learn we're too late
Total: ~700ms from block to revert.
```

**Post-A0-A3 (estimated):**
```
T=0ms         Block arrives via WS, base_fee cached (A1)
T=0-50ms      eth_getLogs: Swap+Sync events (A3)           ← was 400-500ms
T=50-55ms     Opportunity scan (CPU, no RPC)
T=55-205ms    fill_transaction (estimateGas only, A0+A2)   ← was 200ms (gas+nonce+estimate)
Total: ~200-250ms from block to attempt.
```

**Savings: ~450ms (64% reduction).** Whether this is enough depends on whether competitors react to confirmed blocks (speed issue → A3 fixes it) or backrun from mempool (latency irrelevant → A4 needed).

### Network Latency (Already Good)

| Metric | Value |
|--------|-------|
| VPS location | Kent, WA (Vultr) |
| RTT to Alchemy | **5ms** |
| Co-location benefit | ~30ms savings (4% of pipeline) |
| Verdict | **Not the bottleneck** |

---

## Two-Wallet Architecture

| Wallet | Address | Purpose | USDC.e | USDC (native) | MATIC |
|--------|---------|---------|--------|---------------|-------|
| **Live** | `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2` | Trading (at-risk) | 516.70 | 400.00 | 165.57 |
| **Backup** | `0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb` | Deep storage (manual) | 0 | 0 | 0.07 |

**Native USDC ($400) is NOT at risk:** All pools use USDC.e (`0x2791...`). ArbExecutor approval is on USDC.e only.

**Settings:** MAX_TRADE_SIZE_USD=500, MIN_PROFIT_USD=0.10, MAX_SLIPPAGE_PERCENT=0.5

**Base Wallet:** `0x48091E0ee0427A7369c7732f779a09A0988144fa` | 0.0057 ETH | Separate from Polygon for isolation

---

## Architecture

Monolithic bot: WS `subscribe_blocks()` → sync V3+V2 pools → price log → detect → atomic execute

**Current execution pipeline (after A0-A3):**
```
Block arrives (WS) → base_fee cached (A1)
  → eth_getLogs: V3 Swap + V2 Sync events (~50ms, A3)
  → Detector (reserve/tick prices) → min_profit gate ($0.10)
  → [Multicall Quoter skipped]
  → ArbExecutor.sol (fee sentinel routing: V2/Algebra/V3)
  → Pre-set: EIP-1559 gas (baseFee + 5000 gwei priority, A0), nonce (AtomicU64, A2)
  → fill_transaction (estimateGas only — gas/nonce already set)
  → Sign via WS, send_raw via 1RPC (private mempool)
  → Revert on loss (zero capital risk)
```

**Previous pipeline (pre-A0-A3):**
```
Block arrives → poll-sync 23 pools (~400ms, 21 RPC calls)
  → get_gas_price (~50ms, redundant RPC)
  → fill_transaction (estimateGas + nonce + gas = ~200ms)
  → 99.1% REVERT (too slow)
Total: ~700ms from block to revert
```

**Fee sentinel routing (ArbExecutor.sol):**
- `fee = 0` → Algebra SwapRouter (QuickSwap V3)
- `fee = 1..65535` → Standard V3 SwapRouter (Uniswap/SushiSwap V3)
- `fee = 16777215` → V2 Router (`swapExactTokensForTokens`)

**Alchemy plan:** Free tier (30M CU/month). Current poll-sync uses ~22M CU/month. Growth/Scale tiers deprecated Feb 2025 — now Free, Pay As You Go ($0.45/M CU), or Enterprise.

**RPC budget (pre-A3):** WS + ~40 sync calls/block (23 pools). ~20 calls/s burst = ~22M CU/month.
**RPC budget (current, A3 deployed):** eth_getLogs (~75 CU/block = ~3.2M CU/month) + estimateGas on attempts. ~21M CU freed for mempool monitoring.

**Key Alchemy APIs (verified on free tier 2026-02-01):**
- `eth_subscribe("logs")` — pool Swap/Sync events. Works. ~1M CU/month for 23 pools.
- `alchemy_pendingTransactions` — pending DEX txs. Works. Measured: ~200 txs/min to 6 routers (99% V2, 1% V3).
- CU cost: full tx objects ~40 CU/event (346M CU/month at 200/min = $152/mo PAYG). hashesOnly ~2.6 CU/event (23M CU/month = free). But hashesOnly lacks calldata needed for backrun decoding.
- **Caveat:** Alchemy only sees its own Bor node mempool — partial view of Polygon pending txs.

**Private RPC:** 1RPC (metadata privacy only — no ordering advantage). FastLane is dead on Polygon (pivoted to Monad 2025, Chainlink acquired Atlas IP Jan 2026, relay NXDOMAIN).

---

## Completed Phases

| Phase | Date | Summary |
|-------|------|---------|
| V3 swap routing | 01-28 | `exactInputSingle`, V3 SwapRouter |
| Critical bug fixes | 01-29 | Decimal mismatch, liquidity check, trade direction, HALT |
| Phase 1.1 whitelist | 01-29 | Strict enforcement, per-tier liquidity thresholds |
| Phase 2.1 Multicall3 | 01-29 | Batch Quoter pre-screen, 1 RPC call |
| Monolithic live bot | 01-30 | Direct RPC sync, ~1s cycle, in-memory pools |
| SushiSwap V3 integration | 01-30 | Cross-DEX DexType variants, dual-quoter |
| Atomic executor V1 | 01-30 | `ArbExecutor.sol` V1, single-tx V3↔V3 |
| QuickSwap V3 (Algebra) | 01-30 | Tri-DEX, fee=0 sentinel, 5 pools |
| WS block subscription | 01-30 | `subscribe_blocks()`, ~100ms notification |
| V2 pool assessment | 01-30 | 7 whitelist, 2 marginal, 3 dead V2 pools |
| **V2↔V3 atomic execution** | **01-31** | **V2 syncer, V2 DexType, atomic_fee(), ArbExecutorV3 deployed** |
| **Profit reporting fix** | **01-31** | **Quote token decimals instead of wei_to_usd()** |
| **Discord log fix** | **01-31** | **Dynamic log file resolution (newest livebot*.log)** |
| **Multi-chain Phase 1+2** | **01-31** | **`--chain` CLI, Base pool discovery, QuoterV2 fix, deploy, dry-run** |
| **Route cooldown** | **01-31** | **Escalating backoff (10→50→250→1250→1800 blocks)** |
| **Private RPC (1RPC)** | **01-31** | **fill via WS, send_raw via 1RPC. Metadata privacy only.** |
| **4-hour live session analysis** | **02-01** | **686 opps, 113 attempts, 0 trades. 99.1% estimateGas revert.** |
| **Alchemy API verification** | **02-01** | **Free tier: pendingTx + logs subscriptions work. 200 DEX txs/min measured. CU budget mapped.** |
| **A0: Gas priority bump** | **02-01** | **5000 gwei maxPriorityFeePerGas (was ~30 gwei default). Targets top ~30 block position.** |
| **A1: Cache base fee** | **02-01** | **base_fee from block header → executor. Eliminates get_gas_price() RPC (~50ms).** |
| **A2: Pre-cache nonce** | **02-01** | **AtomicU64 nonce tracking. Eliminates nonce lookup from fill_transaction (~50ms).** |
| **A3: Event-driven sync** | **02-01** | **eth_getLogs (V3 Swap + V2 Sync) replaces 21 per-pool RPC calls. ~50ms vs ~400ms. ENV: EVENT_SYNC=true.** |
| **A0-A3 diagnostic** | **02-01** | **2h45m session: 97.1% revert (was 99.1%), 11ms fill latency. Confirms mempool competition.** |
| **A4 plan** | **02-01** | **Mempool monitor plan: 4 phases (observe→simulate→execute→own node). Full plan in docs/.** |
| **Base diagnostic (S11)** | **02-01** | **Atomic verified, phantom spread audit (clean), WS timeout+reconnect, historical analysis, strategic: wait for A4** |
| **A4 Phase 1 (code)** | **02-01** | **Mempool observer: 660 LOC, 11 selectors, Alchemy sub, CSV log, cross-ref tracker. ENV: MEMPOOL_MONITOR=observe.** |
| **A4 Phase 1 (deploy)** | **02-01** | **Polygon + Base deployed. Early: 100% confirmation, 6.9s lead. Base: 0 pending (sequencer). Analysis script written.** |
| **A4 Phase 2 (simulator)** | **02-01** | **AMM state simulator: V2 constant product + V3 sqrtPrice math. 871 LOC, 11 tests. 0.04% median prediction error, 12 cross-DEX opps in 45min.** |

---

## Immediate Next Steps — A4 Phase 3 Execution + A5 Dynamic Gas

**A4 Phase 2 is deployed and proving the simulation is accurate and actionable.**

### Phase 2 Results (first 45 minutes)

| Metric | Value | Significance |
|--------|-------|-------------|
| Simulation accuracy | **0.04% median error** | Math is reliable for execution decisions |
| Perfect predictions | 7/25 (28%) | Within-tick swaps are exact |
| Lead time | **6.0s median** | Ample time for tx construction + submission |
| Opportunities detected | **12 in 45min (~16/hr)** | Consistent signal flow |
| Median est. profit | **$0.45** | Above gas cost ($0.01) by 45x |
| Profitable (>$0.10) | **10/12 (83%)** | High hit rate |
| Top route | SushiV3_030 ↔ UniswapV3_005 | Different fee tiers = persistent spreads |

### Phase 3: Execute from Mempool Signals

1. **Collect 24h+ simulation data** — Bot is running. Run `python3 scripts/analyze_mempool.py` after accumulation for full analysis.
2. **Build Phase 3 execution pipeline** — When SIM OPP fires with spread > threshold, construct and submit a backrun transaction targeting the post-swap pool state.
3. **Implement A5: Dynamic gas** — Replace static 5000 gwei with profit-aware gas bidding (see A5 section below).
4. **Skip estimateGas** — When submitting from mempool signal, skip the 150ms estimateGas call. Set gas limit to safe fixed value (~500K). Simulation already validated the trade.
5. **Minimum threshold**: Lower MIN_PROFIT from $0.10 to $0.05 for mempool-sourced signals (higher conviction, gas is $0.01).
6. **Base strategy:** Alchemy mempool not viable on Base (0 pending txs, centralized sequencer). Deprioritize until sequencer feed evaluation.

---

## Action Plan — Priority Order

### Tier 0: Quick Wins — COMPLETED 2026-02-01

**A0: Gas Priority Bump** — DONE
- Set `maxPriorityFeePerGas = 5000 gwei`, `maxFeePerGas = baseFee + 5000 gwei` in `execute_atomic()`.
- EIP-1559 fields pre-set on tx before `fill_transaction()` (which now only calls estimateGas).
- Both private RPC path (pre-set + fill + sign + send_raw) and public WS path updated.

**A1: Cache Gas Price from Block Header** — DONE
- `executor.set_base_fee(block.base_fee_per_gas)` called in main.rs on each new block.
- Eliminates `get_gas_price()` RPC call (~50ms savings).
- Fallback: if no block arrived yet, first-time call uses `provider.get_gas_price()`.

**A2: Pre-cache Nonce** — DONE
- `AtomicU64` nonce field, initialized from `get_transaction_count` on first use.
- Incremented after each successful `send_raw_transaction` / `call.send()`.
- Nonce pre-set on tx before `fill_transaction()` (eliminates nonce lookup).

### Tier 1: Execution Architecture Redesign — A3 DEPLOYED, A4 PENDING

The 99.1% revert rate is structural. A3 is the diagnostic: if speed alone is the issue (competitors just react faster to confirmed blocks), event-driven sync cuts 400ms and should drop the revert rate. If competitors are backrunning from mempool, A3 won't help and A4 becomes the path.

**A3: Event-Driven Pool State (replace poll-sync)** — DONE
- **Implementation:** `eth_getLogs(block, block, pool_addresses, [Swap_topic, Sync_topic])` — single RPC call per block.
- **V3 Swap parsing:** Extracts `sqrtPriceX96`, `liquidity`, `tick` from event data (160 bytes). Updates `V3PoolState` via `PoolStateManager`.
- **V2 Sync parsing:** Extracts `reserve0`, `reserve1` from event data (64 bytes). Updates `PoolState`.
- **Toggle:** `EVENT_SYNC=true` in `.env.polygon`. Falls back to poll-sync if `eth_getLogs` fails.
- **Savings:** ~350ms per block (1 RPC call @ 75 CU vs ~21 calls @ ~1100 CU). Frees ~21M CU/month.
- **Note:** Used `eth_getLogs` (synchronous, deterministic) instead of `eth_subscribe("logs")` (async stream) for simplicity. All events for the current block are guaranteed present before detection runs.

**A4: Pending Mempool Monitoring (the strategic shift)** — PHASE 2 DEPLOYED
- **Gate passed:** A3 diagnostic shows 97.1% revert rate (>95%). Confirmed: competitors use mempool.
- **Full plan:** `docs/a4_mempool_monitor_plan.md`
- **Phase 1 (Observation):** DONE. 100% confirmation rate, 6.0s median lead time, ~4 decoded/min.
- **Phase 2 (Simulation):** DONE. V2 constant product + V3 sqrtPriceX96 math. 0.04% median error. 12 cross-DEX opps in 45min.
- **Phase 3 (Execution):** NEXT. Submit backrun txs targeting simulated state. Skip estimateGas (A5). Dynamic gas bid.
- **Phase 4 (conditional):** Own Bor node ($80-100/mo) if Alchemy visibility degrades. Currently not needed (100% confirmation).
- **Cross-chain:** Architecture is 100% reusable on Base, Arbitrum, Ethereum, BSC. Same ABIs, same AMM math.
- **CU budget:** V3 monitoring ~3.5M CU/month. Total with A3: ~14.2M CU/month. Within free tier.
- **Files:** `src/mempool/{mod,monitor,decoder,types,simulator}.rs`, `main.rs`, `executor.rs`.
- **Simulation CSVs:** `data/polygon/mempool/simulated_opportunities_*.csv`, `simulation_accuracy_*.csv`

**A5: Dynamic Gas + Skip estimateGas (combined with Phase 3)**
- **What:** Two changes to the execution path for mempool-sourced trades:
  1. **Skip estimateGas:** Set gas limit to fixed safe value (~500K). Sign and send immediately. Saves ~150ms.
  2. **Dynamic gas pricing:** Replace static 5000 gwei with profit-aware bidding.
- **Dynamic gas formula (proposed):**
  ```
  base_priority = trigger_tx.gas_price * 1.05   // slightly outbid the trigger
  profit_cap = expected_profit_wei * 0.50        // willing to pay up to 50% of profit as gas
  min_priority = 1000 gwei                       // floor (avoid underbidding)
  priority_fee = min(profit_cap, max(base_priority, min_priority))
  ```
- **Why dynamic:** Static 5000 gwei wastes money on small opps ($0.05 profit shouldn't pay $0.50 gas) and underbids on large ones ($1.00 profit can afford $0.50 gas to guarantee inclusion position).
- **Risk:** On-chain reverts cost gas (~$0.01-0.05 on Polygon at dynamic rates). Break-even if >5% of mempool signals succeed.
- **Files:** `executor.rs` — new `execute_from_mempool()` method with `skip_estimate: bool`, `dynamic_gas: bool` parameters.

### Tier 2: Further Optimizations (after Tier 1 proves viable)

**A6: Parallel Opportunity Submission**
- **What:** Submit top 2-3 opportunities simultaneously instead of sequentially.
- **Why:** Current loop in `main.rs:500` tries one, waits for result, tries next. Atomic revert protection ensures only profitable ones succeed.
- **Files:** `main.rs` — `tokio::join!` on top N executions.

**A7: Dynamic Trade Sizing**
- **What:** Size per-opportunity based on pool depth and spread width.
- **Why:** $500 fixed size creates unnecessary slippage in thin pools, and leaves money on the table in deep pools.
- **Files:** `detector.rs`, `executor.rs`.

**A8: Pre-built Transaction Templates**
- **What:** Pre-construct and pre-sign tx skeletons for common routes. Only fill in amounts at execution time.
- **Saves:** 10-20ms signing overhead.
- **Files:** `executor.rs` — add tx template cache.

### Tier 3: Strategy Expansion (only if Tier 1 produces results)

**A9: Triangular Arbitrage (Multi-Hop)**
- USDC→WETH→WMATIC→USDC across 3 pools
- Multiplicatively more paths, finds circular arbs
- High complexity

**A10: Flash Loans (Zero-Capital)**
- Aave/Balancer flash loans for $50K+ trades
- Profit = gross - gas (no capital at risk)
- Adds ~100k gas overhead

**A11: Additional Chains (Base, Arbitrum, Optimism)**
- Base: ArbExecutor deployed, dry-run collecting data, WS resilience added. **Decision: wait for A4 to port.** Analysis shows same structural problem (block-reactive can't close). Base sequencer feed gives better mempool visibility than Polygon's Alchemy partial view.
- Arbitrum/Optimism: placeholder dirs created
- Same pattern: .env.{chain}, whitelist, deploy executor, data collect, go live

---

## What Won't Help

| Approach | Why |
|----------|-----|
| **Co-location** | 5ms RTT already. Saves ~30ms (4% of 700ms pipeline). |
| **Faster VPS CPU** | Computation is <10ms. All latency is I/O. |
| **More trading pairs** | More detection, same execution failure. |
| **Lower MIN_PROFIT** | Smaller spreads are even more contested. |
| **Better private RPC** | No MEV auction exists on Polygon. FastLane dead. |

---

## Decision Point — RESOLVED 2026-02-01

A0-A3 deployed and measured (2h 45m session, 34 attempts):

| Metric | Pre-A0-A3 | Post-A0-A3 | Change |
|--------|-----------|------------|--------|
| Revert rate | 99.1% | **97.1%** | -2pp (marginal) |
| Fill latency | ~200ms | **11ms** | -95% (fast!) |
| On-chain submissions | 0 | 0 | No change |
| Opportunities/hr | 170.8 | 84.8 | Cooldown filtering |

**Result: Revert rate stayed >95%.** This confirms competitors are backrunning from mempool, not just faster at reacting to confirmed blocks. The 11ms fill latency proves our execution is fast — the state is stale because others already captured the opportunity from pending txs.

**Decision: Proceed with A4 (mempool monitoring).** See `docs/a4_mempool_monitor_plan.md` for full plan.

**Viability assessment:**
- Capital is NOT the bottleneck ($25k+ available, spreads $0.14-$0.95)
- Latency is NOT the bottleneck (5ms RTT, 11ms fill)
- Compute is NOT the bottleneck (Rust on 1 vCPU, microsecond math)
- Information advantage IS the bottleneck — need to see pending txs
- A4 architecture is 100% reusable across all EVM chains (same ABIs, same AMM math)
- Infrastructure cost fits free tier; own Bor node ~$80-100/mo if needed later

---

## Active Whitelist (v1.4)

**V3 Pools (16 active):**

| DEX | Pair | Fee | Status |
|-----|------|-----|--------|
| UniswapV3 | WETH/USDC | 0.05% | active |
| UniswapV3 | WETH/USDC | 0.30% | active |
| UniswapV3 | WMATIC/USDC | 0.05% | active |
| UniswapV3 | WBTC/USDC | 0.05% | active |
| UniswapV3 | USDT/USDC (×3) | 0.01/0.05/0.30% | active |
| UniswapV3 | DAI/USDC (×2) | 0.01/0.05% | active |
| UniswapV3 | LINK/USDC | 0.30% | **monitoring** |
| SushiswapV3 | USDT/USDC | 0.01% | active |
| SushiswapV3 | WETH/USDC | 0.30% | active |
| QuickSwapV3 | WETH/WMATIC/WBTC/USDT/DAI | dynamic | active (5) |

**V2 Pools (7 active):**

| DEX | Pair | TVL | Impact @$140 | Score |
|-----|------|-----|-------------|-------|
| QuickSwapV2 | WETH/USDC | $2.59M | 0.01% | 100 |
| SushiSwapV2 | WETH/USDC | $493K | 0.06% | 90 |
| QuickSwapV2 | WMATIC/USDC | $1.69M | 0.04% | 100 |
| QuickSwapV2 | USDT/USDC | $628K | 0.04% | 100 |
| SushiSwapV2 | USDT/USDC | $351K | 0.08% | 90 |
| QuickSwapV2 | DAI/USDC | $301K | 0.09% | 90 |
| SushiSwapV2 | DAI/USDC | $197K | 0.14% | 90 |

**V2 Observation (2):** SushiSwapV2 WMATIC/USDC ($255K), QuickSwapV2 WBTC/USDC ($184K).
**Blacklisted:** 22 V3 pools (dead/marginal), 3 V2 dead, 1% fee tier banned.

### Base Whitelist (v1.1 — WETH/USDC only)

**Active (5):**

| DEX | Pair | Fee | Score | Impact@$5k | Pool |
|-----|------|-----|-------|-----------|------|
| UniswapV3 | WETH/USDC | 0.05% | 100 | 0.0% | `0xd0b53D92...` |
| UniswapV3 | WETH/USDC | 0.30% | 100 | 0.0% | `0x6c561B44...` |
| UniswapV3 | WETH/USDC | 0.01% | 100 | 0.2% | `0xb4CB8009...` |
| SushiswapV3 | WETH/USDC | 0.05% | 100 | 1.3% | `0x57713F77...` |
| SushiswapV3 | WETH/USDC | 0.30% | 90 | 4.7% | `0x41595326...` |

### Base Historical Analysis (Session 11, 2026-02-01)

**Data: ~14hr across Jan 31 + Feb 1 (70K+ price rows, 5 pools, 2 DEXes)**

| Metric | Value |
|--------|-------|
| Opportunities/hr | 14.8 |
| Median estimated profit | $0.08 |
| Max estimated profit | $0.27 |
| Profitable routes | 13/20 combos |
| Best route | SushiV3 0.30% → UniV3 0.01% |
| Realistic $/hr at $100-$500 | $0.50–$5.00 (after slippage) |
| Phantom spreads | None detected |
| QuoterV2 verified | Live cast call matches market ($2,443/WETH) |

**Midmarket projections (pre-slippage, pre-competition):**

| Size | $/hr | $/day | Reality |
|------|------|-------|---------|
| $100 | $1.33–1.75 | $32–42 | Realistic |
| $500 | $9.68–11.87 | $232–285 | Mostly realistic |
| $1K+ | $20+ | $480+ | Slippage on SushiV3 eats margin |

**Strategic conclusion:** Real spreads, but block-reactive architecture cannot close trades. Same dynamics as Polygon (97.1% revert). Base is a better A4 target (sequencer = full mempool visibility). **Wait for A4, port from Polygon.**

---

## Contracts

| Contract | Address | Status |
|----------|---------|--------|
| ArbExecutorV3 (Polygon) | `0x7761f012a0EFa05eac3e717f93ad39cC4e2474F7` | **LIVE** — V2+V3 atomic |
| ArbExecutor (Base) | `0x90545f20fd9877667Ce3a7c80D5f1C63CF6AE079` | **DRY RUN** — deployed 01-31 |
| ArbExecutorV2 | `0x1126Ee8C1caAeADd6CF72676470172b3aF39c570` | Retired (V3-only, drained) |
| ArbExecutorV1 | `0xA14e76548D71a2207ECc52c129DB2Ba333cc97Fb` | Retired |

---

## Incident History

| Date | Loss | Root Cause | Fix |
|------|------|-----------|-----|
| 01-29 | $500 | Decimal mismatch + no liquidity check + inverted direction | All three bugs fixed |
| 01-29 | $3.35 | Thin pool + buy-then-continue bug | HALT, 0.01% blacklisted |
| 01-30 | $0 | Negative-profit trade attempted | Quoter profit≤0 guard |
| 01-30 | $0 | ERC20 approval missing | Pre-flight revert, no gas |
| 01-30 | $0 | u128 overflow (sqrtPriceX96) | Store as U256 |
| 01-31 | $0.11 | 1 on-chain atomic revert (position 122/153, 729 gwei) | Gas priority bump needed |

---

## Commands

```bash
# Build
source ~/.cargo/env && cd ~/bots/dexarb/src/rust-bot && cargo build --release

# Start Polygon live bot (--chain polygon loads .env.polygon)
tmux new-session -d -s livebot_polygon "cd ~/bots/dexarb/src/rust-bot && ./target/release/dexarb-bot --chain polygon 2>&1 | tee ~/bots/dexarb/data/polygon/logs/livebot_ws.log"

# Bot watch (kills on first profitable trade — "Trade complete" only)
tmux new-session -d -s botwatch "bash ~/bots/dexarb/scripts/bot_watch.sh"

# Discord status (30 min loop)
tmux new-session -d -s botstatus "bash ~/bots/dexarb/scripts/bot_status_discord.sh --loop"

# Analyze session data
python3 scripts/analyze_bot_session.py

# Start Base bot (dry-run — ArbExecutor deployed, LIVE_MODE=false)
tmux new-session -d -s dexarb-base "cd ~/bots/dexarb/src/rust-bot && ./target/release/dexarb-bot --chain base 2>&1 | tee ~/bots/dexarb/data/base/logs/livebot_$(date +%Y%m%d_%H%M%S).log"
```

---

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Monolithic bot (WS + sync + detect + log + execute) |
| `src/arbitrage/detector.rs` | Unified V2+V3 opportunity detection |
| `src/arbitrage/executor.rs` | Atomic execution via ArbExecutor, profit reporting |
| `src/arbitrage/multicall_quoter.rs` | Batch V3 quoter, V2 passthrough |
| `src/pool/v2_syncer.rs` | V2 reserve sync (getReserves) |
| `src/types.rs` | DexType, V2_FEE_SENTINEL, atomic_fee() |
| `contracts/src/ArbExecutor.sol` | Atomic arb V3 (V2+Algebra+V3 routing) |
| `.env.polygon` | Polygon live config (loaded via --chain polygon) |
| `config/polygon/pools_whitelist.json` | v1.4: 23 active + 22 blacklisted |
| `config/base/pools_whitelist.json` | v1.1: 5 active, 1 observation, 2 blacklisted |
| `.env.base` | Base config (QuoterV2, USDC native, multicall skip) |
| `scripts/analyze_bot_session.py` | Session analysis (log + price CSV parsing) |
| `scripts/analyze_price_logs.py` | Cross-DEX price spreads, volatility, frequency (stdlib only) |
| `scripts/analyze_mempool.py` | A4 mempool analysis: visibility, lead time, decoder, gas, hourly |
| `scripts/bot_watch.sh` | Kill bot after first profitable trade |
| `docs/private_rpc_polygon_research.md` | Private RPC research (FastLane dead, 1RPC metadata-only) |
| `src/mempool/mod.rs` | A4 mempool module (monitor, decoder, types, simulator) |
| `src/mempool/monitor.rs` | Alchemy pendingTx subscription, Phase 2 simulation pipeline, CSV logging |
| `src/mempool/decoder.rs` | Calldata decoder (11 selectors: V3, Algebra, V2) |
| `src/mempool/simulator.rs` | Phase 2: V2/V3 AMM math, pool ID, cross-DEX spread check (871 LOC) |
| `src/mempool/types.rs` | MempoolMode, DecodedSwap, PendingSwap, ConfirmationTracker, SimulationTracker |
| `docs/a4_mempool_monitor_plan.md` | A4 mempool monitor plan (phases, calldata ref, CU budget, cross-chain) |
| `docs/session_summaries/2026-02-01_a4_phase1_mempool_monitor.md` | A4 Phase 1: mempool observer build, architecture, deploy plan |
| `docs/session_summaries/2026-02-01_session11_base_diagnostic.md` | Session 11: Base atomic/phantom audit, WS fix, historical analysis |
| `docs/session_summaries/2026-02-01_a4_deploy_and_analysis.md` | A4 deploy, Base enable, analysis script, early results |

---

| `docs/session_summaries/2026-02-01_a4_phase2_simulator.md` | A4 Phase 2: simulator build, deploy, live results, Phase 3 prep |

---

*Last updated: 2026-02-01 (A4 Phase 2) — AMM state simulator deployed on Polygon. V3 sqrtPrice math: 0.04% median error, 6s lead time. 12 cross-DEX opportunities in 45 min (median $0.45). Phase 3 (execution) is next — submit backrun txs from mempool signals. A5 (dynamic gas) designed. Data collecting continuously.*
