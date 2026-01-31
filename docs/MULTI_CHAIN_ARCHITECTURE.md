# Multi-Chain DEX Arbitrage Architecture (Revised)

**Strategy:** One repo, separate bot processes per chain
**Chains:** Polygon (live) → Base (next) → Arbitrum, Optimism (placeholder)
**Refactor scope:** Moderate — parameterize chain-specific values, keep existing module structure

---

## Document Status

| Version | Date | Notes |
|---------|------|-------|
| v1.0 | 2026-01-30 | Initial draft (without codebase access) |
| **v2.0** | **2026-01-31** | **Revised with direct codebase analysis** |

**v2.0 changes:** Corrected directory structure, aligned with `.env`-based config pattern,
identified actual hardcoded values, scoped DEX differences between chains, sized the
refactoring work against real code.

---

# PART A: Architecture Assessment (What We Actually Have)

## A1. Actual Codebase Structure

```
~/bots/dexarb/
├── src/
│   ├── rust-bot/                        # Rust arbitrage bot
│   │   ├── Cargo.toml                   # Package: dexarb-bot v0.1.0
│   │   ├── .env                         # Dev/paper trading config
│   │   ├── .env.live                    # LIVE config (Polygon, real money)
│   │   ├── src/
│   │   │   ├── main.rs                  # Entry point (monolithic main loop)
│   │   │   ├── config.rs                # dotenv loader: load_config_from_file()
│   │   │   ├── types.rs                 # DexType enum, BotConfig, PoolState, V3PoolState
│   │   │   ├── lib.rs                   # Module exports
│   │   │   ├── price_logger.rs          # CSV price logging (research)
│   │   │   ├── arbitrage/
│   │   │   │   ├── detector.rs          # Opportunity detection (V3+V2 unified)
│   │   │   │   ├── executor.rs          # Trade execution (atomic + legacy)
│   │   │   │   ├── multicall_quoter.rs  # Batch Quoter pre-screening
│   │   │   │   └── mod.rs
│   │   │   ├── pool/
│   │   │   │   ├── state.rs             # PoolStateManager (DashMap)
│   │   │   │   ├── v3_syncer.rs         # V3 sync (Uniswap/Sushi/Algebra)
│   │   │   │   ├── v2_syncer.rs         # V2 sync (QuickSwap/Sushi V2)
│   │   │   │   ├── calculator.rs        # Price calculations
│   │   │   │   └── mod.rs
│   │   │   ├── filters/
│   │   │   │   ├── whitelist.rs         # JSON whitelist enforcement
│   │   │   │   └── mod.rs
│   │   │   ├── tax/                     # IRS Form 8949 logging
│   │   │   ├── paper_trading/           # Paper trading framework
│   │   │   ├── utils/
│   │   │   └── bin/                     # Additional binaries
│   │   │       ├── paper_trading.rs
│   │   │       ├── data_collector.rs
│   │   │       └── tax_export.rs
│   │   └── target/                      # Build artifacts
│   │
│   └── contracts/                       # Solidity (Foundry)
│       ├── foundry.toml
│       ├── src/
│       │   └── ArbExecutor.sol          # Atomic executor (V3+V2, fee sentinel routing)
│       ├── test/
│       ├── script/
│       └── lib/
│
├── config/
│   └── pools_whitelist.json             # Polygon pool whitelist (v1.4)
│
├── data/
│   ├── price_history/                   # V3 price CSVs
│   ├── tax/                             # Trade tax records
│   └── *.log                            # Bot logs
│
├── scripts/                             # Python/Bash utilities
│   ├── verify_v2_pools.py
│   ├── verify_whitelist.py
│   ├── bot_status_discord.sh
│   ├── bot_watch.sh
│   └── ...
│
├── docs/
├── repos/                               # Reference implementations (gitignored)
└── README.md
```

## A2. What Is Already Chain-Agnostic

These components work on any EVM chain **without code changes**:

| Component | File | Why It Works |
|-----------|------|--------------|
| Pool state manager | `pool/state.rs` | Generic DashMap storage, no chain assumptions |
| V3 pool syncer | `pool/v3_syncer.rs` | Generic over `Middleware`, reads from config |
| V2 pool syncer | `pool/v2_syncer.rs` | Generic over `Middleware`, reads from config |
| Trade executor | `arbitrage/executor.rs` | Routes via DexType + config addresses |
| ArbExecutor.sol | `contracts/src/ArbExecutor.sol` | Fee sentinel routing is chain-agnostic |
| Tax logger | `tax/` | Records chain_id from config |
| Price logger | `price_logger.rs` | Writes to configurable directory |
| Whitelist filter | `filters/whitelist.rs` | Reads from configurable JSON path |
| BotConfig loader | `config.rs` | Loads from any `.env` file via `load_config_from_file()` |

## A3. What Is Polygon-Specific (Must Change)

| Item | Location | Current Value | What to Do |
|------|----------|---------------|------------|
| **USDC quote token address** | `detector.rs:25` | `0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174` (USDC.e) | Move to `.env` as `QUOTE_TOKEN_ADDRESS` |
| **Gas cost estimate** | `detector.rs:34` | `$0.05` (Polygon gas math) | Move to `.env` as `ESTIMATED_GAS_COST_USD` |
| **Whitelist fallback path** | `main.rs:73` | `/home/botuser/bots/dexarb/config/pools_whitelist.json` | Make relative or chain-parameterized |
| **Tax log dir fallback** | `main.rs:273` | `/home/botuser/bots/dexarb/data/tax` | Use `data/{chain}/tax` |
| **Price log dir fallback** | `main.rs:284` | `/home/botuser/bots/dexarb/data/price_history` | Use `data/{chain}/price_history` |
| **Config file path** | `main.rs:53` | Hardcoded `.env.live` | Select `.env.{chain}` via CLI arg |
| **Log message** | `main.rs:62` | `"Connecting to Polygon via WebSocket"` | Use chain name from config |
| **DexType variants** | `types.rs` | QuickswapV3, QuickSwapV2 (Polygon-only DEXes) | Keep; simply unused on Base |
| **V2 fee constant** | `detector.rs:37` | 0.30% | Correct for all V2 forks; no change needed |

### What Does NOT Need to Change

- **DexType enum**: Polygon-specific variants (QuickswapV3, QuickSwapV2) can stay.
  Base simply won't have pools of those types in its whitelist. No dead code to remove.
- **Algebra ABI in executor.rs**: The `IAlgebraSwapRouter` ABI stays compiled in.
  It's only invoked when `fee=0` in ArbExecutor.sol, which Base pools won't trigger.
- **ArbExecutor.sol**: Deploy as-is to Base. The Algebra path (`fee=0`) is unreachable
  without Algebra routers, but costs nothing to keep.

---

# PART B: Multi-Chain Refactoring Plan

## B1. Add Chain Selection to Entry Point

**Goal:** Select chain via `--chain` CLI argument or `CHAIN` env var.

### B1.1 New `.env` File Naming Convention

```
src/rust-bot/
├── .env                   # Dev/paper (unchanged)
├── .env.polygon           # Renamed from .env.live
├── .env.base              # New
├── .env.arbitrum           # Placeholder (future)
└── .env.optimism           # Placeholder (future)
```

**Migration:** `cp .env.live .env.polygon` (keep `.env.live` as symlink or alias for
backwards compatibility during transition).

### B1.2 Add CLI Argument to main.rs

Add `clap` dependency and `--chain` flag. Minimal change to entry point:

```rust
// src/main.rs — new chain selection (top of main())
use clap::Parser;

#[derive(Parser)]
#[command(name = "dexarb-bot")]
struct Args {
    /// Chain to run on (polygon, base)
    #[arg(short, long, env = "CHAIN", default_value = "polygon")]
    chain: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let chain = args.chain.to_lowercase();

    // Validate chain
    match chain.as_str() {
        "polygon" | "base" => {},
        _ => anyhow::bail!("Unsupported chain: {}. Use: polygon, base", chain),
    }

    // Load chain-specific .env file
    let env_file = format!(".env.{}", chain);
    let config = load_config_from_file(&env_file)?;

    info!("Starting DEX Arbitrage Bot for {} (chain_id: {})", chain, config.chain_id);
    // ... rest of main unchanged
}
```

**Cargo.toml addition:**
```toml
clap = { version = "4.0", features = ["derive", "env"] }
```

### B1.3 Add New BotConfig Fields

Add these to `BotConfig` in `types.rs` and load from `.env` in `config.rs`:

```rust
// New fields in BotConfig
pub chain_name: String,                    // "polygon" or "base"
pub quote_token_address: Address,          // USDC address for this chain
pub estimated_gas_cost_usd: f64,           // Gas cost per trade in USD
```

**New .env variables** (added to each chain's `.env` file):
```bash
CHAIN_NAME=polygon
QUOTE_TOKEN_ADDRESS=0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174
ESTIMATED_GAS_COST_USD=0.05
```

### B1.4 Update detector.rs to Use Config Values

Replace hardcoded constants with config fields:

```rust
// BEFORE (detector.rs:25):
const USDC_ADDRESS: &str = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174";
const ESTIMATED_GAS_COST_USD: f64 = 0.05;

// AFTER:
// Remove both constants. Use self.config.quote_token_address and
// self.config.estimated_gas_cost_usd in check_pair_unified()
```

### B1.5 Make Data Paths Chain-Aware

Update fallback paths in `main.rs` to include chain name:

```rust
// Tax log dir
let tax_dir = config.tax_log_dir.clone()
    .unwrap_or_else(|| format!("/home/botuser/bots/dexarb/data/{}/tax", config.chain_name));

// Price log dir
let log_dir = config.price_log_dir.clone()
    .unwrap_or_else(|| format!("/home/botuser/bots/dexarb/data/{}/price_history", config.chain_name));
```

---

## B2. Per-Chain Configuration

### B2.1 Polygon .env.polygon (Renamed from .env.live)

All current values stay the same. Add new fields:

```bash
# New fields for multi-chain support
CHAIN_NAME=polygon
QUOTE_TOKEN_ADDRESS=0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174
ESTIMATED_GAS_COST_USD=0.05

# Chain-specific data paths
TAX_LOG_DIR=/home/botuser/bots/dexarb/data/polygon/tax
PRICE_LOG_DIR=/home/botuser/bots/dexarb/data/polygon/price_history
WHITELIST_FILE=/home/botuser/bots/dexarb/config/polygon/pools_whitelist.json
```

### B2.2 Base .env.base (New)

```bash
# .env.base — Base Network Configuration
#
# Created: 2026-01-31

# Network Configuration (Base Mainnet)
RPC_URL=wss://base-mainnet.g.alchemy.com/v2/YOUR_ALCHEMY_KEY
CHAIN_ID=8453
CHAIN_NAME=base

# Quote token (native USDC on Base — NOT USDC.e)
QUOTE_TOKEN_ADDRESS=0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913
ESTIMATED_GAS_COST_USD=0.02

# Wallet Configuration (same wallet works on all EVM chains)
PRIVATE_KEY=<same-or-different-key>

# Trading Parameters (conservative for initial data collection)
MIN_PROFIT_USD=0.05
MAX_TRADE_SIZE_USD=100.0
MAX_SLIPPAGE_PERCENT=0.5

# DEX Addresses (Base Mainnet)
# V2 routers — needed by BotConfig (required fields)
# Use Uniswap Universal Router addresses or zero addresses if unused
UNISWAP_ROUTER=0x0000000000000000000000000000000000000000
SUSHISWAP_ROUTER=0x0000000000000000000000000000000000000000
UNISWAP_FACTORY=0x0000000000000000000000000000000000000000
SUSHISWAP_FACTORY=0x0000000000000000000000000000000000000000

# Uniswap V3 (Base)
UNISWAP_V3_FACTORY=0x33128a8fC17869897dcE68Ed026d694621f6FDfD
UNISWAP_V3_ROUTER=0x2626664c2603336E57B271c5C0b26F421741e481
UNISWAP_V3_QUOTER=0x3d4e44Eb1374240CE5F1B871ab261CD16335B76a

# SushiSwap V3 (Base)
SUSHISWAP_V3_FACTORY=0xc35DADB65012eC5796536bD9864eD8773aBc74C4
SUSHISWAP_V3_ROUTER=0xFB7eF66a7e61224DD6FcD0D7d9C3be5C8B049b9f
SUSHISWAP_V3_QUOTER=0xb1E835Dc2785b52265711e17fCCb0fd018226a6e

# No QuickSwap/Algebra on Base — leave unset
# QUICKSWAP_V3_FACTORY=
# QUICKSWAP_V3_ROUTER=
# QUICKSWAP_V3_QUOTER=

# Trading Pairs (Base — USDC-quoted, same format as Polygon)
# WETH: 0x4200000000000000000000000000000000000006
# USDC: 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913
# cbETH: 0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22
TRADING_PAIRS=0x4200000000000000000000000000000000000006:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913:WETH/USDC

# Performance Settings
POLL_INTERVAL_MS=3000

# Data paths (chain-specific)
WHITELIST_FILE=/home/botuser/bots/dexarb/config/base/pools_whitelist.json
TAX_LOG_DIR=/home/botuser/bots/dexarb/data/base/tax
PRICE_LOG_DIR=/home/botuser/bots/dexarb/data/base/price_history

# Logging
RUST_LOG=dexarb_bot=info,warn

# Tax Logging
TAX_LOG_ENABLED=true

# Price Logging (data collection phase)
PRICE_LOG_ENABLED=true

# Atomic Executor (deploy to Base then fill in)
# ARB_EXECUTOR_ADDRESS=

# Start in DRY RUN mode for data collection
LIVE_MODE=false
```

### B2.3 Config Changes Required in config.rs

Make the four V2 router/factory fields optional (Base doesn't use V2 DEXes initially):

```rust
// types.rs — change required V2 fields to optional
pub uniswap_router: Option<Address>,      // was: Address (required)
pub sushiswap_router: Option<Address>,     // was: Address (required)
pub uniswap_factory: Option<Address>,      // was: Address (required)
pub sushiswap_factory: Option<Address>,    // was: Address (required)
```

**Alternative (lower risk):** Keep them required but accept zero addresses.
The executor already checks DexType before routing, so a zero-address router
is never actually called. This avoids changing the BotConfig struct signature.

**Recommendation:** Keep required, accept zero addresses. Lower risk to live Polygon bot.

---

## B3. Per-Chain Directory Layout

### B3.1 Config Directories

```
config/
├── polygon/
│   └── pools_whitelist.json       # Move from config/pools_whitelist.json
├── base/
│   └── pools_whitelist.json       # New (V3-only, Uniswap + Sushi)
├── arbitrum/                       # Placeholder
│   └── .gitkeep
└── optimism/                       # Placeholder
    └── .gitkeep
```

**Migration:** `cp config/pools_whitelist.json config/polygon/pools_whitelist.json`
(keep original at `config/pools_whitelist.json` until `.env.polygon` is fully tested).

### B3.2 Data Directories

```
data/
├── polygon/
│   ├── price_history/             # Move from data/price_history/
│   ├── tax/                       # Move from data/tax/
│   └── logs/
├── base/
│   ├── price_history/             # New
│   ├── tax/                       # New
│   └── logs/
├── arbitrum/                       # Placeholder
│   └── .gitkeep
└── optimism/                       # Placeholder
    └── .gitkeep
```

**Migration:** Create directories, copy existing data (per CLAUDE.md: copy, don't move).

---

## B4. Per-Chain Whitelist

### B4.1 Base Pool Whitelist Template

Base V3-only (Uniswap V3 + SushiSwap V3). No QuickSwap/Algebra, no V2 initially.

```json
{
  "version": "1.0",
  "last_updated": "2026-01-31T00:00:00Z",
  "changelog": "v1.0: Initial Base whitelist — V3 only (Uniswap V3 + SushiSwap V3). Pool addresses TBD after on-chain discovery.",
  "config": {
    "default_min_liquidity": 1000000000,
    "whitelist_enforcement": "strict",
    "liquidity_thresholds": {
      "v3_100": 10000000000,
      "v3_500": 5000000000,
      "v3_3000": 3000000000,
      "v3_10000": 0
    }
  },
  "whitelist": {
    "pools": [
      {
        "address": "0xTODO_DISCOVER",
        "pair": "WETH/USDC",
        "dex": "UniswapV3",
        "fee_tier": 500,
        "status": "active",
        "min_liquidity": 5000000000,
        "notes": "Discover via cast call on Base factory",
        "added": "2026-01-31"
      },
      {
        "address": "0xTODO_DISCOVER",
        "pair": "WETH/USDC",
        "dex": "UniswapV3",
        "fee_tier": 3000,
        "status": "active",
        "min_liquidity": 3000000000,
        "notes": "Discover via cast call on Base factory",
        "added": "2026-01-31"
      },
      {
        "address": "0xTODO_DISCOVER",
        "pair": "WETH/USDC",
        "dex": "SushiswapV3",
        "fee_tier": 500,
        "status": "active",
        "min_liquidity": 5000000000,
        "notes": "Discover via cast call on Base SushiSwap factory",
        "added": "2026-01-31"
      }
    ]
  },
  "blacklist": {
    "pools": [],
    "fee_tiers": [10000],
    "pairs": []
  }
}
```

### B4.2 Pool Discovery for Base

Use `cast call` (Foundry) to query factory contracts on Base:

```bash
# Uniswap V3 Factory: getPool(tokenA, tokenB, fee) → pool address
WETH=0x4200000000000000000000000000000000000006
USDC=0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913
FACTORY=0x33128a8fC17869897dcE68Ed026d694621f6FDfD

# WETH/USDC 0.05% (fee=500)
cast call $FACTORY "getPool(address,address,uint24)(address)" $WETH $USDC 500 --rpc-url https://mainnet.base.org

# WETH/USDC 0.30% (fee=3000)
cast call $FACTORY "getPool(address,address,uint24)(address)" $WETH $USDC 3000 --rpc-url https://mainnet.base.org

# WETH/USDC 0.01% (fee=100) — stablecoin tier
cast call $FACTORY "getPool(address,address,uint24)(address)" $WETH $USDC 100 --rpc-url https://mainnet.base.org
```

Then run `verify_whitelist.py` adapted for Base (add `--chain` arg).

### B4.3 Initial Base Trading Pairs

| Pair | Priority | Base Token Addresses | Notes |
|------|----------|---------------------|-------|
| **WETH/USDC** | HIGH | WETH: `0x4200...0006`, USDC: `0x8335...fCD6` | Deepest pool on Base |
| **cbETH/WETH** | MEDIUM | cbETH: `0x2Ae3...DEc22`, WETH: `0x4200...0006` | Coinbase ecosystem, unique to Base |
| **USDC/USDT** | LOW | USDC: `0x8335...fCD6`, USDT: `0xfde4...4f71` | Stablecoin arb |
| **WETH/DAI** | LOW | WETH: `0x4200...0006`, DAI: `0x50c5...f0Cb` | Smaller liquidity |

**Start with WETH/USDC only.** Add pairs after data collection confirms viable spreads.

---

## B5. ArbExecutor.sol Deployment to Base

### B5.1 Why No Contract Changes Are Needed

The ArbExecutor.sol contract is already chain-agnostic:

- **Standard V3 path** (`fee=1..65535`): Works for Uniswap V3 and SushiSwap V3 on Base
- **V2 path** (`fee=16777215`): Works if V2 DEXes are added later on Base
- **Algebra path** (`fee=0`): Simply unreachable on Base (no Algebra routers)

Deploy the same bytecode. The constructor only sets `owner = msg.sender`.

### B5.2 Deployment Steps

```bash
cd ~/bots/dexarb/src/contracts

# Deploy to Base
forge create --rpc-url $BASE_RPC_URL \
  --private-key $PRIVATE_KEY \
  src/ArbExecutor.sol:ArbExecutor

# Save the deployed address → put in .env.base as ARB_EXECUTOR_ADDRESS

# Approve USDC for the executor on Base
# (same pattern as Polygon: max uint256 approval)
cast send $BASE_USDC "approve(address,uint256)" $EXECUTOR_ADDRESS $(cast max-uint) \
  --rpc-url $BASE_RPC_URL --private-key $PRIVATE_KEY
```

---

## B6. DEX Differences: Polygon vs Base

| Aspect | Polygon | Base |
|--------|---------|------|
| **Quote token** | USDC.e (`0x2791...`) | Native USDC (`0x8335...`) |
| **WETH** | `0x7ceB...` (WETH) | `0x4200...0006` (WETH) |
| **Native token** | MATIC (~$0.50) | ETH (~$3300) |
| **Uniswap V3** | Yes | Yes |
| **SushiSwap V3** | Yes | Yes |
| **QuickSwap V3 (Algebra)** | Yes (dynamic fees) | **No** |
| **QuickSwap V2** | Yes | **No** |
| **SushiSwap V2** | Yes | **No** (initially) |
| **Aerodrome** | No | **No** (Phase 2 — different AMM model) |
| **BaseSwap** | No | **No** (Phase 2 — V2 fork) |
| **Gas cost** | ~$0.02-0.05/trade | ~$0.005-0.02/trade |
| **Block time** | ~2s | ~2s |
| **Competition** | Moderate | Lower (newer ecosystem) |

### Key Implication

On Base, the bot runs with **fewer DEX pairs per pool** (only Uniswap V3 + SushiSwap V3).
This means:
- Fewer cross-DEX arbitrage opportunities (no V2↔V3, no Algebra↔V3)
- Main opportunity: **cross-fee-tier arb within Uniswap V3** (0.05% ↔ 0.30%)
  and **Uniswap V3 ↔ SushiSwap V3** cross-DEX at same fee tier
- Lower gas costs partially compensate for fewer opportunities

---

## B7. Required BotConfig Changes (Minimal)

### New Optional Fields

```rust
// Add to BotConfig in types.rs
pub chain_name: String,                         // "polygon", "base"
pub quote_token_address: Option<Address>,        // Replaces hardcoded USDC_ADDRESS
pub estimated_gas_cost_usd: Option<f64>,         // Replaces hardcoded $0.05
```

### New .env Variables

```bash
CHAIN_NAME=base                                            # Required
QUOTE_TOKEN_ADDRESS=0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913  # Optional (defaults to Polygon USDC.e for backwards compat)
ESTIMATED_GAS_COST_USD=0.02                                 # Optional (defaults to 0.05)
```

### Backwards Compatibility

If `QUOTE_TOKEN_ADDRESS` is not set, fall back to the current Polygon USDC.e address.
If `CHAIN_NAME` is not set, default to `"polygon"`.
This means the existing `.env.live` works without modification during transition.

---

# PART C: Running Multiple Bots

## C1. Same Binary, Different Configs

The same compiled `dexarb-bot` binary serves all chains. Chain behavior is entirely
determined by the `.env.{chain}` file it loads.

```bash
# Build once
cd ~/bots/dexarb/src/rust-bot
cargo build --release

# Run Polygon
./target/release/dexarb-bot --chain polygon

# Run Base (separate process)
./target/release/dexarb-bot --chain base
```

## C2. Process Management (tmux)

Current setup uses tmux. For multi-chain, use named sessions:

```bash
# Polygon bot
tmux new-session -d -s dexarb-polygon \
  'cd ~/bots/dexarb/src/rust-bot && ./target/release/dexarb-bot --chain polygon 2>&1 | tee ~/bots/dexarb/data/polygon/logs/livebot.log'

# Base bot
tmux new-session -d -s dexarb-base \
  'cd ~/bots/dexarb/src/rust-bot && ./target/release/dexarb-bot --chain base 2>&1 | tee ~/bots/dexarb/data/base/logs/livebot.log'

# Monitor both
tmux ls
tmux attach -t dexarb-polygon
tmux attach -t dexarb-base
```

## C3. Systemd Services (Future — When Stable)

Template service for automatic start/restart:

```ini
# /etc/systemd/system/dexarb@.service
[Unit]
Description=DEX Arb Bot - %i
After=network.target

[Service]
Type=simple
User=botuser
WorkingDirectory=/home/botuser/bots/dexarb/src/rust-bot
ExecStart=/home/botuser/bots/dexarb/src/rust-bot/target/release/dexarb-bot --chain %i
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal
SyslogIdentifier=dexarb-%i

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable dexarb@polygon
sudo systemctl enable dexarb@base
sudo systemctl start dexarb@polygon
sudo systemctl start dexarb@base
```

## C4. VPS Resource Considerations

**Current VPS:** 1 vCPU, 2GB RAM.

| Resource | Polygon Bot | Base Bot | Combined |
|----------|-------------|----------|----------|
| RAM (est.) | ~80-120MB | ~60-100MB | ~180-220MB |
| WS connections | 2 | 2 | 4 |
| RPC calls/block | ~25 (23 pools) | ~5-10 (fewer pools) | ~35 |
| CPU per block | Light (~50ms) | Light (~30ms) | <100ms |

**Assessment:** Two bots fit comfortably on 2GB. The main constraint is RPC rate limits
(Alchemy free tier: 22.2M calls/month). At 25 calls/block × 1 block/2s × 86400s/day =
~1.08M calls/day for Polygon alone. Adding Base (~0.3M/day) = ~1.38M total, well within
the 22.2M monthly limit. Base may need its own Alchemy API key for separate rate tracking.

---

## C5. Discord Reporting (Multi-Chain)

Extend `bot_status_discord.sh` and `hourly_discord_report.py` with `--chain` argument:

```bash
# Report for Polygon
python3 scripts/hourly_discord_report.py --chain polygon

# Report for Base
python3 scripts/hourly_discord_report.py --chain base
```

Each chain reports to the same Discord webhook but prefixes messages with chain name.

---

# PART D: Implementation Checklist

## Phase 1: Refactor for Multi-Chain (Polygon Stays Live)

- [ ] Add `clap` to Cargo.toml dependencies
- [ ] Add `--chain` argument parsing to `main.rs`
- [ ] Add `CHAIN_NAME`, `QUOTE_TOKEN_ADDRESS`, `ESTIMATED_GAS_COST_USD` to BotConfig
- [ ] Update `config.rs` to load new fields from env
- [ ] Update `detector.rs` to use `config.quote_token_address` instead of const
- [ ] Update `detector.rs` to use `config.estimated_gas_cost_usd` instead of const
- [ ] Make data path fallbacks chain-aware in `main.rs`
- [ ] Create `config/polygon/` directory, copy whitelist there
- [ ] Create `data/polygon/{price_history,tax,logs}/` directories, copy existing data
- [ ] Create `.env.polygon` from `.env.live`, add new fields
- [ ] Test: `--chain polygon` produces identical behavior to current `.env.live`
- [ ] Commit: "feat: multi-chain support — chain selection via --chain flag"

## Phase 2: Add Base Support

- [ ] Obtain Alchemy API key for Base (or use public RPC for discovery)
- [ ] Obtain BaseScan API key
- [ ] Run pool discovery on Base (cast call for Uniswap V3 + SushiSwap V3 factories)
- [ ] Verify pool liquidity on Base (adapt verify_whitelist.py with --chain)
- [ ] Create `config/base/pools_whitelist.json` with discovered pools
- [ ] Create `.env.base` with Base-specific addresses
- [ ] Create `data/base/{price_history,tax,logs}/` directories
- [ ] Deploy ArbExecutor.sol to Base
- [ ] Approve USDC on Base for executor
- [ ] Update `.env.base` with executor address
- [ ] Test: `--chain base --dry-run` syncs pools and detects opportunities
- [ ] Fund Base wallet with ETH for gas (~0.01 ETH = ~$33)
- [ ] Fund Base wallet with USDC for trading (~$200-500)
- [ ] Commit: "feat: Base chain support — V3-only (Uniswap + SushiSwap)"

## Phase 3: Parallel Operation

- [ ] Start Base bot in data-collection mode (`LIVE_MODE=false`)
- [ ] Monitor Base price logs for 48+ hours
- [ ] Analyze Base spread patterns (do opportunities exist?)
- [ ] If viable: enable live mode on Base (`LIVE_MODE=true`, conservative sizing)
- [ ] Set up dual tmux sessions (or systemd if stable)
- [ ] Update Discord reporting for both chains

## Phase 4: Placeholder Chains (No Implementation Yet)

- [ ] Create `config/arbitrum/.gitkeep`
- [ ] Create `config/optimism/.gitkeep`
- [ ] Create `data/arbitrum/.gitkeep`
- [ ] Create `data/optimism/.gitkeep`

---

# PART E: What NOT to Change (Scope Control)

To keep this refactoring safe and bounded:

1. **Do NOT reorganize the Rust module structure** (`src/arbitrage/`, `src/pool/`, etc.).
   The current layout works. A `src/chains/` module is unnecessary — chain selection
   happens at the `.env` level, not in Rust code.

2. **Do NOT create a Chain enum or chain registry in Rust.** The `.env` file IS the
   chain config. Adding a Rust-level chain abstraction adds complexity with no benefit
   for 2 chains.

3. **Do NOT convert to TOML config.** The `.env` + `dotenv` pattern works, is proven,
   and the BotConfig struct already handles all parsing. Converting to TOML would
   require rewriting `config.rs` and all tests.

4. **Do NOT modify ArbExecutor.sol.** Deploy as-is to Base. The Algebra path is
   harmless dead code on chains without Algebra DEXes.

5. **Do NOT add Aerodrome/BaseSwap/V2 support in this phase.** V3-only on Base.
   Aerodrome uses a different AMM model (Solidly/Velodrome fork with ve(3,3) mechanics)
   that would require a new pool syncer and quoter. That's a separate project.

6. **Do NOT abstract the DexType enum per chain.** All DexType variants stay compiled.
   Unused variants (QuickswapV3 on Base) simply have no pools in the whitelist and
   are never matched. This is simpler than conditional compilation or chain-specific enums.

---

# PART F: Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Breaking Polygon during refactor | Medium | HIGH | Test `--chain polygon` before any Base work; keep `.env.live` as backup |
| Base pools have no viable spreads | Medium | Low | Data collection phase first; don't deploy capital until confirmed |
| RPC rate limit exceeded | Low | Medium | Separate Alchemy keys per chain; monitor usage |
| VPS memory pressure with 2 bots | Low | Medium | Monitor with `free -h`; Base bot is lighter (fewer pools) |
| Gas estimation wrong for Base | Medium | Low | Conservative `ESTIMATED_GAS_COST_USD`; atomic revert protects capital |

---

# Summary

**What changes:**
- 3 new fields in BotConfig (`chain_name`, `quote_token_address`, `estimated_gas_cost_usd`)
- 1 new dependency (`clap`)
- ~15 lines changed in `main.rs` (chain selection, path parameterization)
- ~5 lines changed in `detector.rs` (use config instead of constants)
- ~10 lines changed in `config.rs` (load new fields)
- New files: `.env.base`, `config/base/pools_whitelist.json`
- New directories: `config/polygon/`, `config/base/`, `data/polygon/`, `data/base/`

**What stays the same:** Everything else. The arbitrage detection algorithm,
pool syncers, trade executor, tax logging, whitelist system, ArbExecutor.sol
contract, and module structure are all unchanged.

**Total estimated diff:** ~50 lines of Rust changes + new config files.
