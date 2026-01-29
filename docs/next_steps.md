# Next Steps - DEX Arbitrage Bot

## Current Status: V3 DETECTION WORKING, EXECUTION BLOCKED

**Date:** 2026-01-29

**Wallet:**
- USDC.e: ~$1,020
- MATIC: 8.21 (gas)
- Approvals: Set

---

## Session Summary (2026-01-29 - Live Testing)

### What Happened

1. **Pre-deployment checklist passed** (140/140 checks via `scripts/checklist_full.sh`)
2. **Started live bot** in tmux with trade monitor
3. **Discovered V2 price inversion bug** - V2 `price()` returns inverted values vs V3 (e.g., 206B vs 0.21), creating phantom 100%+ spread opportunities
4. **Fixed: V3-only detection** - Removed V2 pools from `check_pair_unified()` in `detector.rs`
5. **Fixed: Gas price limit** - Increased `MAX_GAS_PRICE_GWEI` from 100 to 1000 (Polygon spikes to 500+ gwei but still cheap at ~$0.12/swap)
6. **Fixed: Profit threshold** - Lowered `MIN_PROFIT_USD` from 5.0 to 3.0 (real opportunities at $4.88)
7. **V3 detection now working correctly** - Real opportunities found (UNI/USDC 1.19% spread, $4.88 est profit)
8. **Trade execution failing** - "Contract call reverted with data: 0x" - TradeExecutor likely using V2 router for V3 pools

### Key Finding: V3 Prices Correct, V2 Inverted

After V3-only fix, prices are correct:
```
UNI/USDC: 0.05%=0.210194, 0.30%=0.208290, 1.00%=0.205579
```

### Blocking Issue: Trade Executor (Root Cause Confirmed)

The TradeExecutor has **no V3 swap implementation**:

- `get_router_address()` correctly resolves V3 DexTypes to the V3 SwapRouter
- But `execute_swap()` calls `swapExactTokensForTokens` (V2 function) on the V3 router
- The V3 router doesn't have this function → reverts with empty `0x` data
- **Fix needed in `executor.rs`**: Add `ISwapRouter` ABI with `exactInputSingle`, branch swap logic by `dex.is_v3()`

---

## Immediate Next Steps

1. [x] Add V3 support to live bot (detector, syncer, state manager)
2. [x] Add LIVE_MODE environment variable
3. [x] Build and test compilation
4. [x] Start live bot with V3 in tmux
5. [x] Monitor for V3↔V3 opportunities (working - UNI/USDC detected)
6. [ ] **FIX: Add V3 swap routing to TradeExecutor** (use V3 SwapRouter + exactInputSingle)
7. [ ] Validate execution on mainnet

---

## Commands

```bash
# Start live bot
tmux new-session -d -s live-bot
tmux send-keys -t live-bot "./target/release/dexarb-bot 2>&1 | tee data/bot_live.log" Enter

# Monitor for first trade
./scripts/monitor_trade.sh

# Pre-deployment checklist
./scripts/checklist_full.sh
```

---

## Backlog

### Critical — V3 Swap Execution (executor.rs)
- [ ] Add `ISwapRouter` ABI binding (`exactInputSingle`)
- [ ] Add `is_v3()` check in `execute_swap()` to branch V2 vs V3 logic
- [ ] Build V3 params: `(tokenIn, tokenOut, fee, recipient, deadline, amountIn, amountOutMinimum, sqrtPriceLimitX96)`
- [ ] Handle V3 return type (single `uint256`, not array)
- [ ] Approve tokens for V3 SwapRouter address (separate from V2 approvals)

### Deferred
- [ ] Fix V2 price calculation (inverted reserve ratio) — for future V2↔V3 arb

### Monitoring
- [x] Hourly Discord reports
- [x] Spread logger
- [ ] Real-time alerts for high-value opportunities
- [ ] Daily summary reports

### Infrastructure
- [ ] Multi-RPC failover
- [ ] Staggered V3 sync (reduce RPC calls)

---

*Last updated: 2026-01-29*
