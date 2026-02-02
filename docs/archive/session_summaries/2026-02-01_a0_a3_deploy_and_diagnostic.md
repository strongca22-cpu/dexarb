# Session Summary: 2026-02-01 — A0-A3 Deploy + Diagnostic Results

## Objective
Deploy four latency optimizations (A0-A3), measure their impact on the 99.1% estimateGas revert rate, and determine whether the bottleneck is speed (fixable) or mempool-based competition (requires architecture change).

## What Was Built

### A0: Gas Priority Bump
- `maxPriorityFeePerGas = 5000 gwei`, `maxFeePerGas = baseFee + 5000 gwei`
- EIP-1559 fields pre-set on tx before `fill_transaction()`
- Both private RPC (sign + send_raw) and public WS paths updated
- **File:** `executor.rs` — `execute_atomic()` gas pre-setting block

### A1: Cache Base Fee from Block Header
- `executor.set_base_fee(block.base_fee_per_gas)` called in main.rs on each new block
- Eliminates `get_gas_price()` RPC call (~50ms savings)
- **Files:** `executor.rs` (new field + method), `main.rs` (wiring)

### A2: Pre-cache Nonce
- `AtomicU64` nonce field, initialized from `get_transaction_count` on first use
- Incremented after each successful send
- Nonce pre-set on tx before `fill_transaction()` (eliminates nonce lookup)
- **File:** `executor.rs` — `cached_nonce`, `nonce_initialized` fields

### A3: Event-Driven Pool Sync
- Single `eth_getLogs(block, block, pool_addresses, [Swap_topic, Sync_topic])` per block
- V3 Swap event parsing: extracts sqrtPriceX96, liquidity, tick from 160-byte data
- V2 Sync event parsing: extracts reserve0, reserve1 from 64-byte data
- Poll-based fallback when eth_getLogs fails or EVENT_SYNC=false
- **Files:** `main.rs` (event sync setup + block loop), `.env.polygon` (EVENT_SYNC=true)
- **Savings:** ~350ms per block, ~21M CU/month freed

### Bug Fixes
- `executor.rs:689`: `gas_price` → `max_fee` (undefined variable after A0 rename)
- `executor.rs:565-580`: PendingTransaction lifetime fix (save result in local var)
- `bot_watch.sh`: Log path `data/logs/` → `data/polygon/logs/`
- `bot_status_discord.sh`: Dynamic log resolution searches both polygon and legacy paths

### Build & Deploy
- `cargo check` clean, `cargo build --release` clean
- Deployed to `livebot_polygon` tmux session
- Event sync confirmed working: "Event sync: 1 V3 + 0 V2 pools updated"
- All 3 tmux sessions running: livebot_polygon, botwatch, botstatus

## Diagnostic Results (2h 45m session)

### Execution Funnel

| Stage | Count | Rate |
|-------|------:|------|
| Opportunities detected | 233 | 84.8/hr |
| Cooldown-suppressed | 187 | — |
| Execution attempts (TRY) | 34 | 12.4/hr |
| Private mempool sends | 34 | 100% |
| estimateGas reverts | 33 | **97.1%** |
| On-chain submissions | 0 | 0% |
| Successful trades | 0 | 0% |

### Key Findings

1. **Revert rate: 99.1% → 97.1%** — marginal improvement, not the breakthrough needed
2. **Fill latency: 11ms median** — execution path is fast, state is stale
3. **WETH/USDC dominates:** 105 opps (45%), 19 attempts, 1 passed estimateGas (didn't submit)
4. **DAI/USDC phantom spread detected:** QuickSwapV2→UniswapV3_001, $0.24 × 10 with 0 variance
5. **Cooldown working well:** 187 redundant calls saved
6. **Opportunity clustering:** median 2.0s gap, 210 burst pairs (≤5s), confirming transient nature
7. **Price spreads are real:** WETH/USDC SushiV3 vs UniV3 has >0.20% spread 41.4% of the time

### Verdict

**Speed was not the bottleneck.** Competitors operate from the mempool. A4 (mempool monitoring) is the necessary architectural change.

## Decision

Proceed with A4 in phased approach:
1. **Phase 1 (Observation):** Subscribe to pending V3 txs via Alchemy, log decoded swaps, measure visibility + lead time
2. **Phase 2 (Simulation):** Compute post-swap pool state from calldata
3. **Phase 3 (Execution):** Submit backrun txs targeting simulated state

See `docs/a4_mempool_monitor_plan.md` for full plan.

## Viability Assessment

- **Capital:** Not the bottleneck ($25k+ available, spreads are $0.14-$0.95 on $500 trades)
- **Latency:** Not the bottleneck (5ms RTT, 11ms fill latency)
- **Compute:** Not the bottleneck (Rust on 1 vCPU handles everything in microseconds)
- **Information advantage:** THE bottleneck — need to see pending txs before block confirmation
- **Cross-chain portability:** A4 architecture is 100% reusable across all EVM chains (same ABIs, same AMM math)
- **Infrastructure cost:** Fits Alchemy free tier (V3 monitoring ~3.5M CU/month). Own Bor node ~$80-100/mo if needed.

## Files Modified

| File | Changes |
|------|---------|
| `src/rust-bot/src/arbitrage/executor.rs` | A0 (gas pre-set), A1 (cached_base_fee), A2 (AtomicU64 nonce), lifetime fix, gas_price→max_fee fix |
| `src/rust-bot/src/main.rs` | A1 wiring (set_base_fee), A3 (event sync setup + block loop) |
| `src/rust-bot/src/types.rs` | private_rpc_url field |
| `src/rust-bot/src/config.rs` | PRIVATE_RPC_URL loader |
| `src/rust-bot/src/arbitrage/detector.rs` | Test config update |
| `src/rust-bot/.env.polygon` | EVENT_SYNC=true |
| `scripts/bot_watch.sh` | Log path fix |
| `scripts/bot_status_discord.sh` | Dynamic log resolution |
| `docs/next_steps.md` | A0-A3 completion, timing budget, pipeline diagrams |

## Git
- Commit `8a0646f`: `feat: A0-A3 latency optimizations, private RPC, session analyzer`
- 11 files changed, 1701 insertions, 182 deletions

---

*Session date: 2026-02-01*
