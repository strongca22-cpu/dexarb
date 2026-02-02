#!/usr/bin/env python3
# Session Summary: Base Diagnostic & WS Resilience (2026-02-01, Session 11)

## Overview

Verified Base bot atomic execution, assessed phantom spread risks, diagnosed WS subscription behavior, added timeout+reconnect resilience to the block subscription loop, ran historical analysis on all Base data, and concluded Base should wait for A4 mempool architecture before going live.

## Context

- **Preceding work**: Sessions 8-10 deployed ArbExecutor to Base, ran 5hr dry-run, disabled multicall pre-screen
- **Trigger**: User asked two questions: (1) Is Base deploying atomic at all levels / leg risk eliminated? (2) Thorough phantom spread assessment?
- **Process state at start**: Polygon live bot RUNNING (livebot_polygon), Base bot OFFLINE (died from Alchemy WS outage)
- **Build**: 58/58 Rust tests pass

## What Was Done

### 1. Atomic Execution Verification — CONFIRMED

All Base trades route through ArbExecutor (`0x9054...`) atomically. Traced in `executor.rs:208-212`:
- If `arb_executor_address.is_some()` → `execute_atomic()` — always true on Base
- Contract reverts entire tx if either leg fails or profit < minProfit
- Fee sentinel routing handles all 5 pool types: UniV3 100/500/3000, SushiV3 500/3000
- **Leg risk: zero.** Only gas burned on reverts.

Fixed outdated comment in `main.rs:13` — said "V2↔V3 uses legacy two-tx" but code supports V2↔V3 atomically.

### 2. Phantom Spread Assessment

**Live QuoterV2 verification** — called all 5 Base pools via `cast call`:

| Pool | 1 WETH Quote | Gas Est | Ticks Crossed |
|------|-------------|---------|---------------|
| UniV3 0.05% | 2,443.73 USDC | 76K | 1 |
| UniV3 0.30% | 2,433.15 USDC | 67K | 1 |
| UniV3 0.01% | 2,438.96 USDC | 123K | 2 |
| SushiV3 0.05% | 2,410.52 USDC | 741K | 28 |
| SushiV3 0.30% | 2,285.04 USDC | 508K | 18 |

**Key finding**: SushiV3 pools show much lower quotes due to **thin liquidity** (crossing 18-28 ticks at 1 WETH). At $100 trade size (~0.04 WETH), impact is minimal — but at $5K+, the spreads from SushiV3 are largely phantom (slippage eats the spread).

**Token/decimal verification**: WETH=18 decimals, USDC=6 decimals, token0=WETH (`0x4200...`), token1=USDC (`0x8335...`) — all correct.

**Risks assessed**:
- Decimal handling: SAFE (3-layer defense)
- Fee tier arithmetic: SAFE
- Token ordering: SAFE
- QuoterV2 encoding: VERIFIED (live call matches market price)
- Stale price timing: MEDIUM risk (mitigated by quoter safety check)
- No phantom spread patterns detected in historical data

### 3. WS Block Subscription — Misdiagnosis Resolved

**Initial concern**: Bot appeared to stall after receiving 1 block (37 log lines, no new output).

**Investigation**:
- Tested old binary (git stash) — same "stall"
- Tested Alchemy WS via Python websockets — blocks arriving fine every 2s
- Added debug logging (`⏳ Waiting for next block...`)

**Discovery**: Bot was **working the entire time**. The "stall" was sparse stdout logging — the bot only logs iteration status every 100 blocks (~3.3 min). Price data was being written continuously to CSV (25,700 rows in the price file, 5 pools × ~5,140 blocks). The WS subscription was delivering blocks every ~2 seconds.

### 4. WS Timeout + Reconnect Fix (Applied)

Even though the WS wasn't stalling now, added resilience for genuine stalls (like the Alchemy outage that killed the 5hr run):

**Changes to `main.rs`**:
- Added `use tokio::time::{timeout, Duration}`
- Wrapped `block_stream.next().await` in `timeout(Duration::from_secs(30), ...)`
- Outer `'reconnect` loop creates fresh WS provider on each cycle
- On timeout or stream end: `break` inner loop → drop old provider → reconnect
- Up to 50 reconnect attempts before full exit
- Scoped `sub_provider` to loop iteration (satisfies borrow checker)

**Early status log**: Changed `iteration % 100 == 0` to `iteration == 10 || iteration % 100 == 0`. First status log now appears at ~20s instead of ~3.3 min.

### 5. Historical Data Analysis

Ran 3 analysis scripts on all Base data (Jan 31 + Feb 1):

**Trade Size Analysis** (`analyze_trade_sizes.py`):

| Size | Jan 31 $/hr | Feb 1 $/hr | Best Route |
|------|-------------|------------|------------|
| $100 | $1.75 | $1.33 | SushiV3 0.30% → UniV3 0.01% |
| $500 | $11.87 | $9.68 | SushiV3 0.30% → UniV3 0.01% |
| $1,000 | $24.52 | $20.12 | SushiV3 0.30% → UniV3 0.01% |
| $5,000 | $125.97 | $103.73 | SushiV3 0.30% → UniV3 0.01% |

*Midmarket estimates — slippage significant above $500 on SushiV3.*

**Spread Analysis** (`analyze_price_log.py`):
- 25,700 rows, 5 DEX/fee combos, 6,434 blocks over 3.5 hours
- 13/20 pool combos showed profitable spreads
- Top routes: UniV3 0.01% ↔ SushiV3 0.05% (9.5% of blocks profitable at $140)
- SushiV3 0.30% → UniV3 0.01% (4.7% of blocks, but biggest spreads: max $1.53 at $140)

**Bot Session Analysis** (`analyze_bot_session.py` on 5hr log):
- 72 opportunities detected (14.8/hr)
- 25 dry-run execution attempts, 0 on-chain (LIVE_MODE=false)
- Median estimated profit: $0.08, max: $0.27
- 62.5% of opportunities under $0.10
- Burst clustering: 57/72 opps arrived within 5s of each other
- 67/72 concentrated in 14:00 UTC hour (US market open)
- No phantom spread patterns detected

### 6. Strategic Conclusion: Wait for A4

Base exhibits the same structural dynamics as Polygon:
- Real spreads exist but are small ($0.08 median)
- Block-reactive architecture cannot close trades (competitors use mempool)
- The 97.1% revert rate on Polygon would replicate on Base
- Base is actually a **better** A4 target (sequencer feed = full mempool visibility vs Alchemy's partial Bor view)
- A4 architecture is 100% chain-portable — build on Polygon, port to Base mechanically

**Recommendation**: Keep Base dry-run collecting data. Don't fund wallet with trading capital. Build A4 on Polygon first, port to Base as A11.

## Files Changed

| File | Action | Details |
|------|--------|---------|
| `src/rust-bot/src/main.rs` | Modified | WS timeout+reconnect loop, early status log (iter 10), fixed V2↔V3 comment |
| `src/rust-bot/src/arbitrage/detector.rs` | Modified | Added `private_rpc_url: None` to test helper |
| `docs/session_summaries/2026-02-01_session11_base_diagnostic.md` | Created | This file |
| `docs/next_steps.md` | Updated | Session 11 findings, Base assessment |

## Build & Test Results

- **Build**: Clean release (warnings: pre-existing unused imports in syncer/oracle)
- **Tests**: 58/58 pass
- **Bot status**: Polygon live (livebot_polygon), Base dry-run (dexarb-base)

## Key Metrics

| Metric | Value |
|--------|-------|
| Base opportunities/hr | 14.8 |
| Base median profit | $0.08 |
| Base max profit | $0.27 |
| Profitable routes | 13/20 combos |
| Best route | SushiV3 0.30% → UniV3 0.01% |
| WS blocks received | Every ~2s (confirmed working) |
| QuoterV2 verified | 2,443.73 USDC/WETH (matches market) |
| Phantom spreads | None detected |
