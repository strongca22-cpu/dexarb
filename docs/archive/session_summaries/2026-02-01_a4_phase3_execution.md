# Session Summary: A4 Phase 3 — Mempool Execution Pipeline

**Date:** 2026-02-01
**Duration:** ~3 hours (implementation + deploy)
**Status:** DEPLOYED to Polygon LIVE

---

## Objective

Build and deploy the mempool execution pipeline: when the Phase 2 simulator detects a cross-DEX opportunity (SIM OPP) from a pending transaction, submit a backrun transaction targeting the post-swap pool state. Skip estimateGas for speed. Use dynamic gas pricing. Keep the block-reactive pipeline running alongside.

---

## Architecture: mpsc Channel from Monitor to Main Loop

```
Monitor (tokio::spawn)     mpsc::channel(8)     Main Loop (select!)
  SIM OPP detected  ---------> MempoolSignal  ---------> execute_from_mempool()
                                                          (skip estimateGas,
                                                           dynamic gas,
                                                           fixed 500K gas limit)
```

- **Why mpsc:** Executor takes `&mut self` and is owned by `main()`. Channel serializes execution, prevents nonce conflicts, provides backpressure via `try_send()`.
- **tokio::select!**: Main loop listens to both block events AND mempool signals via `LoopEvent` enum. Block-reactive pipeline continues uninterrupted.
- **Stale signal guard:** Signals older than 10s are dropped (trigger likely already confirmed).

---

## Changes Made (7 files)

### 1. `src/mempool/types.rs`
- Added `MempoolSignal` struct: opportunity, trigger gas price, EIP-1559 priority fee, timestamp

### 2. `src/types.rs`
- Added 4 `BotConfig` fields: `mempool_min_profit_usd` (0.05), `mempool_gas_limit` (500K), `mempool_min_priority_gwei` (1000), `mempool_gas_profit_cap` (0.50)

### 3. `src/config.rs`
- Load 4 new env vars with defaults

### 4. `src/arbitrage/executor.rs` (~270 lines added)
- **`execute_from_mempool()`**: Structurally identical to `execute_atomic()` with 3 changes:
  1. Skip estimateGas: set gas to fixed 500K, sign directly (no `fill_transaction()`)
  2. Dynamic gas pricing via `calculate_mempool_gas()`
  3. Lower minProfit ($0.05 vs $0.10)
- **`calculate_mempool_gas()`**: `min(profit_cap, max(trigger * 1.05, floor_gwei))`
  - Profit cap: never spend >50% of expected profit on gas
  - Floor: 1000 gwei minimum (competitive on Polygon)
  - Trigger match: 5% above the trigger tx's priority fee

### 5. `src/mempool/monitor.rs`
- Refactored `run_observation()` into `run_observation_impl()` accepting `Option<Sender<MempoolSignal>>`
- Added `run_execution()` wrapper that passes `Some(signal_tx)`
- Signal emission: fires when `est_profit >= min_profit` AND `spread >= 0.01%`
- Uses `try_send()` — drops signals when channel full (backpressure)

### 6. `src/main.rs` (~150 lines added)
- Created `mpsc::channel(8)` when `MEMPOOL_MONITOR=execute`
- `LoopEvent` enum: `Block`, `Mempool`, `StreamEnd`, `Timeout`
- `tokio::select!` replaces simple timeout in inner loop
- Mempool signal handler: staleness check, `build_mempool_arb_opportunity()`, execute, CSV log, route cooldown
- `build_mempool_arb_opportunity()`: converts `SimulatedOpportunity` -> `ArbitrageOpportunity` via PoolStateManager lookup
- Mempool execution CSV: `mempool_executions_YYYYMMDD.csv`

### 7. `src/mempool/mod.rs`
- Re-exports: `run_execution`, `MempoolSignal`

### Config
- `.env.polygon`: `MEMPOOL_MONITOR=execute`, 4 new vars

### Supporting
- `src/arbitrage/detector.rs`: Test fix (added 4 new BotConfig fields to `create_test_config()`)
- `scripts/bot_watch.sh`: Trigger on "Trade complete" OR "MEMPOOL SUCCESS"
- `scripts/bot_status_discord.sh`: Added mempool stats line (signals/success/fail)

---

## Verification

- **72/72 tests pass** (including existing + test config fix)
- **Clean release build**
- **Deployed to livebot_polygon tmux** with `MEMPOOL_MONITOR=execute`, `LIVE_MODE=true`
- **Startup confirmed:** "A4: Mempool monitor spawned (Phase 3 EXECUTION mode)"
- **First signals processing within 2 minutes of deploy** (1 signal, 1 fail — expected, most will revert)
- **Block-reactive pipeline continues** alongside (2 attempts in first 5 min)
- **Discord report shows mempool stats:** `Mempool: 1 signals | 0 ok | 1 fail`

---

## Safety Profile

| Guard | Mechanism |
|-------|-----------|
| Capital protection | ArbExecutor.sol atomic revert (minProfit enforced on-chain) |
| Nonce serialization | Single executor via mpsc channel |
| Stale signal drop | >10s signals discarded |
| Channel backpressure | try_send() capacity 8, drops when full |
| Profit-capped gas | Never spend >50% of expected profit |
| Route cooldown | Failed mempool trades feed into existing cooldown tracker |
| Rollback | Set `MEMPOOL_MONITOR=observe` and restart |

**Risk per failed mempool trade:** ~$0.01 gas on revert. Break-even if >5% of signals succeed.

---

## Dynamic Gas Pricing Examples

| Scenario | Trigger Priority | Est. Profit | Match (1.05x) | Profit Cap | Floor | Final |
|----------|-----------------|-------------|---------------|------------|-------|-------|
| Small opp | 2000 gwei | $0.25 | 2100 | 500 | 1000 | **1000** (floor) |
| Medium opp | 2000 gwei | $1.00 | 2100 | 2000 | 1000 | **2000** (cap) |
| Large opp | 2000 gwei | $4.00 | 2100 | 8000 | 1000 | **2100** (match) |
| High-gas trigger | 8000 gwei | $4.00 | 8400 | 8000 | 1000 | **8000** (cap) |

---

## Expected Behavior

- **Most mempool submissions will revert on-chain** (~$0.01 each). The spread is often already closed by the time our tx lands.
- **Some should succeed** if we submit before competitors close the spread (6s median lead time gives room).
- **Block-reactive pipeline continues** — dual-pipeline approach means we don't lose existing capability.
- **After 2+ hours:** Review `mempool_executions_*.csv`. If revert rate is >99% with zero successes, revisit gas pricing and timing.

---

## Key Config (`.env.polygon`)

```
MEMPOOL_MONITOR=execute
MEMPOOL_MIN_PROFIT_USD=0.05
MEMPOOL_GAS_LIMIT=500000
MEMPOOL_MIN_PRIORITY_GWEI=1000
MEMPOOL_GAS_PROFIT_CAP=0.50
```

---

## Files Modified

| File | Lines Added | Change |
|------|------------|--------|
| `src/mempool/types.rs` | ~20 | MempoolSignal struct |
| `src/types.rs` | ~15 | 4 BotConfig fields |
| `src/config.rs` | ~16 | 4 env var loaders |
| `src/arbitrage/executor.rs` | ~270 | execute_from_mempool() + calculate_mempool_gas() |
| `src/mempool/monitor.rs` | ~80 | Refactor + run_execution() + signal emission |
| `src/main.rs` | ~150 | Channel, select!, handler, CSV, build_arb_opp |
| `src/mempool/mod.rs` | ~2 | Re-exports |
| `.env.polygon` | ~5 | Config vars |
| `src/arbitrage/detector.rs` | ~4 | Test fix |
| `scripts/bot_watch.sh` | ~5 | MEMPOOL SUCCESS trigger |
| `scripts/bot_status_discord.sh` | ~20 | Mempool stats collection + display |
