# Session Summary: Route Cooldown & Operational Improvements (2026-01-31, Session 10)

## Overview

Implemented route-level cooldown with escalating backoff to eliminate dead/stale spread hammering. Also created a price log analysis script, performed multiple live bot status checks, and updated operational scripts for multi-chain naming (`livebot.polygon`).

## Context

- **Preceding work**: Sessions 8-9 completed Base chain support (Phase 2), Polygon live bot deployed with multicall bypass and V2↔V3 cross-protocol support
- **Trigger**: Live bot was hammering the same failed routes every ~2 seconds (e.g., WBTC/USDC attempted 8+ times in 16 seconds, all reverting)
- **Process state at start**: Bot running with multicall bypass, $500 trade size, atomic executor enabled
- **Root cause**: Structurally dead spreads (permanent fee-tier differences like UniV3_0.01% ↔ UniV3_0.30% = 0.31% round-trip fees) detected every block but can never be profitably executed

## What Changed

### 1. Route Cooldown (`src/rust-bot/src/arbitrage/cooldown.rs`) — NEW

HashMap-based tracker keyed by `(pair_symbol, buy_dex, sell_dex)`. After a route fails, it's suppressed for N blocks. Repeated failures escalate 5× per step:

| Failures | Cooldown | Duration (~2s blocks) |
|----------|----------|----------------------|
| 1 | 10 blocks | ~20 seconds |
| 2 | 50 blocks | ~1.7 minutes |
| 3 | 250 blocks | ~8.3 minutes |
| 4 | 1,250 blocks | ~42 minutes |
| 5+ | 1,800 blocks (cap) | ~1 hour |

On success: entry removed (instant reset). Periodic cleanup bounds memory. 7 unit tests.

### 2. Main Loop Integration (`src/rust-bot/src/main.rs`)

- Opportunities filtered through cooldown after detection, before execution
- Failures recorded in all error branches (Quoter rejection, unknown failure, execution error)
- Successes reset the cooldown for that route
- Status line now includes `N routes cooled` count
- Logs `🧊 N routes suppressed (cooldown), M remaining` when filtering

### 3. Config (`src/rust-bot/src/types.rs`, `src/config.rs`, `.env.polygon`)

- `route_cooldown_blocks: u64` added to BotConfig (default 10, env `ROUTE_COOLDOWN_BLOCKS`)
- Set to 0 to disable entirely

### 4. Price Log Analyzer (`scripts/analyze_price_logs.py`) — NEW

Comprehensive Python script (stdlib-only) analyzing live bot price CSV data:
- 7 analytical sections: price stats, cross-DEX spreads, opportunity frequency, volatility, hourly patterns, top spreads, key takeaways
- Auto-detects current run from log timestamps
- Fixed WBTC/USDC display (prices already in USD terms ~83000, not inverted)
- Usage: `python3 scripts/analyze_price_logs.py`

### 5. Operational Updates

- **Discord status report** (`bot_status_discord.sh`): Title changed to `livebot.polygon [RUNNING]` / `[DOWN]`, process status prominent
- **Bot watch** (`bot_watch.sh`): Kill target updated to `livebot_polygon` tmux session
- **Tmux session**: Renamed from `livebot` to `livebot_polygon` (tmux converts dots to underscores)

## Files Modified

| File | Change |
|------|--------|
| `src/rust-bot/src/arbitrage/cooldown.rs` | **NEW** — RouteCooldown with escalating backoff, 7 tests |
| `src/rust-bot/src/arbitrage/mod.rs` | Added `pub mod cooldown` + re-export |
| `src/rust-bot/src/types.rs` | Added `route_cooldown_blocks` to BotConfig |
| `src/rust-bot/src/config.rs` | Loads `ROUTE_COOLDOWN_BLOCKS` from env |
| `src/rust-bot/src/main.rs` | Init tracker, filter opps, record fail/success, cleanup, status logging |
| `src/rust-bot/src/arbitrage/detector.rs` | Fixed test config (added missing fields) |
| `src/rust-bot/.env.polygon` | Added `ROUTE_COOLDOWN_BLOCKS=10` |
| `scripts/analyze_price_logs.py` | **NEW** — price log statistical analyzer |
| `scripts/bot_status_discord.sh` | Multi-chain naming, [RUNNING]/[DOWN] indicator |
| `scripts/bot_watch.sh` | Updated kill target to `livebot_polygon` |

## Key Findings from Analysis

- **WETH/USDC**: Best arb pair — 0.164% mean spread, 74% of blocks show >0.10% spread
- **USDT/USDC "spreads"**: Structurally dead — 100% of blocks show >0.10% but it's permanent UniV3_0.01% vs UniV3_0.30% gap (0.31% round-trip fees). Route cooldown will suppress these.
- **Trade error decoded**: Custom error `0x4e88422a` = ArbExecutor's `InsufficientProfit(actualProfit, minProfit)`. First attempt showed $0.053 actual vs $0.10 minimum — trade was profitable but below threshold.

## Current State

- **Bot**: Running as `livebot_polygon` (PID active), route cooldown enabled
- **Build**: 58 tests pass, release binary compiled
- **Wallet**: ~516.70 USDC, ~165.57 MATIC
- **Discord**: Reports at :00/:30 with `[RUNNING]`/`[DOWN]` status
- **Botwatch**: Monitoring for first on-chain trade, targeting `livebot_polygon`

## Next Steps

- Monitor cooldown behavior in logs — verify dead spreads get suppressed
- Consider lowering `MIN_PROFIT_USD` from $0.10 to $0.05 (first trade attempt showed $0.053 actual profit)
- Investigate Polygon Fastlane (private mempool) for MEV protection
- Base chain: deploy ArbExecutor, begin data collection
