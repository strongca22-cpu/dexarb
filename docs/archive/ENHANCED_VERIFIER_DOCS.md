# Enhanced Whitelist Verifier - Documentation

## What's New

The enhanced verifier adds **automated pool categorization** and **comprehensive liquidity analysis** to help you maintain optimal pool whitelists.

### Key Enhancements

1. **Three-Tier Categorization**
   - **WHITELIST**: Handles large trades ($1k-$5k) with <5% price impact
   - **MARGINAL** (NEW): Good for small trades ($1-$100) but fails at larger sizes
   - **BLACKLIST**: Unusable or extreme price impact

2. **Liquidity Scoring (0-100)**
   - Quantifies pool quality based on depth at multiple trade sizes
   - Accounts for both max working size and price impact
   - 100 = Perfect depth, 0 = Completely broken

3. **Multi-Size Trade Analysis**
   - Tests at: $1, $10, $100, $1000, $5000
   - Shows exact price impact % at each size
   - Color-coded matrix (Green <5%, Yellow 5-10%, Red >10%)

4. **Automated Recommendations**
   - Suggests which pools to promote/demote
   - Explains reasoning for each suggestion
   - Identifies mismatches between current config and analysis

---

## Usage

### Basic Run (Matrix Only)
```bash
python3 scripts/verify_whitelist_enhanced.py
```
**Output:** Quote depth matrix showing performance at each trade size

### Full Analysis with Recommendations
```bash
python3 scripts/verify_whitelist_enhanced.py --categorize
```
**Output:** 
- Matrix
- Categorization summary (whitelist/marginal/blacklist)
- Specific recommendations for config changes

### Verbose Mode
```bash
python3 scripts/verify_whitelist_enhanced.py --categorize --verbose
```
**Output:** Detailed RPC call results, pool state info

### Custom RPC
```bash
python3 scripts/verify_whitelist_enhanced.py --rpc https://your-rpc-url.com
```

---

## Output Interpretation

### Quote Depth Matrix

```
Pool           Pair          Fee     Max$   $1        $10       $100      $1000     $5000     Impact    Score  Category    
--------------  -----------  ------  -----  --------  --------  --------  --------  --------  --------  -----  -----------
0x8ad5..97Fb   WETH/USDC    0.05%   $5000  OK        0.2%      0.5%      1.2%      2.8%      2.8%      95     WHITELIST   
0xA374..d85a   WMATIC/USDC  0.05%   $100   OK        0.1%      3.2%      FAIL      FAIL      3.2%      35     MARGINAL    
0x0c53..547A   LINK/USDC    0.30%   $1     OK        FAIL      FAIL      FAIL      FAIL      --        5      BLACKLIST   
```

**Column Meanings:**
- **Max$**: Largest trade size that succeeded
- **$1, $10, etc.**: Price impact % at each size (or FAIL if quote reverted)
- **Impact**: Price impact at the largest successful size
- **Score**: Liquidity score 0-100
- **Category**: Automated suggestion

**Color Coding:**
- 🟢 **Green**: <5% impact (excellent)
- 🟡 **Yellow**: 5-10% impact (acceptable for marginal)
- 🔴 **Red**: >10% impact or FAIL (problematic)

---

## Categorization Logic

### WHITELIST Criteria
- ✅ Works at $1000+ with <5% impact
- ✅ Liquidity score ≥ 60
- ✅ Pool initialized and has sufficient base liquidity

**Example:** WETH/USDC 0.05% - handles $5k with 2.8% impact

### MARGINAL Criteria
- ✅ Works at $10-$100 with <10% impact
- ❌ Fails or has high impact at $1000+
- ✅ Liquidity score 20-59

**Use case:** Useful for small arbitrage opportunities but should be limited to max trade size of $100

**Example:** WMATIC/USDC 0.30% - works great up to $100 but reverts at $1000

### BLACKLIST Criteria
- ❌ Fails at $100 or below
- ❌ >20% impact even at small sizes
- ❌ Liquidity score < 20
- ❌ Pool doesn't exist or not initialized

**Example:** Thin pools that only handle $1 quotes

---

## Threshold Configuration

Thresholds are defined at the top of the script:

```python
THRESHOLDS = {
    "marginal_max_size": 100,      # Marginal pools work up to $100
    "whitelist_min_size": 1000,    # Whitelist pools must handle $1000+
    "impact_whitelist": 5.0,       # Max impact % for whitelist
    "impact_marginal": 10.0,       # Max impact % for marginal
    "impact_blacklist": 20.0,      # Blacklist threshold
    "min_liquidity_base": 1_000_000_000,  # Minimum raw liquidity
}
```

**Tuning recommendations:**
- Conservative (fewer opportunities): Tighten `impact_whitelist` to 3.0%
- Aggressive (more opportunities): Raise `marginal_max_size` to 200
- High-volume bot: Raise `whitelist_min_size` to 5000

---

## Integration with Bot

### Step 1: Run Analysis
```bash
python3 scripts/verify_whitelist_enhanced.py --categorize > pool_analysis.txt
```

### Step 2: Review Recommendations
Look for sections like:
```
► PROMOTE TO WHITELIST:
  - WBTC/USDC 0.05% (0x50eaEDB8...)
    Reason: Excellent: $5000 @ 1.2% impact, score=92

► ADD TO MARGINAL (soft blacklist for large trades):
  - USDT/USDC 0.01% (0x3F5C...)
    Reason: Small trade pool: max $100 @ 8.5% impact
```

### Step 3: Update Config

**For MARGINAL pools**, add to a new section in `pools_whitelist.json`:

```json
{
  "marginal": {
    "description": "Pools that work well for small trades only",
    "max_trade_size_usd": 100,
    "pools": [
      {
        "address": "0x3F5C85bb2F8e0a34874Da32eD3f59d934Cd55e71",
        "pair": "USDT/USDC",
        "fee_tier": 100,
        "dex": "UniswapV3",
        "max_working_size": 100,
        "note": "Good up to $100, reverts at $1000"
      }
    ]
  }
}
```

### Step 4: Bot Logic Update

In your detector, add size checks:

```rust
// In detector.rs
fn should_execute_opportunity(opp: &Opportunity, pool_info: &PoolInfo) -> bool {
    // Check if pool is marginal
    if pool_info.category == "marginal" {
        if opp.trade_size_usd > 100.0 {
            return false;  // Skip - pool can't handle this size
        }
    }
    
    // Standard checks
    opp.expected_profit_usd >= MIN_PROFIT_USD
}
```

---

## Liquidity Score Breakdown

| Score | Max Working Size | Impact | Description |
|-------|-----------------|--------|-------------|
| 100   | $5000           | <2%    | Excellent depth - enterprise grade |
| 90-99 | $5000           | 2-5%   | Very good - suitable for large trades |
| 70-89 | $1000-$5000     | <5%    | Good - reliable for medium trades |
| 50-69 | $1000           | 5-10%  | Acceptable - use with caution |
| 30-49 | $100-$1000      | <10%   | Marginal - small trades only |
| 20-29 | $100            | >10%   | Poor - very limited use |
| 0-19  | <$100           | Any    | Blacklist - not usable |

---

## Common Patterns

### Pattern 1: High Fee Pool
```
WETH/USDC 1.00%  -  Max: $100  -  Impact: 15%  -  MARGINAL
```
**Reason:** High fee tier = low liquidity → only small trades work
**Action:** Add to marginal with $100 max size

### Pattern 2: Stablecoin Thin Pool
```
USDT/USDC 0.01%  -  Max: $1000  -  Impact: 0.2%  -  WHITELIST
```
**Reason:** Ultra-low fee attracts all liquidity for stable pairs
**Action:** Promote to whitelist

### Pattern 3: Exotic Pair
```
LINK/USDC 0.30%  -  Max: $10  -  Impact: 8%  -  BLACKLIST
```
**Reason:** Low volume exotic pair = extremely thin
**Action:** Blacklist permanently

---

## Workflow for New Pools

1. **Add to `observation` section** in whitelist JSON
2. **Run enhanced verifier**: `python3 scripts/verify_whitelist_enhanced.py --categorize`
3. **Check recommendation**: Look at suggested category
4. **Promote if whitelist/marginal**: Move to appropriate section
5. **Re-verify**: Run again to confirm

---

## Exit Codes

- `0`: All whitelist pools pass their category criteria
- `1`: Some pools are miscategorized (check recommendations)
- `2`: Configuration error (invalid JSON, missing RPC, etc.)

---

## Future Enhancements (Roadmap)

- [ ] Real-time monitoring mode (`--monitor` flag)
- [ ] Historical tracking (score changes over time)
- [ ] Alert on category changes (pool degraded from whitelist → marginal)
- [ ] Integration with Discord/Telegram for notifications
- [ ] Cross-DEX comparison (which DEX has better depth for same pair?)
- [ ] Optimal trade size calculator (maximize profit while minimizing impact)

---

## Example Output

```
ENHANCED QUOTE DEPTH MATRIX
Green: <5% impact | Yellow: 5-10% | Red: >10% or FAIL
============================================================================

Pool           Pair          Fee     Max$   $1    $10   $100  $1000 $5000 Impact Score Category    
0x8ad5..97Fb   WETH/USDC    0.05%   $5000  OK    0.2%  0.5%  1.2%  2.8%  2.8%   95    WHITELIST   
0xA374..d85a   WMATIC/USDC  0.05%   $5000  OK    0.1%  0.3%  0.8%  1.9%  1.9%   98    WHITELIST   
0x0c53..547A   USDT/USDC    0.05%   $5000  OK    0.1%  0.2%  0.4%  1.1%  1.1%   100   WHITELIST   
0x9b5c..d2a    WBTC/USDC    0.05%   $1000  OK    0.3%  1.2%  4.8%  FAIL  FAIL   4.8%   65    WHITELIST   
0x3F5C..e71    USDT/USDC    0.01%   $100   OK    0.5%  8.5%  FAIL  FAIL  FAIL   8.5%   30    MARGINAL    
0x4290..3Db    LINK/USDC    0.30%   $10    OK    15%   FAIL  FAIL  FAIL  FAIL   15%    10    BLACKLIST   

POOL CATEGORIZATION SUMMARY
============================================================================

WHITELIST (4 pools) - Safe for all trade sizes
  Pool           Pair          Fee     Max Size   Impact     Reason
  0x8ad5..97Fb   WETH/USDC    0.05%   $5000      2.8%       Excellent: $5000 @ 2.8% impact, score=95
  [...]

MARGINAL (1 pool) - Good for small trades only ($1-$100)
  Pool           Pair          Fee     Max Size   Impact     Reason
  0x3F5C..e71    USDT/USDC    0.01%   $100       8.5%       Small trade pool: max $100 @ 8.5% impact

BLACKLIST (1 pool) - Should not be used
  Pool           Pair          Fee     Max Size   Reason
  0x4290..3Db    LINK/USDC    0.30%   $10        High impact even at small size: 15.0% @ $10

RECOMMENDATIONS
============================================================================

✓ All pools are correctly categorized
```

---

**Questions? Issues? Improvements?**
This is designed to be iteratively improved. Add your own categorization logic, adjust thresholds, or extend the analysis as needed.
