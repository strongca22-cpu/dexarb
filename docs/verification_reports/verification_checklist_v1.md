# DEX Arbitrage Bot Verification Checklist
## Pre-Deployment Bug Detection & Validation

**Purpose**: Verify bot accuracy before deploying capital
**Timeline**: Complete all checks before any real trades

---

## 🔴 CRITICAL: Spread Calculation Verification

### **1. Confirm: Executable Spread vs Midmarket Spread**

```bash
# What your bot is calculating:
# Is it BEFORE or AFTER DEX fees?

CHECK:
[ ] View code: src/pool/spread_calculator.rs
[ ] Find the spread calculation formula
[ ] Verify: Does it include DEX fees or not?

CORRECT (Executable spread):
spread = (getAmountOut(sell_dex) - getAmountOut(buy_dex)) / amount
     # This INCLUDES 0.30% fees per DEX

WRONG (Midmarket spread):
spread = (price_sell_dex - price_buy_dex) / price
     # This is BEFORE fees

ACTION:
[ ] If calculating midmarket: STOP - all spreads are wrong
[ ] If calculating executable: ✅ Proceed
```

### **2. Verify DEX Fee Accounting**

```bash
CHECK:
[ ] Are DEX fees (0.30% × 2 = 0.60%) subtracted from spread?
[ ] Are fees hardcoded or configurable per DEX?
[ ] Do calculations match on-chain router math?

TEST:
# Run this query on actual pool:
cast call [UNISWAP_ROUTER] \
  "getAmountsOut(uint,address[])(uint[])" \
  1000000000000000000 \
  [TOKEN_IN,TOKEN_OUT]

# Compare with bot's calculation
# Should match EXACTLY

[ ] On-chain result: _________
[ ] Bot calculation: _________
[ ] Difference: _________ (should be 0)
```

---

## 🔴 CRITICAL: Liquidity & Slippage Validation

### **3. Check Pool Liquidity for Each Pair**

```bash
FOR EACH PAIR YOUR BOT MONITORS:

LINK/USDC on Apeswap:
[ ] Visit: https://polygonscan.com/address/[POOL_ADDRESS]
[ ] Record TVL: $_________ (need >$500K for $5K trades)
[ ] Record 24h volume: $_________ (need >$50K)
[ ] Last trade time: _________ (should be <5 min ago)
[ ] Verdict: ☐ Safe  ☐ Risky  ☐ Skip

UNI/USDC on Uniswap:
[ ] TVL: $_________
[ ] 24h volume: $_________
[ ] Last trade: _________
[ ] Verdict: ☐ Safe  ☐ Risky  ☐ Skip

WMATIC/USDC on Sushiswap:
[ ] TVL: $_________
[ ] 24h volume: $_________
[ ] Last trade: _________
[ ] Verdict: ☐ Safe  ☐ Risky  ☐ Skip

[Add more pairs as needed]

RULE:
✅ TVL >$500K + Volume >$50K = Safe for $5K trades
⚠️ TVL $100K-500K = Risky (high slippage expected)
❌ TVL <$100K = Skip entirely
```

### **4. Verify Slippage Simulation**

```bash
CHECK:
[ ] Does paper trading simulate slippage?
[ ] Is slippage calculated based on trade size / pool size?
[ ] Is slippage formula realistic?

CURRENT SLIPPAGE FORMULA:
# Look in: src/paper_trading/simulated_executor.rs

Expected formula:
slippage_pct = (trade_size / pool_tvl) * slippage_factor
where slippage_factor ≈ 10-20

VERIFY:
[ ] For $5K trade in $500K pool:
    Expected slippage: (5000/500000) * 15 ≈ 0.15%
    Bot calculates: _________% 
    [ ] Match ✅  [ ] Mismatch ❌

[ ] For $5K trade in $100K pool:
    Expected slippage: (5000/100000) * 15 ≈ 0.75%
    Bot calculates: _________% 
    [ ] Match ✅  [ ] Mismatch ❌

ACTION:
[ ] If no slippage simulation: ADD IT IMMEDIATELY
[ ] If slippage unrealistic: FIX FORMULA
```

---

## 🟡 IMPORTANT: Price Data Accuracy

### **5. Verify Pool State Freshness**

```bash
CHECK:
[ ] How often are pools synced? _________ seconds
[ ] Maximum acceptable staleness: _________ seconds
[ ] Is there a timestamp check before using pool data?

TEST:
# Check actual sync timing
tail -f data/spread_history.csv | awk -F',' '{print $1}'

# Verify updates happening every ~1 second
[ ] Updates every 1-2 seconds: ✅
[ ] Updates every 10+ seconds: ⚠️ Too slow
[ ] Updates sporadic: ❌ Fix sync loop

VERIFY:
[ ] Check: src/collector/pool_syncer.rs
[ ] Confirm: Sync interval = _________ ms (should be 1000ms)
[ ] Confirm: Error handling doesn't silently fail
```

### **6. Validate Price Oracle Sources**

```bash
FOR USD PRICE CONVERSIONS (TAX LOGGING):

CHECK:
[ ] Where do USD prices come from?
    ☐ CoinGecko API
    ☐ On-chain oracle (Chainlink)
    ☐ DEX pool price
    ☐ Other: _________

[ ] How often are prices updated? _________ seconds
[ ] What happens if price feed fails?
    ☐ Bot stops trading
    ☐ Uses cached price (how old?)
    ☐ Logs error but continues
    ☐ Unknown/not handled ❌

VERIFY:
[ ] Test: Disconnect price feed
[ ] Observe: Does bot gracefully handle it?
[ ] Result: ☐ Pass  ☐ Fail
```

---

## 🟡 IMPORTANT: On-Chain Execution Verification

### **7. Gas Estimation Accuracy**

```bash
CHECK:
[ ] What gas limit is used for swaps? _________ units
[ ] What gas price is used? _________ gwei
[ ] Is gas price dynamic or hardcoded?

TEST ON TESTNET (Mumbai):
# Execute a test swap
[ ] Estimated gas: _________ units
[ ] Actual gas used: _________ units
[ ] Difference: _________% (should be <10%)

CURRENT GAS COSTS:
[ ] Per swap on Polygon: ~_________ MATIC (estimate)
[ ] In USD: ~$_________ (at current MATIC price)
[ ] Expected: $0.30-0.70
[ ] If >$1.00: ⚠️ Something wrong
```

### **8. Transaction Simulation**

```bash
BEFORE MAINNET DEPLOYMENT:

[ ] Deploy to Polygon Mumbai testnet
[ ] Execute 10 test arbitrage swaps
[ ] Record results:
    Test 1: [ ] Success  [ ] Fail - Reason: _________
    Test 2: [ ] Success  [ ] Fail - Reason: _________
    Test 3: [ ] Success  [ ] Fail - Reason: _________
    ...
    Test 10: [ ] Success  [ ] Fail - Reason: _________

[ ] Success rate: _________% (need 90%+)
[ ] Average execution time: _________ ms (need <5000ms)
[ ] Any reverts? [ ] Yes ❌  [ ] No ✅
```

---

## 🟡 IMPORTANT: Data Collection Accuracy

### **9. Verify Pool Addresses Are Correct**

```bash
FOR EACH DEX + PAIR COMBINATION:

CHECK CONFIG: config/paper_trading.toml

Uniswap WMATIC/USDC:
[ ] Pool address: 0x_________
[ ] Verify on Polygonscan: ✅ Correct  ❌ Wrong
[ ] Verify token0/token1 order matches
[ ] Verify fee tier (if V3): _________

Sushiswap LINK/USDC:
[ ] Pool address: 0x_________
[ ] Verify on Polygonscan: ✅ Correct  ❌ Wrong

Apeswap LINK/USDC:
[ ] Pool address: 0x_________
[ ] Verify on Polygonscan: ✅ Correct  ❌ Wrong
[ ] ⚠️ Check if pool is active (recent trades)

[Add all pairs...]

COMMON BUG:
❌ Using wrong pool address
❌ Token0/Token1 reversed
❌ Using V2 pool when V3 exists (or vice versa)
```

### **10. Validate Token Addresses**

```bash
VERIFY ALL TOKEN ADDRESSES IN CONFIG:

[ ] WETH: 0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619
[ ] WMATIC: 0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270
[ ] USDC: 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174
[ ] WBTC: 0x1BFD67037B42Cf73acF2047067bd4F2C47D9BfD6
[ ] LINK: 0x53E0bca35eC356BD5ddDFebbD1Fc0fD03FaBad39
[ ] UNI: 0xb33EaAd8d922B1083446DC23f610c2567fB5180f
[ ] AAVE: 0xD6DF932A45C0f255f85145f286eA0b292B21C90B

VERIFY METHOD:
# For each token, check on Polygonscan:
https://polygonscan.com/address/[TOKEN_ADDRESS]

[ ] All addresses correct: ✅
[ ] Found wrong address: ❌ Fix immediately
```

---

## 🟢 RECOMMENDED: Strategy Validation

### **11. Verify Profit Thresholds Make Sense**

```bash
CHECK ALL STRATEGIES IN CONFIG:

FOR EACH STRATEGY:

Conservative Strategy:
[ ] Min profit: $_________ (expecting $15+)
[ ] Max trade size: $_________ (expecting $2K-5K)
[ ] Min executable spread: _________% 
    Expected: >0.80% (0.60% fees + 0.10% slippage + 0.10% profit)
    [ ] Above threshold: ✅
    [ ] Below threshold: ❌ Will trade unprofitably!

Moderate Strategy:
[ ] Min profit: $_________
[ ] Max trade size: $_________
[ ] Min spread: _________% 
[ ] Threshold check: ☐ Pass  ☐ Fail

Aggressive Strategy:
[ ] Min profit: $_________
[ ] Max trade size: $_________
[ ] Min spread: _________% 
[ ] Threshold check: ☐ Pass  ☐ Fail

COMMON BUG:
❌ Threshold <0.80% = will trade at a loss
❌ Min profit too high = misses real opportunities
```

### **12. Competition Rate Simulation**

```bash
CHECK:
[ ] What competition rate is assumed? _________%
[ ] Is competition rate realistic for current market?

SUGGESTED:
Conservative: 60-70% (lose to faster bots often)
Moderate: 50-60%
Aggressive: 40-50%

[ ] Current setting: _________%
[ ] Seems reasonable? ☐ Yes  ☐ No (adjust)

VERIFY IN CODE:
# Look in: src/paper_trading/simulated_executor.rs
# Find competition simulation logic
[ ] Competition is randomly simulated: ✅
[ ] Competition rate configurable: ✅
[ ] Competition disabled (always wins): ❌ Unrealistic!
```

---

## 🟢 RECOMMENDED: Integration Tests

### **13. End-to-End Data Flow Test**

```bash
MANUAL TEST:

1. Start data collector:
   [ ] Launches without errors
   [ ] Syncs all configured pools
   [ ] Logs data to spread_history.csv

2. Wait 5 minutes, then check:
   [ ] CSV has entries: wc -l data/spread_history.csv = _________
   [ ] Expected: ~300 entries (60 per pair, 5 pairs)
   [ ] Actual matches expected: ☐ Yes  ☐ No

3. Start paper trading:
   [ ] Launches without errors
   [ ] Reads spread_history.csv
   [ ] Detects opportunities (if any)

4. Check Discord webhook (if configured):
   [ ] Receives test message: ✅
   [ ] Receives opportunity alerts: ✅ (if any found)
   [ ] Formatting looks correct: ✅

5. Check metrics:
   [ ] Can query database for trade history: ✅
   [ ] Can generate reports: ✅
```

### **14. Error Handling Test**

```bash
STRESS TESTS:

Test 1: RPC Disconnect
[ ] Disconnect network
[ ] Observe: Bot behavior _________
[ ] Expected: Logs error, retries, doesn't crash
[ ] Result: ☐ Pass  ☐ Fail

Test 2: Invalid Pool Data
[ ] Manually corrupt a pool address in config
[ ] Restart bot
[ ] Expected: Catches error, logs warning, skips pool
[ ] Result: ☐ Pass  ☐ Fail

Test 3: Price Feed Failure
[ ] Simulate price oracle failure
[ ] Expected: Uses cached price or stops trading
[ ] Result: ☐ Pass  ☐ Fail

Test 4: Database Connection Loss
[ ] Stop database
[ ] Expected: Logs error, retries, doesn't crash
[ ] Result: ☐ Pass  ☐ Fail
```

---

## 🟢 RECOMMENDED: Financial Accuracy

### **15. Verify PnL Calculations**

```bash
MANUAL CALCULATION CHECK:

Pick one opportunity from Discord alert:
Example: LINK/USDC - 7.21% spread

Manual calculation:
Spread: 7.21%
DEX fees: -0.60%
Slippage (estimate): -_________% 
Gas: -$0.50
─────────────────────────────
Expected net: _________% 

Bot's calculation:
Net profit: $_________

[ ] Manual matches bot: ✅
[ ] Difference >5%: ❌ Investigate

COMMON BUGS:
❌ Forgetting to subtract fees
❌ Not accounting for slippage
❌ Gas cost not in USD
❌ Using wrong trade size for %
```

### **16. Tax Logging Verification (If Implemented)**

```bash
IF TAX LOGGING IS ENABLED:

[ ] Check database has tax_records table
[ ] Execute 1 test trade
[ ] Verify tax record created:
    [ ] Timestamp correct
    [ ] USD values populated
    [ ] Cost basis calculated
    [ ] Gain/loss correct
    [ ] Fees recorded

[ ] Export to CSV:
    [ ] RP2 format correct
    [ ] All fields present
    [ ] No NULL values where shouldn't be
```

---

## 🔵 OPTIONAL: Performance Checks

### **17. Latency Measurement**

```bash
MEASURE:

[ ] Pool sync latency: _________ ms (target: <100ms)
[ ] Spread calculation: _________ ms (target: <10ms)
[ ] Opportunity detection: _________ ms (target: <50ms)
[ ] Total loop time: _________ ms (target: <200ms)

[ ] Can process >100 pool updates/sec: ✅
[ ] Loop time <1 second: ✅ (need for real-time arb)
```

### **18. Memory & CPU Usage**

```bash
MONITOR FOR 1 HOUR:

[ ] Memory usage: _________ MB
[ ] Memory growth: _________ MB/hour
[ ] Memory leak detected: ☐ No ✅  ☐ Yes ❌

[ ] CPU usage: _________%
[ ] Acceptable (<50% on 1 core): ☐ Yes  ☐ No
```

---

## 📋 FINAL PRE-DEPLOYMENT CHECKLIST

### **Before Deploying ANY Capital**

```bash
CRITICAL ITEMS (MUST PASS ALL):

[ ] ✅ Spread calculation includes DEX fees
[ ] ✅ All pool addresses verified on Polygonscan
[ ] ✅ All token addresses correct
[ ] ✅ Profit thresholds >0.80% (accounts for fees)
[ ] ✅ Slippage simulation implemented
[ ] ✅ All target pools have >$500K TVL
[ ] ✅ Test swaps on Mumbai testnet successful
[ ] ✅ Gas estimation within 10% of actual
[ ] ✅ Error handling doesn't crash bot
[ ] ✅ Manual PnL calculation matches bot

RECOMMENDED ITEMS:

[ ] ✅ Discord alerts working
[ ] ✅ Tax logging functional (if needed)
[ ] ✅ Database backups configured
[ ] ✅ Metrics/reporting working
[ ] ✅ Competition simulation reasonable

SIGN-OFF:

Date: _____________
Deployer: _____________

I confirm all critical checks passed: _____________ (signature)

Approved capital deployment: $_________ (start with $50-100!)
```

---

## 🚨 Common Bugs Found in Production

### **Known Issues Checklist**

```bash
VERIFY THESE AREN'T PRESENT:

[ ] Bug: Using reserve ratios instead of getAmountOut()
    Fix: Use router.getAmountsOut() for accurate pricing

[ ] Bug: Forgetting to multiply by 10^decimals
    Fix: Handle token decimals correctly (USDC=6, WETH=18)

[ ] Bug: Token0/Token1 order reversed
    Fix: Check pair.token0() < pair.token1() (sorted by address)

[ ] Bug: Not handling pool sync failures
    Fix: Retry with exponential backoff, don't use stale data

[ ] Bug: Hardcoded gas prices
    Fix: Use dynamic gas price from network

[ ] Bug: Race conditions in multi-threaded code
    Fix: Proper Arc<RwLock> or DashMap usage

[ ] Bug: Not checking allowances before swap
    Fix: Approve tokens before first swap

[ ] Bug: Overflow on large numbers
    Fix: Use U256 or Decimal types, not f64

[ ] Bug: Silent failures (errors not logged)
    Fix: Comprehensive error logging
```

---

## 📊 Validation Log Template

```markdown
# Validation Session Log

Date: _______________
Bot Version: _______________
Network: [ ] Mainnet  [ ] Mumbai Testnet

## Critical Checks
- [ ] Spread calculation: PASS / FAIL
- [ ] DEX fees accounted: PASS / FAIL
- [ ] Pool liquidity verified: PASS / FAIL
- [ ] Slippage simulation: PASS / FAIL
- [ ] On-chain verification: PASS / FAIL

## Test Results
- Test swaps executed: _______
- Success rate: _______%
- Average execution time: _______ ms
- Gas cost actual vs estimated: _______%

## Issues Found
1. _______________________________________
   Status: [ ] Fixed  [ ] Investigating  [ ] Won't Fix

2. _______________________________________
   Status: [ ] Fixed  [ ] Investigating  [ ] Won't Fix

## Deployment Decision
- [ ] APPROVED - Deploy with $_________ capital
- [ ] REJECTED - Issues found: _________________
- [ ] DEFERRED - Need more testing: _________________

Signed: _______________
```

---

## 🎯 Quick Reference: "Go / No-Go" Criteria

### **🟢 SAFE TO DEPLOY (All true)**
```
✅ All pools >$500K TVL
✅ Spread calculation verified on-chain
✅ Test trades 90%+ success rate
✅ Profit thresholds >0.80%
✅ Slippage simulation realistic
✅ No crashes in 24h test run
✅ Starting with <$500 capital
```

### **🟡 DEPLOY WITH CAUTION**
```
⚠️ Some pools $100K-500K TVL (higher slippage)
⚠️ Test trades 70-90% success rate
⚠️ Starting with $500-1K capital
⚠️ Discord alerts delayed (>1 min)
```

### **🔴 DO NOT DEPLOY**
```
❌ Any pool <$100K TVL
❌ Spread calculation doesn't match on-chain
❌ Test trades <70% success rate  
❌ No slippage simulation
❌ Profit thresholds <0.80%
❌ Bot crashes on error
❌ Haven't verified on testnet
❌ Starting with >$5K capital (too risky!)
```

---

## 💡 Final Checklist Summary

**Minimum viable verification (1-2 hours)**:
1. ✅ Check spread calculation (executable vs midmarket)
2. ✅ Verify pool liquidity (all pools >$500K TVL)
3. ✅ Test one swap on Mumbai testnet
4. ✅ Verify all addresses correct
5. ✅ Deploy with $50-100 only

**Comprehensive verification (1-2 days)**:
- Complete all sections above
- 24 hour stress test
- Multiple testnet swaps
- Full error handling validation
- Deploy with confidence

**Use this checklist BEFORE every deployment, especially after code changes!**
