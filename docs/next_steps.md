# Next Steps - DEX Arbitrage Bot

## Current Status: LIVE TEST COMPLETE — Architecture Gap Identified

**Date:** 2026-01-30
**Architecture:** Split process — data collector writes JSON, live bot reads JSON
**Data collector:** V3-only whitelist sync (10 pools, parallel refresh, ~21 RPC/cycle)
**Live bot:** Reads JSON, detects cross-fee-tier arb, Multicall3 batch Quoter pre-screen
**Whitelist v1.1:** 10 active pools, 7 blacklisted, strict enforcement
**All processes:** STOPPED (2026-01-30)
**Checklist:** ALL 5 SECTIONS PASS (`scripts/checklist_full.sh`)

---

## Two-Wallet Architecture

| Wallet | Address | Purpose | USDC | MATIC |
|--------|---------|---------|------|-------|
| **Live** | `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2` | Trading (at-risk) | 160.00 | ~7.59 |
| **Backup** | `0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb` | Deep storage (manual) | 356.70 | 0 |

**Settings:** MAX_TRADE_SIZE_USD=140, MIN_PROFIT_USD=0.10, MAX_SLIPPAGE_PERCENT=0.5

---

## Live Test Results (2026-01-30)

- Live bot started, connected, loaded 10 V3 pools from JSON
- Detection loop running (~3s poll), no opportunities detected
- Zero errors, zero capital spent
- Bot watch script (`bot_watch.sh`) confirmed functional
- **Observation:** No exploitable spreads in ~15 min of monitoring — normal for efficient markets

---

## Architecture Gap: ~5s Average Latency

The split architecture adds unnecessary lag for live trading:

| Step | Split (current) | Monolithic (proposed) |
|------|-----------------|----------------------|
| Block produced | 0ms | 0ms |
| RPC sync | 200ms | 400ms (10 pools concurrent) |
| File write → read | 0-10s (poll alignment) | 0ms (in-memory) |
| Detect opportunity | ~0ms | ~0ms |
| Quoter verify | 400ms | 400ms |
| Execute trade | 200ms | 200ms |
| **Total** | **0.8-10.8s (avg ~5.8s)** | **~1.0s** |

On Polygon's 2s blocks, 5s average lag = 2-3 blocks stale. A monolithic bot that syncs + detects + executes in one process eliminates the file I/O indirection and guarantees ~1s latency every cycle.

**Recommendation:** Merge data collector sync logic into the live bot. The existing `V3PoolSyncer` methods (`sync_pool_by_address`, `sync_known_pools_parallel`) already support this — the live bot just needs to call them directly instead of reading JSON. Data collector remains useful for paper trading and monitoring but is not on the critical trading path.

---

## Completed Phases

| Phase | Date | Summary |
|-------|------|---------|
| V3 swap routing | 2026-01-28 | `exactInputSingle` support, V3 SwapRouter integration |
| Critical bug fixes | 2026-01-29 | Decimal mismatch, liquidity check, trade direction, buy-then-continue HALT |
| Shared data architecture | 2026-01-29 | JSON state file, live bot reads from file (zero RPC for price discovery) |
| Parallel V3 sync | 2026-01-29 | `sync_known_pools_parallel()` — all pools concurrent via `join_all` |
| Phase 1.1 whitelist | 2026-01-29 | 10 active pools, 7 blacklisted, strict enforcement, per-tier liquidity thresholds |
| Phase 2.1 Multicall3 | 2026-01-29 | Batch Quoter pre-screen — verify all opps in 1 RPC call, 7 unit tests |
| V3-only data collector | 2026-01-30 | Removed V2 syncing, whitelist-driven pool sync, 60% RPC reduction |
| Deployment checklist | 2026-01-30 | 6 shell scripts (~102 checks), all 5 sections pass |
| Live test | 2026-01-30 | Bot running, zero errors, zero capital spent |

---

## Remaining Tasks

### Priority 1: Monolithic Live Bot
Merge sync + detect + execute into one process to eliminate ~5s file-polling lag. Reuse existing `V3PoolSyncer` methods. Data collector preserved for monitoring/paper trading.

### Priority 2: Observability Fix
Move status log in `main.rs` before the same-block `continue` so it fires every 100 iterations regardless of block state. Currently the `iteration % 100` check is gated behind block-change, causing silent operation for extended periods.

### Priority 3: Poll Interval Alignment
If keeping split architecture, set live bot `POLL_INTERVAL_MS` to match data collector (10000ms) to eliminate wasted file reads. Currently live bot polls at 3s but data updates at 10s — 70% of reads are redundant.

### Deferred
- Phase 1.3: Pool quality scoring (dynamic opportunity ranking)
- Phase 2.2: Adaptive batch sizing (marginal gain with 10-pool whitelist)
- Quoter architecture Options 2/3 (periodic pre-validation, atomic flash loan arb)
- V2 price calculation fix (V2 not in use)

---

## Incident History

| Date | Loss | Root Cause | Fix |
|------|------|-----------|-----|
| 2026-01-29 | $500 | Decimal mismatch + no liquidity check + inverted trade direction | All three bugs fixed |
| 2026-01-29 | $3.35 | WETH/USDC 0.01% thin pool + buy-then-continue bug | HALT on `tx_hash`, 0.01% blacklisted for non-stables |

Both incidents led to architectural improvements (Quoter pre-check, whitelist filter, HALT on committed capital).

---

## Commands

```bash
# Build
source ~/.cargo/env && cd ~/bots/dexarb/src/rust-bot && cargo build --release

# Start data collector
tmux new-session -d -s datacollector "cd ~/bots/dexarb/src/rust-bot && ./target/release/data-collector"

# Start live bot
tmux new-session -d -s livebot "cd ~/bots/dexarb/src/rust-bot && RUST_LOG=dexarb_bot=info ./target/release/dexarb-bot > ~/bots/dexarb/data/livebot.log 2>&1"

# Start bot watch (kills livebot on first trade)
tmux new-session -d -s botwatch "bash ~/bots/dexarb/scripts/bot_watch.sh"

# Pre-deployment checklist
bash ~/bots/dexarb/scripts/checklist_full.sh

# Pool verification
python3 ~/bots/dexarb/scripts/verify_whitelist.py
```

---

## File Reference

| File | Purpose |
|------|---------|
| `.env` | Data collector config (V3-only, 10s poll) |
| `.env.live` | Live bot config (3s poll, LIVE_MODE=true) |
| `config/pools_whitelist.json` | 10 active + 7 blacklisted V3 pools |
| `data/pool_state_phase1.json` | Shared state file (data collector → live bot) |
| `scripts/checklist_full.sh` | Automated deployment checklist (5 sections) |
| `scripts/bot_watch.sh` | Kill live bot on first trade attempt |
| `scripts/verify_whitelist.py` | On-chain pool verification + depth matrix |

---

*Last updated: 2026-01-30 — Live test complete, architecture gap identified, all processes stopped*
