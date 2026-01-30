# DEX Arbitrage Project

Atomic flash loan arbitrage on Polygon with Rust.

## Project Structure

```
dexarb/
├── docs/                           # Strategy & implementation docs
│   ├── dex-arbitrage-complete-strategy.md
│   ├── phase1_implementation_plan.md
│   ├── phase1_execution_checklist.md
│   └── component_mapping_guide.md
├── repos/                          # Reference repos (gitignored)
│   ├── mev-template-rs/
│   ├── amms-rs/
│   ├── crypto-arbitrage-analyzer/
│   └── flashloan-rs/
├── src/
│   ├── rust-bot/                   # Phase 1 Rust arbitrage bot
│   │   ├── Cargo.toml              # Dependencies configured
│   │   ├── .env                    # Credentials (gitignored)
│   │   └── src/                    # Rust source code
│   └── contracts/                  # Solidity smart contracts (Phase 2)
└── README.md
```

## Status: LIVE — Tri-DEX Atomic Arb (UniV3 + SushiV3 + QuickSwapV3)

- [x] Repository initialized
- [x] Documentation added
- [x] Reference repos cloned
- [x] Rust project scaffolded
- [x] Environment configured (Alchemy RPC + wallet)
- [x] Connection verified (~100ms latency)
- [x] **Pool syncing implementation** (Day 2)
  - PoolStateManager with DashMap
  - PoolSyncer with Uniswap V2 ABIs
  - Quickswap + Sushiswap pools syncing
- [x] **Opportunity detection** (Day 3)
  - OpportunityDetector with spread calculation
  - Profit estimation with gas costs
  - All calculations in Rust (microsecond latency)
- [x] **Trade execution** (Day 4)
  - TradeExecutor with Uniswap V2 Router
  - DRY RUN mode by default (safe testing)
  - Slippage protection & gas price checks
- [x] **V3 Pool Integration** (Day 5)
  - V3PoolSyncer for Uniswap V3 concentrated liquidity
  - 3 fee tiers: 0.05%, 0.30%, 1.00%
  - Unified detector comparing V2↔V2, V2↔V3, V3↔V3
  - Key profitable routes: 0.05%↔1.00% (~$10/trade)
- [x] **Tax Logging** (IRS Compliance)
  - 34+ field TaxRecord for Form 8949
  - CSV + JSON dual logging
- [ ] Live trading validation
- [ ] Mainnet deployment

## Quick Start

```bash
source ~/.cargo/env
cd src/rust-bot
cargo build --release && cargo test

# Start live bot
tmux new-session -d -s livebot "cd ~/bots/dexarb/src/rust-bot && RUST_BACKTRACE=1 RUST_LOG=dexarb_bot=info ./target/release/dexarb-bot > ~/bots/dexarb/data/livebot.log 2>&1"

# Check status
tail -20 ~/bots/dexarb/data/livebot.log
```

## Strategy Overview

- **Technology Stack**: Rust (bot), Solidity (contracts), Polygon blockchain
- **Mechanism**: Flash loan arbitrage across DEXs (Uniswap, Sushiswap)
- **Risk Profile**: Low (atomic execution via ArbExecutorV2 — reverts on loss)
- **DEXes**: Uniswap V3, SushiSwap V3, QuickSwap V3 (Algebra Protocol)
- **Pools**: 17 active across 6 pairs (WETH, WMATIC, WBTC, USDT, DAI, LINK)
- **Target**: $5-20 profit/trade; WBTC UniV3↔QuickSwap best combo (60% profitable blocks)

See [docs/dex-arbitrage-complete-strategy.md](docs/dex-arbitrage-complete-strategy.md) for full plan.

## Adding New DEXes or Trading Pairs

**CRITICAL: TVL Assessment Required**

Before adding any new DEX or trading pair to the monitoring list, you MUST perform a TVL (Total Value Locked) assessment. Dead or illiquid pools generate false arbitrage opportunities that waste computation and produce misleading reports.

### TVL Assessment Checklist

1. **Check on-chain reserves** for each pool address
2. **Calculate TVL in USD** using current token prices
3. **Minimum threshold: $10,000 TVL** for inclusion
4. **Document findings** in verification reports

### How to Check TVL

```python
# Use pool reserves from pool_state.json
# TVL = (reserve0 * price0) + (reserve1 * price1)
# Adjust for token decimals: USDC=6, WETH/WMATIC=18, WBTC=8
```

### Currently Excluded Pools (< $1000 TVL)

| Pool | DEX | TVL | Reason |
|------|-----|-----|--------|
| UNI/USDC | Uniswap | $0.12 | Dead pool |
| UNI/USDC | Sushiswap | $550 | Low liquidity |
| LINK/USDC | Uniswap | $10.42 | Dead pool |
| LINK/USDC | Sushiswap | $86 | Low liquidity |
| LINK/USDC | Apeswap | $0.01 | Dead pool |
| WBTC/USDC | Apeswap | $0.09 | Dead pool |
| WBTC/USDC | Sushiswap | $501 | Low liquidity |
| WMATIC/USDC | Apeswap | $462 | Low liquidity |
| UNI/USDC | Apeswap | - | No pool exists |

### Exclusion List Location

Dead pools are excluded in `src/rust-bot/src/bin/paper_trading.rs`:

```rust
const EXCLUDED_POOLS: &[(&str, &str)] = &[
    ("Apeswap", "LINK/USDC"),
    ("Sushiswap", "LINK/USDC"),
    // ... etc
];
```

**Lesson Learned (2026-01-28)**: Without TVL checks, dead V2 pools with $0.12 TVL were being compared to V3 pools, generating $38+ "opportunities" that were completely unfillable.

---

## Tax Logging (IRS Compliance)

Comprehensive tax logging module for US federal tax compliance. See [docs/logging/tax_logging_implementation_plan.md](docs/logging/tax_logging_implementation_plan.md).

### Features

- **TaxRecord**: 34+ fields for IRS Form 8949 compliance
- **CSV/JSON logging**: Dual format for redundancy
- **RP2 export**: Compatible with [RP2 tax software](https://github.com/eprbell/rp2)
- **Price Oracle**: Automatic USD prices from pool state

### Quick Usage

```bash
# Export tax year to RP2 format
cargo run --bin tax-export -- --year 2026 --output rp2_2026.csv

# View tax summary
cargo run --bin tax-export -- --summary --year 2026
```

### Files

```
data/tax/
├── trades_2026.csv    # Primary tax log
├── trades_2026.jsonl  # JSON backup
└── rp2_export_2026.csv # RP2 format for tax software
```

---

## Repository

- **Remote**: `git@github.com:strongca22-cpu/dexarb.git`
- **Wallet**: `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2`
