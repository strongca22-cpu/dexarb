# Phantom Spread Analysis

**Date**: 2026-01-29
**Author**: AI-Generated
**Scope**: All phantom spread incidents observed during live and paper trading

---

## Executive Summary

Phantom spreads are price discrepancies between V3 pools that appear exploitable on paper but have zero executable depth on-chain. They are the single largest source of false signals in the bot's detection pipeline.

Four distinct phantom spread categories have been observed. All share a common root cause: **V3 tick-based prices reflect the last trade, not the current executable depth.** A pool with 749B of in-range liquidity still has a tick/price that looks legitimate — but attempting to swap $140 through it returns a fraction of the expected output.

---

## How V3 Pricing Creates Phantoms

### Tick-Based Price vs. Executable Depth

Uniswap V3 stores price as `sqrtPriceX96` (a Q64.96 fixed-point number) and `tick` (an integer). The relationship is:

```
price = 1.0001^tick × 10^(decimals0 - decimals1)
```

This tick represents **the price at which the last swap occurred** — it is NOT a guarantee that future swaps will execute at that price. The actual execution depends on:

1. **Liquidity concentrated at the current tick range** — V3 liquidity providers choose specific price ranges. If no LP has placed liquidity around the current tick, swaps cannot execute there.

2. **Depth of that liquidity** — Even if some LP covers the current tick, the amount may be dust (e.g., $0.01 of real depth for a $140 trade).

3. **Tick spacing** — The price can jump across empty tick ranges. A swap may cross many ticks to fill, encountering progressively worse prices (slippage).

### The `liquidity` Field Is Misleading

The pool's `liquidity` field represents `sqrt(x * y)` of all in-range positions combined, measured in abstract units — NOT dollars, NOT tokens, NOT depth. A pool can have:

- `liquidity = 749,000,000,000` (749B) — looks substantial
- Actual depth at current tick: **$0.50** of real executable value

This disconnect occurs because:
- The liquidity value aggregates ALL positions in the current tick range
- Positions may be very narrow (a few ticks wide) with dust amounts
- The pool was once active but liquidity was withdrawn, leaving the tick in place

### Why Two Pools Diverge

When a V3 pool has low trading activity, its tick drifts from the market:

1. The 0.05% pool has high volume → tick tracks real market price (e.g., WETH = $2,800)
2. The 0.01% pool has near-zero volume → tick frozen at last trade (e.g., WETH = $1,710)
3. The detector sees a "spread" between these ticks
4. The Quoter confirms the 0.01% pool can only return $85 for $140 of WETH

The 0.01% pool's tick is *technically correct* — it IS the price of the last trade. But that trade may have been weeks ago with dust amounts, and no LP is maintaining positions there now.

---

## Observed Phantom Spread Incidents

### Incident 1: AAVE/USDC — 69% Phantom (Permanent Divergence)

**Date**: 2026-01-29
**Pools**: V3 0.05% and V3 0.30%
**Impact**: Polluted paper trading Discord with ~$9M/15min phantom profit

| Pool | Price (AAVE/USDC) | Liquidity |
|------|-------------------|-----------|
| 0.05% | 0.010822 | Active |
| 0.30% | 0.006390 | Active |

- **Apparent spread**: 69%
- **Quoter reality**: Output 4.98B vs expected 1.5e18 → **302,000× gap**
- **Root cause**: Both pools have active positions but at completely different tick ranges. The ticks are "correct" for each pool's internal state but reflect entirely different price levels. No arbitrageur has bothered to align them because the cost (fees) exceeds the extractable value.
- **Status**: AAVE/USDC removed from all configs. Gate check data preserved.
- **Lesson**: Gate checks (pool existence + activity + Quoter depth) passed. Only live observation (Gate 4) caught the phantom.

### Incident 2: WETH/USDC 0.01% — Low Liquidity Phantom ($3.35 loss)

**Date**: 2026-01-29
**Pools**: V3 0.05% (buy) and V3 0.01% (sell)
**Impact**: 3 buy transactions executed, all sell legs rejected. $3.35 total loss after manual WETH recovery.

| Pool | Tick Price | Liquidity | Quoter Output for 0.0498 WETH |
|------|-----------|-----------|-------------------------------|
| 0.05% | $2,812/WETH | High | 140 USDC (real) |
| 0.01% | $2,800/WETH (tick) | 749B (low) | **85 USDC** ($1,710 effective) |

- **Apparent spread**: 0.78%
- **Quoter reality**: Sell side returns 85 USDC for tokens worth 140 USDC → **39% actual loss**
- **Root cause**: WETH/USDC 0.01% pool has 749B liquidity — above the detector's minimum threshold (`trade_size_usd × 1e6 = 140M`) but representing near-zero real depth for a non-stablecoin pair.
- **Why buy succeeded**: The 0.05% pool has deep liquidity. The buy Quoter pre-check passed correctly.
- **Why sell failed**: The sell Quoter check happens AFTER the buy executes. By then, capital is committed.
- **Compounding factor**: The main loop's "buy-then-continue" bug (see below) caused the bot to buy 3 times instead of stopping after the first failure.
- **Status**: WETH recovered manually. 0.01% tier needs restriction to stablecoins only.

### Incident 3: UNI/USDC 0.30% vs 0.05% — Near-Threshold Rejection

**Date**: 2026-01-29
**Pools**: V3 0.30% (buy) and V3 0.05% (sell)

| Pool | Price (UNI/USDC) | Fee |
|------|-----------------|-----|
| 0.30% | 0.226904 | 0.30% |
| 0.05% | 0.210194 | 0.05% |

- **Apparent spread**: 7.60%
- **Quoter output**: 31.315e18 vs minimum 31.608e18 → **0.93% short**
- **Root cause**: This is borderline — not a permanent phantom like AAVE, but the spread is consumed by price impact. The 0.30% pool has lower liquidity, so buying $140 worth moves the price enough to eliminate the spread.
- **Classification**: Transient phantom — the spread exists in the tick data but the depth at $140 trade size cannot sustain it.

### Incident 4: All 1% Fee Tier Pools — Systematic Phantom (Polygon-Wide)

**Date**: 2026-01-28 through 2026-01-29
**Pools**: All 1% V3 pools on Polygon
**Impact**: Zero — caught before live trading by Quoter pre-checks.

| Pair | 1% Liquidity | 0.05% Liquidity | Ratio |
|------|-------------|-----------------|-------|
| UNI/USDC | 8.84e10 | 1.51e11 | 1.7x lower |
| WBTC/USDC | Exists | Deep | — |
| LINK/USDC | Exists | Deep | — |

- **Quoter result**: 100% rejection rate across all pairs and all trade sizes
- **Root cause**: The 1% fee tier on Polygon has phantom liquidity — positions exist on-chain but no real depth. This may be due to inactive LPs who deployed positions during a promotion and never withdrew.
- **Status**: 1% tier (`fee >= 10000`) filtered at detection time in `detector.rs:105`. Filtered at sync time in `v3_syncer.rs` for data collector.

---

## The Phantom Spread Taxonomy

| Category | Example | Spread | Quoter Gap | Frequency | Detection |
|----------|---------|--------|-----------|-----------|-----------|
| **Permanent divergence** | AAVE 0.05%↔0.30% | 69% | 302,000× | Always present | Observation only |
| **Low-liquidity stale tick** | WETH 0.01% | 0.78% | 1.65× | Persistent | Quoter pre-check |
| **Price impact exhaustion** | UNI 0.30%↔0.05% | 7.60% | 0.99× | Intermittent | Quoter pre-check |
| **Systematic phantom tier** | All 1% pools | Varies | Total | Always | Fee tier filter |

---

## Why Pre-Trade Quoter Checks Are Insufficient

The current architecture checks each leg independently:

```
1. Quoter check: Can buy pool fill the buy order? → YES (real pool)
2. EXECUTE BUY → Capital committed
3. Quoter check: Can sell pool accept the sell? → NO (phantom!)
4. Result: Holding tokens, manual recovery needed
```

The sell-side Quoter check correctly identifies the phantom — but it runs AFTER the buy. A better approach would be to simulate BOTH legs before executing either:

```
1. Quoter check: Can buy pool fill? → YES
2. Quoter check: Can sell pool accept? → NO → SKIP (zero capital risk)
3. Never execute buy → No loss
```

This "dual pre-check" pattern would have prevented the WETH/USDC $3.35 loss entirely. Both Quoter calls are read-only (`.call()`) and cost zero gas.

---

## Defense Layers (Current and Recommended)

### Currently Active

1. **1% fee tier filter** — `detector.rs:105`: Skips all pools with `fee >= 10000`
2. **Zero-liquidity filter** — `v3_syncer.rs`: Skips pools where `liquidity == 0`
3. **Minimum liquidity check** — `detector.rs:191`: Requires `liquidity >= trade_size_usd × 1e6`
4. **Buy-side Quoter pre-check** — `executor.rs:175`: Simulates buy before committing
5. **Sell-side Quoter check** — `executor.rs:246`: Simulates sell after buy (but capital already committed)

### Recommended Additions

6. **Dual pre-check** — Check BOTH buy and sell Quoters before executing either leg
7. **0.01% tier restriction** — Only allow 0.01% pools for stablecoin pairs (USDT/USDC, DAI/USDC) where deep liquidity exists
8. **Minimum Quoter-output ratio** — Reject if `quoter_output / expected_output < 0.90` (catches near-threshold phantoms)
9. **Halt on committed capital** — Fix the buy-then-continue bug so the bot stops immediately when holding unbalanced tokens
10. **Per-pair observation gate** — No pair goes live without 24h of paper-trading observation (already documented in gate check process)

---

## Per-Pool 0.01% Fee Tier Status

| Pair | 0.01% Pool Exists | Liquidity | Classification | Action |
|------|------------------|-----------|---------------|--------|
| WETH/USDC | Yes | 749B (low) | **Phantom for $140 trades** | Exclude from live |
| WMATIC/USDC | Yes | 328T (moderate) | Needs observation | Paper only |
| WBTC/USDC | No | 0 | N/A | Already filtered |
| USDT/USDC | Yes | 128T (deep) | **Real — stablecoin** | Safe for live |
| DAI/USDC | Yes | 150e18 (deep) | **Real — stablecoin** | Safe for live |
| LINK/USDC | No | 0 | N/A | Already filtered |
| UNI/USDC | No | 0 | N/A | Already filtered |

**Conclusion**: Only USDT/USDC and DAI/USDC have real 0.01% liquidity. WETH and WMATIC 0.01% pools have active ticks but insufficient depth.

---

## Key Takeaways

1. **Tick price ≠ executable price.** This is the single most important fact about V3 arbitrage. The tick is a historical artifact. The Quoter is the only source of truth.

2. **The `liquidity` field is not a quality metric.** A pool with 749B liquidity can have $0 of executable depth at the current tick. Only the Quoter reveals real depth.

3. **Low-fee-tier pools on non-stablecoins are dangerous.** The 0.01% fee makes arb look attractive (only 0.06% round-trip), but these pools attract almost no LP capital for volatile assets. The low fee doesn't compensate LPs for impermanent loss, so they don't provide liquidity there.

4. **Phantoms are stable, not transient.** AAVE's 69% phantom persisted for the entire observation period. The 1% tier phantom is permanent. These aren't fleeting mispricings — they're structural features of pools with no active market makers.

5. **Defense must be layered.** No single check catches all phantoms. The fee-tier filter catches 1%. The liquidity filter catches zero-liquidity pools. The Quoter catches the rest. But the Quoter check must happen BEFORE capital is committed (dual pre-check), not after.

---

*Last updated: 2026-01-29*
