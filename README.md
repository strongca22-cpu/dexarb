# DEX Arbitrage Project

Atomic flash loan arbitrage on Polygon with Rust.

## Project Structure

```
dexarb/
├── docs/              # Strategy documentation and references
├── repos/             # Cloned sample GitHub repositories
├── src/
│   ├── rust-bot/      # Off-chain Rust arbitrage bot
│   └── contracts/     # Solidity smart contracts (Foundry)
└── README.md
```

## Strategy Overview

- **Technology Stack**: Rust (bot), Solidity (contracts), Polygon blockchain
- **Mechanism**: Flash loan arbitrage across DEXs (Uniswap, Sushiswap, etc.)
- **Risk Profile**: Low (atomic execution eliminates leg risk)

See [docs/dex-arbitrage-complete-strategy.md](docs/dex-arbitrage-complete-strategy.md) for full implementation plan.

## Repository

- **Remote**: git@github.com:strongca22-cpu/dexarb.git

## Status

Project initialization - awaiting sample repos and additional documentation.
