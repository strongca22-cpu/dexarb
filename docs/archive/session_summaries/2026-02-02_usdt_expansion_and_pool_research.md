# Session Summary: USDT Quote Token Expansion & Pool Research Pipeline

**Date:** 2026-02-02 (evening session)
**Branch:** `feature/alloy-migration`
**Previous session:** `2026-02-02_alloy_migration_completion.md`

---

## Objective

Analyze mempool swap data to understand pair/pool coverage gaps, then expand the bot's trading surface by adding USDT as a third quote token and building research tooling for ongoing pool discovery.

---

## Commits This Session

| Hash | Description | Files Changed |
|------|-------------|---------------|
| `4d0968d` | feat: USDT quote token, hybrid pipeline, adaptive sizing, pool expansion tooling | 15 files (+2698/-244) |
| `a1ecac0` | fix: correct 6 wrong USDT pool addresses in whitelist v1.8 | 1 file (6 address corrections) |

---

## Work Completed

### 1. Mempool Pair Distribution Analysis

Analyzed 14,137 decoded pending swaps from Feb 1-2 mempool observation data:

- **USDT** is #1 token by appearances (43.6% of swaps) — was NOT a recognized quote token
- Current 8-pair whitelist covered only **16.7%** of on-chain swap activity
- 53% of swaps involve unknown tokens (mostly single-pair bot activity)
- Identified gap: WETH/USDT (1,221 swaps), WMATIC/USDT (371), DAI/USDT cross-DEX arb

### 2. Phase A: USDT as Third Quote Token (Code Changes)

**types.rs**: Added `quote_token_address_usdt: Option<Address>` to BotConfig. Updated `is_quote_token()` to check all three quote tokens (USDC.e, native USDC, USDT).

**config.rs**: Load `QUOTE_TOKEN_ADDRESS_USDT` env var.

**detector.rs**: No logic changes needed — already isolates arbs by quote_token address (USDT pools only compare against other USDT pools). Added `quote_token_address_usdt: None` to test config.

**simulator.rs**: Updated PairLookup to include USDT as quote token.

**.env.polygon**: Added USDT env var, expanded TRADING_PAIRS from 8 → 11 (added WETH/USDT, WMATIC/USDT, DAI/USDT).

### 3. Pool Discovery & Depth Assessment Tooling

**scripts/pool_scanner.py**: Queries 5 DEX factory contracts (UniswapV3, SushiSwapV3, QuickSwapV3 Algebra, QuickSwapV2, SushiSwapV2) for all base × quote × fee combinations. Discovered 258 pools, 181 with liquidity.

**scripts/depth_assessment.py**: Queries quoter contracts at $100/$500/$5K trade sizes. Calculates price impact, recommends max_trade_size_usd, rates pools as ADD/THIN/DEAD. Results: 19 ADD, 88 THIN, 42 DEAD.

### 4. Whitelist v1.8 Update

Added 10 new USDT-quoted pools across 3 pairs:
- **WETH/USDT**: UniV3(500), UniV3(3000), QuickSwapV2, SushiSwapV2 — 4 pools, 3 DEX families
- **WMATIC/USDT**: UniV3(500), UniV3(3000), QuickSwapV2 — 3 pools, 2 DEX families
- **DAI/USDT**: UniV3(100), UniV3(3000), QuickSwapV2 — 3 pools, 2 DEX families

### 5. Address Bug Found & Fixed

6 of 10 USDT pool addresses in the whitelist were transcribed incorrectly during a previous context-exhausted session (addresses started correctly but diverged mid-string — hallucination pattern). Discovered when V2 pools failed to sync on startup. Fixed by cross-referencing against pool_scan_results.csv (factory-derived, verified addresses).

### 6. Hybrid Pipeline Types (from earlier in multi-session)

Added `HybridCache` and `CachedOpportunity` types for mempool → block confirmation flow. Mempool signals cached by trigger tx hash; executed only after trigger tx appears in confirmed block.

### 7. Per-Pool Adaptive Trade Sizing

`WhitelistFilter.max_trade_size_for()` reads per-pool USD caps from whitelist JSON. Detector uses `min(buy_pool, sell_pool)` as effective trade size. Min profit scales proportionally with floor at 2× gas cost.

### 8. Bot Deployed in Observe Mode

Running in tmux session `dexarb-observe`:
- `LIVE_MODE=false` (DRY RUN — no real transactions)
- `MEMPOOL_MONITOR=observe` (log pending swaps, no execution)
- 50 pools synced (37 V3 + 13 V2)
- 3 quote tokens active (USDC.e, native USDC, USDT)
- Event sync processing pool updates every block
- Mempool capturing pending swaps with 2-9s lead times

---

## Phase B Assessment (Long-Tail Tokens)

Investigated SAND, SOL (Wormhole), CRV as expansion candidates. **Conclusion: not viable for arb.**

- **SAND**: Only 1 ADD-quality pool (UniV3 0.30%). QSV2 partner has 79.6% impact at $500 — too thin for reliable arb.
- **SOL**: All pools DEAD or near-dead on Polygon. Wormhole-wrapped SOL has negligible liquidity.
- **CRV**: 3 DEX families (UniV3, QSV2, SushiV2) for USDC.e, but all THIN. No ADD-quality pools.

**Key insight**: Mempool swap volume ≠ arbable cross-DEX spreads. A token can have high swap volume routed through a single DEX (or aggregator multi-hop) with no second pool deep enough to arb against.

### THIN Pool Analysis

88 THIN pools total, only 5 with <10% impact at $500. Best candidate: WMATIC/USDT SushiSwapV2 (adds 3rd DEX family). Economics at $100 trades: ~$0.35 net per opportunity at 1% gap. Deferred pending more adaptive infrastructure.

---

## Strategic Conclusions

1. **Expanding quote tokens for existing blue-chip pairs** (USDT done, WETH next via Phase D) is higher value than adding new token symbols with thin liquidity.
2. **Phase D (WETH-quoted pairs)** is the next major expansion: WBTC/WETH (716 swaps), WMATIC/WETH (411), AAVE/WETH (376), LINK/WETH (286). Requires 18-decimal quote token handling in detector.
3. **USDC.e/USDC native stablecoin arb** (406 swaps) is a near-zero-risk opportunity — both tokens are already quote tokens.
4. The long-tail strategy works better as "maximize DEX × fee-tier × quote-token coverage for liquid tokens" rather than "add obscure tokens."

---

## Current State

- **Branch**: `feature/alloy-migration` at commit `a1ecac0`
- **Bot**: Running in tmux `dexarb-observe` (DRY RUN + observe mode)
- **Pool count**: 50 (37 V3 + 13 V2) across 11 pairs, 3 quote tokens
- **Tests**: 77/77 passing
- **Build**: Clean (warnings only, no errors)

## Files Modified/Created

### Modified
- `src/rust-bot/src/types.rs` — USDT quote token field + is_quote_token()
- `src/rust-bot/src/config.rs` — USDT env var loading
- `src/rust-bot/src/arbitrage/detector.rs` — test config, adaptive sizing
- `src/rust-bot/src/arbitrage/executor.rs` — per-opportunity min_profit
- `src/rust-bot/src/filters/whitelist.rs` — max_trade_size_for()
- `src/rust-bot/src/main.rs` — hybrid pipeline integration
- `src/rust-bot/src/mempool/mod.rs` — exports
- `src/rust-bot/src/mempool/monitor.rs` — execution pipeline
- `src/rust-bot/src/mempool/simulator.rs` — USDT in PairLookup
- `src/rust-bot/src/mempool/types.rs` — HybridCache, CachedOpportunity
- `config/polygon/pools_whitelist.json` — v1.8 (10 USDT pools, address fixes)

### Created
- `scripts/pool_scanner.py` — Factory-based pool discovery
- `scripts/depth_assessment.py` — Quoter-based depth analysis
- `src/rust-bot/src/bin/backfill_events.rs` — Historical event backfill utility
- `data/polygon/pool_scan_results.csv` — 258 discovered pools
- `data/polygon/depth_assessment.csv` — Impact analysis for 149 pools

## Next Steps

1. **Monitor observe-mode output** — Let bot run overnight, analyze USDT opportunity frequency
2. **Phase D planning** — WETH as quote token (18-decimal detector generalization)
3. **USDC.e/USDC native arb** — Pure stablecoin pair, near-zero risk
4. **Go live decision** — Switch to LIVE_MODE=true once USDT opportunities are validated in dry run
