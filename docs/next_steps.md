# Next Steps - DEX Arbitrage Bot

## Status: LIVE — V3 Tri-DEX + Midmarket Fix + V2↔V3 Next

**Date:** 2026-01-30
**Pools:** 16 active V3 (9 Uni + 2 Sushi + 5 QS). LINK/USDC demoted to monitoring (sole pool, zero arb).
**Execution:** Atomic via `ArbExecutorV2` (`0x1126...c570`)
**Build:** 44/44 tests pass, clean release build
**Opportunities:** 0 post-fix — V3↔V3 spreads structurally below fee thresholds (see analysis below)
**Next:** WS block subscription → V2↔V3 cross-protocol (9 deep V2 pools found)

---

## Two-Wallet Architecture

| Wallet | Address | Purpose | USDC.e | USDC (native) | MATIC |
|--------|---------|---------|--------|---------------|-------|
| **Live** | `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2` | Trading (at-risk) | 516.70 | 400.00 | 166.94 |
| **Backup** | `0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb` | Deep storage (manual) | 0 | 0 | 0.07 |

**Fund transfer (2026-01-30):** Consolidated $356.70 USDC.e from backup → live wallet (TX: `0x16ecf...2ad77`). 0.05 MATIC sent live → backup for gas (TX: `0x11f70...bcc4`).

**Native USDC ($400) is NOT at risk:** All 17 pools use USDC.e (`0x2791...`). ArbExecutor approval is on USDC.e only. Native USDC (`0x3c499...`) has zero approval, zero pool references — completely untouched by bot operations.

**Settings:** MAX_TRADE_SIZE_USD=140, MIN_PROFIT_USD=0.10, MAX_SLIPPAGE_PERCENT=0.5

---

## Monolithic Architecture (Implemented 2026-01-30)

Previous split architecture (data collector → JSON file → live bot) had ~5s average latency.
Monolithic bot syncs pools directly via RPC, eliminating file I/O indirection:

| Step | Split (old) | Monolithic (current) |
|------|-------------|---------------------|
| Fetch block | 0ms | ~50ms |
| RPC sync | 200ms | 400ms (12 pools concurrent) |
| File write → read | 0-10s (poll alignment) | 0ms (in-memory) |
| Detect + Quoter + Execute | ~600ms | ~600ms |
| **Total** | **0.8-10.8s (avg ~5.8s)** | **~1.0s** |

**Main loop:** fetch block → skip if same → `sync_known_pools_parallel()` → price log → detect → Multicall3 verify → execute

**RPC budget:** 1 block call/iteration + 35 sync calls on new block (17 pools × 2 + 1). At 3s poll with ~50% same-block skips: ~6 calls/s avg = ~15.5M/month (Alchemy free tier: 22.2M, 70% utilization).

---

## Completed Phases

| Phase | Date | Summary |
|-------|------|---------|
| V3 swap routing | 2026-01-28 | `exactInputSingle` support, V3 SwapRouter integration |
| Critical bug fixes | 2026-01-29 | Decimal mismatch, liquidity check, trade direction, buy-then-continue HALT |
| Shared data architecture | 2026-01-29 | JSON state file, live bot reads from file (zero RPC for price discovery) |
| Parallel V3 sync | 2026-01-29 | `sync_known_pools_parallel()` — all pools concurrent via `join_all` |
| Phase 1.1 whitelist | 2026-01-29 | 10 active pools, 7 blacklisted, strict enforcement, per-tier liquidity thresholds |
| Phase 2.1 Multicall3 | 2026-01-29 | Batch Quoter pre-screen — verify all opps in 1 RPC call, 7 unit tests |
| V3-only data collector | 2026-01-30 | Removed V2 syncing, whitelist-driven pool sync, 60% RPC reduction |
| Deployment checklist | 2026-01-30 | 6 shell scripts (~102 checks), all 5 sections pass |
| Live test (split) | 2026-01-30 | Bot running, zero errors, zero capital spent |
| Monolithic live bot | 2026-01-30 | Direct RPC sync, ~1s cycle, observability fix |
| Checklist + monitoring | 2026-01-30 | Checklist updated for monolithic, Discord status, bot watch |
| SushiSwap V3 recon | 2026-01-30 | Factory verified on-chain, 12 pools found, 2 promoted to active |
| SushiSwap V3 integration | 2026-01-30 | Cross-DEX: DexType variants, dual-quoter (V1/V2), executor routing, whitelist v1.2 |
| Gas estimate fix | 2026-01-30 | Detector $0.50→$0.05, executor $0.50→$0.01 (Polygon-accurate) |
| Historical price logging | 2026-01-30 | `PriceLogger` module — daily CSV rotation, ~240 rows/min, zero RPC cost |
| Atomic executor | 2026-01-30 | `ArbExecutor.sol` deployed to Polygon, single-tx execution, zero leg risk |
| QuickSwap V3 (Algebra) | 2026-01-30 | Tri-DEX: DexType, Algebra sync (globalState), tri-quoter, ArbExecutorV2 with Algebra router, 5 pools whitelisted |
| u128 overflow fix | 2026-01-30 | Removed `as_u128()` panics across 7 files; v3_syncer stores sqrtPriceX96 as U256 directly |
| Price data analysis | 2026-01-30 | 129K rows, 6.4h (QS-era only): WBTC/USDC UniV3↔QS best combo (60% prof), $37/hr at $500 size (midmarket ceiling) |
| Negative-profit guard | 2026-01-30 | Multicall Quoter now rejects `profit<=0`; main.rs belt-and-suspenders `quoted_profit_raw>0` filter |

---

## Active Whitelist (v1.3)

16 active pools across 3 DEXes (1 monitoring):

| DEX | Pair | Fee | Status |
|-----|------|-----|--------|
| UniswapV3 | WETH/USDC | 0.05% | active |
| UniswapV3 | WETH/USDC | 0.30% | active |
| UniswapV3 | WMATIC/USDC | 0.05% | active |
| UniswapV3 | WBTC/USDC | 0.05% | active |
| UniswapV3 | USDT/USDC | 0.05% | active |
| UniswapV3 | USDT/USDC | 0.30% | active |
| UniswapV3 | USDT/USDC | 0.01% | active |
| UniswapV3 | DAI/USDC | 0.05% | active |
| UniswapV3 | DAI/USDC | 0.01% | active |
| UniswapV3 | LINK/USDC | 0.30% | **monitoring** (sole pool) |
| SushiswapV3 | USDT/USDC | 0.01% | active |
| SushiswapV3 | WETH/USDC | 0.30% | active |
| QuickSwapV3 | WETH/USDC | dynamic (~0.09%) | active |
| QuickSwapV3 | WMATIC/USDC | dynamic (~0.09%) | active |
| QuickSwapV3 | WBTC/USDC | dynamic (~0.09%) | active |
| QuickSwapV3 | USDT/USDC | dynamic (~0.001%) | active |
| QuickSwapV3 | DAI/USDC | dynamic (~0.001%) | active |

**Blacklisted:** QuickSwap LINK/USDC (`0xEFdC...4C`) — 98% price impact, no liquidity.

**SushiSwap V3 Contracts (Polygon):**

| Contract | Address |
|----------|---------|
| Factory | `0x917933899c6a5F8E37F31E19f92CdBFF7e8FF0e2` |
| SwapRouter | `0x0aF89E1620b96170e2a9D0b68fEebb767eD044c3` |
| QuoterV2 | `0xb1E835Dc2785b52265711e17fCCb0fd018226a6e` |

**QuickSwap V3 (Algebra Protocol) Contracts (Polygon):**

| Contract | Address |
|----------|---------|
| Factory | `0x411b0fAcC3489691f28ad58c47006AF5E3Ab3A28` |
| SwapRouter | `0xf5b509bB0909a69B1c207E495f687a596C168E12` |
| QuoterV2 | `0xa15F0D7377B2A0C0c10db057f641beD21028FC89` |

---

## Roadmap — Priority Order

### Tier 1: Highest Impact (addresses 0-opportunity problem)

**P1: Atomic Execution via Custom Smart Contract** --- DONE (2026-01-30)
- `ArbExecutor.sol` deployed: `0xA14e76548D71a2207ECc52c129DB2Ba333cc97Fb`
- Both V3 swap legs in one atomic tx — reverts on loss, zero leg risk
- USDC approved (max uint256), Foundry test suite (6 tests), Rust executor integrated
- Flash loan extension available as future add-on

**P2: V2 ↔ V3 Cross-Protocol Arbitrage**
- V2 pools (QuickSwap, SushiSwap V2) use constant-product pricing, update differently from V3
- Price divergence between V2 and V3 likely larger and more frequent than V3↔V3
- Needs: re-enable V2 pool syncing, fix price format incompatibility (`detector.rs:104`), V2↔V3 comparison
- V2 0.3% fee → V2↔V3_0.05% round-trip = 0.35%, but divergence should be larger
- Complexity: Medium — **NOTE:** previously explored, hit issues; may need investigation
- Impact: Potentially highest — V2/V3 drift is a well-known arb source

**P3: QuickSwap V3 (Algebra Protocol) — Third DEX** --- DONE (2026-01-30)
- Algebra protocol: dynamic fees, `globalState()` instead of `slot0()`, no fee param in quoter/router
- `ArbExecutorV2` deployed (`0x1126...c570`): branches on fee==0 → Algebra, else → standard V3
- Tri-quoter: Multicall3 routes to UniV1/SushiV2/AlgebraV2 per leg
- 5 pools whitelisted (WETH, WMATIC, WBTC, USDT, DAI), LINK blacklisted (98% impact)
- Cross-DEX advantage: QS ~0.09% + UniV3 0.05% = **0.14% RT** (was 0.35% best)
- **Result:** 9 opportunities in first 7600 scans (was 0 with 12 pools)

**P4: Triangular Arbitrage (Multi-Hop Routes)**
- A→B→C→A across three pools (e.g., USDC→WETH→WMATIC→USDC)
- Finds circular arb that two-pool scanning misses entirely
- Complexity: High (route enumeration, multi-leg execution)

### Tier 2: Medium Impact (improve detection quality & speed)

**P5: Dynamic Trade Sizing Based on Liquidity Depth**
- Currently uses fixed `max_trade_size_usd`
- Quoter probes at multiple sizes to find optimal trade amount per opportunity
- Complexity: Medium

**P6: Websocket Block Subscription (Latency Reduction)**
- Switch from polling at `poll_interval_ms` to `eth_subscribe("newHeads")`
- Detect new blocks instantly vs up to 3s latency (may miss blocks at 2s Polygon block time)
- Complexity: Low (ethers-rs supports natively)

**P7: Private Transaction Submission (MEV Protection)**
- Send via Flashbots Protect or Polygon private mempool (e.g., Marlin)
- Prevents sandwich attacks on bot's trades
- Complexity: Low-Medium — defensive only

### Tier 3: Operational Quality

**P8: Automatic Whitelist Refresh**
- Periodically re-run Quoter depth probes on all pools, auto-promote/demote based on liquidity
- Complexity: Medium

**P9: Backtesting on Historical Blocks**
- Replay historical blocks through detector to validate strategy edge
- Use price logger data to analyze: when do spreads appear? Which pairs? What time of day?
- Critical diagnostic: if historical analysis shows zero opportunities with perfect execution, the strategy needs fundamental changes
- Complexity: Medium

### Recommended Priority (Updated 2026-01-30 evening)

| # | Item | Rationale |
|---|------|-----------|
| -- | ~~Atomic smart contract (P1)~~ | **DONE** |
| -- | ~~QuickSwap V3 (P3)~~ | **DONE** |
| -- | ~~Analyze price data (P9)~~ | **DONE** |
| -- | ~~Negative-profit guard~~ | **DONE** |
| -- | ~~ERC20 approval~~ | **DONE** — max allowance set |
| -- | ~~Midmarket spread fix~~ | **DONE** — SELL_ESTIMATE_FACTOR 0.95→1.0, token ordering fix, slippage 10%→1% |
| 1 | WS block subscription (P6) | $0 cost, ~20 lines, catches blocks in ~100ms vs 3s poll delay |
| 2 | V2↔V3 cross-protocol (P2) | **Essential** — V3↔V3 spreads don't exceed fees; 9 deep V2 pools found |
| 3 | Increase trade size (P5) | $140→$500 after first V2↔V3 trade |
| 4 | Flash loan extension (P1+) | Zero-capital arb at $50K+ sizes |

---

## Incident History

| Date | Loss | Root Cause | Fix |
|------|------|-----------|-----|
| 2026-01-29 | $500 | Decimal mismatch + no liquidity check + inverted trade direction | All three bugs fixed |
| 2026-01-29 | $3.35 | WETH/USDC 0.01% thin pool + buy-then-continue bug | HALT on `tx_hash`, 0.01% blacklisted for non-stables |

| 2026-01-30 | $0 | Negative-profit trade attempted: Quoter returned `profit_raw=-10608070` but `both_legs_valid=true` passed filter | Quoter now sets `both_legs_valid=false` when `profit<=0`; main.rs adds `quoted_profit_raw>0` guard |
| 2026-01-30 | $0 | `ERC20: transfer amount exceeds balance` on atomic exec — wallet hadn't approved ArbExecutor to spend USDC | Reverted at `eth_estimateGas` (pre-flight), no gas spent. Approval needed before live trading. |

All incidents led to architectural improvements (Quoter pre-check, whitelist filter, HALT on committed capital, profit guard).

---

## Disk Budget (Price Logging)

- ~175 bytes/row, 12 pools, ~20 blocks/min = ~240 rows/min
- **1 day:** ~58 MB
- **1 month:** ~1.7 GB
- **Disk free:** ~20 GB = ~11 months runway
- Consider 90-day retention or gzip compression if space becomes tight

---

## Commands

```bash
# Build
source ~/.cargo/env && cd ~/bots/dexarb/src/rust-bot && cargo build --release

# Start live bot (monolithic — no data collector needed)
tmux new-session -d -s livebot "cd ~/bots/dexarb/src/rust-bot && RUST_BACKTRACE=1 RUST_LOG=dexarb_bot=info ./target/release/dexarb-bot > ~/bots/dexarb/data/livebot.log 2>&1"

# Start bot watch (kills livebot on first trade)
tmux new-session -d -s botwatch "bash ~/bots/dexarb/scripts/bot_watch.sh"

# Discord status (30 min, 10 min initial delay)
tmux new-session -d -s botstatus "sleep 600 && bash ~/bots/dexarb/scripts/bot_status_discord.sh --loop"

# Checklist
bash ~/bots/dexarb/scripts/checklist_full.sh
```

---

## File Reference

| File | Purpose |
|------|---------|
| `src/main.rs` | Monolithic live bot (sync + detect + price log + execute) |
| `src/price_logger.rs` | Historical price CSV logger (daily rotation) |
| `src/arbitrage/detector.rs` | Opportunity detection, gas estimate ($0.05) |
| `src/arbitrage/executor.rs` | Trade execution: atomic (ArbExecutor) + legacy two-tx fallback |
| `src/arbitrage/multicall_quoter.rs` | Batch Quoter pre-screen, tri-quoter (V1 Uni/V2 Sushi/Algebra QS) |
| `contracts/src/ArbExecutor.sol` | Atomic arb contract V2 (Polygon: `0x1126...c570`) — Algebra + standard V3 |
| `contracts/test/ArbExecutor.t.sol` | Foundry tests (4 unit + 2 fork) |
| `.env.live` | Live bot config (3s poll, LIVE_MODE=true, ARB_EXECUTOR_ADDRESS set) |
| `config/pools_whitelist.json` | 16 active V3 pools (9 Uni + 2 Sushi + 5 QS) + 1 monitoring, whitelist v1.4 |
| `scripts/analyze_price_log.py` | Cross-DEX spread analysis, per-pool stats, profitability |
| `scripts/analyze_trade_sizes.py` | Trade size profitability estimates ($100-$5000) |
| `scripts/verify_quickswap_pools.py` | Algebra pool assessment (dynamic fees, $1-$5000 quotes) |
| `scripts/bot_watch.sh` | Auto-kill on first trade |
| `scripts/bot_status_discord.sh` | Discord status report (30 min loop) |
| `scripts/checklist_full.sh` | Deployment checklist (5 sections) |
| `scripts/verify_whitelist.py` | Dollar-value Quoter matrix, dual-quoter support |
| `data/price_history/` | Price log CSV directory (~58 MB/day) |

---

## Price Data Analysis Summary (2026-01-30, QuickSwap-era: 6h 25m, 129K rows)

**Data window:** 08:44–15:09 UTC (QuickSwap-enabled bot only, pre-crash). 7,605 blocks, 17 pools, 6 pairs.

**Best combo:** WBTC/USDC UniV3(0.05%) ↔ QuickSwap(~0.088%) = 0.138% RT fee. Profitable 60% of blocks.

| Trade Size | Prof. Blocks | Total $ (6.4h) | $/hour | $/day est. |
|------------|-------------|----------------|--------|-----------|
| $100 | 1,201 | $33 | $5.22 | $125 |
| $500 | 1,994 | $238 | $37.10 | $890 |
| $1,000 | 1,994 | $496 | $77.30 | $1,855 |

Midmarket ceiling estimates (no slippage). Realistic yield ~30-50% after slippage/MEV/gas spikes.

**Top combos by pair (at $140):**

| Pair | Combo | Prof% | MaxNet$ |
|------|-------|-------|---------|
| WBTC/USDC | UniV3_0.05% ↔ QS(878) | 60.1% | $0.13 |
| USDT/USDC | SushiV3_0.01% ↔ QS(10) | 11.1% | $0.00 |
| WBTC/USDC | UniV3_0.05% ↔ QS(876) | 9.2% | $0.22 |
| WETH/USDC | SushiV3_0.30% ↔ UniV3_0.05% | 3.2% | $0.24 |
| WMATIC/USDC | UniV3_0.05% ↔ QS(900) | 2.0% | $0.14 |

**Bot vs analysis gap:** Bot detected 9 opportunities in ~7,600 scans; analysis shows 447+ profitable blocks. Gap = detector min-spread threshold + Quoter rejection + gas gate.

**Gas spike observed:** 1558 gwei (max 1000 allowed) — blocked 2 WETH/USDC executions correctly.

---

## V3↔V3 Structural Analysis (2026-01-30)

**Finding:** V3 cross-DEX spreads are structurally below fee thresholds. At $140 trade size, minimum executable spread is 0.012% (post slippage fix). But observed max V3 spreads are:

| Pool Combo | RT Fee | Max Observed Spread | Exceeds? |
|---|---|---|---|
| USDT Uni 0.01% + Sushi 0.01% | 0.02% | 0.01% | No |
| WETH Uni 0.05% + QS ~0.09% | 0.14% | 0.04% | No |
| WETH Uni 0.30% + Sushi 0.30% | 0.60% | 0.06% | No |

**Root cause:** V3 pools on Polygon are efficiently arbitraged by existing bots. Spreads rarely reach fee thresholds.

**Solution:** V2↔V3 cross-protocol. V2 constant-product pools lag V3 during volatility (only update on trades). Divergence reaches 0.5-2%+, well above 0.35% V2↔V3 round-trip fee.

## V2 Pool Liquidity (Verified On-Chain 2026-01-30)

9 viable V2 pools found:

| Pair | DEX | TVL |
|---|---|---|
| WETH/USDC | QuickSwapV2 | $2.59M |
| WETH/USDC | SushiSwapV2 | $494K |
| WMATIC/USDC | QuickSwapV2 | $1.69M |
| WMATIC/USDC | SushiSwapV2 | $255K |
| WBTC/USDC | QuickSwapV2 | $184K |
| USDT/USDC | QuickSwapV2 | $628K |
| USDT/USDC | SushiSwapV2 | $351K |
| DAI/USDC | QuickSwapV2 | $301K |
| DAI/USDC | SushiSwapV2 | $197K |

Dead: WBTC/SushiV2 ($508), LINK/both V2 ($12-$98).

V2 factory addresses already in `.env.live`: QuickSwapV2 (`0x5757...`), SushiSwapV2 (`0xc35D...`).

---

## Midmarket Spread Fix (2026-01-30)

**Bugs fixed:**
1. `SELL_ESTIMATE_FACTOR` 0.95→1.0 — 5% haircut created phantom $6.45 loss per $140 trade
2. Token ordering — WMATIC/WBTC pairs had inverted buy/sell and wrong trade_size units
3. Slippage estimate 10%→1% — V3 concentrated liquidity has <0.01% slippage at $140-500

**Files changed:** `types.rs` (quote_token_is_token0 field), `detector.rs` (direction-aware buy/sell), `multicall_quoter.rs` (direction-aware encoding), `executor.rs` (direction-aware contract calls).

---

## ArbExecutor Funding Model

The contract (`0x1126...c570`) does **not** hold funds. It uses `transferFrom(caller → contract)` per trade, then returns all tokens. The EOA wallet is the source.

**Live trading status (2026-01-30):**
1. ~~EOA wallet must call `USDC.approve(ArbExecutor, type(uint256).max)`~~ — **DONE** (verified on-chain: max allowance set)
2. Wallet USDC.e balance: $516.70 (trade size $140, headroom for $500 scale-up)
3. Wallet MATIC: 166.94 (~$0.01/trade on Polygon — gas for ~16,000+ trades)

---

## Incident: u128 Overflow Crash (2026-01-30)

**Symptom:** Bot panicked at `primitive-types` `as_u128()` after detecting WMATIC/USDC QuickSwap opportunity.
**Root cause:** `v3_syncer.rs` did needless `U256 → u128 → U256` roundtrip on sqrtPriceX96 (uint160 can exceed u128). Additional unsafe `as_u128()` calls in quoter/executor/detector.
**Fix:** Removed truncation in v3_syncer (4 sites); replaced `as_u128()` with `low_u128()` in 7 files; added overflow guard in multicall_quoter profit calc.
**Status:** Fixed, 44/44 tests pass, bot stable 500+ iterations post-fix.

---

*Last updated: 2026-01-30 — Live: tri-DEX (16 V3 pools) + midmarket fix + slippage fix. V3↔V3 structurally unprofitable. 9 deep V2 pools found. Next: WS block subscription → V2↔V3 cross-protocol*
