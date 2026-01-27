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

## Status: Phase 1 Day 2 Complete

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
- [ ] Opportunity detection (Day 3)
- [ ] Trade execution (Day 4)
- [ ] Testnet testing (Day 5)
- [ ] Mainnet deployment (Day 6-7)

## Quick Start

```bash
# Source Rust environment
source ~/.cargo/env

# Build
cd src/rust-bot
cargo build --release

# Run
cargo run --release
```

## Strategy Overview

- **Technology Stack**: Rust (bot), Solidity (contracts), Polygon blockchain
- **Mechanism**: Flash loan arbitrage across DEXs (Uniswap, Sushiswap)
- **Risk Profile**: Low (atomic execution eliminates leg risk)
- **Target**: $5-20 profit/trade, 2-5 trades/day (Phase 1)

See [docs/dex-arbitrage-complete-strategy.md](docs/dex-arbitrage-complete-strategy.md) for full plan.

## Repository

- **Remote**: `git@github.com:strongca22-cpu/dexarb.git`
- **Wallet**: `0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2`
