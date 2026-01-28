# Session Summary: Multi-Config Paper Trading Implementation

**Date:** 2026-01-28
**Duration:** ~2 sessions
**Status:** ✅ Running in production (data collection + paper trading)

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
└─────────────────────────────────────────────────────────────┘
```

### New Files Created

| File | Purpose |
|------|---------|
| `src/bin/data_collector.rs` | Data collector binary |
| `src/bin/paper_trading.rs` | Paper trading binary (v2) |
| `src/data_collector/mod.rs` | Data collector module |
| `src/data_collector/shared_state.rs` | JSON shared state |
| `src/paper_trading/toml_config.rs` | TOML config reader |
| `config/paper_trading.toml` | Strategy configurations |

### Dependencies Added

```toml
toml = "0.8"
signal-hook = "0.3"
signal-hook-tokio = { version = "0.3", features = ["futures-v0_3"] }
```

---

## Current Architecture

### Binaries

1. **`data-collector`** - Continuous pool state syncing
2. **`paper-trading`** - Strategy execution with hot reload
3. **`dexarb-bot`** - Original monolithic bot (unchanged)

### Running Services

```bash
# View tmux session
tmux attach -t dexarb
# Window 0: collector - data collection
# Window 1: paper - paper trading

# Hot reload config
nano /home/botuser/bots/dexarb/config/paper_trading.toml
kill -HUP $(pgrep paper-trading)
```

### Key Files

| Path | Purpose |
|------|---------|
| `config/paper_trading.toml` | Edit strategies here |
| `data/pool_state.json` | Shared state (auto-generated) |
| `logs/data_collector_*.log` | Collector logs |
| `logs/paper_trading_*.log` | Paper trading logs |

---

## Current Status

- **Data Collector:** ✅ Running, syncing 4 pools every 1s
- **Paper Trading:** ✅ Running, 8 strategies enabled
- **Opportunities Found:** 0 (spreads too tight ~0.03%, need >0.3%)

### Why No Opportunities?

Current Polygon spreads (~0.03%) are below profit thresholds after accounting for:
- $0.50 estimated gas cost
- 15% slippage estimate

To test detection, edit `config/paper_trading.toml`:
```toml
min_profit_usd = -1.0  # Allow negative profit for testing
```

---

## To Resume

1. **Check services:**
   ```bash
   tmux ls
   ps aux | grep -E "(data-collector|paper-trading)"
   ```

2. **View logs:**
   ```bash
   tail -f /home/botuser/bots/dexarb/logs/paper_trading_*.log
   ```

3. **Modify strategies:**
   ```bash
   nano /home/botuser/bots/dexarb/config/paper_trading.toml
   kill -HUP $(pgrep paper-trading)
   ```

4. **Restart if needed:**
   ```bash
   tmux kill-session -t dexarb
   # Then restart per instructions in this doc
   ```

---

## Git Status

```
Uncommitted:
  config/paper_trading.toml          (new - TOML config)
  data/pool_state.json               (generated - shared state)
  src/bin/data_collector.rs          (new)
  src/data_collector/                (new module)
  src/paper_trading/toml_config.rs   (new)
  Cargo.toml                         (modified - new deps/bins)
  src/bin/paper_trading.rs           (modified - v2 architecture)
```

---

## Next Steps

1. **Monitor** for 24-48 hours
2. **Tune thresholds** based on actual spread distribution
3. **Add more pairs** if spreads improve on other tokens
4. **Deploy winning config** for live trading
