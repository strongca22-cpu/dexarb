# Session Summary: Multi-Config Paper Trading Implementation

**Date:** 2026-01-28
**Duration:** ~3 sessions
**Status:** ✅ Running in production (data collection + paper trading + Discord alerts)

---

## Objective

Implement multi-configuration paper trading system to test strategies simultaneously on live data before deploying capital.

---

## Session 1: Initial Implementation

### 1. Reviewed External Resources

**Cloned repos in `dexarb/repos/`:**
- `amms-rs` - darkforestry's AMM library (StateSpaceManager, V2/V3 syncing)
- `artemis` - Paradigm's MEV framework (Collector/Strategy/Executor pattern)
- `mev-template-rs` - DeGatchi's MEV bot template
- `flashloan-rs` - Flashloan SDK

**Key insight:** amms-rs uses `Arc<RwLock<StateSpace>>` pattern.

### 2. Implemented Artemis Pattern

Created `src/paper_trading/` module with Collector/Strategy/Executor traits.

---

## Session 2: Split Architecture with Hot Reload

### Problem Identified

User wanted ability to modify paper trading parameters on the fly without restarting data collection.

### Solution: File-Based Architecture

Split into two independent processes:

```
┌─────────────────────────────────────────────────────────────┐
│              DATA COLLECTOR (always running)                │
│  - Syncs pools every 1s from Polygon RPC                    │
│  - Writes to shared JSON state file                         │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼ /data/pool_state.json
┌─────────────────────────────────────────────────────────────┐
│              PAPER TRADING (hot-reloadable)                 │
│  - Reads from shared state file                             │
│  - Reads config from TOML                                   │
│  - SIGHUP handler for config reload                         │
│  - Discord webhook alerts                                   │
└─────────────────────────────────────────────────────────────┘
```

---

## Session 3: Executable Spread Fix + Discord Alerts

### Critical Bug Fix: Midmarket vs Executable Spread

**Problem:** Original code measured midmarket spread (raw price difference between DEXs).
This incorrectly showed ~0.29% opportunities that would actually be unprofitable.

**Solution:** Calculate executable spread = midmarket spread - DEX fees (0.6% round trip)

```rust
const DEX_FEE_PERCENT: f64 = 0.30;  // 0.3% per swap
const ROUND_TRIP_FEE_PERCENT: f64 = DEX_FEE_PERCENT * 2.0;  // 0.6%

let executable_spread = midmarket_spread - (ROUND_TRIP_FEE_PERCENT / 100.0);
```

**Result:** 0.29% midmarket spread becomes -0.31% executable (unprofitable). Need >0.75-1.0% midmarket for profit.

### Added Discord Alerts

New module `src/paper_trading/discord_alerts.rs`:
- Sends webhook notifications when opportunities detected
- Aggregates across all 13 strategies
- Shows which strategies caught it, won/lost to competition, best strategy, profit range

### Added Discovery Mode (Scenario 13)

Ultra-low threshold strategy to detect ANY opportunities for market analysis:
```toml
[[strategy]]
name = "Discovery Mode"
min_profit_usd = -50.0
max_slippage_percent = 0.001
simulate_competition = false
```

---

## Current Architecture

### Binaries

1. **`data-collector`** - Continuous pool state syncing
2. **`paper-trading`** - Strategy execution with hot reload + Discord alerts
3. **`dexarb-bot`** - Original monolithic bot (unchanged)

### Running Services

```bash
# View tmux session
tmux attach -t dexarb
# Window 0: collector - data collection
# Window 1: paper - paper trading with Discord

# Hot reload config
nano /home/botuser/bots/dexarb/config/paper_trading.toml
kill -HUP $(pgrep paper-trading)
```

### Key Files

| Path | Purpose |
|------|---------|
| `config/paper_trading.toml` | 13 strategy configurations |
| `src/rust-bot/.env` | Discord webhook + RPC config |
| `data/pool_state.json` | Shared state (auto-generated) |
| `logs/paper_trading_*.log` | Paper trading logs |
| `src/paper_trading/discord_alerts.rs` | Discord notification module |

---

## 13 Paper Trading Strategies

| # | Strategy | Threshold | Trade Size | Competition |
|---|----------|-----------|------------|-------------|
| 1 | Conservative | 0.25% | $500 | 80% |
| 2 | Moderate | 0.50% | $1,000 | 60% |
| 3 | Aggressive | 1.00% | $1,500 | 40% |
| 4 | Whale | 0.40% | $5,000 | 75% |
| 5 | Micro Trader | 0.50% | $100 | 50% |
| 6 | WETH Specialist | 0.50% | $1,200 | 65% |
| 7 | WMATIC Specialist | 0.60% | $1,000 | 45% |
| 8 | Diversifier | 0.50% | $1,000 | 55% |
| 9 | Speed Demon | 0.50% | $1,000 | 55% |
| 10 | Tortoise | 0.40% | $1,000 | 70% |
| 11 | Gas Cowboy | 0.50% | $1,200 | 50% |
| 12 | Penny Pincher | 0.50% | $1,000 | 65% |
| 13 | Discovery Mode | 0.001% | $100 | 0% (disabled) |

---

## Current Status

- **Data Collector:** ✅ Running, syncing 4 pools every 1s
- **Paper Trading:** ✅ Running, 13 strategies enabled
- **Discord Alerts:** ✅ Enabled (webhook configured)
- **Opportunities Found:** 0 (executable spreads below 0.6% threshold)

### Why No Opportunities?

Current Polygon DEX markets are highly efficient:
- Midmarket spreads: ~0.003% - 0.3%
- After 0.6% DEX fees: All negative executable spread
- Need >0.75% midmarket spread for any profit

---

## Key Learnings

1. **Midmarket ≠ Executable:** Always subtract DEX fees (0.6% for Uniswap V2/Sushi)
2. **Market Efficiency:** Professional MEV bots keep spreads near-zero
3. **Latency Matters:** 1s polling insufficient for competitive arbitrage
4. **Discovery Mode:** Useful for understanding actual spread distribution

---

## To Resume

1. **Check services:**
   ```bash
   tmux attach -t dexarb
   pgrep -a "paper-trading\|data-collector"
   ```

2. **View Discord alerts:** Check configured Discord channel

3. **Restart with webhook:**
   ```bash
   cd /home/botuser/bots/dexarb/src/rust-bot
   DISCORD_WEBHOOK="your_webhook_url" ./target/release/paper-trading
   ```

---

## Next Steps

1. **Monitor** for 24-48 hours for volatility events
2. **Consider websockets** for lower latency data collection
3. **Add more DEXs** (QuickSwap, Balancer) with different fee structures
4. **Analyze** when spreads actually exceed 0.6% threshold
