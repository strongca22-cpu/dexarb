# Next Steps - DEX Arbitrage Bot

## Current Status: ALL STOPPED — Pre-Deploy RPC Audit Complete

**Date:** 2026-01-29
**Config:** Split — live bot reads `.env.live` (7 pairs), data collector reads `.env` (7 pairs)
**Architecture:** Shared data — live bot reads pool state from JSON (data collector writes)
**Phase 1.1:** COMPLETED — whitelist filter built, tested (8/8), binary compiled, NOT deployed
**Phase 2.1:** COMPLETED — Multicall3 batch Quoter pre-screening, 7 unit tests pass, binary compiled
**Whitelist v1.1:** Trimmed 16→10 pools, 7 blacklisted pools, 1 observation. Only deepest pools remain.
**LIVE_MODE:** true in `.env.live` (bot stopped — new binary ready but not started)
**All processes:** STOPPED (all 5 tmux sessions killed 2026-01-29)
**Paper trader:** OBSOLETE — V2-only, no overlap with V3 live strategy, burns ~40-50% of RPC budget for zero value

**Two-Wallet Architecture (2026-01-29):**

| Wallet | Address | Purpose | USDC | MATIC |
|--------|---------|---------|------|-------|
| **Live** | `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2` | Trading (at-risk capital) | 160.00 | ~7.73 |
| **Backup** | `0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb` | Deep storage (manual only) | 356.70 | 0 |

- Live wallet key: configured in `.env.live` `PRIVATE_KEY`
- Backup wallet key: stored in `/home/botuser/bots/dexarb/.env.backup` (NOT used by any bot process)
- Transfer from backup → live: manual only (Python script or web3 CLI)
- Live wallet holds enough for ONE $140 trade + gas buffer

**Current Settings:**
- MAX_TRADE_SIZE_USD=140.0
- MIN_PROFIT_USD=0.10
- MAX_SLIPPAGE_PERCENT=0.5

---

## Incident Report (2026-01-29)

### What Happened

1. V3 `exactInputSingle` swap support added to `executor.rs` — compiled and deployed
2. Bot detected UNI/USDC opportunity: buy 1.00% tier @ 0.2056, sell 0.05% @ 0.2102
3. **Buy leg EXECUTED on-chain**: 500 USDC → 0.0112 UNI (worth $0.05)
4. Sell leg failed at gas estimation: "Too little received"
5. **Net loss: ~$500** due to three critical bugs

### Root Cause 1: `calculate_min_out` Decimal Mismatch

`executor.rs` computed `amount_in_raw * price` without converting token decimals:
- Input: 500,000,000 (500 USDC, 6 decimals)
- Price: 0.205579
- Computed min_out: 102,275,689
- In UNI's 18-decimal format: **0.0000000001 UNI** — effectively zero slippage protection
- Correct min_out: ~102 * 10^18 = 1.02 * 10^20

### Root Cause 2: No Pool Liquidity Check

The V3 1.00% UNI/USDC pool had almost no liquidity. The 500 USDC trade consumed everything, receiving 99.99% less UNI than expected. The detector checks price but never checks if pool liquidity can absorb the trade size.

### Root Cause 3: Trade Direction Inverted (NEW — discovered during fix analysis)

The detector's `check_pair_unified` assigned:
- `buy_pool` = LOWER V3 price (0.2056 UNI/USDC) — fewer UNI per USDC
- `sell_pool` = HIGHER V3 price (0.2102 UNI/USDC) — more UNI per USDC

But V3 price = token1/token0 (UNI per USDC for this pair). The execute flow does:
- Step 1: token0→token1 (USDC→UNI) on buy_pool → gets FEWER UNI
- Step 2: token1→token0 (UNI→USDC) on sell_pool → gets FEWER USDC per UNI

This trades BACKWARDS — buying expensive and selling cheap. Even with perfect liquidity:
- Wrong direction: 500 USDC → 101.8 UNI → 484 USDC = **-$16 loss**
- Correct direction: 500 USDC → 105.0 UNI → 505.6 USDC = **+$5.60 profit**

### On-Chain Evidence

- Buy tx: `0x4dbb48aeac557cde8ca986d422d0d70515a29c74588429116aea833fe110acae`
- Approval tx: `0x781ae7e444067b2ab93ec010892ff7c699aff27e3e684b39742b80908702243b`
- Sell tx: never sent (reverted at `eth_estimateGas`)

---

## Critical Fixes — All Completed

1. [x] **Fix `calculate_min_out` decimal conversion** — now converts raw→human→raw with proper 10^(decimals) scaling
2. [x] **Add pool liquidity check** — detector rejects pools with liquidity < 1000 (dust) and < trade_size*1e6 (too thin)
3. [x] **Use V3 Quoter for pre-trade simulation** — `quoteExactInputSingle` called before execution; aborts if output < min_out
4. [x] **Parse actual `amountOut` from receipt logs** — parses ERC20 Transfer events instead of using min_out placeholder
5. [x] **Fix trade direction** — buy_pool = HIGHER V3 price (more token1 per token0), sell_pool = LOWER price (more token0 per token1)
6. [x] **Add token decimals to ArbitrageOpportunity** — detector passes actual decimals from V3PoolState to executor

### Previously Completed (V3 swap routing)
- [x] Add `ISwapRouter` ABI binding (`exactInputSingle`)
- [x] Add `is_v3()` check in `swap()` to branch V2 vs V3 logic
- [x] Build V3 `ExactInputSingleParams` struct
- [x] Token approvals routed to V3 SwapRouter

### Deferred
- [ ] Fix V2 price calculation (inverted reserve ratio)

---

## Live Verification Test (2026-01-29)

### Test 1: $50 Trade Cap (~40 min run)
- **Settings**: MAX_TRADE_SIZE_USD=50, MIN_PROFIT_USD=0.25
- **Result**: Zero opportunities detected in ~70 scan iterations
- **Reason**: $0.50 gas cost dominates at $50 trade size. Best spread (WBTC 1.32%) yields only $0.094 net at $50.

### Test 2: $200 Trade Cap
- **Settings**: MAX_TRADE_SIZE_USD=200, MIN_PROFIT_USD=0.10
- **Result**: 4 opportunities detected immediately
  - WBTC/USDC 1%↔0.05%: spread 1.17%, est $1.61
  - LINK/USDC 0.05%↔1%: spread 0.83%, est $0.99
  - LINK/USDC 0.30%↔1%: spread 0.38%, est $0.19
  - UNI/USDC 0.30%↔0.05%: spread 0.48%, est $0.37
- **Trade attempt**: Bot selected WBTC (highest profit)
- **Quoter rejection**: Quoted output 64,507,401 vs required 178,610,877,064
  - The 1% pool had ~$0.06 of real WBTC — nowhere near the $200 needed
  - This is exactly the scenario that caused the $500 loss previously
- **Capital at risk**: ZERO (Quoter uses `.call()` — read-only, no gas spent)
- **Watcher behavior**: "Trade failed" triggered `stop_after_trade.sh`, which killed bot and reset LIVE_MODE=false

### Verification Conclusions

1. **V3 Quoter pre-trade simulation works perfectly** — correctly rejected thin-pool trade with zero capital risk
2. **Opportunity detection works** — found 4 real spread opportunities across V3 fee tiers
3. **Auto-stop watcher works** — detected trade event, killed bot, reset LIVE_MODE
4. **1% fee tier pools are unreliable** — stale prices with negligible liquidity; Quoter handles this correctly
5. **Most realistic candidate**: UNI/USDC 0.30%↔0.05% (both pools have real liquidity: 151B and 6.1T units)

---

## Post-Verification Improvements (2026-01-29)

### 1. Watcher Updated — Quoter Rejections No Longer Stop Bot

`stop_after_trade.sh` now distinguishes safe vs dangerous events:
- **STOP**: "Trade complete", "Buy swap failed", "Sell swap failed", "Execution error"
- **CONTINUE**: "Trade failed" (Quoter rejections — zero capital, logged and counted)

### 2. Try-All Execution — Bot Falls Through Quoter Rejections

`main.rs` now iterates all detected opportunities in profit order:
- Try #1 (best) → if Quoter rejects → try #2 → ... → try #N
- Stops on: success, on-chain failure, or unexpected error
- Previously: only tried the best, so 1% pool phantom opportunities blocked all real trades

### 3. Detector Returns All Profitable Combinations

`detector.rs` `check_pair_unified()` now returns ALL profitable fee tier combinations per pair (not just the best). This means for LINK/USDC, both 0.05%↔1% AND 0.30%↔1% are returned. The executor tries them in profit order and falls through Quoter-rejected 1% routes.

### 4. Trade Size Lowered to $140

Based on log analysis of UNI/USDC 0.30%↔0.05% spread history:
- The spread was profitable (≥0.45% exec) about 25 of 40 minutes observed
- At 0.48% exec spread (most common): min trade = $139 for net ≥ $0.10
- At 0.86% exec spread (peak): min trade = $78
- $140 is the minimum to catch the commonly-observed 0.48% spread

### 5. Sell-Leg Quoter Check Added

`executor.rs` now simulates BOTH legs before committing the sell transaction:
- **Buy-leg Quoter**: before any capital is committed (zero risk if rejected)
- **Sell-leg Quoter**: after buy succeeds, before sell tx (catches price movement between legs)
- If sell Quoter rejects: logs `"Sell swap failed"` → watcher STOPs → manual exit needed
- Uses actual `amount_received` from buy (not estimated) as sell input

---

## Live Test 3: $140 Trade Cap — Try-All + Sell Quoter (2026-01-29)

### Settings
- MAX_TRADE_SIZE_USD=140, MIN_PROFIT_USD=0.10, MAX_SLIPPAGE_PERCENT=0.5
- LIVE_MODE=true, bot + watcher in tmux sessions

### Results (running)
- 3 opportunities detected per cycle (same as before):
  - WBTC/USDC 1%↔0.05%: 1.06% spread, est $0.84
  - UNI/USDC 0.30%↔0.05%: 0.96% spread, est $0.71
  - WBTC/USDC 1%↔0.30%: 0.85% spread, est $0.57

- **Try-all execution working**: bot iterates all 3, Quoter rejects each, moves to next scan
- **WBTC 1% pools**: Quoter returns 64.5M vs required 125B — empty pool, as expected
- **UNI 0.30%↔0.05%**: Quoter returns 29.435e18 UNI vs required 29.663e18 — **0.77% short**
  - The $140 trade causes ~1.26% price impact on the 0.30% pool
  - Even if forced: 140 USDC → 29.435 UNI → $139.95 USDC = **-$0.55 loss** (including gas)
  - **Quoter is correctly preventing a losing trade**

### Key Finding: Price Impact vs Spread

The detector estimates 0.96% executable spread, but the actual price impact of $140 on the 0.30% pool consumes most of it. The V3 concentrated liquidity near the current tick is shallow enough that $140 moves the price ~1.26%.

- **Detector's spot-price spread**: 0.96% (looks profitable)
- **Quoter's actual execution**: -0.55% (would lose money)
- **Root cause**: Spot price ≠ executable price at trade size

### Implications

1. Quoter safety system is working perfectly — prevents all unprofitable trades
2. The 0.30%↔0.05% spread needs to be wider (~2%+) to overcome price impact at $140
3. Bot will keep scanning safely until a wider spread appears
4. Zero capital at risk — all rejections are read-only `.call()` simulations

### Status
- **Bot**: running in `live-bot` tmux, scanning every ~10s
- **Watcher**: running in `bot-watcher` tmux, monitoring for trade events
- **Capital**: unchanged (zero spent)

---

## Paper Trading Liquidity Filter (2026-01-29, ~05:20 UTC)

### Problem
Paper trading reports inflated by ~200+ phantom 1% fee tier opportunities per hour. Discord hourly reports showed $4,322 "potential profit" — almost entirely from phantom 1% pool routes that the live Quoter rejects every time.

### Fix
1% fee tier pools (fee >= 10000) excluded entirely from paper trading scanner in `paper_trading.rs`. ALL 1% pools on Polygon have negligible executable liquidity — confirmed by live Quoter testing across UNI, WBTC, and LINK.

The V3 `liquidity` field alone can't distinguish phantom from real pools (UNI 1% at 8.84e10 vs UNI 0.05% at 1.51e11 — only 1.7x apart). Since paper trading has no Quoter access, fee-tier exclusion is the most reliable approach.

### Result
Paper trading now only reports 0.30% ↔ 0.05% routes. Reports will show realistic opportunity counts and profit estimates.

---

## RPC Call Budget Analysis (2026-01-29, Updated)

### Detailed Call Audit (code-traced)

**Data Collector** (per 10s iteration):

| Component | Calls | Source |
|-----------|-------|--------|
| V2 sync (7 pairs × 2 DEXes × 5 calls) | 42-70 | `syncer.rs:53` `initial_sync()` |
| V3 sync (1 pair staggered × 3 tiers × ~6 calls) | 15-18 | `v3_syncer.rs:330` `sync_v3_pools_subset()` |
| Block number | 1 | `data_collector/mod.rs:157` |
| **Total** | **~58-89** | **~5.8-8.9 calls/sec** |

**Paper Trading** (per 10s iteration):

| Component | Calls | Source |
|-----------|-------|--------|
| V2 sync (own `initial_sync()`) | 42-70 | `paper_trading/collector.rs:94` |
| Block number | 1 | `paper_trading/collector.rs:100` |
| **Total** | **~43-71** | **~4.3-7.1 calls/sec** |

**Live Bot** (reads JSON, RPC only on opportunities):

| Event | Calls |
|-------|-------|
| No opportunity (99% of cycles) | 0 |
| Quoter-rejected opportunity | 2 (gas + Quoter) |
| Executed trade | ~8-10 |
| **Average** | **<0.5 calls/sec** |

### Combined Rate

| Scenario | Calls/sec | Monthly | vs Alchemy Free (300M CU) |
|----------|-----------|---------|---------------------------|
| Data Collector + Paper + Live | ~10.6-16.5 | 27.5-42.8M calls (~876M CU) | **Exceeds** |
| Data Collector + Live only | ~6.3-9.4 | 16.3-24.4M calls (~507M CU) | **Marginal** |
| Data Collector only (no V2) | ~1.6-1.9 | 4.1-4.9M calls (~128M CU) | **Safe** |

### Key Finding

The `.env` comment "37 calls/cycle = 9.6M/month" was for the data collector V3 portion only. The V2 sync (`initial_sync()` called every iteration) adds 42-70 calls. Paper trading duplicates this. Combined rate likely exceeds Alchemy free tier.

### Recommendation

Deploy with **data collector + live bot only** (no paper trading). Consider removing V2 sync from the data collector since the live bot is V3-only.

**Note:** Alchemy uses Compute Units, not raw calls. `eth_call` = 26 CU. Free tier = 300 CU/sec burst, 300M CU/month.

---

## Architecture Overhaul — Implemented (2026-01-29)

### Problem: 30s Cycles

Previous cycle = 10s sleep + 5-8s V2 sync + 10.5s V3 sync + 0.6s Quoter = ~30s total. V3 sync was sequential (21 pools × 500ms each).

### Changes Implemented

1. **[DONE] Poll interval → 3s** (from 10s) — `.env` updated
2. **[DONE] Drop V2 sync entirely** — `main.rs` no longer imports/uses `PoolSyncer`. V2 code retained but not called.
3. **[DONE] Drop 1% fee tier from live detector + syncer** — `v3_syncer.rs` skips `fee >= 10000`, `detector.rs` filters in `check_pair_unified()`
4. **[DONE] Parallelize V3 sync** — `v3_syncer.rs` added `sync_known_pools_parallel()`: all pool slot0+liquidity calls run concurrently via `futures::future::join_all`
5. **[DEFERRED] Quoter architecture** — Per-cycle Quoter retained for safety. User will discuss Options 2/3 (periodic Quoter, atomic multi-leg) in a future session.

### Files Modified

| File | Change |
|------|--------|
| `src/main.rs` | V3-only loop, parallel sync, V2 imports removed |
| `src/pool/v3_syncer.rs` | 1% filter, `sync_known_pools_parallel()` method |
| `src/arbitrage/detector.rs` | V3-only scan, 1% filter in `check_pair_unified()` |
| `.env` | `POLL_INTERVAL_MS=3000` |

### Expected Impact

| Metric | Before | After |
|--------|--------|-------|
| Cycle time | ~30s | ~4-5s |
| RPC calls/cycle | ~203 | ~30 (14 V3 pools × 2 calls + block check) |
| Latency to trade | ~30s | ~4-5s |

### Deployment Status

- **Build**: compiles successfully after `tokio::join!` lifetime fix
- **Deployment**: LIVE — V3-only binary in `live-bot` tmux (2026-01-29)
- **RPC**: Alchemy WSS (migrated from PublicNode 2026-01-29)
- **Active pairs**: 7 (WETH, WMATIC, WBTC, USDT, DAI, LINK, UNI — all /USDC)
- **Cycle time**: ~3.6s (confirmed from logs)
- **Capital at risk**: zero (all Quoter rejections are read-only)

---

## V4 Pair Expansion — Gate Check Results (2026-01-29)

Pool gate checks run via `scripts/pool_gate_check.py`. See `docs/v4_alternate_pairings_buildout.md` for full details.

### Actionable Items

- [x] **AAVE/USDC** — Added, observed, **removed**. Phantom 69% spread confirmed (0.05% @ 0.010822 vs 0.30% @ 0.006390, Quoter gap 302,000x). Polluted paper trading Discord with ~$9M/15min phantom profit. Removed from `.env`, `paper_trading.toml`, and all strategies. Gate check data preserved in docs.
- [x] **Add 0.01% fee tier** — DONE (2026-01-29). Added `UniswapV3_001` to `DexType`, `(100, ...)` to `V3_FEE_TIERS`. Data-collector and paper-trading restarted with 18 V3 pools (was 14). Live bot unchanged (still 14 pools). Paper trading detecting 0.01%↔0.05% routes (0.06% round-trip fee). USDT/USDC and DAI/USDC 0.01% pools confirmed active with deep liquidity.
- [x] **Filter zero-liquidity 0.01% pools** — DONE (2026-01-29). `sync_v3_pool()` now returns `None` for pools with `liquidity == 0`. WBTC/LINK/UNI 0.01% pools (zero liquidity, phantom prices) no longer synced. 21→18 V3 pools. Saves 6 RPC calls/cycle.
- [ ] **Prune inactive pairs** — after 48h of spread data, remove any pairs that never show spread variation (saves 4 RPC calls/cycle each).
- [x] **Separate data-collector config from live bot** — DONE. Live bot reads `.env.live` (7 proven pairs), data collector/paper trading read `.env`. Config split via `load_config_from_file()` in `config.rs`.

### Eliminated Candidates

CRV, SUSHI, BAL, GRT, SNX, 1INCH, GHST, COMP, stMATIC, wstETH — all failed gate checks (missing pools or zero liquidity at 0.05% tier). Re-check monthly.

---

## Still Deferred

### Quoter Architecture (Options 2/3)

User requested deferring this discussion. Options under consideration:
- **Option 2**: Periodic Quoter (pre-validate pools every 10-30 min, trade instantly on validated pools)
- **Option 3**: Atomic multi-leg via smart contract (flash loan arb — borrow, swap, swap, repay in one tx)

Both reduce leg risk. Per-cycle Quoter is retained until one of these is implemented.

### Other

- [ ] Fix V2 price calculation (inverted reserve ratio) — V2 not in use
- [x] Alchemy RPC migration — DONE (2026-01-29). All processes on Alchemy WSS.
- [ ] Optimize for >$140 trade sizes (need deeper V3 liquidity)
- [x] Separate live bot config from dev/paper config — DONE. Live bot reads `.env.live`, data collector reads `.env`.

---

## Commands

```bash
# Build
cd /home/botuser/bots/dexarb/src/rust-bot && cargo build --release

# Start live bot
tmux new-session -d -s live-bot -c /home/botuser/bots/dexarb/src/rust-bot
tmux send-keys -t live-bot "./target/release/dexarb-bot 2>&1 | tee ../../data/bot_live.log" Enter

# Start auto-stop watcher
tmux new-session -d -s bot-watcher -c /home/botuser/bots/dexarb
tmux send-keys -t bot-watcher "./scripts/stop_after_trade.sh" Enter

# Pre-deployment checklist
./scripts/checklist_full.sh

# Pool gate checks (validate candidate pairs)
python3 scripts/pool_gate_check.py AAVE                    # single known token
python3 scripts/pool_gate_check.py --group A               # Group A candidates
python3 scripts/pool_gate_check.py --group stablecoin --fees 100,500,3000  # 0.01% check
python3 scripts/pool_gate_check.py --all                   # everything
```

---

## Shared Data Architecture — Implemented (2026-01-29)

### Problem: Alchemy Free Tier Exceeded

Combined RPC usage (data collector + live bot each making independent calls):
- Data collector: 37 calls/3s = 12.3/sec
- Live bot: 29 calls/3s = 9.7/sec
- Combined: 22 calls/sec = ~57M/month (2.6x over Alchemy free tier of 22.2M)
- Result: WebSocket "Control frame too big" + HTTP 429, all processes dead at 12:34

### Solution: Single Data Source

- Data collector makes ALL RPC calls, writes pool state to JSON (`pool_state_phase1.json`)
- Live bot reads JSON for pool prices (zero RPC for price discovery)
- RPC used ONLY for Quoter pre-checks and trade execution
- Data collector bumped to 10s poll interval

| Scenario | Data Collector | Live Bot | Combined | Monthly |
|----------|---------------|----------|----------|---------|
| **Before** | 37 calls/3s (12.3/sec) | 29 calls/3s (9.7/sec) | 22.0/sec | ~57M |
| **After** | 37 calls/10s (3.7/sec) | ~0 (file reads) | ~3.7/sec | ~9.6M |

### Files Modified

| File | Change |
|------|--------|
| `src/main.rs` | Removed V3PoolSyncer, reads from SharedPoolState JSON |
| `src/types.rs` | Added `pool_state_file: Option<String>` to BotConfig |
| `src/config.rs` | Parse `POOL_STATE_FILE` env var |
| `.env.live` | Added `POOL_STATE_FILE` path, `POLL_INTERVAL_MS=3000` (file reads are free) |
| `.env` | `POLL_INTERVAL_MS=10000` (data collector at 10s) |

### Benefits

- Live bot auto-inherits new pools when data collector is updated (no rebuild needed)
- Adding pairs = update data collector `.env` + restart data collector
- File reads are essentially free — live bot can poll at 3s even though data updates at 10s

---

## Buy-Then-Continue Bug — Fixed (2026-01-29)

### The Bug

When the executor returned a failed `TradeResult` after a buy succeeded but sell Quoter rejected, the error message contained "Quoter":

```
"Sell Quoter rejected after buy executed (holding token1, manual sell needed): V3 Quoter: output 85120245 < min_out 140878328"
```

The main loop at `main.rs:219` checked `error_msg.contains("Quoter")` and did `continue` — treating ALL Quoter mentions as zero-capital-risk. This caused the bot to keep trying new opportunities while holding unbalanced tokens from a previous buy.

### Impact

3 buy transactions executed for WETH/USDC, all sell legs rejected. Wallet held ~0.149 WETH that required manual recovery.

### The Fix

`main.rs:219-240` now checks `result.tx_hash.is_some()` FIRST:

```rust
// CRITICAL: If a transaction was submitted on-chain, capital is
// committed. HALT immediately — do NOT try next opportunity.
if result.tx_hash.is_some() {
    error!("🚨 HALT: On-chain tx submitted but trade failed...");
    break;
}

// No tx submitted = pre-trade rejection (zero capital risk)
if error_msg.contains("Quoter") || error_msg.contains("Gas price") {
    continue;  // Safe to try next opportunity
}
```

This is safe because:
- `tx_hash` is `None` for all pre-trade rejections (gas check, Quoter pre-check)
- `tx_hash` is `Some(...)` only when a transaction was actually submitted on-chain
- If any on-chain tx was submitted and the trade failed, we halt immediately

---

## WETH/USDC 0.01% Incident — $3.35 Loss (2026-01-29)

### What Happened

1. Live bot started with new shared data architecture (reads from JSON)
2. Data collector provided 18 V3 pools including WETH/USDC 0.01% (liquidity: 749B)
3. Detector found WETH/USDC spread: Buy 0.05% @ 0.000355, Sell 0.01% @ 0.000352 → 0.78% spread
4. Buy Quoter check passed (0.05% pool has real depth)
5. Buy executed: 140 USDC → 0.0498 WETH
6. Sell Quoter check: 0.01% pool returned only 85 USDC for 0.0498 WETH (vs expected 141 USDC)
7. **Buy-then-continue bug**: bot continued to next cycle, bought WETH two more times
8. Total: 3 buys executed (420 USDC spent), all sell legs rejected

### Recovery

- Manual sell of 0.149 WETH on 0.05% pool → recovered 416.68 USDC
- Net loss: $3.35 (swap fees + gas for 3 buys, 5 reverted buys, 1 approve, 1 manual sell)

### Root Cause

The WETH/USDC 0.01% pool has a valid tick/price but near-zero executable depth for $140 trades. The `liquidity` value (749B) passed the detector's minimum threshold (140M) but doesn't represent real depth at the current tick.

### Fixes Applied

1. **Buy-then-continue bug fixed** (see above)
2. **0.01% tier** should be restricted to stablecoin pairs (USDT/USDC, DAI/USDC only)
3. See `docs/phantom_spread_analysis.md` for full analysis

---

## Two-Wallet Architecture — Implemented (2026-01-29)

### Design

| Wallet | Role | Risk Level |
|--------|------|-----------|
| **Live wallet** | Trading bot uses this for swaps | At risk — any code bug could drain it |
| **Backup wallet** | Deep storage, never used by bots | Safe — only accessed manually |

### Rules

1. Live wallet holds MAX one trade worth of capital ($140) + gas buffer (~$20 MATIC)
2. Backup wallet is NEVER configured in any `.env` file
3. Transfers backup → live are manual-only (Python script, never automated)
4. After a profitable trade, excess profits stay in live wallet until manually swept to backup
5. If live wallet is drained by a bug, maximum loss = $160 instead of entire portfolio

### Wallet Details

- **Live**: `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2` — key in `.env.live`
- **Backup**: `0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb` — key in `.env.backup`

### Current Balances (2026-01-29 ~18:30 UTC)

| Wallet | USDC | WETH | UNI | MATIC |
|--------|------|------|-----|-------|
| Live | 160.00 | 0 | 0.011 (dust) | ~7.73 |
| Backup | 356.70 | 0 | 0 | 0 |

---

## Phase 1.1: Static Whitelist/Blacklist — COMPLETED (2026-01-29)

### What Was Built

A JSON-driven whitelist/blacklist filter for V3 pools, integrated into the opportunity detector. Replaces hardcoded fee tier and liquidity checks with a configurable system.

### Files Created/Modified

| File | Change |
|------|--------|
| `config/pools_whitelist.json` | 16 whitelisted pools, 1 blacklisted pool (WETH/USDC 0.01%), 1 blacklisted tier (1%), 1 observation pool (WMATIC/USDC 0.01%) |
| `src/filters/mod.rs` | NEW — module declaration |
| `src/filters/whitelist.rs` | NEW — `WhitelistFilter` with O(1) HashSet-based lookup, per-tier liquidity thresholds, 6 unit tests |
| `src/arbitrage/detector.rs` | Integrated `WhitelistFilter` (supersedes hardcoded checks) |
| `src/types.rs` | Added `whitelist_file: Option<String>` to `BotConfig` |
| `src/config.rs` | Parse `WHITELIST_FILE` env var |
| `src/lib.rs` | Added `pub mod filters` |
| `.env` / `.env.live` | Added `WHITELIST_FILE` path |

### Pool Classification

| Category | Count | Details |
|----------|-------|---------|
| Whitelisted | 10 | Deepest pools only (<5% impact@$5K): WETH 0.05%+0.30%, WMATIC 0.05%, WBTC 0.05%, USDT 0.01%+0.05%+0.30%, DAI 0.01%+0.05%, LINK 0.30% |
| Blacklisted | 7 pools + 1 tier | 6 thin/marginal pools (depth matrix) + WETH/USDC 0.01% (live incident); entire 1.00% tier |
| Observation | 1 | WMATIC/USDC 0.01% (94.6% impact@$5K, works at $100) |

### Per-Tier Liquidity Thresholds

| Tier | Fee | Min Liquidity |
|------|-----|---------------|
| 0.01% | 100 | 10B |
| 0.05% | 500 | 5B |
| 0.30% | 3000 | 3B |
| 1.00% | 10000 | Blacklisted |

### Build & Test

- `cargo build --release`: SUCCESS
- `cargo test`: 8/8 passing (6 whitelist + 2 detector)

### Deployment Status

- Binary built but **NOT deployed** — live bot remains stopped
- Next: restart live bot to activate whitelist filtering

---

## Whitelist Verifier & Quote Depth Matrix (2026-01-29)

### Verifier Script (`scripts/verify_whitelist.py`)

Python script (stdlib only, curl + JSON-RPC) that validates all pools in `config/pools_whitelist.json` on-chain.

**5 checks per whitelisted pool:**

| # | Check | Pass Criteria |
|---|-------|---------------|
| 1 | Pool exists | `eth_getCode` returns bytecode |
| 2 | slot0 valid | `sqrtPriceX96 > 0` |
| 3 | Liquidity threshold | `liquidity >= pool.min_liquidity` |
| 4 | Fee match | On-chain fee == whitelist `fee_tier` |
| 5 | Quote check | `quoteExactInputSingle` $1 USDC → output > 0 |

**Blacklist verification:** $1 + $140 quotes, price impact >5% = confirmed still problematic.

**Results (2026-01-29):** 16/16 whitelisted PASS, 1/1 blacklisted confirmed dead (76.4% impact at $140), 1/1 observation PASS basic checks.

### Quote Depth Matrix

Runs $1, $10, $100, $1000, $5000 quotes per pool to reveal executable depth.

**Whitelist v1.1 — 10 pools (deepest only, <5% impact@$5K):**

| Pool | Fee | $1 | $100 | $1K | $5K | Impact@$5K |
|------|-----|----|------|-----|-----|------------|
| WETH/USDC | 0.05% | $1.00 | $99.99 | $999 | $4987 | 0.3% |
| WETH/USDC | 0.30% | $1.00 | $99.96 | $996 | $4909 | 1.8% |
| WMATIC/USDC | 0.05% | $1.00 | $99.92 | $992 | $4808 | 3.8% |
| WBTC/USDC | 0.05% | $1.00 | $100.04 | $1000 | $4996 | 0.1% |
| USDT/USDC | 0.01% | $1.00 | $100.00 | $1000 | $4999 | 0.0% |
| USDT/USDC | 0.05% | $1.00 | $100.00 | $1000 | $4996 | 0.1% |
| USDT/USDC | 0.30% | $1.00 | $100.00 | $1000 | $4993 | 0.1% |
| DAI/USDC | 0.01% | $1.00 | $100.00 | $1000 | $4999 | 0.0% |
| DAI/USDC | 0.05% | $1.00 | $100.00 | $1000 | $4999 | 0.0% |
| LINK/USDC | 0.30% | $1.00 | $99.99 | $999 | $4971 | 0.6% |

**Blacklisted — 7 pools (dead, thin, or marginal):**

| Pool | Fee | Impact@$5K | Reason |
|------|-----|------------|--------|
| WETH/USDC | 0.01% | 99.2% | Live incident — caused $3.35 loss |
| UNI/USDC | 0.05% | 100% | Dead — $10 in → $1.29 out |
| UNI/USDC | 0.30% | 16.8% | Marginal — rapid-fire repeat trades drain spread |
| DAI/USDC | 0.30% | 99.1% | Exhausted — maxes at ~$44 |
| LINK/USDC | 0.05% | 73.1% | Thin — impact eats spread at $140 |
| WMATIC/USDC | 0.30% | 53.5% | Thin — 8%+ loss at $100 |
| WBTC/USDC | 0.30% | 5.5% | Borderline — rapid-fire repeat trades risky |

### Arb-Viable Pairs (after trimming)

| Pair | Pools | Arb viable? |
|------|-------|-------------|
| WETH/USDC | 0.05% + 0.30% | Yes |
| USDT/USDC | 0.01% + 0.05% + 0.30% | Yes (3-way) |
| DAI/USDC | 0.01% + 0.05% | Yes |
| WMATIC/USDC | 0.05% only | No — orphan |
| WBTC/USDC | 0.05% only | No — orphan |
| LINK/USDC | 0.30% only | No — orphan |

### Depth Analysis Action Items — COMPLETED

- [x] **Blacklist UNI/USDC 0.05%** — dead pool
- [x] **Blacklist UNI/USDC 0.30%** — marginal, rapid-fire risk
- [x] **Blacklist DAI/USDC 0.30%** — exhausted liquidity
- [x] **Blacklist LINK/USDC 0.05%** — thin at $140
- [x] **Blacklist WMATIC/USDC 0.30%** — thin at $100
- [x] **Blacklist WBTC/USDC 0.30%** — borderline, conservative pruning

### Usage

```bash
python3 scripts/verify_whitelist.py                   # Full verification + matrix
python3 scripts/verify_whitelist.py --update          # + update last_verified timestamps
python3 scripts/verify_whitelist.py --verbose         # Show raw hex data
```

---

## Paper Trader — OBSOLETE (2026-01-29)

The paper trading module is V2-only and provides zero value for the V3 live strategy.

### Problems

1. **V2-only detection** — `strategy.rs:91` calls `get_pools_for_pair()` (V2), never `get_v3_pools_for_pair()`. Live bot is V3-only.
2. **Own V2 syncer** — Runs independent `PoolSyncer::initial_sync()` every iteration (~70 RPC calls/10s). Doesn't read from shared JSON.
3. **No whitelist** — Scans all pools including dead/thin ones.
4. **No Quoter** — Can't distinguish real from phantom spreads.
5. **V2 pairs unreliable** — Same pairs where V2 price inversion bugs caused the $500 loss.

### If Paper Trading Is Needed Later

Rewrite to:
- Read from shared JSON (not own V2 syncer) — zero additional RPC
- Use V3 detector logic (`check_pair_unified`) instead of V2 `get_pools_for_pair`
- Apply whitelist filtering
- Test V3-relevant parameters (fee tier combos, spread thresholds, trade sizes per depth)

---

## Phase 1 Optimization — Remaining Tasks

### Phase 1.2: Enhanced Liquidity Thresholds — COVERED (2026-01-29)

Static per-pool/per-tier liquidity thresholds are implemented via whitelist v1.1:
- 0.05% tier: min 5B liquidity
- 0.30% tier: min 3B liquidity
- Per-pool overrides in `config/pools_whitelist.json`
- On-chain verification via `scripts/verify_whitelist.py`

Dynamic tick-range-aware analysis deferred — static thresholds sufficient for current pool set.

### Phase 1.3: Pool Quality Scoring (NOT STARTED)

- Real-time pool scoring based on:
  - Liquidity depth at current tick
  - Historical spread stability
  - Quoter pass/fail rate
  - Price movement frequency
- Dynamic opportunity ranking
- See `docs/phase_1_2_optimization_plan.md` Section 1.3

### Phase 2.1: Multicall3 Batch Quoter — COMPLETED (2026-01-29)

Batch-verifies all detected opportunities (buy+sell legs) in a single Multicall3 `aggregate3` RPC call before execution. Filters out opportunities where either leg cannot be filled, ranks survivors by quoted profit.

- **New module**: `src/arbitrage/multicall_quoter.rs` — MulticallQuoter struct, ABI encoding, QuoterV1 revert data decoding
- **Integration**: `main.rs` — batch verify inserted between `scan_opportunities()` and executor loop
- **Fallback**: If Multicall3 call fails, all opportunities pass through to executor (existing behavior)
- **Safety**: Executor's own per-leg Quoter checks NOT removed — batch pre-screen is additive
- **RPC savings**: From 2N sequential Quoter calls to 1 Multicall batch + 2 executor Quoter calls (for the one executed opportunity). N=5 typical: 10 → 3 calls.
- **Tests**: 7 unit tests (ABI encoding, QuoterV1 revert decoding, error handling) — all pass

### Phase 2.2: Adaptive Batch Sizing — DEFERRED

With max ~5 opportunity combinations per cycle (whitelist v1.1 trimming), fixed batching is sufficient. Adaptive sizing adds complexity for marginal gain.

---

*Last updated: 2026-01-29 (Phase 2.1 Multicall3 batch Quoter — built, tested, binary compiled)*
