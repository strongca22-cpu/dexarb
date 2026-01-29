# Next Steps - DEX Arbitrage Bot

## Current Status: LIVE — Scanning, No Trades Yet (Price Impact Too High)

**Date:** 2026-01-29
**Config:** Split — live bot reads `.env.live` (7 pairs), data collector reads `.env` (7 pairs)
**LIVE_MODE:** true (bot + watcher running in tmux)

**Wallet (unchanged — zero capital spent):**
- USDC.e: 520.02
- UNI: 0.0112 (dust from original incident)
- MATIC: 8.06 (gas)

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

## RPC Call Budget Analysis (2026-01-29)

| Metric | Value |
|--------|-------|
| Calls per scan cycle | ~29 (14 V3 pools × 2 + 1 block) |
| Current interval | 3s |
| RPC provider | Alchemy (free tier, 22.2M calls/month) |
| Monthly calls (estimated) | ~25.1M |
| Calls/sec | ~9.7 |

**Note:** Migrated from PublicNode WSS to Alchemy WSS on 2026-01-29. PublicNode dropped WebSocket connections under burst load during V3 sync. Alchemy is stable (~40ms/pool vs ~500ms on PublicNode). 7-pair load within Alchemy free tier (22.2M/month).

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
- [ ] **Add 0.01% fee tier** — USDT/USDC and DAI/USDC have active 0.01% pools. Code change: add `UniswapV3_001` to `DexType`, add `(100, ...)` to `V3_FEE_TIERS`. Unlocks 0.01%↔0.05% routes (0.06% round-trip fee).
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

*Last updated: 2026-01-29 (AAVE removed — phantom confirmed. Config separated: .env.live for live bot, .env for dev/paper. 7 active pairs.)*
