# Session Summary: A4 Phase 2 — AMM State Simulator

**Date:** 2026-02-01 (session ~05:55–07:50 UTC)
**Commit:** `aaa0729` — pushed to main
**Status:** Phase 2 DEPLOYED and COLLECTING DATA on Polygon

---

## What Was Built

A4 Phase 2 adds AMM math simulation to the mempool monitor. When a pending
WETH/USDC or WMATIC/USDC swap is decoded, the bot:

1. **Identifies the pool** — maps token addresses + router + fee to a DexType
2. **Simulates post-swap state** — V2 constant product or V3 sqrtPriceX96 math
3. **Checks cross-DEX spreads** — compares simulated post-swap price against all other pools for the same pair
4. **Logs opportunities** — CSV with spread, estimated profit, trigger details
5. **Validates accuracy** — on block confirmation, compares predicted vs actual price

### Files Created/Modified

| File | Lines | Change |
|------|-------|--------|
| `src/mempool/simulator.rs` | 871 (NEW) | V2/V3 AMM math, pool ID, cross-DEX check, 11 unit tests |
| `src/mempool/types.rs` | +130 | SimulatedPoolState, SimulatedOpportunity, SimulationTracker |
| `src/mempool/monitor.rs` | +214 | Simulation pipeline, accuracy validation, CSV logging |
| `src/mempool/mod.rs` | +3 | `pub mod simulator` export |
| `src/main.rs` | +6 | Pass PoolStateManager (Arc clone) to mempool monitor |

### Key Math

- **V2**: `amountOut = (amountIn * 997 * reserveOut) / (reserveIn * 1000 + amountIn * 997)`
- **V3**: Uniswap SqrtPriceMath with overflow fallback (`getNextSqrtPriceFromAmount0RoundingUp`, `getNextSqrtPriceFromAmount1RoundingDown`)
- **Tick boundary**: Soft check — allows up to 10 tick spacings (~1% price move), accuracy CSV quantifies degradation
- **Fee application**: `amount_after_fee = amount_in * (1_000_000 - fee) / 1_000_000`

---

## Bugs Found and Fixed During Deploy

1. **Tick boundary too restrictive** — Original check only allowed 1 tick spacing (~0.1% move). Most moderate swaps cross multiple ticks. Relaxed to 10 spacings.

2. **Accuracy tracker only stored opportunities** — SimulationTracker only tracked sims that found cross-DEX opps. Changed to track ALL simulations, providing far more accuracy data.

3. **Missing `debug!` import** in monitor.rs after changing log levels.

4. **V3 sqrtPrice U256 overflow** — `numerator1 * sqrtPriceX96` exceeded U256 max for deep pools. Added Uniswap-style fallback formula that avoids the large intermediate product.

---

## Live Results (25 validated, 12 opportunities, ~45 min runtime)

### Simulation Accuracy

| Metric | Value |
|--------|-------|
| Samples validated | 25 |
| Median error | **0.040%** |
| Mean error | 0.063% |
| Perfect (0.00%) | 7/25 (28%) |
| Under 0.05% | 14/25 (56%) |
| Under 0.10% | 18/25 (72%) |
| Max error | 0.250% |

**By pair:**
- WETH/USDC: 18 samples, 0.040% median error
- WMATIC/USDC: 7 samples, 0.110% median error

**By DEX:**
- UniswapV3_005: 20 samples, 0.040% median error
- QuickswapV3: 4 samples, 0.060% median error
- UniswapV3_030: 1 sample, 0.000% error

### Lead Time

| Metric | Value |
|--------|-------|
| Min | 2,785ms |
| Median | 5,978ms |
| Mean | 6,099ms |
| Max | 8,884ms |

### Simulated Opportunities

| Metric | Value |
|--------|-------|
| Total opportunities | 12 |
| Median spread | 0.101% |
| Max spread | 0.212% |
| Median est. profit | $0.45 |
| Max est. profit | $1.00 |
| Profitable (>$0.10) | 10/12 (83%) |
| Profitable (>$0.50) | 3/12 (25%) |

**Top routes:**
- SushiV3_030 → UniswapV3_005: 5 opps (WETH/USDC)
- UniswapV3_005 → QuickSwapV2: 2 opps (WMATIC/USDC)
- UniswapV3_005 → QuickswapV3: 2 opps (WMATIC/USDC)

---

## Key Insights

1. **The math works.** 0.04% median error on V3 price prediction. Single-tick approximation is excellent for typical swap sizes. Higher errors (0.1-0.25%) correlate with multiple swaps hitting the same pool in one block — expected and quantifiable.

2. **Opportunities are real and actionable.** 12 cross-DEX opportunities in ~45 minutes. Median $0.45 estimated profit. On Polygon gas costs ~$0.01 per tx, so even the smallest opportunities ($0.05) are net-positive.

3. **Lead time is excellent.** 6 second median — ample time to construct, sign, and submit a backrun transaction. Even with a 200ms execution pipeline, we'd have 5.8s margin.

4. **SushiV3_030 ↔ UniswapV3_005 is the dominant route.** These pools have different fee tiers (0.30% vs 0.05%), so large swaps on one create persistent cross-DEX spreads until arbitraged.

5. **Phase 3 is clearly viable.** The combination of accurate simulation + long lead time + real opportunities means the execution layer can proceed.

---

## What Phase 3 Needs (Prep Notes)

### Execution Architecture

When `SIM OPP` fires with `spread > threshold`:

1. **Build backrun tx** — Call ArbExecutor.sol with buy_pool/sell_pool from the opportunity
2. **Skip estimateGas (A5)** — Set gas limit to fixed safe value (~500K). Simulation already validated the trade.
3. **Dynamic gas (A5 enhancement)** — Don't use static 5000 gwei. Gas bid should scale with: expected profit, spread size, competitive gas from the trigger tx.
4. **Sign + send immediately** — Via private RPC (1RPC). Goal: land in same block as the trigger swap, positioned after it.
5. **Nonce management** — Use cached nonce (A2), increment optimistically.

### Key Questions for Phase 3

- **Gas strategy**: Static 5000 gwei wastes money on small opps, underbids on large ones. Dynamic: `priority_fee = min(expected_profit * 0.5, trigger_tx.gas_price * 1.1)`?
- **Confirmation timing**: Should we submit immediately on SIM OPP, or wait until the trigger tx appears in a block? Immediate = risk of trigger not confirming. Wait = lose the 6s lead time advantage.
- **Minimum profit threshold**: Current detector uses $0.10. For mempool-sourced signals with higher conviction, can lower to $0.05 (gas is $0.01).
- **Concurrent submissions**: If 3 SIM OPPs fire in quick succession, submit all or filter to best?

### Files to Modify

| File | Change |
|------|--------|
| `executor.rs` | Add `execute_from_mempool()` — skips estimateGas, dynamic gas, accepts SimulatedOpportunity |
| `monitor.rs` | Add execution trigger in SIM OPP branch (call executor via channel/Arc) |
| `main.rs` | Pass executor handle to mempool monitor |
| `types.rs` | Add `MempoolMode::Execute` actual implementation |

---

## Session Actions

1. Built Phase 2 from plan (`ticklish-hugging-pony.md`) — types, simulator, monitor integration, main.rs wiring
2. Fixed V3 U256 overflow with Uniswap-style fallback
3. All 72 tests passing (11 new simulator + 61 existing)
4. Deployed 4 iterations (debug logging → tick boundary fix → accuracy tracker fix → final)
5. Committed as `aaa0729`, pushed to main
6. Bot running in livebot_polygon tmux, collecting data continuously

---

## CSV Outputs

| File | Rows | Purpose |
|------|------|---------|
| `data/polygon/mempool/pending_swaps_20260201.csv` | 534 | All decoded pending swaps |
| `data/polygon/mempool/simulated_opportunities_20260201.csv` | 12 | Cross-DEX arb opportunities from simulation |
| `data/polygon/mempool/simulation_accuracy_20260201.csv` | 25 | Predicted vs actual price comparison |

---

*Next session: Analyze accumulated data (24h target), then build Phase 3 execution pipeline.*
