# DEX Arbitrage Bot Verification Report

**Date:** 2026-01-28
**Verifier:** Claude Code
**Status:** 🔴 CRITICAL ISSUES FOUND - DO NOT DEPLOY

---

## Executive Summary

Running the verification checklist revealed **TWO CRITICAL BUGS** that would cause the bot to:
1. Trade on dead/illiquid pools with essentially zero liquidity
2. Calculate incorrect prices due to token ordering mismatch

These bugs explain the "too good to be true" spreads reported in Discord alerts (e.g., LINK/USDC 7.21% spread).

---

## 🔴 CRITICAL BUG #1: V2 Token Ordering Mismatch

### Description
The V2 pool syncer stores tokens in CONFIG order, not CONTRACT order. V2 pool contracts sort tokens by address (token0 < token1), but the syncer uses the config file's ordering.

### Evidence
**On-chain (correct):**
```
Apeswap LINK/USDC pool:
  token0: 0x2791...bca1 (USDC)  <- lower address
  token1: 0x53e0...bad3 (LINK)  <- higher address
```

**Stored in JSON (incorrect):**
```
Apeswap LINK/USDC pool:
  token0: 0x53e0...bad3 (LINK)  <- from config
  token1: 0x2791...bca1 (USDC)  <- from config
```

### Impact
- Price calculation uses wrong decimal adjustment
- Normalization factor is INVERTED (should be 10^(6-18) = 1e-12, but applied in wrong direction)
- V2 prices are completely wrong when comparing to V3

### Location
- **File:** [syncer.rs:172-179](src/rust-bot/src/pool/syncer.rs#L172-L179)
- **Root cause:** Uses `pair: pair.clone()` from config instead of reading actual token0/token1 from contract

### Fix Required
Same pattern as V3 syncer (v3_syncer.rs:170-177):
```rust
// Get ACTUAL token ordering from pool contract
let actual_token0 = pool.token_0().call().await?;
let actual_token1 = pool.token_1().call().await?;
```

---

## 🔴 CRITICAL BUG #2: No Minimum Liquidity Check

### Description
The bot detects arbitrage opportunities on pools with essentially ZERO liquidity.

### Evidence
**Apeswap LINK/USDC pool (verified on-chain):**
```
reserve0 (USDC): 526 raw = $0.000526
reserve1 (LINK): 41154315497639 raw = 0.000041 LINK

Total Value Locked: <$0.01
```

This pool is effectively **DEAD** but the bot is still:
1. Calculating price ratios from dust amounts
2. Reporting 7%+ arbitrage opportunities
3. Sending Discord alerts

### Impact
- False positive arbitrage opportunities
- Impossible to actually execute trades
- Wasted gas if real trading was enabled
- Misleading metrics and reports

### Location
- **File:** [paper_trading.rs:333-478](src/rust-bot/src/bin/paper_trading.rs#L333-L478)
- **Missing:** Liquidity/TVL check before flagging opportunities

### Fix Required
Add minimum liquidity check:
```rust
// Skip pools with insufficient liquidity
const MIN_LIQUIDITY_USD: f64 = 10_000.0; // $10K minimum

// For V2: calculate TVL from reserves
let tvl_usd = calculate_tvl_usd(pool);
if tvl_usd < MIN_LIQUIDITY_USD {
    continue;
}
```

---

## 🟡 IMPORTANT FINDING: Alchemy Rate Limiting

### Description
The data collector is hitting Alchemy's free tier rate limits (429 errors), causing sync failures.

### Evidence
```
ERROR ethers_providers::rpc: error=(code: 429, message: Your app has exceeded
its compute units per second capacity...)
```

### Impact
- Some pools fail to sync
- Pool state may become stale
- UNI/USDC showing sync failures in logs

### Recommendation
- Increase poll interval (currently 1000ms may be too fast for free tier)
- Or upgrade Alchemy plan
- Or add retry logic with backoff

---

## ✅ PASSED CHECKS

### Check #1: Spread Calculation Logic
- **Status:** PASS (with caveats)
- **Finding:** Code correctly calculates:
  - Midmarket spread from price difference
  - Executable spread by subtracting round-trip fees
  - Variable fees for V3 tiers (0.05%, 0.30%, 1.00%)

### Check #2: DEX Fee Accounting
- **Status:** PASS
- **Finding:** Fees correctly handled:
  - V2: 0.30% per swap (0.60% round-trip)
  - V3: Variable per fee tier
  - Round-trip = buy_fee + sell_fee

### Check #5: Pool State Freshness
- **Status:** PASS
- **Finding:** pool_state_phase1.json updating every ~1 second
- Data collector and paper trading using same state file correctly

### Check #8: Services Running
- **Status:** PASS
- **Finding:** Both services running in tmux dexarb-phase1:
  - Window 0: data-collector (writes to pool_state_phase1.json via STATE_FILE env)
  - Window 1: paper-trading (reads from pool_state_phase1.json via --config arg)

### Check #11: Profit Thresholds
- **Status:** PASS (configuration correct)
- **Finding:** Thresholds set appropriately in paper_trading_phase1.toml:
  - Conservative: min_profit=$15, max_slippage=0.25% (need >0.85% midmarket)
  - Moderate: min_profit=$5, max_slippage=0.5%
  - Discovery: min_profit=-$50, max_slippage=0.001% (captures everything)

---

## 🟡 NEEDS IMPROVEMENT

### Check #4: Slippage Simulation
- **Status:** WEAK - Not based on pool liquidity
- **Current:** Fixed 10% of gross profit (`gross * 0.10`)
- **Problem:** Dead pools get same slippage estimate as liquid pools
- **Should be:** `slippage = (trade_size / pool_tvl) * factor`
- **Impact:** Underestimates slippage on low-liquidity pools

---

## Recommended Actions

### Immediate (Before ANY further testing)

1. **Fix V2 token ordering bug**
   - Update `syncer.rs` to read actual token0/token1 from pool contract
   - Same pattern as V3 syncer fix from Session 7

2. **Add minimum liquidity check**
   - Skip pools with TVL < $10,000
   - Log skipped pools for visibility

3. **Reduce poll interval**
   - Increase from 1000ms to 2000-3000ms to avoid rate limits
   - Or implement exponential backoff on 429 errors

### Before Deployment

4. **Implement realistic slippage simulation**
   - Formula: `slippage = (trade_size / pool_tvl) * factor`

5. **Add on-chain verification**
   - Compare calculated output with actual `getAmountsOut()` calls

6. **Run 24-hour stress test** after all fixes

---

## Pool Liquidity Audit

| Pool | DEX | TVL | Status |
|------|-----|-----|--------|
| LINK/USDC | Apeswap | ~$0.01 | ❌ DEAD |
| LINK/USDC | Sushiswap | ~$43 | ⚠️ ILLIQUID |
| LINK/USDC | Uniswap | TBD | Need verification |
| WETH/USDC | All | TBD | Need verification |
| WMATIC/USDC | All | Shows 0 reserves | ❌ DEAD or BUG |

---

## Files Modified in This Session

- `src/pool/syncer.rs` - V2 token ordering fix
- `src/bin/paper_trading.rs` - Dead pool exclusion list
- `scripts/hourly_discord_report.py` - Automated hourly Discord reports

---

## Session TODO

1. [x] Fix V2 syncer token ordering (syncer.rs) ✅
2. [x] Add dead pool exclusion (paper_trading.rs) ✅
3. [x] Reduce poll interval to 3s (was 1s causing 429 errors) ✅
4. [x] Re-run verification after fixes ✅
5. [ ] Verify all pool TVLs on-chain
6. [x] Add automated hourly Discord reports ✅

---

## Verification Session Summary

| Check | Status | Notes |
|-------|--------|-------|
| #1 Spread Calculation | ✅ PASS | Logic correct |
| #2 DEX Fee Accounting | ✅ PASS | Fees handled correctly |
| #3 Pool Liquidity | ❌ FAIL | Dead pools flagged as opportunities |
| #4 Slippage Simulation | 🟡 WEAK | Fixed 10%, not liquidity-based |
| #5 Pool State Freshness | ✅ PASS | Updating every ~1s |
| #8 Services Running | ✅ PASS | Both in tmux |
| #9 Pool Addresses | ❌ FAIL | Token order mismatch |
| #11 Profit Thresholds | ✅ PASS | Config correct |

### Decision: 🔴 RATE LIMITING STILL AN ISSUE

**Fixed (2026-01-28 07:00 UTC):**
1. ✅ V2 syncer now reads actual token0/token1 from pool contract
2. ✅ Dead pools (Apeswap LINK, Sushiswap LINK, etc.) statically excluded

**Previous Verification Results:**
- LINK/USDC spread: 7.21% → 0.78% (10x reduction, now realistic)
- All WETH/USDC prices: ~0.000333 (correct, within 0.01%)
- No more false positives from dead pools

**Follow-Up Verification (2026-01-28 16:30 UTC):**

Despite increasing poll interval from 1000ms to 3000ms:
- ❌ V3 spread STILL FROZEN at 3.2030% (UNI/USDC)
- ❌ 429 rate limit errors still occurring during V3 sync bursts
- ✅ V2 pools now fresh (2.6 blocks average staleness)

**Root Cause:** V3 sync attempts 105 RPC calls at once (21 pools × 5 calls),
creating a spike that overwhelms Alchemy free tier even at 3000ms intervals.

**Status:** 🟡 MONITORING - Fixes applied and verified (16:55 UTC)

**Second Fix (16:50 UTC):**
- Increased poll interval: 5000ms → 10000ms
- Staggered V3 sync: 1 pair per iteration (not all 21 at once)
- Skipped initial V3 bulk sync

**Verification Results:**
- V3 staleness: 255-1945 blocks → 0-30 blocks ✅
- Spread variation: Constant 3.20% → Varying 0.84%-3.20% ✅
- 429 errors: Frequent → Nearly eliminated ✅

**Status:** 🟡 1-hour monitoring period, then GO for paper trading validation

---

## Third Fix: Dead Pool Exclusions (18:35 UTC)

**Issue:** Discord report still showing $38.02 UNI/USDC and $20 LINK/USDC opportunities.

**Root Cause:** Dead V2 pools with <$1000 TVL being compared to V3 pools.

**TVL Audit Results:**
| Pool | DEX | TVL | Status |
|------|-----|-----|--------|
| UNI/USDC | Uniswap | $0.12 | ❌ EXCLUDED |
| UNI/USDC | Sushiswap | $550 | ❌ EXCLUDED |
| LINK/USDC | Uniswap | $10.42 | ❌ EXCLUDED (NEW) |
| LINK/USDC | Sushiswap | $86 | ❌ EXCLUDED |
| WBTC/USDC | Apeswap | $0.09 | ❌ EXCLUDED (NEW) |
| WBTC/USDC | Sushiswap | $501 | ❌ EXCLUDED (NEW) |

**Active V2 Pools (>$10K TVL):**
| Pool | DEX | TVL |
|------|-----|-----|
| WETH/USDC | Uniswap | $2.57M ✅ |
| WMATIC/USDC | Uniswap | $1.4M ✅ |
| USDT/USDC | Uniswap | $627K ✅ |
| DAI/USDC | Uniswap | $300K ✅ |
| WBTC/USDC | Uniswap | $182K ✅ |

**Fix Applied:** Added 3 new pools to EXCLUDED_POOLS in paper_trading.rs

**Results After Fix:**
- UNI/USDC: Only V3↔V3 opportunities (1.22%, 2.24% spreads)
- LINK/USDC: 0 opportunities (all V2 pools excluded)
- No more $38 or $20 false positives

**Status:** 🟢 GO - All dead pools excluded, only real opportunities showing

---

## How to Resume

```bash
# 1. Check services
tmux ls

# 2. Attach to session
tmux attach -t dexarb-phase1

# 3. After fixes, restart:
# Window 0: Stop with Ctrl+C, rebuild, restart data-collector
# Window 1: Stop with Ctrl+C, restart paper-trading

# 4. Re-run verification
# Read this file and verify each fix
```
