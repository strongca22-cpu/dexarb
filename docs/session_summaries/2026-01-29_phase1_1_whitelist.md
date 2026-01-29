# Session Summary: Phase 1.1 Static Whitelist/Blacklist (2026-01-29)

## Overview

Implemented Phase 1.1 of the optimization plan: a static whitelist/blacklist system for V3 pool filtering. Extracted real pool addresses from the running data collector's JSON, created a config file, built a Rust filter module, and integrated it into the opportunity detector.

## Context

- **Preceding work**: Answered architecture questionnaire (Q1-Q15), reviewed full `phase_1_2_optimization_plan.md` (~2900 lines)
- **Trigger**: User said "Proceed with phase 1.1 and pull the good pool addresses"
- **Process state at start**: Data collector RUNNING (10s poll), paper trading RUNNING, live bot STOPPED

## Key Changes

### 1. Pool Whitelist Config (`config/pools_whitelist.json`) — NEW

Extracted all 18 real V3 pool addresses from `data/pool_state_phase1.json` (live data collector output). Categorized into:

| Category | Count | Criteria |
|----------|-------|----------|
| **Whitelisted** | 16 | 0.05% + 0.30% for all 7 pairs, plus 0.01% for stablecoin pairs (USDT/USDC, DAI/USDC) |
| **Blacklisted** | 1 pool + 1 tier | WETH/USDC 0.01% (caused $3.35 loss), entire 1.00% tier |
| **Observation** | 1 | WMATIC/USDC 0.01% (needs monitoring before promotion) |

Per-tier liquidity thresholds:
- 0.01% (100): min 10B (stablecoins only)
- 0.05% (500): min 5B
- 0.30% (3000): min 3B
- 1.00% (10000): blacklisted entirely

Enforcement mode: **strict** (only whitelisted pools participate in detection).

### 2. Filter Module (`src/filters/`) — NEW

**`src/filters/mod.rs`**: Module declaration, re-exports `PoolWhitelist` and `WhitelistFilter`.

**`src/filters/whitelist.rs`** (~340 lines):
- `PoolWhitelist`: Serde structs for JSON deserialization
- `WhitelistFilter`: Precomputed `HashSet`-based filter
  - `load(path: &str) -> Result<Self>`: Load from JSON
  - `from_config(raw: PoolWhitelist) -> Self`: Build lookup sets
  - `is_pool_allowed(&self, address: &Address, fee_tier: u32, pair: &str) -> bool`: O(1) validation — checks tier blacklist, pool blacklist, pair blacklist, whitelist enforcement
  - `min_liquidity_for(&self, address: &Address, fee_tier: u32) -> u128`: Per-pool override > per-tier default > global default
- 6 unit tests: tier blacklist, pool blacklist, whitelisted allowed, strict rejects unknown, min liquidity override, min liquidity default

### 3. Integration into Detector (`src/arbitrage/detector.rs`) — MODIFIED

- `OpportunityDetector` constructor loads `WhitelistFilter` from config path (falls back to permissive defaults if no path)
- `check_pair_unified()` pool loop now calls:
  - `self.whitelist.is_pool_allowed()` — supersedes old hardcoded `fee >= 10000` check
  - `self.whitelist.min_liquidity_for()` — supersedes old hardcoded `liquidity < 1000` check

### 4. Config Changes

- `src/types.rs`: Added `whitelist_file: Option<String>` to `BotConfig`
- `src/config.rs`: Parse `WHITELIST_FILE` from environment
- `src/lib.rs`: Added `pub mod filters;`
- `.env` and `.env.live`: Added `WHITELIST_FILE=/home/botuser/bots/dexarb/config/pools_whitelist.json`

## Build & Test Results

- `cargo build --release`: SUCCESS
- `cargo test`: 8 tests passing (6 whitelist + 2 detector), 0 failures

## Errors Encountered & Fixed

1. **`no 'WhitelistFilter' in 'filters'`** — `filters/mod.rs` only re-exported `PoolWhitelist`. Fixed by adding `WhitelistFilter` to the `pub use`.
2. **`missing field 'pool_state_file'`** — Test helper `create_test_config()` was missing the `pool_state_file` field added in a prior session. Fixed by adding `pool_state_file: None`.

## Files Modified/Created

| File | Status | Change |
|------|--------|--------|
| `config/pools_whitelist.json` | NEW | 16 whitelisted pools, 1 blacklisted, 1 observation |
| `src/rust-bot/src/filters/mod.rs` | NEW | Module declaration |
| `src/rust-bot/src/filters/whitelist.rs` | NEW | WhitelistFilter impl + 6 tests |
| `src/rust-bot/src/lib.rs` | MODIFIED | Added `pub mod filters` |
| `src/rust-bot/src/types.rs` | MODIFIED | Added `whitelist_file` field |
| `src/rust-bot/src/config.rs` | MODIFIED | Parse `WHITELIST_FILE` env var |
| `src/rust-bot/src/arbitrage/detector.rs` | MODIFIED | Integrated WhitelistFilter |
| `src/rust-bot/.env` | MODIFIED | Added `WHITELIST_FILE` path |
| `src/rust-bot/.env.live` | MODIFIED | Added `WHITELIST_FILE` path |

## Process State at End

- **Data collector**: RUNNING in `dexarb-phase1:0` (10s poll)
- **Paper trading**: RUNNING in `dexarb-phase1:1`
- **Live bot**: STOPPED (new binary built with Phase 1.1 but NOT restarted)
- **Live wallet**: 160.00 USDC, ~7.73 MATIC
- **Backup wallet**: 356.70 USDC

## Whitelist Verifier Script (`scripts/verify_whitelist.py`) — NEW

Created a Python verification script that checks all whitelist/blacklist/observation pools on-chain:

### 5-Check Verification (per pool)
1. **Exists** — `eth_getCode` returns bytecode
2. **slot0** — `sqrtPriceX96 > 0`
3. **Liquidity** — meets whitelist min_liquidity threshold
4. **Fee match** — on-chain fee == whitelist fee_tier
5. **Quote** — `quoteExactInputSingle` with $1 USDC returns > 0

### Quote Depth Matrix
Runs quotes at **$1, $10, $100, $1000, $5000** for every pool. Shows PASS/FAIL grid + estimated USD received + price impact at max size vs $1 baseline.

### Blacklist Verification
Compares $1 vs $140 quotes to measure price impact. >5% impact = confirmed still problematic.

### Results (2026-01-29)
- 16/16 whitelisted pools: **PASS**
- 1/1 blacklisted pool (WETH/USDC 0.01%): **Confirmed dead** — 76.4% impact at $140
- 1/1 observation pool (WMATIC/USDC 0.01%): **PASS** basic checks

### Depth Findings (from matrix)

**Recommended Whitelist — 12 pools (solid at $140 trade size):**

| Pool | Fee | $1 | $100 | $1K | $5K | Impact@$5K |
|------|-----|----|------|-----|-----|------------|
| WETH/USDC | 0.05% | $1.00 | $99.99 | $999 | $4987 | 0.3% |
| WETH/USDC | 0.30% | $1.00 | $99.96 | $996 | $4909 | 1.8% |
| WMATIC/USDC | 0.05% | $1.00 | $99.92 | $992 | $4808 | 3.8% |
| WBTC/USDC | 0.05% | $1.00 | $100.04 | $1000 | $4996 | 0.1% |
| WBTC/USDC | 0.30% | $1.00 | $99.82 | $981 | $4725 | 5.5% |
| USDT/USDC | 0.01% | $1.00 | $100.00 | $1000 | $4999 | 0.0% |
| USDT/USDC | 0.05% | $1.00 | $100.00 | $1000 | $4996 | 0.1% |
| USDT/USDC | 0.30% | $1.00 | $100.00 | $1000 | $4993 | 0.1% |
| DAI/USDC | 0.01% | $1.00 | $100.00 | $1000 | $4999 | 0.0% |
| DAI/USDC | 0.05% | $1.00 | $100.00 | $1000 | $4999 | 0.0% |
| LINK/USDC | 0.30% | $1.00 | $99.99 | $999 | $4971 | 0.6% |
| UNI/USDC | 0.30% | $1.00 | $99.20 | $932 | $4157 | 16.8% |

**Recommended Blacklist — 4 pools (dead or too thin at $140):**

| Pool | Fee | $1 | $100 | $1K | $5K | Impact@$5K | Reason |
|------|-----|----|------|-----|-----|------------|--------|
| UNI/USDC | 0.05% | $1.00 | $1.33 | $1.33 | $1.33 | 100% | Dead — $10 in → $1.29 out |
| DAI/USDC | 0.30% | $1.00 | $44 | $44 | $44 | 99.1% | Exhausted — maxes at ~$44 |
| LINK/USDC | 0.05% | $1.00 | $97.75 | $808 | $1344 | 73.1% | Thin — impact eats spread at $140 |
| WMATIC/USDC | 0.30% | $1.00 | $91.73 | $801 | $2323 | 53.5% | Thin — 8%+ loss at $100 |

**Already Blacklisted:**

| Pool | Fee | $1 | $100 | $5K | Impact@$5K | Status |
|------|-----|----|------|-----|------------|--------|
| WETH/USDC | 0.01% | $1.00 | $30 | $41 | 99.2% | Blacklisted (caused $3.35 loss) |

**Observation:**

| Pool | Fee | $1 | $100 | $5K | Impact@$5K | Notes |
|------|-----|----|------|-----|------------|-------|
| WMATIC/USDC | 0.01% | $1.00 | $60 | $272 | 94.6% | Works at $100, thin at scale |

### Usage
```bash
python3 scripts/verify_whitelist.py                   # Full verification + matrix
python3 scripts/verify_whitelist.py --update          # + update last_verified timestamps
python3 scripts/verify_whitelist.py --verbose         # Show raw hex data
```

## What's Next

- **Deploy**: Restart live bot with new binary to activate whitelist filtering
- **Consider blacklisting**: UNI/USDC 0.05% (effectively dead) and DAI/USDC 0.30% (maxes at ~$44)
- **Phase 1.2**: Enhanced Liquidity Thresholds (tick-range-aware liquidity checks)
- **Phase 1.3**: Pool Quality Scoring (dynamic scoring based on real-time metrics)
- **Phase 2**: Multicall Batching (reduce RPC calls further)

## Git Status

Changes are compiled and tested but not yet committed.
