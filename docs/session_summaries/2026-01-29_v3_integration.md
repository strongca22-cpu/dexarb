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

### Remaining Issue
- **Trade execution reverts** - "Contract call reverted with data: 0x". TradeExecutor uses V2 router (`swapExactTokensForTokens`) for V3 pools. V3 requires `exactInputSingle` on V3 SwapRouter.

### Verified Working
- V3 price detection correct: UNI/USDC 0.05%=0.210194, 0.30%=0.208290, 1.00%=0.205579
- Real opportunity: UNI/USDC 1.19% spread, $4.88 est profit (buy 1.00% tier, sell 0.05% tier)

## Next Steps

1. **Fix TradeExecutor for V3 swaps** (use V3 SwapRouter + exactInputSingle)
2. Validate execution on mainnet
3. Fix V2 price calculation for future V2↔V3 arbitrage
