# Session Summary: A4 Phase 1 Deploy + Analysis Script

**Date:** 2026-02-01
**Session:** A4 Deploy, Base enable, mempool analysis script
**Duration:** ~30 minutes

---

## What Was Done

### 1. Git Commit & Push
- Committed all A4 Phase 1 code + docs (17 files, +2,956 lines) to main
- Commit: `a51db7b` — `feat: A4 Phase 1 mempool observer, multi-chain discord, session docs`

### 2. Polygon Live Bot Deployed with A4 Observer
- Built release binary (`cargo build --release`, 1m23s, warnings only)
- Started `livebot_polygon` tmux session with MEMPOOL_MONITOR=observe
- Mempool observer confirmed active: `alchemy_pendingTransactions subscription active (3 routers)`
- First PENDING log line within 10 seconds of startup

### 3. Base Bot Enabled with A4 Observer
- Added `MEMPOOL_MONITOR=observe` to `.env.base`
- Added `EVENT_SYNC=true` to `.env.base` (was missing — Base was using poll-based sync)
- Restarted `dexarb-base` tmux session
- Result: 0 pending txs observed — confirms Base sequencer has no public mempool visible through Alchemy

### 4. Support Sessions Started
- `botwatch` — kills livebot on first profitable trade ("Trade complete" trigger)
- `botstatus` — Discord status report (30min loop, multi-chain)

### 5. A4 Mempool Analysis Script Written
- New file: `scripts/analyze_mempool.py`
- Answers A4 decision gate questions from CSV + log data
- Sections: decision gate, router breakdown, decoder coverage, token pairs, gas analysis, hourly pattern, cumulative stats
- Tested against live data — working correctly
- Companion to existing `analyze_bot_session.py` and `analyze_price_logs.py`

---

## Early Observation Results (first ~15 minutes)

### Polygon
| Metric | Value | Threshold | Status |
|--------|-------|-----------|--------|
| Pending swaps decoded | 90 | — | ~4/min |
| Confirmation rate | 100% | >30% | **PASS** |
| Median lead time | 6,984ms | >500ms | **PASS (14x threshold)** |
| Lead time range | 2,316 — 10,249ms | — | All >2s |
| Decode rate | 100% | — | All selectors covered |
| Undecoded | 0 | — | Clean |

**Router breakdown:**
- UniswapV3: 77% of pending txs (all exactInputSingle)
- AlgebraV3 (QuickSwap): 23% (mix of algebraExactInputSingle + exactInput)
- SushiSwap V3: 0 pending txs observed in this window

### Base
| Metric | Value | Notes |
|--------|-------|-------|
| Pending swaps | 0 | Sequencer model — no public mempool |
| Confirmation rate | 0% | Expected |

**Conclusion:** Base mempool monitoring via Alchemy is not viable. Base needs sequencer feed or alternative approach.

---

## Strategic Discussion: Dynamic Trade Sizing (A7)

Discussed dynamic pricing strategy — key conclusions:
- Dynamic sizing is a **Phase 2/3 dependency**, not standalone
- In current block-reactive pipeline (97.1% revert), optimizing trade size doesn't help
- In mempool backrun pipeline, sizing depends on post-swap simulated state
- Optimal approach: A4 Phase 2 simulator → spread detection → optimal size → Phase 3 executor
- V2 constant-product sizing has closed-form solution
- V3 within single tick range is tractable; cross-tick rarely needed for arb-size swaps
- Capital allocation: sequential with full budget (atomic revert protection handles failures)

---

## Files Modified

| File | Change |
|------|--------|
| `scripts/analyze_mempool.py` | **NEW** — A4 mempool analysis script (decision gate, router, gas, hourly) |
| `.env.base` | Added `MEMPOOL_MONITOR=observe`, `EVENT_SYNC=true` |

## Files Committed (prior session, pushed this session)

17 files — see commit `a51db7b` for full list.

---

## Running tmux Sessions

| Session | Chain | Mode | Status |
|---------|-------|------|--------|
| `livebot_polygon` | Polygon | LIVE + observe | Collecting data |
| `dexarb-base` | Base | DRY RUN + observe | EVENT_SYNC enabled, 0 mempool txs |
| `botwatch` | — | — | Watching for first trade |
| `botstatus` | — | — | Discord 30min loop |

---

## Next Steps

1. **Let observation run 24h+** — need robust statistics before A4 Phase 2 decision
2. **Run `python3 scripts/analyze_mempool.py`** after 24h for full report
3. **If Polygon passes gates** (currently 100% / 6.9s — well above thresholds): proceed to A4 Phase 2 (AMM state simulation)
4. **Base strategy:** mempool approach not viable via Alchemy. Consider sequencer feed or focus on Polygon.
5. **Commit analysis script** — not yet committed (created after push)

---

*Session concluded with 4 tmux sessions running, 90+ pending swaps logged, early data strongly favoring A4 Phase 2.*
