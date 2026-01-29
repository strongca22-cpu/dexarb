# Next Steps - DEX Arbitrage Bot

## Current Status: HALTED — $500 LOSS ON FIRST V3 TRADE

**Date:** 2026-01-29
**LIVE_MODE:** false (disabled after incident)

**Wallet (post-incident):**
- USDC.e: 520.02 (was ~1,020)
- UNI: 0.0112 (dust from failed arb)
- MATIC: 8.06 (gas)

---

## Incident Report (2026-01-29)

### What Happened

1. V3 `exactInputSingle` swap support added to `executor.rs` — compiled and deployed
2. Bot detected UNI/USDC opportunity: buy 1.00% tier @ 0.2056, sell 0.05% @ 0.2102
3. **Buy leg EXECUTED on-chain**: 500 USDC → 0.0112 UNI (worth $0.05)
4. Sell leg failed at gas estimation: "Too little received"
5. **Net loss: ~$500** due to two critical bugs

### Root Cause 1: `calculate_min_out` Decimal Mismatch

`executor.rs:553` computes `amount_in_raw * price` without converting token decimals:
- Input: 500,000,000 (500 USDC, 6 decimals)
- Price: 0.205579
- Computed min_out: 102,275,689
- In UNI's 18-decimal format: **0.0000000001 UNI** — effectively zero slippage protection
- Correct min_out: ~102 * 10^18 = 1.02 * 10^20

### Root Cause 2: No Pool Liquidity Check

The V3 1.00% UNI/USDC pool had almost no liquidity. The 500 USDC trade consumed everything, receiving 99.99% less UNI than expected. The detector checks price but never checks if pool liquidity can absorb the trade size.

### On-Chain Evidence

- Buy tx: `0x4dbb48aeac557cde8ca986d422d0d70515a29c74588429116aea833fe110acae`
- Approval tx: `0x781ae7e444067b2ab93ec010892ff7c699aff27e3e684b39742b80908702243b`
- Sell tx: never sent (reverted at `eth_estimateGas`)

---

## Critical Fixes Required Before Re-enabling LIVE_MODE

1. [ ] **Fix `calculate_min_out` decimal conversion** — must scale output by `10^(out_decimals - in_decimals)`
2. [ ] **Add pool liquidity check** — reject trades where pool liquidity < trade_size
3. [ ] **Use V3 Quoter for pre-trade simulation** — call `quoteExactInputSingle` before executing
4. [ ] **Parse actual `amountOut` from V3 Swap event** — current code uses `min_amount_out` as placeholder

### Previously Completed (V3 swap routing)
- [x] Add `ISwapRouter` ABI binding (`exactInputSingle`)
- [x] Add `is_v3()` check in `swap()` to branch V2 vs V3 logic
- [x] Build V3 `ExactInputSingleParams` struct
- [x] Token approvals routed to V3 SwapRouter

### Deferred
- [ ] Fix V2 price calculation (inverted reserve ratio)

---

## Commands

```bash
# Start live bot (LIVE_MODE=false for dry run)
tmux new-session -d -s live-bot
tmux send-keys -t live-bot "./target/release/dexarb-bot 2>&1 | tee data/bot_live.log" Enter

# Pre-deployment checklist
./scripts/checklist_full.sh
```

---

*Last updated: 2026-01-29*
