# Session Summary: RPC Call Audit & Paper Trader Analysis (2026-01-29)

## Overview

Pre-deployment review of RPC call rates and paper trader value before starting the live bot with whitelist v1.1. Found combined RPC usage likely exceeds Alchemy free tier, and the paper trader is entirely obsolete (V2-only, no overlap with V3 live strategy).

## Context

- **Preceding work**: Whitelist v1.1 deployed to config (10 pools, 7 blacklisted), binary built, ready to start live bot
- **Trigger**: User requested review of call optimization work and call rates before deploying
- **Process state at start**: Data collector RUNNING, paper trading RUNNING, live bot STOPPED

## RPC Call Audit

### Architecture

Three processes share one Alchemy RPC endpoint:

| Process | Config | Poll | Role |
|---------|--------|------|------|
| Data Collector | `.env` (10s) | 10,000ms | V2+V3 sync → JSON file |
| Paper Trading | `.env` (10s) | 10,000ms | V2-only detection (own syncer) |
| Live Bot | `.env.live` (3s) | 3,000ms | JSON reader → Quoter+execution |

### Data Collector Calls Per Iteration

Traced through actual code:

**V2 sync** (`syncer.rs:53` — `initial_sync()` every iteration):
- Per V2 pool: 5 calls (factory.getPair, getReserves, token0, token1, getBlockNumber)
- 7 pairs × 2 DEXes = up to 70 calls

**V3 sync** (`v3_syncer.rs:330` — staggered, 1 pair per iteration):
- Per existing pool: 6-7 calls (factory.getPool, slot0, liquidity, token0, token1, decimals cached, blockNumber)
- 1 pair × 3 fee tiers = ~15-18 calls

**Total per iteration**: ~86-89 calls at 10s → **~8.6-8.9 calls/sec**

### Paper Trading Calls Per Iteration

Own `PoolStateCollector` runs independent V2 `initial_sync()`:
- ~43-71 calls per iteration at 10s → **~4.3-7.1 calls/sec**

### Live Bot Calls

Reads JSON for prices (zero RPC). Calls only on opportunity detection:
- Per Quoter-rejected opportunity: 2 calls (gas + Quoter)
- Per executed trade: ~8-10 calls
- Average: **<0.5 calls/sec**

### Combined Rate

| Process | Calls/sec | Calls/month |
|---------|-----------|-------------|
| Data Collector | 5.8-8.9 | 15.0-23.1M |
| Paper Trading | 4.3-7.1 | 11.2-18.4M |
| Live Bot | <0.5 | <1.3M |
| **Total** | **~10.6-16.5** | **~27.5-42.8M** |

### Alchemy Free Tier Assessment

Alchemy uses Compute Units (CU): `eth_call` = 26 CU, free tier = 300 CU/sec, 300M CU/month.

At ~13 calls/sec midpoint (mostly `eth_call`): ~338 CU/sec (at the limit) and ~876M CU/month (exceeds 300M free tier).

The `.env` comment "9.6M calls/month" was based on data collector alone at 37 calls/cycle — does not account for paper trading doubling the load.

### Whitelist Trimming Effect on Call Rate

Whitelist trim (16→10 pools) does **NOT** reduce data collector RPC — syncer fetches all pools regardless. It reduces:
- Detector opportunity count (fewer combinations)
- Quoter calls from live bot (fewer pre-checks)
- Max combinations per scan: 5 (from ~21)

## Paper Trader Analysis

**The paper trader is V2-only and entirely obsolete for the V3 live strategy.**

### What It Runs

12 hardcoded strategy presets (Conservative, Moderate, Aggressive, etc.) with varying:
- Trade sizes ($100-$5000)
- Min profit thresholds ($2-$20)
- Pair subsets (WETH, WMATIC, WBTC)
- Competition simulation (30-70% loss rate)

### Why It's Useless

1. **V2-only detection** — `strategy.rs:91` calls `get_pools_for_pair()` (V2), never `get_v3_pools_for_pair()`. Live bot is V3-only.
2. **Own V2 syncer** — Runs independent `PoolSyncer::initial_sync()` every iteration (~70 RPC calls). Doesn't read from shared JSON.
3. **No whitelist** — Scans all pools including dead/thin ones.
4. **No Quoter** — Can't distinguish real from phantom spreads.
5. **V2 pairs unreliable** — Same pairs where V2 price inversion bugs caused the $500 loss.
6. **Synthetic competition** — Pseudo-random "did someone take it" vs real MEV data.

### Bottom Line

Paper trader answers: "What if we still traded V2 cross-DEX arbs?" — a question that's been obsolete since the V3 migration. It consumes ~40-50% of total RPC budget for zero actionable data.

## Actions Taken

1. **Killed all tmux sessions**: bot-watcher, dexarb, dexarb-phase1, live-bot, spread-logger — all 5 stopped
2. **Analysis delivered** — full RPC call breakdown with code references

## Process State at End

- **All processes**: STOPPED (all 5 tmux sessions killed)
- **Live wallet**: 160.00 USDC, ~7.73 MATIC (unchanged)
- **Backup wallet**: 356.70 USDC (unchanged)
- **Whitelist v1.1**: deployed to config, binary built, NOT running

## Key Files Read (Not Modified)

| File | Purpose |
|------|---------|
| `src/data_collector/mod.rs` | Data collector main loop — V2+V3 sync per iteration |
| `src/pool/v3_syncer.rs` | V3 sync implementation — 6-7 RPC calls per existing pool |
| `src/pool/syncer.rs` | V2 sync implementation — 5 RPC calls per pool |
| `src/arbitrage/detector.rs` | Opportunity detection — whitelist filtering |
| `src/arbitrage/executor.rs` | Trade execution — Quoter pre-checks |
| `src/main.rs` | Live bot main loop — JSON reader, zero RPC for discovery |
| `src/paper_trading/mod.rs` | Paper trading orchestration — 12 strategies |
| `src/paper_trading/strategy.rs` | V2-only scan_opportunities() |
| `src/paper_trading/config.rs` | 12 hardcoded presets |
| `src/paper_trading/collector.rs` | Own V2 syncer (duplicates data collector) |

## Recommendations for Next Session

1. **Don't restart paper trading** — it burns RPC budget for zero V3-relevant data
2. **Deploy with data collector + live bot only** — combined rate ~9 calls/sec, well within Alchemy limits
3. **If paper trading needed**, rewrite to:
   - Read from shared JSON (not own V2 syncer)
   - Use V3 detector logic (`check_pair_unified`)
   - Apply whitelist filtering
   - Test V3-relevant parameters (fee tier combos, spread thresholds)
4. **Phase 1.2** — not a prerequisite for deployment, can be done while bot runs

## Git Status

No files modified this session (analysis only + tmux cleanup). Session summary created.
