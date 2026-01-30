# Next Steps - DEX Arbitrage Bot

## Status: LIVE — Cross-DEX + Price Logging Active

**Date:** 2026-01-30
**Live sessions:** `livebot` (3s poll), `botwatch` (auto-kill), `botstatus` (Discord/30min)
**Pools:** 12 active (10 UniswapV3 + 2 SushiswapV3), cross-DEX scanning enabled
**Gas estimate:** $0.05 (detector), $0.01 (executor) — Polygon-accurate
**Price logging:** ON — `data/price_history/prices_YYYYMMDD.csv` (~58 MB/day, ~1.7 GB/month)
**Build:** 43/43 tests pass, clean release build

---

## Two-Wallet Architecture

| Wallet | Address | Purpose | USDC | MATIC |
|--------|---------|---------|------|-------|
| **Live** | `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2` | Trading (at-risk) | 160.00 | ~7.59 |
| **Backup** | `0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb` | Deep storage (manual) | 356.70 | 0 |

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

**RPC budget:** 1 block call/iteration + 25 sync calls on new block (12 pools × 2 + 1). At 3s poll with ~50% same-block skips: ~6 calls/s avg = ~15M/month (Alchemy free tier: 22.2M).

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

---

## Active Whitelist (v1.2)

12 active pools across 2 DEXes:

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
| UniswapV3 | LINK/USDC | 0.30% | active |
| SushiswapV3 | USDT/USDC | 0.01% | active |
| SushiswapV3 | WETH/USDC | 0.30% | active |

**SushiSwap V3 Contracts (Polygon):**

| Contract | Address |
|----------|---------|
| Factory | `0x917933899c6a5F8E37F31E19f92CdBFF7e8FF0e2` |
| SwapRouter | `0x0aF89E1620b96170e2a9D0b68fEebb767eD044c3` |
| QuoterV2 | `0xb1E835Dc2785b52265711e17fCCb0fd018226a6e` |

---

## Roadmap — Priority Order

### Tier 1: Highest Impact (addresses 0-opportunity problem)

**P1: Atomic Execution via Custom Smart Contract**
- Execute both swaps in one atomic tx — if second leg fails or profit < threshold, entire tx reverts
- Eliminates leg risk (the $500 incident class), enables flash loans (borrow → swap → swap → repay)
- Complexity: High (Solidity contract + deployment + testing)

**P2: V2 ↔ V3 Cross-Protocol Arbitrage**
- V2 pools (QuickSwap, SushiSwap V2) use constant-product pricing, update differently from V3
- Price divergence between V2 and V3 likely larger and more frequent than V3↔V3
- Needs: re-enable V2 pool syncing, fix price format incompatibility (`detector.rs:104`), V2↔V3 comparison
- V2 0.3% fee → V2↔V3_0.05% round-trip = 0.35%, but divergence should be larger
- Complexity: Medium — **NOTE:** previously explored, hit issues; may need investigation
- Impact: Potentially highest — V2/V3 drift is a well-known arb source

**P3: QuickSwap V3 (Algebra Protocol) — Third DEX**
- QuickSwap uses Algebra V3 on Polygon — dynamic fees (not fixed tiers), different ABI
- Creates 3-way cross-DEX grid: UniV3 ↔ SushiV3 ↔ QuickV3
- More DEX pairs = exponentially more comparison combinations
- Complexity: Medium-High (different ABI, dynamic fee handling)

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

### Recommended Priority

| # | Item | Rationale |
|---|------|-----------|
| 1 | Analyze price log data (P9) | Data is now accumulating — answer whether opportunities exist at all |
| 2 | Atomic smart contract (P1) | Eliminates leg risk, enables flash loans, foundational |
| 3 | V2↔V3 cross-protocol (P2) | Largest untapped divergence source (needs investigation of prior issues) |
| 4 | QuickSwap V3 (P3) | Third DEX adds many comparison pairs |
| 5 | Websocket subscription (P6) | Low effort, reduces latency to near-zero |

---

## Incident History

| Date | Loss | Root Cause | Fix |
|------|------|-----------|-----|
| 2026-01-29 | $500 | Decimal mismatch + no liquidity check + inverted trade direction | All three bugs fixed |
| 2026-01-29 | $3.35 | WETH/USDC 0.01% thin pool + buy-then-continue bug | HALT on `tx_hash`, 0.01% blacklisted for non-stables |

Both incidents led to architectural improvements (Quoter pre-check, whitelist filter, HALT on committed capital).

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
tmux new-session -d -s livebot "cd ~/bots/dexarb/src/rust-bot && RUST_LOG=dexarb_bot=info ./target/release/dexarb-bot > ~/bots/dexarb/data/livebot.log 2>&1"

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
| `src/arbitrage/executor.rs` | Trade execution, dual-router (Uni/Sushi), gas logging ($0.01) |
| `src/arbitrage/multicall_quoter.rs` | Batch Quoter pre-screen, dual-quoter (V1/V2) |
| `.env.live` | Live bot config (3s poll, LIVE_MODE=true, PRICE_LOG_ENABLED=true) |
| `config/pools_whitelist.json` | 12 active V3 pools (10 Uni + 2 Sushi), whitelist v1.2 |
| `scripts/bot_watch.sh` | Auto-kill on first trade |
| `scripts/bot_status_discord.sh` | Discord status report (30 min loop) |
| `scripts/checklist_full.sh` | Deployment checklist (5 sections) |
| `scripts/verify_whitelist.py` | Dollar-value Quoter matrix, dual-quoter support |
| `data/price_history/` | Price log CSV directory (~58 MB/day) |

---

*Last updated: 2026-01-30 — Live cross-DEX (12 pools), gas fix, price logging active, analyzing data next*
