# Next Steps - DEX Arbitrage Bot

## Status: ALLOY MIGRATION COMPLETE — Ready for Hetzner Node + Deployment

**Date:** 2026-02-02 (updated)
**Polygon:** 32 active pools (25 V3 + 7 V2), atomic via `ArbExecutorV3` (`0x7761...`), WS block sub, private RPC (1RPC)
**Base:** 5 active V3 pools (whitelist v1.1), ArbExecutor deployed (`0x9054...`), EVENT_SYNC=true, dry-run (no mempool)
**Build:** alloy 1.5.2 — 73/73 Rust tests, 0 warnings, clean release build. Zero ethers-rs dependencies.
**Whitelist:** v1.6 — 32 active (23 original + 9 native USDC) + 22 blacklisted. Dual-USDC support deployed.
**Mode:** Bot offline (alloy validated 2hr live session, stopped for VPS resource conservation).
**A4 Phase 3:** DEPLOYED + ANALYZED. 3 on-chain txs submitted. All reverted — **transaction ordering problem** confirmed (our tx lands before trigger in same block, spread doesn't exist yet).
**A7 alloy migration:** DONE. Full ethers-rs → alloy 1.5 port on `feature/alloy-migration` branch. 2hr live validation passed. Alchemy mempool subscription fixed.
**Key finding:** Polygon has no Flashbots-equivalent. Mempool backrunning without validator-level access is structurally limited. Code/latency is fine (263ms same-block delivery). Market structure (no ordered bundles) is the barrier.
**Strategic pivot:** Own Bor node on Hetzner dedicated server → eliminate 250ms RPC latency → hybrid mempool-informed block-reactive strategy → long-tail pool expansion → micro-latency optimizations.
**Next:** Order Hetzner, set up Bor node, add IPC transport (Phase 7), expand pool coverage to 200+, deploy streamlined alloy build.

### Bugs Fixed (committed)
1. **PoolStateManager key collision** — `DashMap<(DexType, String)>` → `DashMap<Address>`. Native USDC pools were overwriting USDC.e pools. Fixed: 25 V3 pools now active (was 19).
2. **chain_id=0 in mempool tx signing** — `execute_from_mempool()` skips `fill_transaction()` but forgot to set chain_id on EIP-1559 tx. Added `inner.chain_id = Some(self.config.chain_id.into())` in both RPC paths.
3. **Alchemy mempool subscription (alloy)** — `subscribe_full_pending_transactions()` returns hashes only on Alchemy. Fixed: use raw `subscribe(("alchemy_pendingTransactions", params))` with `hashesOnly: false`.

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

### Network Latency (WAS "good" — now understood as the primary bottleneck)

| Metric | Current (Vultr + Alchemy) | Target (Hetzner + Bor) |
|--------|--------------------------|------------------------|
| VPS location | Kent, WA (Vultr) | Frankfurt, DE (Hetzner) |
| Block arrival | ~250ms (Alchemy WS) | <10ms (P2P, validator-peered) |
| Mempool access | Filtered (Alchemy partial view) | Unfiltered (P2P gossip) |
| RPC round-trip | ~250ms (network) | <1ms (IPC/localhost) |
| Tx submission | ~250ms (to Alchemy, relay to network) | <1ms (direct P2P to validators) |
| **Verdict** | **THE bottleneck** | **Eliminated** |

**Revised understanding:** The 5ms RTT to Alchemy was misleading. The actual bottleneck is the full round-trip through Alchemy's infrastructure — we measured 250ms signal-to-submit on mempool txs. A local Bor node with IPC eliminates this entirely.

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

Monolithic bot (alloy 1.5): WS `subscribe_blocks()` → Header → sync V3+V2 pools → price log → detect → atomic execute

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
| **Pool expansion scan** | **02-01** | **197 factory queries. 9 whitelist-quality native USDC pools found (all USDC.e dead). AAVE/USDC added. Whitelist v1.5. Catalog: `docs/pool_expansion_catalog.md`.** |
| **A4 Phase 3 (execution)** | **02-01** | **Mempool execution pipeline: mpsc channel, tokio::select!, execute_from_mempool(), dynamic gas, skip estimateGas. A5 merged. Dual pipeline (block + mempool). LIVE.** |
| **A7 alloy migration** | **02-02** | **Full ethers-rs → alloy 1.5 port. 27 files, 45 compilation fixes, 33 test fixes. 2hr live validation (4200 blocks, 167 opps). Alchemy mempool sub fixed. 73/73 tests, 0 warnings, zero ethers deps.** |

---

## Immediate Next Steps — Hetzner Node Migration (alloy Port DONE)

### Strategic Context

A4 Phase 3 mempool execution is live but structurally limited on Polygon:
- **3 on-chain txs submitted**, all reverted ("Too little received")
- Our tx at index 25, trigger at index 106 in same block — we landed BEFORE the trigger
- Polygon has no Flashbots-equivalent for guaranteed bundle ordering
- 250ms Alchemy RPC round-trip dominates our latency budget
- **Conclusion:** Need own node + long-tail pool expansion strategy

### The Plan (3 phases)

**Phase A: Infrastructure (Hetzner + Bor node)**
- Order Hetzner AX52 or AX62 dedicated server (Frankfurt/Helsinki datacenter)
- Recommended: Ryzen 7 5800X+ for single-core boost, 64GB RAM, 2x1TB NVMe
- Install Bor + Heimdall, download Polygon snapshot (~400GB), sync to chain tip
- Configure Bor for low-latency local RPC: HTTP (8545) + WebSocket (8546) + IPC
- Enable `txpool` API for unfiltered mempool access
- Optimize P2P peering: add Polygon validator nodes as static/trusted peers
- See: Separate Claude chat prompt for Hetzner setup walkthrough

**Phase B: alloy Migration** — **DONE (2026-02-02)**
- Full ethers-rs → alloy 1.5 migration on `feature/alloy-migration` branch
- Phases 1-6 complete (types, U256, ABI, contracts, providers, tx building)
- Phase 7 (IPC transport) deferred until Hetzner Bor node is running
- Phase 8 (validation) partially done — 2hr live validation passed
- **Detailed plan + status:** `docs/alloy_port_plan.md`
- **Session summary:** `docs/session_summaries/2026-02-02_alloy_migration_completion.md`
- Key wins achieved: `sol!` compile-time ABI, fewer allocations, modern maintained library
- Remaining: IPC transport for local Bor node (~1ms vs 250ms RPC)

**Phase C: Long-Tail Pool Expansion + Hybrid Pipeline**
- Build pool discovery script (query factory contracts for all Polygon pools)
- Expand whitelist from 32 → 200+ pools with sufficient liquidity
- Implement hybrid pipeline: mempool pre-builds → block-confirmed execution
- Target: uncaptured opportunities on lower-competition pairs
- Collect data for 1 week to validate long-tail thesis

### Hybrid Pipeline Architecture (target)

```
Mempool (P2P via Bor node)
  Pending swap detected on Pool A
    → Simulator predicts post-swap state
    → Pre-build arb route (Pool A → Pool B)
    → Pre-sign tx (amounts estimated, deadline set)
    → Cache in pending_mempool_opps

Block N confirmed (via IPC, <1ms)
  → Check: did Block N contain the trigger tx?
    → YES: submit pre-built tx immediately (~5-10ms)
    → NO: discard stale pre-build
  → ALSO: run normal block-reactive scan for non-mempool opps
```

**Latency budget comparison:**

| Component | Current (Alchemy) | Hetzner + alloy |
|-----------|-------------------|-----------------|
| Block/mempool arrival | ~120ms network | <1ms (IPC) |
| Detection | ~5ms | ~0.5ms (cache lookup) |
| Route evaluation | ~3ms | ~0.5ms (pre-computed) |
| Tx building | ~3ms | ~0.5ms (template) |
| Signing | ~1.5ms | ~0.5ms (optimized lib) |
| Submit to network | ~120ms network | <1ms (local P2P) |
| **Total** | **~253ms** | **~3-7ms** |

### Micro-Latency Optimization Stack (post-migration)

| Tier | Optimization | Savings | Priority |
|------|-------------|---------|----------|
| **1** | IPC Unix socket to Bor | 0.5-1ms/call | Do with alloy port |
| **1** | Pre-built tx templates | 1-2ms | After alloy |
| **1** | In-memory state cache (always current) | 2-3ms detection | After alloy |
| **2** | CPU pinning (`taskset -c 4 ./bot`) | 0.1-0.5ms variance | After deploy |
| **2** | Kernel network tuning (io_uring) | 0.2-0.5ms | After deploy |
| **2** | Huge pages for DashMap | 0.1-0.3ms | After deploy |
| **3** | alloy over ethers-rs | 1-2ms | Part of port |
| **3** | Pre-computed ABI calldata | 0.5-1ms | After alloy |
| **3** | Custom secp256k1 (C lib) | 0.5-0.7ms | Optional |
| **3** | Arena allocation for hot path | 0.3-0.5ms | Optional |
| **4** | Bor peering optimization (validator peers) | 50-200ms block arrival | Critical — do first |
| **4** | Multi-path tx submission | Variable | After peering |
| **4** | Local nonce tracking (no query) | 0.3-0.5ms | Already done (A2) |

### Monitoring Checklist (VPS — alloy build validated)

The alloy bot has been validated in a 2hr live session (2026-02-02). Bot is currently stopped.
1. Review `data/polygon/mempool/mempool_executions_*.csv` periodically
2. Gas spend is negligible (~$0.00/revert for reverts, ~$0.0002/on-chain revert)
3. Restart alloy bot when ready for extended data collection: `tmux new-session -d -s livebot_polygon "cd ~/bots/dexarb/src/rust-bot && ./target/release/dexarb-bot --chain polygon 2>&1 | tee ~/bots/dexarb/data/polygon/logs/livebot_alloy_$(date +%Y%m%d_%H%M%S).log"`
4. Next deploy target: Hetzner dedicated server with Bor node + IPC transport

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

**A4: Pending Mempool Monitoring (the strategic shift)** — PHASE 3 DEPLOYED
- **Gate passed:** A3 diagnostic shows 97.1% revert rate (>95%). Confirmed: competitors use mempool.
- **Full plan:** `docs/a4_mempool_monitor_plan.md`
- **Phase 1 (Observation):** DONE. 100% confirmation rate, 6.0s median lead time, ~4 decoded/min.
- **Phase 2 (Simulation):** DONE. V2 constant product + V3 sqrtPriceX96 math. 0.04% median error. 12 cross-DEX opps in 45min.
- **Phase 3 (Execution):** DEPLOYED. mpsc channel → main loop → `execute_from_mempool()`. Skip estimateGas, dynamic gas (profit-capped), 500K gas limit. Dual pipeline (block-reactive + mempool).
- **Phase 4:** Own Bor node → NOW PLANNED as A6. Hetzner dedicated server. See "Immediate Next Steps" section above.
- **Cross-chain:** Architecture is 100% reusable on Base, Arbitrum, Ethereum, BSC. Same ABIs, same AMM math.
- **CU budget:** V3 monitoring ~3.5M CU/month. Total with A3: ~14.2M CU/month. Within free tier.
- **Files:** `src/mempool/{mod,monitor,decoder,types,simulator}.rs`, `main.rs`, `executor.rs`.
- **Execution CSVs:** `data/polygon/mempool/mempool_executions_*.csv`
- **Simulation CSVs:** `data/polygon/mempool/simulated_opportunities_*.csv`, `simulation_accuracy_*.csv`

**A5: Dynamic Gas + Skip estimateGas** — MERGED INTO A4 PHASE 3
- **Implemented as part of Phase 3 execution pipeline:**
  1. **Skip estimateGas:** Fixed 500K gas limit. Sign and send immediately. Saves ~150ms.
  2. **Dynamic gas pricing:** `calculate_mempool_gas()` in executor.rs.
- **Dynamic gas formula (deployed):**
  ```
  match_trigger = trigger_priority * 1.05        // slightly outbid the trigger
  profit_cap = (est_profit * 0.50 / $0.50) * 1e18 / 500K   // never spend >50% of profit
  min_priority = 1000 gwei                       // competitive floor on Polygon
  priority_fee = min(profit_cap, max(match_trigger, min_priority))
  ```
- **Config:** `MEMPOOL_MIN_PRIORITY_GWEI=1000`, `MEMPOOL_GAS_PROFIT_CAP=0.50` in `.env.polygon`.
- **Risk:** On-chain reverts cost ~$0.01. Break-even if >5% of mempool signals succeed.

### Tier 2: Hetzner Node + alloy Migration — A7 DONE, A6 NEXT

**A6: Hetzner Dedicated Server + Bor Node**
- **What:** Own Polygon full node on dedicated hardware, co-located with the bot
- **Why:** Eliminates 250ms Alchemy RPC round-trip. Unfiltered P2P mempool. No rate limits.
- **Cost:** ~$80-150/mo (Hetzner AX52/AX62)
- **Setup guide:** Separate Claude chat session (prompt saved)
- **Key config:** Enable txpool API, IPC endpoint, optimize P2P peering with validators

**A7: ethers-rs → alloy Migration** — **DONE (2026-02-02)**
- **What:** Full migration from ethers-rs 2.0 to alloy 1.5 (successor library)
- **Why:** IPC transport support, 1-2ms hot-path savings, maintained library, sol! macro
- **Scope:** 27 files changed (+2830/-2427), 45 compilation fixes, 33 test fixes
- **Validation:** 2hr live session (4200 blocks, 167 opps, stable WS, zero reconnects)
- **Plan:** `docs/alloy_port_plan.md` (Phases 1-6 done, Phase 7 IPC deferred, Phase 8 partial)
- **Branch:** `feature/alloy-migration` — ready to merge to main
- **Remaining:** Phase 7 (IPC transport) — add when Hetzner Bor node is running

**A8: Hybrid Mempool-Informed Block-Reactive Pipeline**
- **What:** Use mempool signals to pre-build txs, execute on block confirmation
- **Why:** Solves the ordering problem — we wait for block confirmation (trigger confirmed) then submit pre-built tx instantly
- **Files:** `main.rs` (pending_mempool_opps cache), `executor.rs` (execute_prebuilt)

**A9: Long-Tail Pool Expansion (200+ pools)**
- **What:** Expand coverage from 32 pools/2 pairs to 200+ pools across 20+ pairs
- **Why:** Competition is concentrated on WETH/USDC. Long-tail pairs have uncaptured opportunities.
- **Dependencies:** Own node (no RPC rate limits), alloy port (IPC for performance)
- **Files:** Pool discovery script, whitelist expansion, detector updates

**A10: Parallel Opportunity Submission**
- **What:** Submit top 2-3 opportunities simultaneously instead of sequentially.
- **Why:** Current loop in `main.rs:500` tries one, waits for result, tries next. Atomic revert protection ensures only profitable ones succeed.
- **Files:** `main.rs` — `tokio::join!` on top N executions.

**A11: Dynamic Trade Sizing**
- **What:** Size per-opportunity based on pool depth and spread width.
- **Why:** $500 fixed size creates unnecessary slippage in thin pools, and leaves money on the table in deep pools.
- **Files:** `detector.rs`, `executor.rs`.

**A12: Pre-built Transaction Templates**
- **What:** Pre-construct and pre-sign tx skeletons for common routes. Only fill in amounts at execution time.
- **Saves:** 1-2ms on local node (was 10-20ms — most savings were network, not compute).
- **Files:** `executor.rs` — add tx template cache.
- **Part of:** Micro-latency optimization stack (Tier 3 in optimization table above)

### Tier 3: Strategy Expansion (after node + alloy are validated)

**A13: Triangular Arbitrage (Multi-Hop)**
- USDC→WETH→WMATIC→USDC across 3 pools
- Multiplicatively more paths, finds circular arbs
- High complexity

**A14: Flash Loans (Zero-Capital)**
- Aave/Balancer flash loans for $50K+ trades
- Profit = gross - gas (no capital at risk)
- Adds ~100k gas overhead

**A15: Additional Chains (Base, Arbitrum, Optimism)**
- Base: ArbExecutor deployed, dry-run collecting data, WS resilience added. **Decision: wait for A4 to port.** Analysis shows same structural problem (block-reactive can't close). Base sequencer feed gives better mempool visibility than Polygon's Alchemy partial view.
- Arbitrum/Optimism: placeholder dirs created
- Same pattern: .env.{chain}, whitelist, deploy executor, data collect, go live

---

## What Won't Help (at current infrastructure level)

| Approach | Why | Revisit? |
|----------|-----|----------|
| **Lower MIN_PROFIT** | Smaller spreads are even more contested. | No |
| **Better private RPC** | No MEV auction exists on Polygon. FastLane dead. | No |
| **ethers-rs tuning alone** | Migrated to alloy 1.5 (A7 DONE). IPC pending Hetzner. | DONE — IPC next |
| **More pairs on Alchemy** | Rate-limited at 30M CU/month. | YES — no limits on own node |
| **Co-location near exchange** | DEX arb isn't centralized exchange HFT. Co-locate near validators instead. | YES — Hetzner Frankfurt |

## What WILL Help (new strategy)

| Approach | Expected Impact | Status |
|----------|----------------|--------|
| **Own Bor node** | 250ms → <1ms block/mempool | PLANNED (A6) |
| **Validator peering** | 50-200ms faster block arrival | PLANNED (A6 config) |
| **alloy + IPC** | 1-2ms compute savings (now 10-15% of total) | **A7 DONE** — IPC pending Bor node |
| **Hybrid pipeline** | Pre-built txs, 5-10ms total execution | PLANNED (A8) |
| **200+ pool coverage** | Access long-tail uncaptured opportunities | PLANNED (A9) |
| **Micro-latency stack** | CPU pinning, huge pages, arena alloc | AFTER migration |

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

## Active Whitelist (v1.5)

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

# Bot watch (kills on first profitable trade — "Trade complete" or "MEMPOOL SUCCESS")
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
| `config/polygon/pools_whitelist.json` | v1.5: 23 active + 9 native USDC candidates + 22 blacklisted |
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
| `src/mempool/types.rs` | MempoolMode, MempoolSignal, DecodedSwap, PendingSwap, ConfirmationTracker, SimulationTracker |
| `docs/a4_mempool_monitor_plan.md` | A4 mempool monitor plan (phases, calldata ref, CU budget, cross-chain) |
| `docs/session_summaries/2026-02-01_a4_phase1_mempool_monitor.md` | A4 Phase 1: mempool observer build, architecture, deploy plan |
| `docs/session_summaries/2026-02-01_session11_base_diagnostic.md` | Session 11: Base atomic/phantom audit, WS fix, historical analysis |
| `docs/session_summaries/2026-02-01_a4_deploy_and_analysis.md` | A4 deploy, Base enable, analysis script, early results |

---

| `docs/session_summaries/2026-02-01_a4_phase2_simulator.md` | A4 Phase 2: simulator build, deploy, live results, Phase 3 prep |
| `docs/session_summaries/2026-02-01_pool_expansion_scan.md` | Pool expansion: factory scan, native USDC discovery, whitelist v1.5 |
| `docs/session_summaries/2026-02-01_a4_phase3_execution.md` | A4 Phase 3: mempool execution pipeline, dynamic gas, deploy |
| `docs/pool_expansion_catalog.md` | Full pool expansion catalog: tiers, addresses, implementation roadmap |
| `docs/alloy_port_plan.md` | ethers-rs → alloy migration plan: 8 phases, file-by-file mapping, type conversion tables |
| `docs/session_summaries/2026-02-02_alloy_migration_completion.md` | **NEW** — alloy migration: 45 compile fixes, 33 test fixes, 2hr live validation, API reference table |

---

*Last updated: 2026-02-02 (alloy migration complete) — A7 ethers-rs → alloy 1.5 migration DONE: 27 files, 45 compile fixes, 33 test fixes, 73/73 tests, 0 warnings, zero ethers-rs deps. 2hr live validation passed (4200 blocks, 167 opps, stable WS). Alchemy mempool subscription fixed (raw subscribe with alchemy_pendingTransactions). Branch: `feature/alloy-migration`. Next: merge to main, order Hetzner, set up Bor node, add IPC transport (Phase 7), expand to 200+ pools.*
