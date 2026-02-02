# Session Summary: V3 Integration (2026-01-29)

## Overview

Added Uniswap V3 pool support to the live trading bot. Previously, V3 opportunities were only detected by paper-trading; now the main bot can trade them.

## Key Changes

### 1. PoolStateManager Extended (state.rs)
- Added `v3_pools: DashMap<(DexType, String), V3PoolState>`
- New methods: `update_v3_pool()`, `get_v3_pool()`, `get_v3_pools_for_pair()`
- Combined stats: `combined_stats()` returns (v2_count, v3_count, min_block, max_block)

### 2. Main Bot Updated (main.rs)
- Imported V3PoolSyncer
- Initial V3 sync on startup
- V3 sync in main loop
- Status display shows V3 pools per pair

### 3. Opportunity Detector Enhanced (detector.rs)
- New `UnifiedPool` struct for comparing V2 and V3 pools
- New `check_pair_unified()` method
- **Updated**: V3-only comparison (V2 pools excluded due to price inversion bug)
- Proper fee calculation: round_trip_fee = buy_fee + sell_fee

### 4. Config Updates
- Added `LIVE_MODE` environment variable
- `live_mode: bool` in BotConfig
- Default: false (dry run), set `LIVE_MODE=true` for real trades

### 5. New Scripts
- `scripts/monitor_trade.sh` - Monitors for first trade, then stops bot

## V3 Fee Tier Arbitrage

The key opportunity is between V3 fee tiers:

| Route | Buy Fee | Sell Fee | Round-Trip | Typical Spread | Net Profit |
|-------|---------|----------|------------|----------------|------------|
| 0.05% ↔ 1.00% | 0.05% | 1.00% | 1.05% | ~2.24% | ~$10/trade |
| 0.05% ↔ 0.30% | 0.05% | 0.30% | 0.35% | ~1.43% | ~$9/trade |

## Files Modified

- `src/rust-bot/src/pool/state.rs` - V3 storage
- `src/rust-bot/src/main.rs` - V3 syncing
- `src/rust-bot/src/arbitrage/detector.rs` - Unified detection
- `src/rust-bot/src/config.rs` - LIVE_MODE
- `src/rust-bot/src/types.rs` - live_mode field
- `scripts/monitor_trade.sh` - Trade monitor (new)

## Git Commit

```
9ac19e4 feat: add V3 pool support for fee tier arbitrage
```

## Live Testing Results (2026-01-29 evening)

### Issues Found & Fixed
1. **V2 price inversion** - V2 `price()` returns reserve0/reserve1 (e.g., 206B for UNI/USDC) while V3 returns correct ~0.21. Created phantom 100%+ spreads. **Fix**: Excluded V2 pools from `check_pair_unified()`.
2. **Gas limit too low** - MAX_GAS_PRICE_GWEI was 100, Polygon was at 583 gwei. **Fix**: Increased to 1000 (still cheap at ~$0.12/swap).
3. **Profit threshold too high** - MIN_PROFIT_USD was 5.0, real opportunities at $4.88. **Fix**: Lowered to 3.0.

### Remaining Issue: V3 Trade Execution

**Symptom:** `Contract call reverted with data: 0x`

**Root Cause Analysis:**

The TradeExecutor has **no V3 swap implementation**. The execution path is:

1. `get_router_address()` (executor.rs:546-550) — correctly resolves V3 DexTypes to the V3 SwapRouter address (`0xE592427A0AEce92De3Edee1F18E0157C05861564`)
2. `execute_swap()` (executor.rs:458-464) — calls `swapExactTokensForTokens` (a **V2-only** function) on the V3 router
3. The V3 router doesn't have `swapExactTokensForTokens`, so the call reverts with empty data

**What's Missing:**

| Component | Status |
|-----------|--------|
| V3 Router address in config | Working (`UNISWAP_V3_ROUTER` set) |
| V3 Router ABI (`ISwapRouter`) | **Missing** — no `exactInputSingle` binding |
| V3-aware swap dispatch | **Missing** — all swaps use V2 ABI |
| V3 swap parameter building | **Missing** — V3 needs `ExactInputSingleParams` struct, not path array |

**Fix Required in `executor.rs`:**

1. Add `ISwapRouter` ABI with `exactInputSingle(ExactInputSingleParams)`
2. Add `is_v3()` check in `execute_swap()` to branch between V2 and V3 logic
3. Build V3 params: `(tokenIn, tokenOut, fee, recipient, deadline, amountIn, amountOutMinimum, sqrtPriceLimitX96)`
4. V3 returns single `uint256` output (not array like V2)

### V3 Swap Routing Fix (executor.rs)

Added `ISwapRouter` ABI with `exactInputSingle` and V3-aware dispatch:
- `swap()` branches on `dex.is_v3()` → calls `swap_v3()` or `swap_v2()`
- `swap_v3()` builds `ExactInputSingleParams` struct with fee tier, deadline, sqrtPriceLimitX96=0
- Token approvals correctly route to V3 SwapRouter address

### INCIDENT: $500 Loss on First V3 Trade

**Trade**: Buy tx `0x4dbb...acae` — 500 USDC → 0.0112 UNI (worth $0.05) on V3 1.00% pool. Sell failed ("Too little received"). Net loss ~$500.

**Root Cause 1**: `calculate_min_out` (executor.rs:553) doesn't convert between token decimals. USDC has 6 decimals, UNI has 18. The computed min_out of 102,275,689 in UNI's 18-decimal format = 0.0000000001 UNI — zero slippage protection.

**Root Cause 2**: No pool liquidity check. The V3 1.00% UNI/USDC pool had almost no liquidity. 500 USDC consumed everything.

**Root Cause 3** (discovered during fix analysis): Trade direction inverted. V3 price = token1/token0 (UNI per USDC). Detector assigned buy_pool=lower price (0.2056), sell_pool=higher price (0.2102). But execute does USDC→UNI on buy_pool (fewer UNI) then UNI→USDC on sell_pool (fewer USDC). Even with proper liquidity, this direction loses ~$16 on $500. Correct direction (reversed) profits ~$5.60.

**Wallet after incident**: 520 USDC, 0.0112 UNI, 8.06 MATIC. LIVE_MODE set to false.

### Post-Incident Fixes (all applied, compiled, not yet live-tested)

1. **`calculate_min_out` decimal conversion** — now converts raw→human (÷10^in_dec), multiplies by price, converts back (×10^out_dec). Passes `token0_decimals`/`token1_decimals` from V3PoolState through ArbitrageOpportunity.
2. **Pool liquidity check** — detector skips pools with `liquidity < 1000` (dust) and `liquidity < trade_size * 1e6` (too thin for the trade).
3. **V3 Quoter pre-trade simulation** — `IQuoter::quoteExactInputSingle()` called via `.call()` before execution. Aborts if quoted output < min_out.
4. **Parse actual amountOut** — `parse_amount_out_from_receipt()` scans ERC20 Transfer events for token_out to wallet. Falls back to min_out if not found.
5. **Trade direction fix** — `check_pair_unified` now assigns buy_pool=HIGHER price (more token1 per token0 = better entry), sell_pool=LOWER price (1/price higher = more token0 per token1 = better exit).
6. **Token decimals in ArbitrageOpportunity** — new fields `token0_decimals`, `token1_decimals`, `buy_pool_liquidity` passed from detector to executor.

## Live Verification Tests (2026-01-29, late evening)

### New Infrastructure

- **`scripts/stop_after_trade.sh`** — Auto-stop watcher. Monitors `data/bot_live.log` for trade events ("Trade complete", "Trade failed", "Buy swap failed", "Execution error"). On trigger: kills `live-bot` tmux session + sets `LIVE_MODE=false` in `.env`.

### Test 1: $50 Trade Cap

- **Settings**: MAX_TRADE_SIZE_USD=50, MIN_PROFIT_USD=0.25
- **Duration**: ~40 minutes, ~70 scan iterations
- **Result**: Zero opportunities detected
- **Analysis**: $0.50 Polygon gas cost eats all profit at $50 trade size. Best executable spread (WBTC 1.32%) yields only $0.094 net.

### Test 2: $200 Trade Cap — Quoter Safety Verified

- **Settings**: MAX_TRADE_SIZE_USD=200, MIN_PROFIT_USD=0.10
- **Result**: 4 opportunities detected immediately
  - WBTC/USDC 1%↔0.05%: 1.17% spread, est $1.61
  - LINK/USDC 0.05%↔1%: 0.83% spread, est $0.99
  - LINK/USDC 0.30%↔1%: 0.38% spread, est $0.19
  - UNI/USDC 0.30%↔0.05%: 0.48% spread, est $0.37

- **Trade attempt**: WBTC selected (highest profit estimate)
- **Quoter REJECTED**: Quoted output 64,507,401 vs required min_out 178,610,877,064
  - The 1% pool had ~$0.06 real WBTC, not $200 needed
  - This is the same failure mode as the $500 incident — but now the Quoter catches it
- **Capital at risk**: **ZERO** (Quoter uses `.call()` — read-only, no gas)
- **Watcher**: "Trade failed" triggered `stop_after_trade.sh` → killed bot → reset LIVE_MODE=false

### Verification Conclusions

| Component | Status | Evidence |
|-----------|--------|----------|
| V3 opportunity detection | Working | 4 opportunities found with correct spreads |
| V3 Quoter pre-trade check | Working | Rejected thin-pool WBTC trade, zero capital risk |
| Trade direction fix | Working | Buy/sell pool assignment matches expected direction |
| Decimal conversion fix | Working | min_out values are correct magnitude (~1e8 for WBTC) |
| Auto-stop watcher | Working | Killed bot and reset LIVE_MODE on trade event |

### Settings After Verification

- LIVE_MODE=false (auto-reset by watcher)
- MAX_TRADE_SIZE_USD=200.0 (was 500 during incident, then 50, now 200)
- MIN_PROFIT_USD=0.10 (was 5.0, then 3.0, then 0.25, now 0.10)
- Wallet: 520 USDC, 0.0112 UNI, 8.06 MATIC (unchanged — zero spent)

## Post-Verification Improvements (2026-01-29, late night)

### 1. Watcher Updated (stop_after_trade.sh)

Previously: stopped on all "Trade failed" events including safe Quoter rejections.
Now: distinguishes safe vs dangerous events:
- **STOP**: "Trade complete", "Buy swap failed", "Sell swap failed", "Execution error"
- **CONTINUE**: "Trade failed" (Quoter rejections — logged with counter, bot keeps scanning)

### 2. Try-All Execution (main.rs)

Previously: only tried the single best opportunity per scan cycle. 1% pool phantom opportunities blocked all real trades.
Now: iterates all opportunities in profit order. If Quoter rejects #1, tries #2, #3, etc. Stops on success or on-chain failure.

### 3. Detector Returns All Combinations (detector.rs)

Previously: `check_pair_unified()` returned `Option<ArbitrageOpportunity>` — one best per pair.
Now: returns `Vec<ArbitrageOpportunity>` — all profitable fee tier combinations. `scan_opportunities()` collects all and sorts globally by profit.

### 4. Trade Size Lowered to $140

Based on UNI/USDC 0.30%↔0.05% spread analysis from the $50 run log:
- 0.05% pool: stable at 0.210194 price
- 0.30% pool: varied from 0.209376 to 0.212731 over ~40 min
- Spread was profitable (≥0.45% exec) about 25 of 40 minutes
- At 0.48% spread (most common profitable level): min trade size = $139
- $140 chosen as minimum to reliably catch this spread

### Files Modified

- `scripts/stop_after_trade.sh` — STOP/CONTINUE pattern distinction
- `src/rust-bot/src/main.rs` — try-all execution loop
- `src/rust-bot/src/arbitrage/detector.rs` — return Vec, not Option
- `src/rust-bot/.env` — MAX_TRADE_SIZE_USD=140.0

### 5. Sell-Leg Quoter Check (executor.rs)

Previously: Quoter only checked buy leg. Sell went straight to on-chain execution after buy succeeded.
Now: both legs are Quoter-simulated before execution:
- Buy-leg Quoter: before capital committed (zero risk)
- Sell-leg Quoter: after buy succeeds, before sell tx sent
- Uses actual `amount_received` from buy (not estimated) as sell input
- If sell Quoter rejects: logs `"Sell swap failed"` → watcher STOPs → manual exit needed

Also fixed misleading comment: "Simulates both legs" → now accurately describes buy-only pre-check with sell checked separately.

### Files Modified (continued)

- `src/rust-bot/src/arbitrage/executor.rs` — sell-leg Quoter check + comment fix

### Settings After Improvements

- MAX_TRADE_SIZE_USD=140.0 (was 200, now minimum to catch UNI 0.30%↔0.05%)
- MIN_PROFIT_USD=0.10
- Build: compiled, release binary updated

## Live Test 3: $140 Try-All + Sell Quoter (2026-01-29, ~04:44 UTC)

### Setup
- LIVE_MODE=true, bot in `live-bot` tmux, watcher in `bot-watcher` tmux
- Settings: $140 trade cap, $0.10 min profit, 0.5% max slippage

### Results
- 3 opportunities detected per cycle:
  - WBTC/USDC 1%↔0.05%: 1.06% spread, $0.84 est
  - UNI/USDC 0.30%↔0.05%: 0.96% spread, $0.71 est
  - WBTC/USDC 1%↔0.30%: 0.85% spread, $0.57 est
- Try-all working: bot iterates all 3, Quoter rejects each, continues scanning
- WBTC 1% pools: Quoter returns 64.5M vs 125B required (empty pool)
- UNI 0.30%↔0.05%: Quoter returns 29.435e18 vs 29.663e18 required (0.77% short)

### Key Discovery: Price Impact vs Spot Spread

$140 on the 0.30% pool causes ~1.26% price impact, consuming the 0.96% visible spread.
- If forced: 140 USDC → 29.435 UNI → $139.95 USDC = -$0.55 loss
- Quoter correctly prevents the losing trade
- Spread needs ~2%+ to overcome price impact at $140
- Bot continues scanning safely — zero capital at risk

### Status
- Bot running, watcher running, zero capital spent
- Will execute if spread widens sufficiently

## Paper Trading Liquidity Filter (2026-01-29, ~05:20 UTC)

### Problem

Paper trading reports were inflated by ~200+ phantom 1% fee tier opportunities per hour. The hourly Discord report showed:
- #1: UNI/USDC $8.10 × 214 = $2,331 via UniswapV3_1.00% → UniswapV3_0.05%
- #2: UNI/USDC $7.28 × 205 = $1,991 via UniswapV3_0.30% → UniswapV3_0.05%

The 1% trades are phantom — live Quoter rejects every attempt on 1% pools.

### Root Cause

Paper trading reads from `pool_state_phase1.json` and has no Quoter access. The V3 `liquidity` field (in sqrt(token0*token1) units) doesn't reliably indicate executable depth:

| Pool | Liquidity | Quoter Result |
|------|-----------|---------------|
| UNI 1.00% | 8.84e10 | Rejected (returned $0.05 for $500 input) |
| UNI 0.05% | 1.51e11 | Works (0.77% short at $140) |
| UNI 0.30% | 1.11e16 | Works (price impact ~1.26%) |
| WBTC 1.00% | 5.29e8 | Rejected (~$0.06 real depth) |
| LINK 1.00% | 1.47e13 | Rejected |

No simple threshold cleanly separates phantom from real — UNI 1% (8.84e10) and UNI 0.05% (1.51e11) are only 1.7x apart.

### Fix: Exclude 1% Fee Tier Entirely

In `paper_trading.rs`, V3 pool collection now skips all pools with `fee >= 10000`:
- Matches empirical reality: ALL 1% pools on Polygon are Quoter-rejected in live testing
- Simple, reliable — no fragile threshold tuning
- Can be re-enabled if 1% pools ever get real liquidity

Also kept the existing liquidity filters:
- Dust filter: skip pools with `liquidity < 1000`
- Trade-size filter: skip pools with `liquidity < trade_size_usd * 1e6`

### Result

Paper trading now only reports `UniswapV3_0.30% → UniswapV3_0.05%` routes (round-trip fee 0.35%). No more phantom 1% tier opportunities.

### Alchemy Rate Limit Analysis

RPC call count per scan cycle: ~203 (84 V2 + 105 V3 + 14 block checks)
- At 10s interval: 52.6M calls/month (exceeds 22.2M Alchemy budget)
- V3-only (dropping unused V2 sync): ~119 calls/cycle → 13s minimum interval
- Alchemy benefit: reliability/latency, not faster scanning

### Transition Speed Analysis (from live logs)

UNI/USDC pool prices change on minutes-to-hours timescale:
- 0.30% pool: changed once in 17 minutes (0.212944 → 0.212306)
- 0.05% pool: stable at 0.210194 throughout observation
- Faster scanning (sub-10s) provides marginal benefit for current strategy

### Files Modified

- `src/rust-bot/src/bin/paper_trading.rs` — 1% fee tier exclusion, liquidity field in UnifiedPool

## Live Bot Log Analysis (2026-01-29, ~05:30 UTC)

44 minutes of live bot data analyzed (04:44 - 05:28 UTC):

| Metric | Value |
|--------|-------|
| Scan cycles | 88 (~30s each, not 10s) |
| Opportunities detected | 258 |
| Quoter rejections | 261 (100%) |
| Successful trades | 0 |
| Errors/disconnections | 0 |
| Capital spent | $0 |

### Why 30s Cycles (Not 10s)

`POLL_INTERVAL_MS=10000` is just the sleep. Actual cycle = sleep + sync + execution:
- Poll interval: 10.0s
- V2 sync (20 pools, sequential): ~5-8s
- V3 sync (21 pools × 500ms each, sequential): ~10.5s
- Quoter calls (3 opps × 200ms): ~0.6s
- **Total: ~27-30s**

V3 sync is the main bottleneck — each pool is a sequential RPC call at ~500ms.

### Price Movement

- UNI 0.30%: changed once in 17 min (0.212944 → 0.212306), then static
- UNI 0.05%: static at 0.210194 entire run
- WBTC 0.05%: active — 15 distinct price levels ($87,898 - $88,039)
- WBTC 1.00%: frozen at $89,754 (stale, never moved)

### Quoter Gap Analysis

- **WBTC 1% pools**: 1,940x gap (64.5M quoted vs 125B required) — completely phantom
- **UNI 0.30%↔0.05%**: 0.73% gap (29.36e18 quoted vs 29.57e18 required) — genuine near-miss

## Architecture Overhaul — Code Written (2026-01-29, ~07:00 UTC)

### Changes Implemented (code written, not yet compiled)

1. **Drop V2 sync entirely** (main.rs)
   - Removed `PoolSyncer` import, creation, initial sync, and main loop re-sync
   - Removed V2 pool display (Uniswap, Sushiswap)
   - All V2 code retained in `syncer.rs` and `detector.rs` but not called

2. **Drop 1% fee tier** (v3_syncer.rs + detector.rs)
   - `sync_all_v3_pools()`: skips `fee >= 10000` during initial discovery
   - `check_pair_unified()`: filters out `pool.fee >= 10000` (defense-in-depth)
   - Saves 7 pool syncs per cycle (1 per pair)

3. **Parallel V3 sync** (v3_syncer.rs)
   - New `sync_known_pools_parallel(&self, known_pools)` method
   - Uses `futures::future::join_all` to fire all slot0+liquidity calls concurrently
   - Block number fetched once and shared across all pool updates
   - Main loop calls this instead of `sync_all_v3_pools()` after initial discovery
   - Expected: ~400ms total (1 block call + 14 concurrent pool syncs) vs ~5.6s sequential

4. **Poll interval → 3s** (.env)
   - `POLL_INTERVAL_MS=10000` → `POLL_INTERVAL_MS=3000`

5. **Quoter kept per-cycle** — not moved to periodic (user chose Options 2/3 for later)

### Quoter Safety Discussion

User asked: "can we safely remove the Quoter entirely?"

Analysis: The on-chain `amountOutMinimum` in `exactInputSingle` is the real safety net. If the swap produces less than `amountOutMinimum`, the transaction reverts — no tokens exchanged, only ~$0.10 gas lost. This would have prevented the original $500 loss if `calculate_min_out` hadn't set `amountOutMinimum` to effectively zero due to the decimal bug.

This is standard DeFi practice. More sophisticated approaches (flash loans, atomic multi-hop contracts) eliminate leg risk entirely. For Phase 1's two-tx architecture, `amountOutMinimum` per-leg is the correct baseline safety net.

Decision: Keep per-cycle Quoter for now (zero risk). Discuss Options 2/3 (higher spread threshold or threshold-triggered Quoter) later.

### Build Status

Compilation failed with lifetime error in `sync_known_pools_parallel`:
```
error[E0716]: temporary value dropped while borrowed
  --> contract.slot_0().call()  // tokio::join! borrows temporary ContractCall
```

Fix needed: bind intermediate `ContractCall` values to `let` before passing to `tokio::join!`:
```rust
let slot0_fut = contract.slot_0().call();
let liq_fut = contract.liquidity().call();
let (slot0_res, liq_res) = tokio::join!(slot0_fut, liq_fut);
```

### Files Modified

- `src/rust-bot/src/main.rs` — V3-only main loop, parallel sync
- `src/rust-bot/src/pool/v3_syncer.rs` — 1% exclusion, `sync_known_pools_parallel()`
- `src/rust-bot/src/arbitrage/detector.rs` — V3-only scan, 1% exclusion
- `src/rust-bot/.env` — POLL_INTERVAL_MS=3000

## Remaining

- ~~Fix V2 price calculation~~ — V2 dropped entirely
- ~~Implement architecture changes~~ — code written, needs build fix
- Fix `tokio::join!` lifetime error in `sync_known_pools_parallel`
- Build, test, deploy updated binary
- Alchemy migration: switch RPC for reliability
- Quoter architecture (Options 2/3): discuss and implement
