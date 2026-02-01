# Next Steps - DEX Arbitrage Bot

## Status: LIVE Polygon — A0-A3 Deployed, Diagnostic Running

**Date:** 2026-02-01
**Polygon:** 23 active pools (16 V3 + 7 V2), atomic via `ArbExecutorV3` (`0x7761...`), WS block sub, private RPC (1RPC)
**Base:** 5 active V3 pools (whitelist v1.1), ArbExecutor deployed (`0x9054...`), 5hr dry-run done (25 simulated trades)
**Build:** 58/58 Rust tests, clean release build
**Mode:** WS block subscription (~2s Polygon blocks), 3 tmux sessions (livebot, botstatus, botwatch)
**A0-A3:** Deployed 2026-02-01. Event sync active, 5000 gwei priority, cached gas+nonce. Monitoring revert rate for diagnostic.

---

## Key Finding: 4-Hour Live Session Analysis (2026-01-31)

**Data source:** `scripts/analyze_bot_session.py` — rerunnable on any log.

### Execution Funnel

| Stage | Count | Rate |
|-------|------:|------|
| Opportunities detected | 686 | 170.8/hr |
| Cooldown-suppressed routes | 539 | — |
| Execution attempts (TRY #) | 113 | 28.1/hr |
| Reverted at estimateGas | 112 | **99.1% of attempts** |
| Submitted on-chain | 0 | 0% |
| Successful trades | 0 | 0% |

### Root Cause: Stale-State Execution

The bot detects spreads from **confirmed block state**. By the time `fill_transaction` (estimateGas) executes ~50ms later, the spread has already been captured by faster actors. All 112 failures returned "Too little received" — the spread no longer exists at execution time.

### The Spreads Are Real

Cross-DEX spread analysis (360K price rows) confirms persistent, genuine spreads:

| Pair | Route | Spread >0.10% | Spread >0.20% |
|------|-------|--------------|--------------|
| WETH/USDC | SushiV3_0.30% vs UniV3_0.05% | 71.8% of observations | 37.2% |
| WETH/USDC | QuickswapV3 vs UniV3_0.30% | 67.9% | 20.5% |
| WMATIC/USDC | QuickswapV3 vs UniV3_0.05% | 10.4% | 1.3% |
| WBTC/USDC | QuickswapV3 vs UniV3_0.05% | 24.3% | 3.0% |

No phantom spreads detected. Opportunities are real but uncapturable at current architecture.

### Timing Budget (Block → estimateGas Revert)

**Pre-A0-A3 (measured):**
```
T=0ms         Block arrives via WS
T=0-400ms     V3 pool sync (21 parallel RPC calls)        ← BIGGEST chunk
T=400-500ms   V2 pool sync (7 parallel RPC calls)
T=500-505ms   Opportunity scan (CPU, no RPC)
T=505-555ms   get_gas_price() (redundant RPC)              ← WASTED
T=555-705ms   fill_transaction / estimateGas                ← 99.1% learn we're too late
Total: ~700ms from block to revert.
```

**Post-A0-A3 (estimated):**
```
T=0ms         Block arrives via WS, base_fee cached (A1)
T=0-50ms      eth_getLogs: Swap+Sync events (A3)           ← was 400-500ms
T=50-55ms     Opportunity scan (CPU, no RPC)
T=55-205ms    fill_transaction (estimateGas only, A0+A2)   ← was 200ms (gas+nonce+estimate)
Total: ~200-250ms from block to attempt.
```

**Savings: ~450ms (64% reduction).** Whether this is enough depends on whether competitors react to confirmed blocks (speed issue → A3 fixes it) or backrun from mempool (latency irrelevant → A4 needed).

### Network Latency (Already Good)

| Metric | Value |
|--------|-------|
| VPS location | Kent, WA (Vultr) |
| RTT to Alchemy | **5ms** |
| Co-location benefit | ~30ms savings (4% of pipeline) |
| Verdict | **Not the bottleneck** |

---

## Two-Wallet Architecture

| Wallet | Address | Purpose | USDC.e | USDC (native) | MATIC |
|--------|---------|---------|--------|---------------|-------|
| **Live** | `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2` | Trading (at-risk) | 516.70 | 400.00 | 165.57 |
| **Backup** | `0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb` | Deep storage (manual) | 0 | 0 | 0.07 |

**Native USDC ($400) is NOT at risk:** All pools use USDC.e (`0x2791...`). ArbExecutor approval is on USDC.e only.

**Settings:** MAX_TRADE_SIZE_USD=500, MIN_PROFIT_USD=0.10, MAX_SLIPPAGE_PERCENT=0.5

**Base Wallet:** `0x48091E0ee0427A7369c7732f779a09A0988144fa` | 0.0057 ETH | Separate from Polygon for isolation

---

## Architecture

Monolithic bot: WS `subscribe_blocks()` → sync V3+V2 pools → price log → detect → atomic execute

**Current execution pipeline (after A0-A3):**
```
Block arrives (WS) → base_fee cached (A1)
  → eth_getLogs: V3 Swap + V2 Sync events (~50ms, A3)
  → Detector (reserve/tick prices) → min_profit gate ($0.10)
  → [Multicall Quoter skipped]
  → ArbExecutor.sol (fee sentinel routing: V2/Algebra/V3)
  → Pre-set: EIP-1559 gas (baseFee + 5000 gwei priority, A0), nonce (AtomicU64, A2)
  → fill_transaction (estimateGas only — gas/nonce already set)
  → Sign via WS, send_raw via 1RPC (private mempool)
  → Revert on loss (zero capital risk)
```

**Previous pipeline (pre-A0-A3):**
```
Block arrives → poll-sync 23 pools (~400ms, 21 RPC calls)
  → get_gas_price (~50ms, redundant RPC)
  → fill_transaction (estimateGas + nonce + gas = ~200ms)
  → 99.1% REVERT (too slow)
Total: ~700ms from block to revert
```

**Fee sentinel routing (ArbExecutor.sol):**
- `fee = 0` → Algebra SwapRouter (QuickSwap V3)
- `fee = 1..65535` → Standard V3 SwapRouter (Uniswap/SushiSwap V3)
- `fee = 16777215` → V2 Router (`swapExactTokensForTokens`)

**Alchemy plan:** Free tier (30M CU/month). Current poll-sync uses ~22M CU/month. Growth/Scale tiers deprecated Feb 2025 — now Free, Pay As You Go ($0.45/M CU), or Enterprise.

**RPC budget (pre-A3):** WS + ~40 sync calls/block (23 pools). ~20 calls/s burst = ~22M CU/month.
**RPC budget (current, A3 deployed):** eth_getLogs (~75 CU/block = ~3.2M CU/month) + estimateGas on attempts. ~21M CU freed for mempool monitoring.

**Key Alchemy APIs (verified on free tier 2026-02-01):**
- `eth_subscribe("logs")` — pool Swap/Sync events. Works. ~1M CU/month for 23 pools.
- `alchemy_pendingTransactions` — pending DEX txs. Works. Measured: ~200 txs/min to 6 routers (99% V2, 1% V3).
- CU cost: full tx objects ~40 CU/event (346M CU/month at 200/min = $152/mo PAYG). hashesOnly ~2.6 CU/event (23M CU/month = free). But hashesOnly lacks calldata needed for backrun decoding.
- **Caveat:** Alchemy only sees its own Bor node mempool — partial view of Polygon pending txs.

**Private RPC:** 1RPC (metadata privacy only — no ordering advantage). FastLane is dead on Polygon (pivoted to Monad 2025, Chainlink acquired Atlas IP Jan 2026, relay NXDOMAIN).

---

## Completed Phases

| Phase | Date | Summary |
|-------|------|---------|
| V3 swap routing | 01-28 | `exactInputSingle`, V3 SwapRouter |
| Critical bug fixes | 01-29 | Decimal mismatch, liquidity check, trade direction, HALT |
| Phase 1.1 whitelist | 01-29 | Strict enforcement, per-tier liquidity thresholds |
| Phase 2.1 Multicall3 | 01-29 | Batch Quoter pre-screen, 1 RPC call |
| Monolithic live bot | 01-30 | Direct RPC sync, ~1s cycle, in-memory pools |
| SushiSwap V3 integration | 01-30 | Cross-DEX DexType variants, dual-quoter |
| Atomic executor V1 | 01-30 | `ArbExecutor.sol` V1, single-tx V3↔V3 |
| QuickSwap V3 (Algebra) | 01-30 | Tri-DEX, fee=0 sentinel, 5 pools |
| WS block subscription | 01-30 | `subscribe_blocks()`, ~100ms notification |
| V2 pool assessment | 01-30 | 7 whitelist, 2 marginal, 3 dead V2 pools |
| **V2↔V3 atomic execution** | **01-31** | **V2 syncer, V2 DexType, atomic_fee(), ArbExecutorV3 deployed** |
| **Profit reporting fix** | **01-31** | **Quote token decimals instead of wei_to_usd()** |
| **Discord log fix** | **01-31** | **Dynamic log file resolution (newest livebot*.log)** |
| **Multi-chain Phase 1+2** | **01-31** | **`--chain` CLI, Base pool discovery, QuoterV2 fix, deploy, dry-run** |
| **Route cooldown** | **01-31** | **Escalating backoff (10→50→250→1250→1800 blocks)** |
| **Private RPC (1RPC)** | **01-31** | **fill via WS, send_raw via 1RPC. Metadata privacy only.** |
| **4-hour live session analysis** | **02-01** | **686 opps, 113 attempts, 0 trades. 99.1% estimateGas revert.** |
| **Alchemy API verification** | **02-01** | **Free tier: pendingTx + logs subscriptions work. 200 DEX txs/min measured. CU budget mapped.** |
| **A0: Gas priority bump** | **02-01** | **5000 gwei maxPriorityFeePerGas (was ~30 gwei default). Targets top ~30 block position.** |
| **A1: Cache base fee** | **02-01** | **base_fee from block header → executor. Eliminates get_gas_price() RPC (~50ms).** |
| **A2: Pre-cache nonce** | **02-01** | **AtomicU64 nonce tracking. Eliminates nonce lookup from fill_transaction (~50ms).** |
| **A3: Event-driven sync** | **02-01** | **eth_getLogs (V3 Swap + V2 Sync) replaces 21 per-pool RPC calls. ~50ms vs ~400ms. ENV: EVENT_SYNC=true.** |

---

## Action Plan — Priority Order

### Tier 0: Quick Wins — COMPLETED 2026-02-01

**A0: Gas Priority Bump** — DONE
- Set `maxPriorityFeePerGas = 5000 gwei`, `maxFeePerGas = baseFee + 5000 gwei` in `execute_atomic()`.
- EIP-1559 fields pre-set on tx before `fill_transaction()` (which now only calls estimateGas).
- Both private RPC path (pre-set + fill + sign + send_raw) and public WS path updated.

**A1: Cache Gas Price from Block Header** — DONE
- `executor.set_base_fee(block.base_fee_per_gas)` called in main.rs on each new block.
- Eliminates `get_gas_price()` RPC call (~50ms savings).
- Fallback: if no block arrived yet, first-time call uses `provider.get_gas_price()`.

**A2: Pre-cache Nonce** — DONE
- `AtomicU64` nonce field, initialized from `get_transaction_count` on first use.
- Incremented after each successful `send_raw_transaction` / `call.send()`.
- Nonce pre-set on tx before `fill_transaction()` (eliminates nonce lookup).

### Tier 1: Execution Architecture Redesign — A3 DEPLOYED, A4 PENDING

The 99.1% revert rate is structural. A3 is the diagnostic: if speed alone is the issue (competitors just react faster to confirmed blocks), event-driven sync cuts 400ms and should drop the revert rate. If competitors are backrunning from mempool, A3 won't help and A4 becomes the path.

**A3: Event-Driven Pool State (replace poll-sync)** — DONE
- **Implementation:** `eth_getLogs(block, block, pool_addresses, [Swap_topic, Sync_topic])` — single RPC call per block.
- **V3 Swap parsing:** Extracts `sqrtPriceX96`, `liquidity`, `tick` from event data (160 bytes). Updates `V3PoolState` via `PoolStateManager`.
- **V2 Sync parsing:** Extracts `reserve0`, `reserve1` from event data (64 bytes). Updates `PoolState`.
- **Toggle:** `EVENT_SYNC=true` in `.env.polygon`. Falls back to poll-sync if `eth_getLogs` fails.
- **Savings:** ~350ms per block (1 RPC call @ 75 CU vs ~21 calls @ ~1100 CU). Frees ~21M CU/month.
- **Note:** Used `eth_getLogs` (synchronous, deterministic) instead of `eth_subscribe("logs")` (async stream) for simplicity. All events for the current block are guaranteed present before detection runs.

**A4: Pending Mempool Monitoring (the strategic shift)**
- **What:** Subscribe to `alchemy_pendingTransactions` to detect large DEX swaps before they confirm. Compute the post-swap price. Submit arb tx targeting the future state (backrunning).
- **Why:** This is how professional MEV bots operate. They don't react to confirmed blocks — they react to pending transactions and submit arbs that land in the same block, immediately after the target swap.
- **Requires:** Available on free tier (verified 2026-02-01). Full tx objects needed for calldata decoding — at ~200 DEX txs/min, costs ~346M CU/month ($152/mo PAYG). But after A3 frees ~21M CU, V3-only monitoring (~2 txs/min) fits in free tier (~3.5M CU/month). V2 routers can use hashesOnly + selective `eth_getTransactionByHash` for relevant pairs.
- **Caveat:** Alchemy sees only its own Bor node mempool — partial view. On Polygon, validators run independent Bor nodes with separate mempools. We won't see every pending swap, but Alchemy is a major provider.
- **Architecture change:**
  1. Watch for pending txs to DEX router addresses (filter by `toAddress`).
  2. Decode calldata to determine swap direction, token, and amount.
  3. Simulate post-swap pool state (apply the swap to current reserves/liquidity).
  4. If the resulting cross-DEX spread exceeds fee threshold, submit arb tx immediately.
  5. Use high gas priority to land immediately after the target swap.
- **Files:** New `src/mempool/monitor.rs`, new `src/mempool/decoder.rs` (swap calldata parsing), modify `main.rs` to run mempool monitor alongside block loop.
- **Complexity:** High. Swap calldata decoding, state simulation, race conditions.
- **Gate:** Only pursue if A3 diagnostic shows revert rate still >95% (proving competitors use mempool, not just speed).

**A5: Skip estimateGas (combined with A4)**
- **What:** When submitting from mempool signal, skip `fill_transaction` (estimateGas). Set gas limit to a fixed safe value (e.g., 500K). Sign and send immediately.
- **Why:** estimateGas adds ~150ms. When we have mempool conviction (we know the spread will exist after the pending swap confirms), simulation is wasted time.
- **Risk:** On-chain reverts cost gas (~$0.76 at 5,000 gwei). Only viable if mempool-sourced signals have >10% success rate (break-even vs gas cost).
- **Files:** `executor.rs` — add `skip_estimate: bool` parameter to `execute_atomic()`.

### Tier 2: Further Optimizations (after Tier 1 proves viable)

**A6: Parallel Opportunity Submission**
- **What:** Submit top 2-3 opportunities simultaneously instead of sequentially.
- **Why:** Current loop in `main.rs:500` tries one, waits for result, tries next. Atomic revert protection ensures only profitable ones succeed.
- **Files:** `main.rs` — `tokio::join!` on top N executions.

**A7: Dynamic Trade Sizing**
- **What:** Size per-opportunity based on pool depth and spread width.
- **Why:** $500 fixed size creates unnecessary slippage in thin pools, and leaves money on the table in deep pools.
- **Files:** `detector.rs`, `executor.rs`.

**A8: Pre-built Transaction Templates**
- **What:** Pre-construct and pre-sign tx skeletons for common routes. Only fill in amounts at execution time.
- **Saves:** 10-20ms signing overhead.
- **Files:** `executor.rs` — add tx template cache.

### Tier 3: Strategy Expansion (only if Tier 1 produces results)

**A9: Triangular Arbitrage (Multi-Hop)**
- USDC→WETH→WMATIC→USDC across 3 pools
- Multiplicatively more paths, finds circular arbs
- High complexity

**A10: Flash Loans (Zero-Capital)**
- Aave/Balancer flash loans for $50K+ trades
- Profit = gross - gas (no capital at risk)
- Adds ~100k gas overhead

**A11: Additional Chains (Base, Arbitrum, Optimism)**
- Base: ArbExecutor deployed, dry-run done, needs same architecture fixes
- Arbitrum/Optimism: placeholder dirs created
- Same pattern: .env.{chain}, whitelist, deploy executor, data collect, go live

---

## What Won't Help

| Approach | Why |
|----------|-----|
| **Co-location** | 5ms RTT already. Saves ~30ms (4% of 700ms pipeline). |
| **Faster VPS CPU** | Computation is <10ms. All latency is I/O. |
| **More trading pairs** | More detection, same execution failure. |
| **Lower MIN_PROFIT** | Smaller spreads are even more contested. |
| **Better private RPC** | No MEV auction exists on Polygon. FastLane dead. |

---

## Decision Point

A0-A3 deployed 2026-02-01. Run the bot for 4+ hours then re-run `analyze_bot_session.py`:

- **If revert rate drops below ~80%:** Event-driven sync alone is enough to compete. Proceed with gas bump tuning and A6-A8 optimizations.
- **If revert rate stays >95%:** Confirmed that competitors are backrunning from mempool, not just faster at reacting to blocks. Proceed with A4 (mempool monitoring) or evaluate whether Polygon MEV is viable for our infrastructure class.
- **If revert rate drops but 0 profitable trades:** Gas priority is the remaining bottleneck. Tune A0 upward.

---

## Active Whitelist (v1.4)

**V3 Pools (16 active):**

| DEX | Pair | Fee | Status |
|-----|------|-----|--------|
| UniswapV3 | WETH/USDC | 0.05% | active |
| UniswapV3 | WETH/USDC | 0.30% | active |
| UniswapV3 | WMATIC/USDC | 0.05% | active |
| UniswapV3 | WBTC/USDC | 0.05% | active |
| UniswapV3 | USDT/USDC (×3) | 0.01/0.05/0.30% | active |
| UniswapV3 | DAI/USDC (×2) | 0.01/0.05% | active |
| UniswapV3 | LINK/USDC | 0.30% | **monitoring** |
| SushiswapV3 | USDT/USDC | 0.01% | active |
| SushiswapV3 | WETH/USDC | 0.30% | active |
| QuickSwapV3 | WETH/WMATIC/WBTC/USDT/DAI | dynamic | active (5) |

**V2 Pools (7 active):**

| DEX | Pair | TVL | Impact @$140 | Score |
|-----|------|-----|-------------|-------|
| QuickSwapV2 | WETH/USDC | $2.59M | 0.01% | 100 |
| SushiSwapV2 | WETH/USDC | $493K | 0.06% | 90 |
| QuickSwapV2 | WMATIC/USDC | $1.69M | 0.04% | 100 |
| QuickSwapV2 | USDT/USDC | $628K | 0.04% | 100 |
| SushiSwapV2 | USDT/USDC | $351K | 0.08% | 90 |
| QuickSwapV2 | DAI/USDC | $301K | 0.09% | 90 |
| SushiSwapV2 | DAI/USDC | $197K | 0.14% | 90 |

**V2 Observation (2):** SushiSwapV2 WMATIC/USDC ($255K), QuickSwapV2 WBTC/USDC ($184K).
**Blacklisted:** 22 V3 pools (dead/marginal), 3 V2 dead, 1% fee tier banned.

### Base Whitelist (v1.1 — WETH/USDC only)

**Active (5):**

| DEX | Pair | Fee | Score | Impact@$5k | Pool |
|-----|------|-----|-------|-----------|------|
| UniswapV3 | WETH/USDC | 0.05% | 100 | 0.0% | `0xd0b53D92...` |
| UniswapV3 | WETH/USDC | 0.30% | 100 | 0.0% | `0x6c561B44...` |
| UniswapV3 | WETH/USDC | 0.01% | 100 | 0.2% | `0xb4CB8009...` |
| SushiswapV3 | WETH/USDC | 0.05% | 100 | 1.3% | `0x57713F77...` |
| SushiswapV3 | WETH/USDC | 0.30% | 90 | 4.7% | `0x41595326...` |

---

## Contracts

| Contract | Address | Status |
|----------|---------|--------|
| ArbExecutorV3 (Polygon) | `0x7761f012a0EFa05eac3e717f93ad39cC4e2474F7` | **LIVE** — V2+V3 atomic |
| ArbExecutor (Base) | `0x90545f20fd9877667Ce3a7c80D5f1C63CF6AE079` | **DRY RUN** — deployed 01-31 |
| ArbExecutorV2 | `0x1126Ee8C1caAeADd6CF72676470172b3aF39c570` | Retired (V3-only, drained) |
| ArbExecutorV1 | `0xA14e76548D71a2207ECc52c129DB2Ba333cc97Fb` | Retired |

---

## Incident History

| Date | Loss | Root Cause | Fix |
|------|------|-----------|-----|
| 01-29 | $500 | Decimal mismatch + no liquidity check + inverted direction | All three bugs fixed |
| 01-29 | $3.35 | Thin pool + buy-then-continue bug | HALT, 0.01% blacklisted |
| 01-30 | $0 | Negative-profit trade attempted | Quoter profit≤0 guard |
| 01-30 | $0 | ERC20 approval missing | Pre-flight revert, no gas |
| 01-30 | $0 | u128 overflow (sqrtPriceX96) | Store as U256 |
| 01-31 | $0.11 | 1 on-chain atomic revert (position 122/153, 729 gwei) | Gas priority bump needed |

---

## Commands

```bash
# Build
source ~/.cargo/env && cd ~/bots/dexarb/src/rust-bot && cargo build --release

# Start Polygon live bot (--chain polygon loads .env.polygon)
tmux new-session -d -s livebot_polygon "cd ~/bots/dexarb/src/rust-bot && ./target/release/dexarb-bot --chain polygon 2>&1 | tee ~/bots/dexarb/data/polygon/logs/livebot_ws.log"

# Bot watch (kills on first profitable trade — "Trade complete" only)
tmux new-session -d -s botwatch "bash ~/bots/dexarb/scripts/bot_watch.sh"

# Discord status (30 min loop)
tmux new-session -d -s botstatus "bash ~/bots/dexarb/scripts/bot_status_discord.sh --loop"

# Analyze session data
python3 scripts/analyze_bot_session.py

# Start Base bot (dry-run — ArbExecutor deployed, LIVE_MODE=false)
tmux new-session -d -s dexarb-base "cd ~/bots/dexarb/src/rust-bot && ./target/release/dexarb-bot --chain base 2>&1 | tee ~/bots/dexarb/data/base/logs/livebot_$(date +%Y%m%d_%H%M%S).log"
```

---

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Monolithic bot (WS + sync + detect + log + execute) |
| `src/arbitrage/detector.rs` | Unified V2+V3 opportunity detection |
| `src/arbitrage/executor.rs` | Atomic execution via ArbExecutor, profit reporting |
| `src/arbitrage/multicall_quoter.rs` | Batch V3 quoter, V2 passthrough |
| `src/pool/v2_syncer.rs` | V2 reserve sync (getReserves) |
| `src/types.rs` | DexType, V2_FEE_SENTINEL, atomic_fee() |
| `contracts/src/ArbExecutor.sol` | Atomic arb V3 (V2+Algebra+V3 routing) |
| `.env.polygon` | Polygon live config (loaded via --chain polygon) |
| `config/polygon/pools_whitelist.json` | v1.4: 23 active + 22 blacklisted |
| `config/base/pools_whitelist.json` | v1.1: 5 active, 1 observation, 2 blacklisted |
| `.env.base` | Base config (QuoterV2, USDC native, multicall skip) |
| `scripts/analyze_bot_session.py` | Session analysis (log + price CSV parsing) |
| `scripts/bot_watch.sh` | Kill bot after first profitable trade |
| `docs/private_rpc_polygon_research.md` | Private RPC research (FastLane dead, 1RPC metadata-only) |

---

*Last updated: 2026-02-01 — A0-A3 deployed and live. Event-driven sync (A3) confirmed working: eth_getLogs parses V3 Swap + V2 Sync events per block. Gas priority 5000 gwei (A0), cached base_fee (A1), AtomicU64 nonce (A2). Estimated ~450ms latency reduction (700ms→250ms). Diagnostic running: if revert rate drops below 80%, speed was the issue. If >95%, proceed to A4 (mempool backrunning).*
