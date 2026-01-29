# DEX Arbitrage V4 - Alternate Pairings Buildout Plan

## Multi-Pair Expansion Strategy

**Version**: 4.4
**Date**: 2026-01-29
**Based On**: Live bot data, $500 incident postmortem, Quoter gap analysis
**Goal**: Systematically expand pair coverage with empirical validation per pair

---

## Current State

### Active Pairings (7 pairs, 14 V3 pools)

All paired with USDC.e (`0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174`).
Each monitored at 0.05% and 0.30% V3 fee tiers. 1% fee tier excluded (phantom liquidity on Polygon, confirmed by Quoter testing).

| # | Pair | Token Address | Fee Tiers | Status |
|---|------|--------------|-----------|--------|
| 1 | WETH/USDC | `0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619` | 0.05%, 0.30% | Active |
| 2 | WMATIC/USDC | `0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270` | 0.05%, 0.30% | Active |
| 3 | WBTC/USDC | `0x1BFD67037B42Cf73acF2047067bd4F2C47D9BfD6` | 0.05%, 0.30% | Active |
| 4 | USDT/USDC | `0xc2132D05D31c914a87C6611C10748AEb04B58e8F` | 0.05%, 0.30% | Active |
| 5 | DAI/USDC | `0x8f3Cf7ad23Cd3CaDbD9735AFf958023239c6A063` | 0.05%, 0.30% | Active |
| 6 | LINK/USDC | `0x53E0bca35eC356BD5ddDFebbD1Fc0fD03FaBad39` | 0.05%, 0.30% | Active |
| 7 | UNI/USDC | `0xb33EaAd8d922B1083446DC23f610c2567fB5180f` | 0.05%, 0.30% | Active |

**Removed:**
| Pair | Reason | Date |
|------|--------|------|
| AAVE/USDC | Phantom 69% spread (302,000x Quoter gap). Pools active but tick-based prices permanently divergent. | 2026-01-29 |

### Architecture Constraints

These are hard constraints that limit what pairs can be added without code changes:

1. **USDC-only pairing** — `detector.rs` hardcodes `1e6` (USDC's 6 decimals) for trade size calculation and minimum liquidity thresholds. The price oracle (`price_oracle.rs`) only looks up `/USDC` pairs. Non-USDC pairs (e.g., WETH/DAI, WMATIC/USDT) will break price lookups and position sizing.

2. **V3-only** — V2 sync is dropped from the main loop. V2 code exists but is not called. Re-enabling V2 would require re-adding `PoolSyncer` to `main.rs` and fixing the V2 price calculation inversion bug.

3. **0.05% and 0.30% fee tiers only** — `V3_FEE_TIERS` in `v3_syncer.rs` defines three tiers (500, 3000, 10000) but 1% (10000) is filtered out at sync and detection time. The 0.01% fee tier (100) is NOT included in `V3_FEE_TIERS` — adding it requires a code change (see "0.01% Fee Tier" section below).

4. **Flat .env config** — Pairs are defined as `token0:token1:symbol` in a comma-separated `TRADING_PAIRS` string. No per-pair configuration (min spread, max trade size, etc.). All pairs share the same global `MAX_TRADE_SIZE_USD`, `MIN_PROFIT_USD`, and `MAX_SLIPPAGE_PERCENT`.

5. **Single-threaded execution** — The bot tries opportunities sequentially in profit order. While scanning is parallel, only one trade can execute at a time.

6. **RPC budget** — Each pair adds ~4 RPC calls/cycle (2 pools x 2 calls). At 3s poll interval, 7 pairs = 14 pools = ~29 calls/cycle = ~9.7 calls/sec. On Alchemy WSS (free tier 22.2M/month, ~25.1M projected).

### Key Lessons from Live Testing

- **Spot price != execution price.** The detector sees a 0.96% spread but $140 causes 1.26% price impact on the shallow pool. The Quoter correctly rejects these.
- **Pool `liquidity` field is unreliable for quality assessment.** UNI 1% pool had 8.84e10 liquidity vs UNI 0.05% at 1.51e11 — only 1.7x apart — yet the 1% pool was completely phantom. Only the Quoter can determine executable depth.
- **Zero-capital validation works.** All Quoter checks use `.call()` (read-only simulation). 132 consecutive rejections with zero gas spent.
- **More pairs = more chances to catch transient spikes.** The strategy is not persistent narrow spreads but catching brief wide spreads across many pairs.

---

## Pool Assessment Criteria

Every pool must pass these checks before a pair is added to live trading. This process exists because we lost $500 to a pool that looked healthy by on-chain metrics but had negligible executable liquidity.

### Gate 1: Pool Existence Check

```
REQUIREMENT: V3 pool must exist at BOTH 0.05% and 0.30% fee tiers.

METHOD:
  cast call 0x1F98431c8aD98523631AE4a59f267346ea31F984 \
    "getPool(address,address,uint24)(address)" \
    [TOKEN] [USDC] [FEE_TIER] \
    --rpc-url https://polygon-bor.publicnode.com

PASS: Both pools return non-zero addresses.
FAIL: Either pool returns 0x000...000 — pair cannot be traded (only one fee tier = no cross-tier arb).
```

### Gate 2: Pool Activity Check

```
REQUIREMENT: Both pools must have non-stale prices (updated within last 1000 blocks).

METHOD:
  1. Query slot0() for sqrtPriceX96 and tick
  2. Query liquidity()
  3. Check last swap event block vs current block

PASS: sqrtPriceX96 > 0, liquidity > 1000, price moved within last 1000 blocks.
FAIL: Zero price, dust liquidity, or stale (no swaps in >1000 blocks).
```

### Gate 3: Quoter Depth Check

```
REQUIREMENT: Pool can absorb the trade size without excessive price impact.

METHOD:
  Call quoteExactInputSingle() with MAX_TRADE_SIZE_USD worth of input token.
  Compare quoted output to spot-price expected output.

PASS: Quoted output >= 95% of spot-price expectation (price impact < 5%).
FAIL: Quoted output significantly less than expected — pool too shallow.

NOTE: This is the check that prevented a repeat of the $500 loss. Do not skip.
```

### Gate 4: Spread Logger Observation

```
REQUIREMENT: The pair must show observable spreads between its 0.05% and 0.30% pools
             over a 24-48 hour observation window.

METHOD:
  1. Add pair to .env TRADING_PAIRS
  2. Run bot (it will scan and log spreads but Quoter will reject unprofitable ones)
  3. Collect spread data from logs for 24-48 hours
  4. Analyze: How often does executable spread appear? At what magnitude?

PASS: At least some observable spread differential between fee tiers.
FAIL: Pools track each other exactly (zero spread) or one pool is completely stale.

This is an empirical gate — no assumptions about spike frequency or magnitude.
The data tells you whether the pair has any arb potential at all.
```

### Gate 5: Paper Trade Verification

```
REQUIREMENT: Paper trading shows realistic opportunity detection for the pair.

METHOD:
  Monitor paper trading reports for the new pair over 24-48 hours.
  Check that detected opportunities are not phantom (would the Quoter reject them all?).

PASS: At least some opportunities where Quoter gap is < 5%.
FAIL: All opportunities have massive Quoter gaps (phantom pools or stale pricing).
```

---

## Candidate Pairs — Gate Check Results (2026-01-29)

Gate checks run via `scripts/pool_gate_check.py` at block ~82274740.

### Results Summary

| Token | Gate 1 (Exist) | Gate 2 (Active) | Gate 3 (Quoter) | Result |
|-------|---------------|-----------------|-----------------|--------|
| **AAVE** | 0.05% + 0.30% | Both active | Both executable | **PASS** |
| CRV | 0.05% + 0.30% | 0.05% zero liq | -- | FAIL |
| SUSHI | 0.30% only | -- | -- | FAIL |
| BAL | Neither exists | -- | -- | FAIL |
| GRT | 0.05% + 0.30% | 0.05% zero liq | -- | FAIL |
| SNX | 0.30% only | -- | -- | FAIL |
| 1INCH | 0.05% only | -- | -- | FAIL |
| GHST | 0.05% + 0.30% | 0.05% zero liq | -- | FAIL |
| COMP | 0.30% only | -- | -- | FAIL |
| stMATIC | 0.05% + 0.30% | 0.05% zero liq | -- | FAIL |
| wstETH | 0.05% only | -- | -- | FAIL |

**Stablecoin 0.01% fee tier check:**

| Token | 0.01% | 0.05% | 0.30% | Gate 3 | Result |
|-------|-------|-------|-------|--------|--------|
| **USDT** | Active | Active | Active | All executable | **PASS** |
| **DAI** | Active | Active | Active | All executable | **PASS** |

### Actionable Outcomes

1. **AAVE/USDC** — Ready for Round 2 (add to `.env` for observation). Pool addresses:
   - 0.05%: `0x693b52abdb6df2ea735eb19244a9e55374ebdf60`
   - 0.30%: `0xa236278bec0e0677a48527340cfb567b4e6e9adc`

2. **USDT/USDC and DAI/USDC** — 0.01% pools exist and are active. Adding 0.01% fee tier (Round 4) unlocks new arb routes for these already-active pairs. USDT 0.01%↔0.05% is particularly interesting (only 0.06% round-trip fee).

3. **9 of 11 alt-token candidates failed** — Polygon V3 ecosystem is thin for non-major tokens. Most only have a single fee tier pool, or the 0.05% pool has zero liquidity. No further candidates to pursue from Groups A/B/C at this time.

4. **Native USDC** (`0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359`) — Not checked. Requires special handling (USDC.e vs USDC arb, not token/USDC arb). Deferred.

### Candidate Details (for reference)

| Token | Polygon Address | 0.05% Pool | 0.30% Pool | Notes |
|-------|----------------|-----------|-----------|-------|
| AAVE | `0xD6DF932A45C0f255f85145f286eA0b292B21C90B` | `0x693b52...` | `0xa23627...` | **PASS** |
| CRV | `0x172370d5Cd63279eFa6d502DAB29171933a610AF` | exists, zero liq | `0xea8a6f...` | 0.05% dead |
| SUSHI | `0x0b3F868E0BE5597D5DB7fEB59E1CADBb0fdDa50a` | missing | `0x836f03...` | no cross-tier |
| BAL | `0x9a71012B13CA4d3D0Cda72A5D7Bab2E3d5C3E8A6` | missing | missing | no V3 pools |
| GRT | `0x5fe2B58c013d7601147DcdD68C143A77499f5531` | exists, zero liq | `0x5ae43c...` | 0.05% dead |
| SNX | `0x50B728D8D964fd00C2d0AAD81718b71311feF68a` | missing | `0x647244...` | no cross-tier |
| 1INCH | `0x9c2C5fd7b07E95EE044DDeba0E97a665F142394f` | `0x8d7ca6...` | missing | no cross-tier |
| GHST | `0x385Eeac5cB85A38A9a07A70c73e0a3271CfB54A7` | exists, zero liq | `0x6e65db...` | 0.05% dead |
| COMP | `0x8505b9d2254A7Ae468c0E9dd10Ccea3A837aef5c` | missing | `0x2d4e28...` | no cross-tier |
| stMATIC | `0x3A58a54C066FdC0f2D55FC9C89F0415C92eBf3C4` | exists, zero liq | `0x26770c...` | 0.05% dead |
| wstETH | `0x03b54A6e9a984069379fae1a4fC4dBAE93B3bCCD` | `0x1c4c46...` | missing | no cross-tier |

---

## 0.01% Fee Tier Consideration

### What It Is

Uniswap V3 has a 0.01% (1 bps) fee tier designed for stablecoin pairs. On Polygon:
- USDT/USDC and DAI/USDC likely have 0.01% pools
- These pools have extremely low swap fees
- Cross-tier arb: 0.01% <-> 0.05% gives only 0.06% round-trip fee (vs 0.35% for 0.05% <-> 0.30%)

### Code Change Required

`V3_FEE_TIERS` in `v3_syncer.rs` currently defines:
```rust
pub const V3_FEE_TIERS: [(u32, DexType); 3] = [
    (500, DexType::UniswapV3_005),   // 0.05%
    (3000, DexType::UniswapV3_030),  // 0.30%
    (10000, DexType::UniswapV3_100), // 1.00%
];
```

Adding 0.01% requires:
1. Add `UniswapV3_001` variant to `DexType` enum in `types.rs`
2. Add `(100, DexType::UniswapV3_001)` to `V3_FEE_TIERS`
3. Update detector fee logic to handle 0.01% combinations
4. Test: verify 0.01% pools exist for USDT/USDC and DAI/USDC on Polygon

### When to Do This

Defer until after at least one candidate pair from Group A passes all gates. The 0.01% tier is only useful for stablecoin pairs (USDT/USDC, DAI/USDC) which are already active. It adds new arb routes to existing pairs rather than new pairs.

---

## Phased Rollout Process

### Principle: One Pair at a Time

Each new pair goes through the full gate process independently. Do not batch-add pairs.

**Why one at a time:**
- Isolates problems. If a new pair causes issues (RPC errors, phantom detection, excessive logging), you know exactly which pair is responsible.
- Validates the process. If the first candidate fails all gates, it reveals whether the issue is the specific token or a broader problem with the approach.
- Maintains RPC budget. Each pair adds ~4 calls/cycle. Adding 5 pairs at once = +20 calls/cycle = noticeable impact.

### Step-by-Step Per-Pair Process

```
FOR EACH CANDIDATE PAIR:

Step 1: Address Verification
  - Confirm token address on PolygonScan
  - Confirm token is not rebasing, fee-on-transfer, or deprecated
  - Confirm token has 18 decimals (or note if different — executor handles arbitrary decimals
    but detector assumes USDC is the 6-decimal side)
  - Duration: 5 minutes

Step 2: Gate 1 — Pool Existence
  - Run cast call for both 0.05% and 0.30% fee tiers
  - If either pool doesn't exist: STOP. This pair has no cross-tier arb route.
  - Record both pool addresses
  - Duration: 2 minutes

Step 3: Gate 2 — Pool Activity
  - Query slot0(), liquidity() for both pools
  - Check prices are non-zero and reasonable
  - Check liquidity is above dust threshold (> 1000)
  - Duration: 5 minutes

Step 4: Add to .env (observation mode)
  - Append the new pair to TRADING_PAIRS
  - Rebuild binary (cargo build --release)
  - Restart bot in tmux
  - The bot will scan and detect opportunities but Quoter will reject unprofitable ones
  - Duration: 10 minutes to deploy, then 24-48 hours observation

Step 5: Gate 3 — Quoter Depth (automatic)
  - The live bot performs this check automatically on every detected opportunity
  - Review logs: what Quoter gap does this pair show?
  - If gap is consistently >20%: pool is too shallow at current trade size
  - If gap is <5%: pool has executable depth
  - Duration: included in Step 4 observation window

Step 6: Gate 4 — Spread Logger Analysis
  - Extract spread data for this pair from bot logs
  - How often does a spread appear between the 0.05% and 0.30% pools?
  - What magnitude? (0.1%? 0.5%? 2%+?)
  - Is the spread variable (transient spikes) or static (frozen)?
  - Duration: analysis after Step 4 observation

Step 7: Decision
  - KEEP: Pair shows variable spreads, Quoter gaps are manageable, pools are active
  - REMOVE: Pools are stale, no spreads, or Quoter rejects 100% with large gaps
  - DEFER: Inconclusive — needs longer observation or different trade size
```

### Rollout Sequence (Updated with Gate Check Results)

Gate 1-3 checks completed 2026-01-29. Results narrow the expansion path significantly.

**Round 1: Validate existing pairs (no code changes)** — IN PROGRESS

The bot is scanning 7 pairs. After 38 minutes of logs (06:03-06:41 UTC):
- Only **UNI/USDC** shows any spread (0.65% net, every cycle, Quoter-rejected)
- Other 6 pairs: spreads < 0.22% (below 0.35% round-trip fee threshold)
- No non-UNI opportunity detected across 644 scan cycles
- Need 24-48+ hours of data to catch transient spikes on the other pairs

Pending analysis:
- Do WETH, WMATIC, WBTC, LINK, USDT, DAI ever spike above 0.35%?
- If any pair shows zero spread variation over 48h, consider removing to free RPC budget

**Round 2: AAVE/USDC** — ADDED then REMOVED (2026-01-29)

AAVE passed Gates 1-3 (pools exist, both active, Quoter executable at $140). However, live observation revealed:
- 0.05% pool price: 0.010822 | 0.30% pool price: 0.006390 → **69% apparent spread**
- Quoter rejects every cycle: output 4.98B vs expected 1.5e18 (302,000x gap)
- This is a **phantom spread** — tick-based prices permanently divergent, no executable profit
- Paper trading Discord reports flooded with ~$9M/15min phantom profit
- **Removed** from `.env`, `paper_trading.toml`, all strategies. Gate check data preserved above.
- **Lesson**: Gate checks (pool existence + activity + Quoter depth) are necessary but not sufficient. Observation period (Gate 4) caught a phantom that passed all automated checks.

**Round 3: 0.01% fee tier for stablecoins** — CONFIRMED VIABLE

Gate checks confirmed both USDT/USDC and DAI/USDC have active 0.01% pools:
- USDT 0.01% pool: `0xdac8a8e6dbf8c690ec6815e0ff03491b2770255d` (liquidity: 128T)
- DAI 0.01% pool: `0x5645dcb64c059aa11212707fbf4e7f984440a8cf` (liquidity: 150e18)
- Both Quoter-executable at $140 trade size

Implementation:
1. Add `UniswapV3_001` to `DexType` enum in `types.rs`
2. Add `(100, DexType::UniswapV3_001)` to `V3_FEE_TIERS` in `v3_syncer.rs`
3. This gives existing pairs new routes: 0.01%↔0.05% (0.06% round-trip) and 0.01%↔0.30% (0.31% round-trip)
4. Rebuild, restart, observe spread data

**Round 4: No further alt-token candidates available**

All Group A/B/C candidates failed gate checks:
- CRV, GRT, GHST, stMATIC: pools exist but 0.05% tier has zero liquidity
- SUSHI, SNX, COMP: only one fee tier pool exists (no cross-tier arb possible)
- BAL: no V3 pools at all
- wstETH, 1INCH: only one fee tier exists

Re-check periodically (monthly) — new pools may be created as Polygon V3 ecosystem grows. Use `python3 scripts/pool_gate_check.py --group A` to re-run.

**Round 6: Special cases**

- Native USDC (`0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359`) vs USDC.e arb
- stMATIC/USDC (liquid staking derivative)
- These require additional analysis of token mechanics

---

## RPC Budget Impact

### Current Load

| Metric | Value |
|--------|-------|
| Active pairs | 7 |
| V3 pools (0.05% + 0.30% each) | 14 |
| RPC calls/cycle | ~29 (14 pools x 2 + 1 block) |
| Poll interval | 3s |
| Calls/sec | ~9.7 |
| Monthly calls | ~25.1M |
| RPC provider | Alchemy WSS (free tier) |

### Projected Load by Pair Count

| Active Pairs | Pools | Calls/Cycle | Calls/Sec | Monthly |
|-------------|-------|-------------|-----------|---------|
| 7 (current) | 14 | 29 | 9.7 | 25.1M |
| 9 | 18 | 37 | 12.3 | 32.0M |
| 10 | 20 | 41 | 13.7 | 35.4M |
| 12 | 24 | 49 | 16.3 | 42.3M |
| 15 | 30 | 61 | 20.3 | 52.7M |

### RPC Provider Considerations

- **Alchemy free tier (current):** 22.2M calls/month. Current 7-pair load (~25.1M) within budget.
- **Alchemy Growth ($49/month):** 300M calls/month. Supports up to ~30+ pairs comfortably.
- **PublicNode (backup):** Free but drops WebSocket connections under burst load. Not suitable for V3 sync.
- **Recommendation:** Monitor Alchemy free tier usage. Upgrade to Growth if throttled or adding more pairs.

### Pruning Inactive Pairs

If Round 1 analysis shows that some of the current 7 pairs have completely stale pools or zero spread, remove them from `.env`. This frees RPC budget for new pairs that might perform better. A pair with frozen prices adds 4 wasted RPC calls per cycle.

---

## Future Code Changes (Reference)

These are NOT immediate tasks. They document what code changes would be needed for specific expansion directions.

### For non-USDC pairs (e.g., WETH/WMATIC)

Files affected:
- `detector.rs`: Remove hardcoded `1e6` in trade size calculation (line ~220). Use actual token decimals from V3PoolState.
- `detector.rs`: Remove hardcoded `1e6` in minimum liquidity check (line ~190).
- `price_oracle.rs`: Remove `/USDC` string matching (lines ~162, 184, 218). Support arbitrary pair pricing.
- `executor.rs`: Already handles arbitrary decimals via `calculate_min_out`. No change needed.

### For per-pair configuration (different thresholds per pair)

Currently all pairs share global `MAX_TRADE_SIZE_USD`, `MIN_PROFIT_USD`, `MAX_SLIPPAGE_PERCENT`. Per-pair config would require:
- New config format (TOML or extended .env)
- `BotConfig` struct changes in `types.rs`
- Detector to look up per-pair thresholds
- Executor to use per-pair slippage/size limits

### For 0.01% fee tier

- Add `UniswapV3_001` to `DexType` enum in `types.rs`
- Add `(100, DexType::UniswapV3_001)` to `V3_FEE_TIERS` in `v3_syncer.rs`
- Update any fee-tier display logic in detector/logging

---

## Verification Commands

### Check if V3 pool exists for a token pair

```bash
# Replace TOKEN_ADDR with the candidate token address
# USDC.e: 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174
# V3 Factory: 0x1F98431c8aD98523631AE4a59f267346ea31F984

# Check 0.05% pool (fee = 500)
cast call 0x1F98431c8aD98523631AE4a59f267346ea31F984 \
  "getPool(address,address,uint24)(address)" \
  TOKEN_ADDR 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174 500 \
  --rpc-url https://polygon-bor.publicnode.com

# Check 0.30% pool (fee = 3000)
cast call 0x1F98431c8aD98523631AE4a59f267346ea31F984 \
  "getPool(address,address,uint24)(address)" \
  TOKEN_ADDR 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174 3000 \
  --rpc-url https://polygon-bor.publicnode.com

# Check 0.01% pool (fee = 100) — for stablecoin pairs only
cast call 0x1F98431c8aD98523631AE4a59f267346ea31F984 \
  "getPool(address,address,uint24)(address)" \
  TOKEN_ADDR 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174 100 \
  --rpc-url https://polygon-bor.publicnode.com
```

Non-zero result = pool exists. `0x0000000000000000000000000000000000000000` = no pool.

### Check pool state (slot0 + liquidity)

```bash
# Replace POOL_ADDR with the pool address from the factory call above

# Get current price and tick
cast call POOL_ADDR \
  "slot0()(uint160,int24,uint16,uint16,uint16,uint8,bool)" \
  --rpc-url https://polygon-bor.publicnode.com

# Get current in-range liquidity
cast call POOL_ADDR \
  "liquidity()(uint128)" \
  --rpc-url https://polygon-bor.publicnode.com
```

### Verify token address on PolygonScan

```
https://polygonscan.com/token/TOKEN_ADDR
```

Confirm: correct token name, not a scam/clone, has transfer activity.

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 4.0 | 2026-01-28 | Initial draft with profit projections and V2/V3/1% routes |
| 4.1 | 2026-01-29 | Revised: removed profit projections, removed 1% fee tier, removed V2 routes, identified all 7 current pairs as already active, added empirical validation gates, phased per-pair rollout process, RPC budget analysis, 0.01% fee tier consideration |
| 4.2 | 2026-01-29 | Gate check results: AAVE passes (only new candidate). CRV/SUSHI/BAL/GRT/SNX/1INCH/GHST/COMP/stMATIC/wstETH all fail. USDT+DAI 0.01% pools confirmed active. Created `scripts/pool_gate_check.py`. Updated rollout sequence with empirical findings. |
| 4.3 | 2026-01-29 | AAVE/USDC added (8th pair). Phantom 69% spread observed — Quoter rejects every cycle. Alchemy WSS migration (from PublicNode). 1% fee tier filter added to data collector's `sync_v3_pools_subset()`. RPC budget updated for 8 pairs on Alchemy. |
| 4.4 | 2026-01-29 | AAVE/USDC removed — phantom confirmed (302,000x Quoter gap, $9M/15min phantom profit in Discord reports). Config separated: `.env.live` for live bot, `.env` for dev/paper. Back to 7 pairs. |

---

*Last updated: 2026-01-29 (v4.4 — AAVE removed, config separated, 7 active pairs)*
