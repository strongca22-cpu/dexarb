# Next Steps — Immediate Action Items

**Date:** 2026-02-02
**Context:** See `master_plan.md` for strategy, PnL data, and lessons learned.

---

## Priority 1: Merge & Deploy Infrastructure

### 1. Merge alloy branch to main

- **What:** Merge `feature/alloy-migration` → `main`
- **Why:** alloy 1.5 migration is validated (2hr live, 73/73 tests, 0 warnings). No reason to keep on a feature branch.
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

### 5. IPC Transport (A7 Phase 7)

- **What:** Add Unix socket transport to the alloy provider for IPC to local Bor
- **Why:** IPC is faster than HTTP/WS for localhost — eliminates TCP overhead
- **Files:** `src/main.rs`, `src/arbitrage/executor.rs` — provider construction
- **How:** alloy 1.5 has `connect_ipc()` — swap `connect_http`/`connect_ws` for IPC path
- **Config:** Add `POLYGON_IPC_PATH=/mnt/polygon-data/bor/bor.ipc` to `.env.polygon`
- **Dependency:** Bor node running with IPC enabled

### 6. Hybrid Pipeline (A8)

- **What:** Use mempool signals to pre-build txs, execute on block confirmation
- **Why:** Solves the tx ordering problem — wait for trigger to be confirmed, then submit pre-built tx instantly
- **Architecture:**
  ```
  Mempool: pending swap → simulate → pre-build arb tx → cache
  Block N confirmed: did it contain the trigger? → YES: submit cached tx → NO: discard
  ```
- **Files:** `src/main.rs` (pending_mempool_opps cache), `src/arbitrage/executor.rs` (execute_prebuilt)
- **Dependency:** Own node (full mempool visibility via txpool API)

### 7. Pool Expansion to 200+ (A9)

- **What:** Expand coverage from 32 pools to 200+ across 20+ pairs
- **Why:** Competition is concentrated on WETH/USDC. Long-tail pairs (LINK/USDC, AAVE/USDC, etc.) have fewer competitors.
- **How:** Pool discovery script (query factory contracts), liquidity scoring, whitelist expansion
- **Reference:** `pool_expansion_catalog.md` has 61 candidate pools already cataloged
- **Dependency:** Own node (no RPC rate limits to support 200+ pool sync)

---

## Priority 3: Optimization (after hybrid pipeline is working)

### 8. Parallel Opportunity Submission (A10)

- **What:** Submit top 2-3 opportunities simultaneously via `tokio::join!`
- **Why:** Current loop tries one at a time. Atomic revert ensures only profitable ones succeed.
- **Files:** `src/main.rs`

### 9. Dynamic Trade Sizing (A11)

- **What:** Size per-opportunity based on pool depth and spread width
- **Why:** $500 fixed size leaves money on the table in deep pools, creates slippage in thin ones
- **Files:** `src/arbitrage/detector.rs`, `src/arbitrage/executor.rs`

### 10. Pre-built Transaction Templates (A12)

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

*Last updated: 2026-02-02. See `master_plan.md` for full strategy and data.*
