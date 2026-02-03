# Next Steps — Immediate Action Items

**Date:** 2026-02-03 (updated 00:30 UTC)
**Context:** See `master_plan.md` for strategy. Latest session: `archive/session_summaries/2026-02-03_live_validation_and_wallet_funding.md`

---

## Current State (as of 2026-02-03 00:30 UTC)

- **Branch:** `feature/alloy-migration` (fee_to_u24 fix committed, profit threshold changes in .env only)
- **Bot:** Running LIVE in tmux `dexarb-live` (LIVE_MODE=true, MEMPOOL_MONITOR=execute)
- **Pools:** 66 (45 V3 + 21 V2) across 15 pairs, 4 quote tokens
- **Tests:** 80/80 passing, release binary clean
- **Whitelist:** v1.9 — 66 active pools, 16 blacklisted

### What's new since last update:
- ✅ **fee_to_u24() bug fixed** — V2 sentinel was truncated from 24-bit to 16-bit, silently breaking all V2↔V3 and V2↔V2 arbs
- ✅ **Live validation run** — 52 execution attempts across 50 minutes, hybrid pipeline fully functional
- ✅ **WETH funded ($101) + MAX approved** — 4 WETH-quoted pairs now active
- ✅ **USDT funded ($150) + MAX approved** — swapped from native USDC on-chain, 3 USDT-quoted pairs now active
- ✅ **All 15 pairs executable** — 8 USDC.e + 3 USDT + 4 WETH
- ✅ **Profit thresholds lowered** — MIN_PROFIT_USD=0.01, MEMPOOL_MIN_PROFIT_USD=0.001
- ❌ **0 captures** — all 52 attempts fail at estimateGas (InsufficientProfit). Spreads consumed by faster bots before our ~5.4s latency allows execution.
- ⚠️ **$145 USDT stuck on Ethereum mainnet** — accidental wrong-network send, recoverable with ETH gas

### Wallet Balances:
| Token | Balance | Approval | Active Pairs |
|-------|---------|----------|-------------|
| USDC.e | $516.70 | MAX | 8 |
| Native USDC | $400.00 | MAX | 0 (no pairs configured) |
| USDT | $150.07 | MAX | 3 |
| WETH | ~$101 | MAX | 4 |
| MATIC/POL | ~15 | N/A | Gas |

---

## Priority 1: Close First Transaction (Validation)

### 1. Monitor Overnight Run with All 15 Pairs

- **What:** Let bot run overnight with USDT + WETH pairs now active
- **Why:** Different pairs (DAI/USDT, WBTC/WETH) may have less MEV competition than WMATIC/USDC
- **Check:** `tmux attach -t dexarb-live` or `tail -f data/polygon/logs/live_validation_20260203_002513.log`

### 2. Add Native USDC Trading Pairs

- **What:** Research and add native USDC pool addresses to config and whitelist
- **Why:** $400 funded + approved but zero pairs configured. Many V3 pools now trade against native USDC.
- **How:** Use `scripts/pool_scanner.py` to find native USDC pools, verify depth, add to whitelist + TRADING_PAIRS

### 3. Consider Skip-estimateGas for Hybrid Path

- **What:** Add `SKIP_ESTIMATE_GAS=true` env var. Hybrid hits skip estimateGas and send with fixed gas limit (500K).
- **Why:** If any thin positive spread survives our latency, this catches it. Cost of reverted tx on Polygon: ~$0.01.
- **Risk:** Burns ~$0.01/revert. At ~12 hybrid hits per 50 min, that's ~$0.15/hour if all revert.
- **Tradeoff:** Worth it for validation — one successful on-chain tx proves the entire stack works.

### 4. Reduce Hardcoded Priority Fee

- **What:** Make `priority_fee` in `execute_atomic()` configurable via env var (currently hardcoded 5000 gwei)
- **Why:** 5000 gwei = ~$0.70/tx. Even successful $0.40 arbs would lose money. Needs to be ~500-1000 gwei.
- **File:** `src/rust-bot/src/arbitrage/executor.rs:445`

---

## Priority 2: Deploy Infrastructure

### 5. Order Hetzner Dedicated Server (A6)

- **What:** Hetzner AX102 (or AX52) in Falkenstein, Germany
- **Why:** Eliminates 250ms Alchemy round-trip. This is THE primary bottleneck. The bot's architecture is validated — it just needs speed.
- **Evidence:** 52 execution attempts, 0 captures. Spreads exist at detection but are gone 3.5s later.
- **Cost:** ~$140/mo (AX102) or ~$80/mo (AX52)
- **Details:** See `hetzner_bor_architecture.md`

### 6. Set up Bor + Heimdall Node

- **What:** Install Bor + Heimdall, download Polygon snapshot (~600GB), sync to tip
- **Why:** Local node = <1ms RPC, unfiltered P2P mempool, no rate limits
- **Time:** ~4-6 hours snapshot download + ~30min catchup
- **Details:** See `hetzner_bor_architecture.md` sections 2-4

### 7. Merge alloy Branch to Main

- **What:** Merge `feature/alloy-migration` → `main`
- **Why:** alloy 1.5 migration + fee_to_u24 fix + USDT/WETH expansion all validated. 80/80 tests.
- **How:** `git checkout main && git merge feature/alloy-migration`

---

## Priority 3: Optimization (after node is live)

### 8. IPC Transport (A7)
- Swap `connect_ws` for `connect_ipc()` on Hetzner with local Bor
- Status: config field ready, just need the node

### 9. USDC.e/Native USDC Stablecoin Arb
- Pure stablecoin arb between bridged and native USDC pools
- Near-zero risk, both tokens peg to $1

### 10. Parallel Opportunity Submission (A10)
- Submit top 2-3 opportunities simultaneously via `tokio::join!`

### 11. Dynamic Trade Sizing (A11)
- Spread-responsive sizing (wider spread → bigger trade)

### 12. Per-Route Performance Tracking
- HashMap of route → success/fail counts for data-driven optimization

### 13. Pre-built Transaction Templates (A12)
- Pre-construct and pre-sign tx skeletons, fill amounts at execution time

---

## Priority 4: Strategy Expansion (future)

### Triangular Arbitrage (A13)
USDC→WETH→WMATIC→USDC across 3 pools.

### Flash Loans (A14)
Aave/Balancer flash loans for $50K+ trades.

### Chain #2 (A15)
Replicate on a second chain with dedicated server.

---

## Deferred / Housekeeping

- Recover $145 USDT from Ethereum mainnet (needs ETH gas)
- Fix PairLookup ambiguity for multi-quote-token mempool detection
- Alerting system for Hetzner node health

---

## Quick Reference

```bash
# Build release binary
source ~/.cargo/env && cd ~/bots/dexarb/src/rust-bot && cargo build --release

# Run Polygon live bot
tmux new-session -d -s dexarb-live \
  "cd ~/bots/dexarb/src/rust-bot && RUST_LOG=dexarb_bot=info,warn ./target/release/dexarb-bot --chain polygon \
  2>&1 | tee ~/bots/dexarb/data/polygon/logs/live_$(date +%Y%m%d_%H%M%S).log"

# Run tests
cd ~/bots/dexarb/src/rust-bot && cargo test

# Check bot status
tmux capture-pane -t dexarb-live -p -S -30

# Check wallet balances
cast call 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174 "balanceOf(address)(uint256)" 0xa532eb528ae17efc881fce6894a08b5b70ff21e2 --rpc-url wss://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8
```

---

## Live Bot Status

Bot running in tmux `dexarb-live` with all 15 pairs active.
- Log: `data/polygon/logs/live_validation_20260203_002513.log`
- Config: `MIN_PROFIT_USD=0.01`, `MEMPOOL_MIN_PROFIT_USD=0.001`
- All quote tokens funded and approved (USDC.e, USDT, WETH, native USDC)
- Let it run — monitoring for first capture on less competitive pairs

---

*Last updated: 2026-02-03 00:30 UTC. See `master_plan.md` for full strategy.*
