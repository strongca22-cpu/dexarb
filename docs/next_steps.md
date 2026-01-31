# Next Steps - DEX Arbitrage Bot

## Status: LIVE — Atomic V2↔V3 + V3↔V3, 23 Pools, WS Block Sub

**Date:** 2026-01-31
**Pools:** 23 active (16 V3 + 7 V2), 1 V3 monitoring, 2 V2 observation
**Execution:** Atomic via `ArbExecutorV3` (`0x7761f012a0EFa05eac3e717f93ad39cC4e2474F7`) — V3↔V3, V2↔V3, V2↔V2
**Build:** 51/51 Rust tests, 10/10 Solidity fork tests, clean release build
**Mode:** WS block subscription (~2s Polygon blocks), 3 tmux sessions (livebot, botstatus, botwatch)
**Steady state:** DAI/USDC V2→V3 0.14% spread detected every block — correctly filtered by quoter (below 0.31% RT fee). Waiting for transient volatility spikes.

---

## Two-Wallet Architecture

| Wallet | Address | Purpose | USDC.e | USDC (native) | MATIC |
|--------|---------|---------|--------|---------------|-------|
| **Live** | `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2` | Trading (at-risk) | 516.70 | 400.00 | 165.57 |
| **Backup** | `0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb` | Deep storage (manual) | 0 | 0 | 0.07 |

**Native USDC ($400) is NOT at risk:** All pools use USDC.e (`0x2791...`). ArbExecutor approval is on USDC.e only.

**Settings:** MAX_TRADE_SIZE_USD=140, MIN_PROFIT_USD=0.10, MAX_SLIPPAGE_PERCENT=0.5

---

## Architecture

Monolithic bot: WS `subscribe_blocks()` → sync V3+V2 pools → price log → detect → Multicall3 verify → atomic execute

**Execution pipeline:**
```
Detector (reserve/tick prices) → min_profit gate ($0.10)
  → Multicall Quoter (V3 on-chain, V2 passthrough)
  → ArbExecutor.sol (fee sentinel routing: V2/Algebra/V3)
  → Revert on loss (zero risk)
```

**Fee sentinel routing (ArbExecutor.sol):**
- `fee = 0` → Algebra SwapRouter (QuickSwap V3)
- `fee = 1..65535` → Standard V3 SwapRouter (Uniswap/SushiSwap V3)
- `fee = 16777215` → V2 Router (`swapExactTokensForTokens`)

**RPC budget:** WS + ~40 sync calls/block (23 pools). ~20 calls/s burst = ~22M/month (Alchemy free: 22.2M).

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

---

## Contracts

| Contract | Address | Status |
|----------|---------|--------|
| ArbExecutorV3 | `0x7761f012a0EFa05eac3e717f93ad39cC4e2474F7` | **LIVE** — V2+V3 atomic |
| ArbExecutorV2 | `0x1126Ee8C1caAeADd6CF72676470172b3aF39c570` | Retired (V3-only, drained) |
| ArbExecutorV1 | `0xA14e76548D71a2207ECc52c129DB2Ba333cc97Fb` | Retired |

---

## Roadmap — Priority Order

### Tier 1: Data-Driven (collect before acting)

**P1: Collect Price History Data (1-3 days)**
- Bot is running, logging all spreads. Let data accumulate.
- Analyze: when do transient spikes occur? Which pairs? Time-of-day patterns?
- Informs all decisions below (trade size, pairs, thresholds).

**P2: Increase Trade Size ($140 → $500)**
- $500 at 0.33% net spread = $1.65 vs $0.46 at $140
- Wait for price data to confirm opportunity frequency first
- Promote 2 observation V2 pools (both viable at $500)

### Tier 2: Execution Improvements

**P3: Private Transaction Submission (MEV Protection)**
- Flashbots Protect or Polygon Fastlane
- Prevents frontrunning/sandwiching of bot trades
- Likely the biggest single edge improvement

**P4: Dynamic Trade Sizing**
- Size per-opportunity based on pool depth and spread width
- Wider spread → bigger trade (up to pool depth limit)

**P5: Gas Price Optimization**
- Use `eth_maxPriorityFeePerGas` with minimal tip
- Currently uses provider default (sometimes overpays)

### Tier 3: Strategy Expansion

**P6: Triangular Arbitrage (Multi-Hop)**
- USDC→WETH→WMATIC→USDC across 3 pools
- Multiplicatively more paths, finds circular arbs
- High complexity

**P7: Flash Loans (Zero-Capital)**
- Aave/Balancer flash loans for $50K+ trades
- Profit = gross - gas (no capital at risk)
- Adds ~100k gas overhead

**P8: Additional Chains**
- Arbitrum, Base, Optimism — same architecture, different RPCs

---

## Steady-State Spread Analysis

DAI/USDC V2→V3 shows persistent 0.14% spread every block. This is **structural** (not arbitrageable):
- QuickSwapV2 fee: 0.30%
- UniV3 0.01% fee: 0.01%
- Round-trip: 0.31% > 0.14% spread

Real opportunities are **transient** — large swaps, liquidations, or volatility push prices past fee equilibrium. These happen during active market hours, not quiet periods.

---

## Incident History

| Date | Loss | Root Cause | Fix |
|------|------|-----------|-----|
| 01-29 | $500 | Decimal mismatch + no liquidity check + inverted direction | All three bugs fixed |
| 01-29 | $3.35 | Thin pool + buy-then-continue bug | HALT, 0.01% blacklisted |
| 01-30 | $0 | Negative-profit trade attempted | Quoter profit≤0 guard |
| 01-30 | $0 | ERC20 approval missing | Pre-flight revert, no gas |
| 01-30 | $0 | u128 overflow (sqrtPriceX96) | Store as U256 |

---

## Commands

```bash
# Build
source ~/.cargo/env && cd ~/bots/dexarb/src/rust-bot && cargo build --release

# Start live bot
tmux new-session -d -s livebot "cd ~/bots/dexarb/src/rust-bot && RUST_LOG=dexarb_bot=info,warn cargo run --release --bin dexarb-bot -- --env .env.live 2>&1 | tee ~/bots/dexarb/data/logs/livebot_$(date +%Y%m%d_%H%M%S).log"

# Bot watch (kills on first trade)
tmux new-session -d -s botwatch "bash ~/bots/dexarb/scripts/bot_watch.sh"

# Discord status (30 min loop)
tmux new-session -d -s botstatus "bash ~/bots/dexarb/scripts/bot_status_discord.sh --loop"
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
| `.env.live` | Live config (ARB_EXECUTOR_ADDRESS, LIVE_MODE=true) |
| `config/pools_whitelist.json` | v1.4: 23 active + 22 blacklisted |

---

*Last updated: 2026-01-31 session 5 — V2↔V3 atomic execution deployed and live. ArbExecutorV3 at 0x7761...4F7. Profit reporting fixed (quote token decimals). Bot running: 23 pools (16 V3 + 7 V2), WS block sub, steady-state spreads filtered correctly. Next: collect data, analyze spread patterns, then scale trade size.*
