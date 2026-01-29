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

**Wallet after incident**: 520 USDC, 0.0112 UNI, 8.06 MATIC. LIVE_MODE set to false.

## Next Steps

1. **Fix `calculate_min_out` decimal conversion** — scale by `10^(out_decimals - in_decimals)`
2. **Add pool liquidity check** — reject trades where liquidity < trade_size
3. **Use V3 Quoter for pre-trade simulation** — `quoteExactInputSingle` before executing
4. **Parse actual amountOut from Swap event** — current code uses placeholder
5. Fix V2 price calculation for future V2↔V3 arbitrage
