# Session: A4 Phase 1 — Mempool Monitor (Observation Mode)

**Date:** 2026-02-01
**Duration:** ~1 session
**Predecessor:** A0-A3 diagnostic (97.1% revert rate confirmed mempool-based competition)

---

## Objective

Build Phase 1 of the mempool monitor: subscribe to pending DEX swap transactions, decode calldata, log to CSV, and cross-reference against confirmed blocks to measure Alchemy's mempool visibility and lead time on Polygon.

## What Was Built

### New Module: `src/rust-bot/src/mempool/`

| File | Purpose | LOC |
|------|---------|-----|
| `types.rs` | `PendingSwap`, `DecodedSwap`, `MempoolMode`, `ConfirmationTracker` | ~130 |
| `decoder.rs` | Calldata decoder for 11 swap selectors (V3/Algebra/V2) | ~280 |
| `monitor.rs` | WS subscription loop, CSV logging, block cross-reference | ~230 |
| `mod.rs` | Module exports | ~20 |
| **Total** | | **~660** |

### Modified Files

| File | Change |
|------|--------|
| `lib.rs` | Added `pub mod mempool;` |
| `types.rs` | Added `mempool_monitor_mode: String` to `BotConfig` |
| `config.rs` | Loads `MEMPOOL_MONITOR` env var (default: "off") |
| `main.rs` | Spawns mempool monitor as async task when mode is observe/execute |
| `.env.polygon` | Added `MEMPOOL_MONITOR=observe` |
| `detector.rs` | Added new field to test BotConfig initializer |

### Architecture Decisions

1. **Self-contained async task** — spawned via `tokio::spawn`, creates its own WS connections (separate from the block loop). No shared state with the arb pipeline.

2. **Dual WS connections** — one for `alchemy_pendingTransactions` subscription, one for RPC calls (`get_block` for cross-reference). Avoids borrow conflicts between subscription stream and RPC methods.

3. **V3 routers only for Phase 1** — ~2 txs/min on Polygon V3 routers, ~3.5M CU/month for full tx objects. V2 routers are 99% of volume but cost 342M CU/month at full objects — deferred.

4. **Alchemy filtered subscription** — `alchemy_pendingTransactions` with `toAddress` filter on 3 V3 router addresses (Uniswap V3, SushiSwap V3, QuickSwap V3 Algebra). Returns full `Transaction` objects including calldata.

5. **Auto-reconnect** — up to 50 retries with 5s delay, mirrors main loop pattern. Inner/outer loop separation for WS lifecycle.

6. **No new crate dependencies** — ethers-rs has built-in ABI decoding (`ethers::abi::decode`). CSV written with `std::io::Write`. No `csv` crate needed.

### Calldata Decoder Coverage (11 selectors)

**V3 SwapRouter:**
- `exactInputSingle` (0x414bf389) — single-hop, exact input
- `exactInput` (0xc04b8d59) — multi-hop, packed path decoding
- `exactOutputSingle` (0xdb3e2198) — single-hop, exact output
- `exactOutput` (0xf28c0498) — multi-hop reversed path
- `multicall(uint256,bytes[])` (0x5ae401dc) — recursive inner call decoding
- `multicall(bytes[])` (0xac9650d8) — recursive inner call decoding

**Algebra (QuickSwap V3):**
- `exactInputSingle` (0xbc651188) — no fee field, dynamic fees

**V2 Router:**
- `swapExactTokensForTokens` (0x38ed1739)
- `swapTokensForExactTokens` (0x8803dbee)
- `swapExactETHForTokens` (0x7ff36ab5)
- `swapExactTokensForETH` (0x18cbafe5)

### Output Format

**CSV log:** `data/{chain}/mempool/pending_swaps_YYYYMMDD.csv`

Columns: `timestamp_utc, tx_hash, router, router_name, function, token_in, token_out, amount_in, amount_out_min, fee_tier, gas_price_gwei, max_priority_fee_gwei`

**Tracing output (per-tx):**
- `PENDING:` — decoded swap details (router, function, tokens, amount, fee, gas)
- `CONFIRMED:` — matched tx with lead time in ms and block number
- `MEMPOOL STATS` — periodic (every ~10 min): confirmation rate %, median/mean lead time, tracking count

### Cross-Reference Tracking

`ConfirmationTracker` maintains an in-memory `HashMap<TxHash, (Instant, String)>`:
- Pending swaps added on decode
- Every 6 seconds: fetch new blocks, check tx hash list against tracker
- Compute lead time = `Instant::now() - seen_at` for each match
- Cleanup entries >2 min old (dropped from mempool)
- Stats: confirmation_rate(), median_lead_time_ms(), mean_lead_time_ms()

## Build Status

- `cargo check --all-targets`: clean (no new warnings)
- `cargo test --lib`: 61/61 pass (3 new mempool tests + 58 existing)
- New tests: `test_selector_hex`, `test_selector_hex_short`, `test_decode_v3_path`

## Risk Assessment

| Risk | Level | Mitigation |
|------|-------|------------|
| CU budget | Low | V3 only = ~3.5M CU/month. Total with A3: ~14.2M/30M free |
| WS connection failure | Low | Auto-reconnect (50 retries, 5s delay) |
| Calldata decode errors | Low | Returns None for unknown selectors, logs undecoded txs |
| Alchemy subscription unsupported | Low | `alchemy_pendingTransactions` verified on Polygon free tier |
| Observation mode affects main bot | None | Separate async task, own WS connections, no shared state |

## Live vs Dry Run Decision

**Live run is the only meaningful option.** The mempool monitor is pure observation:
- Does not submit transactions
- Does not spend gas
- Does not touch funds
- Does not affect the main arb pipeline

You need real mempool data to answer the Phase 1 gate questions. There's no meaningful "dry run" for passive observation.

## Decision Gate (after 24h+ live observation)

| Metric | Threshold | Action |
|--------|-----------|--------|
| V3 swap visibility | >30% | Proceed to Phase 2 (AMM simulation) |
| V3 swap visibility | <20% | Evaluate own Bor node ($80-100/mo) |
| Median lead time | >500ms | Sufficient for backrun submission |
| Median lead time | <200ms | May need co-located node |

## Next Steps

1. **Build release binary** and deploy alongside live bot
2. **Run 24h+** observation on Polygon mainnet
3. **Analyze CSV data** — visibility rate, lead time distribution, peak hours
4. **Decision gate** — proceed to Phase 2 or evaluate infrastructure changes

---

*Files created: 4 new (mempool module). Files modified: 6. Tests: 3 new, 61 total passing. No new dependencies.*
