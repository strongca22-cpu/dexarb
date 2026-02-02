# Next Steps — Immediate Action Items

**Date:** 2026-02-02 (updated late evening)
**Context:** See `master_plan.md` for strategy. Session summary: `archive/session_summaries/2026-02-02_usdt_expansion_and_pool_research.md`

---

## Current State (as of 2026-02-02 22:30 UTC)

- **Branch:** `feature/alloy-migration` (uncommitted Phase D changes)
- **Bot:** Running in tmux `dexarb-observe` (DRY RUN + MEMPOOL_MONITOR=observe)
- **Pools:** 66 (45 V3 + 21 V2) across 15 pairs, 4 quote tokens (USDC.e, native USDC, USDT, WETH)
- **Tests:** 77/77 passing, build clean
- **Whitelist:** v1.9 — 16 new WETH-quoted pools, all factory-derived + depth-assessed

### What's new since last update:
- ✅ WETH as fourth quote token (Phase D complete — code + pools + deploy)
- ✅ Dynamic decimal handling in detector/executor (fixes 4 hardcoded `1e6` locations)
- ✅ Pre-computed `min_profit_raw` on ArbitrageOpportunity (no USD→token conversion in hot path)
- ✅ `u128` casts to prevent overflow at 18-decimal trade sizes
- ✅ 16 new WETH-quoted pools: WBTC/WETH (4), WMATIC/WETH (4), AAVE/WETH (4), LINK/WETH (4)
- ✅ Pool scanner updated for WETH quotes (352 discovered pools)
- ✅ Depth assessment updated for 18-decimal quote tokens
- ✅ Deadtime prep guide archived (one useful concept extracted below)
- ❌ Phase B long-tail tokens (SAND/SOL/CRV) — not viable, insufficient multi-DEX depth

---

## Priority 1: Merge & Deploy Infrastructure

### 1. Merge alloy branch to main

- **What:** Merge `feature/alloy-migration` → `main`
- **Why:** alloy 1.5 migration + USDT + WETH expansion validated (77/77 tests, 66 pools syncing). No reason to keep on a feature branch.
- **How:** `git checkout main && git merge feature/alloy-migration`
- **Risk:** None. Feature branch is a superset of main. No merge conflicts expected.

### 2. Order Hetzner Dedicated Server (A6)

- **What:** Order Hetzner AX102 (or AX52) in Falkenstein, Germany
- **Why:** Eliminates 250ms Alchemy RPC round-trip. Unfiltered P2P mempool. No rate limits. This is the primary bottleneck.
- **Cost:** ~$140/mo (AX102) or ~$80/mo (AX52)
- **Details:** See `hetzner_bor_architecture.md` for full spec, setup, and architecture.

### 3. Set up Bor + Heimdall Node

- **What:** Install Bor + Heimdall, download Polygon snapshot (~600GB), sync to chain tip
- **Why:** Local node = <1ms RPC instead of 250ms Alchemy
- **Time:** ~4-6 hours with snapshot, then ~30min catchup sync
- **Config:** Enable txpool API, IPC endpoint, localhost-only binding
- **Details:** See `hetzner_bor_architecture.md` sections 2-4.

### 4. Deploy Bot on Hetzner

- **What:** Install Rust toolchain, build release binary, configure `.env.polygon` for local node
- **Why:** Bot and node on same machine = IPC transport, zero network latency
- **Config changes:**
  ```
  POLYGON_HTTP_RPC=http://localhost:8545
  POLYGON_WS_RPC=ws://localhost:8546
  ```
- **Details:** See `hetzner_bor_architecture.md` section 6.

---

## Priority 2: Software Improvements (after node is live)

### 5. IPC Transport (A7 Phase 7) — PARTIALLY DONE

- **What:** Add Unix socket transport to the alloy provider for IPC to local Bor
- **Status:** `ws_rpc_url` config field added (splits mempool WS from main RPC). IPC transport selection ready when Bor node is available.
- **Remaining:** Swap `connect_http`/`connect_ws` for `connect_ipc()` when running on Hetzner with local Bor
- **Dependency:** Bor node running with IPC enabled

### 6. Hybrid Pipeline (A8) — TYPES DONE, INTEGRATION PENDING

- **What:** Use mempool signals to pre-build txs, execute on block confirmation
- **Status:** `HybridCache` and `CachedOpportunity` types implemented. DRY_RUN testing showed 5 HYBRID HITs. Main loop integration needs to be wired up for live execution.
- **Remaining:** Wire `hybrid_cache.check_block()` into the block-reactive main loop to trigger execution
- **Dependency:** Own node for full mempool visibility

### 7. Alerting System for Hetzner — NEW (extracted from archived deadtime prep guide)

- **What:** Node health monitoring script — process checks, disk space, block freshness, peer count
- **Why:** When running own Bor node, need automated alerting if node falls behind, disk fills, or peers drop
- **Scope:** Shell script + cron or systemd timer. Check: bor process alive, heimdall alive, latest block < 30s old, peer count > 3, disk usage < 90%
- **When:** Build during Bor node sync dead time

### 8. Pool Expansion — DONE (v1.9), ONGOING VIA TOOLING

- **What:** Expanded from 25 pools to 66 across 15 pairs. Pool scanner + depth assessment tooling built for ongoing discovery.
- **Current coverage:** 66 pools (45 V3 + 21 V2), 4 quote tokens, 15 pair symbols
- **Tooling:** `scripts/pool_scanner.py` (factory queries), `scripts/depth_assessment.py` (quoter-based impact)
- **Data:** `data/polygon/pool_scan_results.csv` (352 pools), `data/polygon/depth_assessment.csv` (208 assessed)
- **Lesson:** Long-tail tokens (SAND, CRV, SOL) have high mempool swap volume but only 1 DEX with real liquidity. Arb requires ≥2 deep pools on different DEXes. The real expansion is quote-token diversification (USDT done, WETH done) for existing blue-chip tokens.

---

## Priority 3: Optimization (after hybrid pipeline is working)

### 9. USDC.e/USDC Native Stablecoin Arb — NEW

- **What:** Pure stablecoin arb between USDC.e and native USDC pools
- **Why:** 406 mempool swaps/day. Near-zero risk — both tokens peg to $1. Both already recognized as quote tokens.
- **How:** Add pool pairs where token0=USDC.e, token1=native USDC (or vice versa) to whitelist. Detector already handles same-quote-token isolation.
- **Risk:** Very low — stablecoin peg risk only

### 10. Parallel Opportunity Submission (A10)

- **What:** Submit top 2-3 opportunities simultaneously via `tokio::join!`
- **Why:** Current loop tries one at a time. Atomic revert ensures only profitable ones succeed.
- **Files:** `src/main.rs`

### 11. Dynamic Trade Sizing (A11) — PARTIALLY DONE

- **What:** Size per-opportunity based on pool depth and spread width
- **Status:** Per-pool `max_trade_size_usd` caps implemented via whitelist. Detector uses `min(buy_pool, sell_pool)` for effective trade size. Scaled min_profit with 2× gas floor.
- **Remaining:** Spread-responsive sizing (wider spread → bigger trade within pool cap). ~10 lines in detector.

### 12. Per-Route Performance Tracking — NEW

- **What:** HashMap of route key → success/fail counts, average profit, average gas
- **Why:** Foundation for data-driven optimization. Identify which routes are consistently profitable vs dead weight.
- **Scope:** ~30 lines in executor. Log to CSV per session.

### 13. Pre-built Transaction Templates (A12)

- **What:** Pre-construct and pre-sign tx skeletons for common routes. Fill amounts at execution time.
- **Saves:** 1-2ms on local node
- **Files:** `src/arbitrage/executor.rs`

---

## Priority 4: Strategy Expansion (future)

### Triangular Arbitrage (A13)
USDC→WETH→WMATIC→USDC across 3 pools. Multiplicatively more paths.

### Flash Loans (A14)
Aave/Balancer flash loans for $50K+ trades. Profit = gross - gas (no capital at risk).

### Chain #2 (A15)
Replicate the dedicated-server model on a second chain. Own server, not shared.

---

## Quick Reference

```bash
# Build release binary
source ~/.cargo/env && cd ~/bots/dexarb/src/rust-bot && cargo build --release

# Run Polygon live bot
tmux new-session -d -s livebot_polygon \
  "cd ~/bots/dexarb/src/rust-bot && ./target/release/dexarb-bot --chain polygon \
  2>&1 | tee ~/bots/dexarb/data/polygon/logs/livebot_alloy_$(date +%Y%m%d_%H%M%S).log"

# Run tests
cd ~/bots/dexarb/src/rust-bot && cargo test

# Analyze session data
python3 scripts/analyze_bot_session.py
```

---

## Observation Bot Status

Bot is running in tmux `dexarb-observe` with Phase D binary (66 pools, 4 quote tokens, 15 pairs).
- DRY_RUN mode — no real transactions
- MEMPOOL_MONITOR=observe — logging pending swaps
- Collecting data on WETH-quoted opportunity frequency
- No action needed — just let it continue

---

*Last updated: 2026-02-02 22:30 UTC. See `master_plan.md` for full strategy and data.*
