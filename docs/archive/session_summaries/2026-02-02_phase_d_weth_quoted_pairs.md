# Session Summary: Phase D — WETH-Quoted Pairs & Dynamic Decimal Handling

**Date:** 2026-02-02 (late evening session)
**Branch:** `feature/alloy-migration` (uncommitted changes)
**Previous session:** `2026-02-02_usdt_expansion_and_pool_research.md`

---

## Objective

Add WETH (18 decimals) as the 4th quote token, fix all hardcoded `1e6` assumptions in the codebase, pre-compute `min_profit_raw` to eliminate USD→token conversion from the executor hot path, and prevent `u64` overflow at 18-decimal trade sizes.

---

## Commits This Session

No commits made — all changes are uncommitted on `feature/alloy-migration`. Ready to commit.

---

## Work Completed

### 1. Dynamic Decimal Handling (Core Fix)

Replaced 4 hardcoded `1e6` locations that assumed 6-decimal quote tokens:

| # | File | What | Fix |
|---|------|------|-----|
| 1 | detector.rs | `min_liquidity` calculation | `(effective_trade_size / quote_usd_price * 10^quote_decimals) as u128` |
| 2 | detector.rs | `trade_size` calculation | Same dynamic approach, `U256::from(... as u128)` |
| 3 | executor.rs ~L447 | Block-reactive `min_profit_raw` | Read pre-computed `opportunity.min_profit_raw` with `1e6` fallback |
| 4 | executor.rs ~L767 | Mempool `min_profit_raw` | Same backwards-compat pattern |

**Stablecoin regression safety:** For 6-decimal stablecoins, `trade_size / 1.0 * 1e6` = `trade_size * 1e6` — identical output to old hardcoded path.

### 2. Pre-Computed min_profit_raw

Instead of converting USD→token in the executor's hot path, the **detector pre-computes** `min_profit_raw` and stores it on the `ArbitrageOpportunity` struct:

```
min_profit_raw = (scaled_min_profit_usd / quote_usd_price) * 10^quote_decimals
```

Executor reads it directly. Falls back to legacy `* 1e6` path when `min_profit_raw == U256::ZERO` (backwards compatibility for any code path that doesn't set it).

### 3. u64 Overflow Prevention

`$500 * 1e18 = 5e20` exceeds `u64::MAX` (1.84e19). After dividing by `weth_price_usd` the result fits, but all trade_size and min_profit_raw computations now use `as u128` (U256 accepts u128) for safety.

Changed in: detector.rs (2 locations), main.rs (1 location).

### 4. WETH Quote Token Integration

**types.rs:**
- Added `quote_token_address_weth: Option<Address>` and `weth_price_usd: f64` to BotConfig
- Added `quote_token_usd_price(&self, addr) -> f64` helper (returns `weth_price_usd` for WETH, `1.0` for stablecoins)
- Updated `is_quote_token()` to include WETH
- Added `min_profit_raw: U256` field to `ArbitrageOpportunity` (default `U256::ZERO`)

**config.rs:** Load `QUOTE_TOKEN_ADDRESS_WETH` and `WETH_PRICE_USD` (default 3300.0) env vars.

**detector.rs:**
- Added `quote_decimals: u8` to `UnifiedPool` struct
- Compute `q_decimals` from pool token decimals during V3 and V2 pool collection
- Dynamic decimal conversion in opportunity detection loop
- Set `min_profit_raw` on all generated opportunities

**executor.rs:** Both block-reactive and mempool paths read `opportunity.min_profit_raw` with backwards-compat `1e6` fallback.

**main.rs:** Mempool opportunity builder computes `min_profit_raw` using `config.quote_token_usd_price()` + dynamic decimals.

**simulator.rs:** Added WETH to PairLookup quote_tokens.

### 5. WETH Price Oracle Design

Static `weth_price_usd` from env var (default 3300.0). A 10% WETH move changes min_profit by ~$0.01 — irrelevant for a safety threshold. No per-trade conversion in executor hot path. User explicitly preferred this over dynamic lookup to avoid adding latency to ms-critical processing.

### 6. Pool Discovery & Whitelist v1.9

**pool_scanner.py:** Added WETH to `QUOTE_TOKENS`. Re-ran: 352 pools discovered (was 258).

**depth_assessment.py:** Replaced hardcoded `QUOTE_DECIMALS = 6` with dynamic system:
```python
QUOTE_TOKEN_INFO = {
    "USDC.e":      {"decimals": 6,  "usd_price": 1.0},
    "USDC native": {"decimals": 6,  "usd_price": 1.0},
    "USDT":        {"decimals": 6,  "usd_price": 1.0},
    "WETH":        {"decimals": 18, "usd_price": 3300.0},
}
```
Re-ran: 31 ADD, 113 THIN, 64 DEAD (208 pools assessed).

**Whitelist v1.9** — 16 new WETH-quoted pools added (all factory-derived):

| Pair | Pools | DEX Families |
|------|-------|-------------|
| WBTC/WETH | 4 | UniV3(500), UniV3(3000), QSV2, SushiV2 |
| WMATIC/WETH | 4 | UniV3(500), UniV3(3000), QSV2, SushiV2 |
| AAVE/WETH | 4 | UniV3(500), UniV3(3000), QSV2, SushiV2 |
| LINK/WETH | 4 | UniV3(500), UniV3(3000), QSV2, SushiV2 |

Total: 66 pools (45 V3 + 21 V2) across 15 pairs, 4 quote tokens.

### 7. .env.polygon Update

Added:
```
QUOTE_TOKEN_ADDRESS_WETH=0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619
WETH_PRICE_USD=3300
```
Added 4 WETH-quoted pairs to TRADING_PAIRS: WBTC/WETH, WMATIC/WETH, AAVE/WETH, LINK/WETH (15 total).

### 8. Build & Test

- `cargo check`: passed (no errors)
- `cargo test`: 77/77 passing
- Release binary built successfully

### 9. Deploy in Observe Mode

Stopped old observe bot, deployed new binary in tmux `dexarb-observe`:
- DRY_RUN mode (no real transactions)
- MEMPOOL_MONITOR=observe (logging pending swaps)
- 66 pools synced (45 V3 + 21 V2)
- WETH pools showing reasonable prices: WBTC/WETH ~33.44, WMATIC/WETH ~0.0000486, AAVE/WETH ~18.26, LINK/WETH ~0.00419

### 10. Housekeeping

- **Archived** `docs/node-sync-deadtime-prep-guide.md` → `docs/archive/`
- **Updated** `docs/next_steps.md` — reflects Phase D completion, added alerting system item, USDC.e/USDC native arb, per-route tracking, observation bot status

---

## Key Design Decisions

1. **Static WETH price vs. dynamic oracle** — Static env var chosen over per-trade RPC lookup. User's rationale: "I'm sensitive to decisions which slow down ms processing." A 10% price move changes min_profit by $0.01, irrelevant for safety thresholds.

2. **Pre-computed min_profit_raw vs. executor conversion** — Detector pre-computes and stores on opportunity struct. Executor reads directly. Zero-value fallback preserves backwards compatibility for any code path that doesn't set it.

3. **u128 vs. u64 for intermediate casts** — u128 prevents overflow at 18-decimal trade sizes ($500 * 1e18 = 5e20 > u64::MAX). U256 accepts u128 natively.

4. **Factory-derived addresses only** — All 16 new pool addresses come from `pool_scanner.py` (on-chain factory calls). Previous session had 6 hallucinated addresses; this session avoided that entirely.

---

## Phase B Assessment (Long-Tail Tokens — Not Viable)

Carried forward from previous session: SAND, SOL (Wormhole), CRV investigated and rejected. High mempool swap volume but only 1 DEX with real liquidity per token. Arb requires ≥2 deep pools on different DEXes.

**Key insight:** The real expansion strategy is quote-token diversification (USDT done, WETH done) for existing blue-chip tokens, not adding obscure tokens with thin liquidity.

---

## Current State

- **Branch**: `feature/alloy-migration` (uncommitted Phase D changes)
- **Bot**: Running in tmux `dexarb-observe` (DRY RUN + observe mode)
- **Pool count**: 66 (45 V3 + 21 V2) across 15 pairs, 4 quote tokens
- **Quote tokens**: USDC.e, native USDC, USDT, WETH
- **Tests**: 77/77 passing
- **Build**: Clean

## Files Modified

| File | Changes |
|------|---------|
| `src/rust-bot/src/types.rs` | BotConfig: +weth fields, +helpers. ArbitrageOpportunity: +min_profit_raw |
| `src/rust-bot/src/config.rs` | Load QUOTE_TOKEN_ADDRESS_WETH, WETH_PRICE_USD |
| `src/rust-bot/src/arbitrage/detector.rs` | UnifiedPool: +quote_decimals. Fix 3× `1e6`. Pre-compute min_profit_raw |
| `src/rust-bot/src/arbitrage/executor.rs` | Read min_profit_raw with backwards-compat fallback |
| `src/rust-bot/src/main.rs` | Mempool builder: compute min_profit_raw, u64→u128 cast |
| `src/rust-bot/src/mempool/simulator.rs` | PairLookup: add WETH |
| `src/rust-bot/.env.polygon` | +WETH env vars, +4 trading pairs (15 total) |
| `config/polygon/pools_whitelist.json` | v1.9: +16 WETH-quoted pools (66 total) |
| `scripts/depth_assessment.py` | Dynamic decimals for 18-decimal quote tokens |
| `scripts/pool_scanner.py` | Added WETH to QUOTE_TOKENS |
| `data/polygon/pool_scan_results.csv` | Regenerated: 352 pools |
| `data/polygon/depth_assessment.csv` | Regenerated: 208 pools assessed |
| `docs/next_steps.md` | Reflects Phase D completion + new items |
| `docs/node-sync-deadtime-prep-guide.md` | → archived to docs/archive/ |

## Next Steps

1. **Commit Phase D changes** — Merge `feature/alloy-migration` → `main`
2. **Monitor WETH observe output** — Analyze opportunity frequency for WETH-quoted pairs
3. **Order Hetzner server** — Eliminates 250ms Alchemy round-trip (primary bottleneck)
4. **USDC.e/USDC native stablecoin arb** — 406 swaps/day, near-zero risk
5. **IPC transport** — When local Bor node is available
6. **Hybrid pipeline integration** — Wire mempool cache into block-reactive execution
