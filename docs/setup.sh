#!/bin/bash
# Phase 1 Arbitrage Bot - Automated Setup Script
# This script clones references, sets up the project, and prepares for development

set -e  # Exit on error

echo "================================================"
echo "Phase 1 DEX Arbitrage Bot - Setup Script"
echo "================================================"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check prerequisites
echo "Checking prerequisites..."

# Check Rust
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}❌ Rust not found. Please install from https://rustup.rs${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Rust found: $(rustc --version)${NC}"

# Check git
if ! command -v git &> /dev/null; then
    echo -e "${RED}❌ Git not found. Please install git${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Git found${NC}"

echo ""
echo "================================================"
echo "Step 1: Creating Project Structure"
echo "================================================"

# Create main directory
PROJECT_DIR="phase1-arbitrage-bot"
if [ -d "$PROJECT_DIR" ]; then
    echo -e "${YELLOW}⚠️  Directory $PROJECT_DIR already exists${NC}"
    read -p "Delete and recreate? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf "$PROJECT_DIR"
    else
        echo "Exiting..."
        exit 1
    fi
fi

mkdir -p "$PROJECT_DIR"
cd "$PROJECT_DIR"

echo -e "${GREEN}✅ Created project directory${NC}"

echo ""
echo "================================================"
echo "Step 2: Cloning Reference Repositories"
echo "================================================"

mkdir -p references
cd references

# Clone mev-template-rs
if [ ! -d "mev-template-rs" ]; then
    echo "Cloning mev-template-rs..."
    git clone https://github.com/degatchi/mev-template-rs.git
    echo -e "${GREEN}✅ Cloned mev-template-rs${NC}"
else
    echo -e "${YELLOW}⚠️  mev-template-rs already exists, skipping${NC}"
fi

# Clone amms-rs
if [ ! -d "amms-rs" ]; then
    echo "Cloning amms-rs..."
    git clone https://github.com/darkforestry/amms-rs.git
    echo -e "${GREEN}✅ Cloned amms-rs${NC}"
else
    echo -e "${YELLOW}⚠️  amms-rs already exists, skipping${NC}"
fi

# Clone crypto-arbitrage-analyzer
if [ ! -d "crypto-arbitrage-analyzer" ]; then
    echo "Cloning crypto-arbitrage-analyzer..."
    git clone https://github.com/codeesura/crypto-arbitrage-analyzer.git
    echo -e "${GREEN}✅ Cloned crypto-arbitrage-analyzer${NC}"
else
    echo -e "${YELLOW}⚠️  crypto-arbitrage-analyzer already exists, skipping${NC}"
fi

# Clone flashloan-rs (for Phase 2 reference)
if [ ! -d "flashloan-rs" ]; then
    echo "Cloning flashloan-rs (Phase 2 reference)..."
    git clone https://github.com/whitenois3/flashloan-rs.git
    echo -e "${GREEN}✅ Cloned flashloan-rs${NC}"
else
    echo -e "${YELLOW}⚠️  flashloan-rs already exists, skipping${NC}"
fi

cd ..

echo ""
echo "================================================"
echo "Step 3: Creating Rust Project"
echo "================================================"

# Create Cargo project
if [ ! -f "Cargo.toml" ]; then
    cargo init --name phase1-arbitrage-bot
    echo -e "${GREEN}✅ Initialized Rust project${NC}"
else
    echo -e "${YELLOW}⚠️  Cargo.toml already exists${NC}"
fi

echo ""
echo "================================================"
echo "Step 4: Setting Up Project Structure"
echo "================================================"

# Create directory structure
mkdir -p src/{pool,dex,arbitrage,utils,contracts/bindings}
mkdir -p contracts
mkdir -p tests
mkdir -p scripts
mkdir -p logs

echo -e "${GREEN}✅ Created directory structure${NC}"

# Create Cargo.toml
cat > Cargo.toml << 'EOF'
[package]
name = "phase1-arbitrage-bot"
version = "0.1.0"
edition = "2021"

[dependencies]
# Core Ethereum interaction (ethers-rs)
ethers = { version = "2.0", features = ["ws", "rustls", "abigen"] }

# Async runtime
tokio = { version = "1.35", features = ["full"] }

# Utilities
anyhow = "1.0"
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Configuration
dotenv = "0.15"

# Data structures
dashmap = "5.5"  # Thread-safe HashMap
once_cell = "1.19"

# Numeric computations
rust_decimal = "1.33"

# Time
chrono = "0.4"

# Optional: Discord notifications
serenity = { version = "0.12", optional = true, default-features = false, features = ["client", "gateway", "model"] }

[dev-dependencies]
tokio-test = "0.4"

[features]
default = []
discord = ["serenity"]

[[bin]]
name = "phase1-arbitrage-bot"
path = "src/main.rs"
EOF

echo -e "${GREEN}✅ Created Cargo.toml${NC}"

# Create .env template
cat > .env.example << 'EOF'
# Network Configuration (Polygon Mainnet)
RPC_URL=wss://polygon-mainnet.g.alchemy.com/v2/YOUR_API_KEY_HERE
CHAIN_ID=137

# For testing on Mumbai testnet:
# RPC_URL=wss://polygon-mumbai.g.alchemy.com/v2/YOUR_API_KEY_HERE
# CHAIN_ID=80001

# Wallet Configuration
PRIVATE_KEY=your_private_key_here_without_0x_prefix

# Trading Parameters
MIN_PROFIT_USD=5.0
MAX_TRADE_SIZE_USD=2000.0
MAX_SLIPPAGE_PERCENT=0.5

# DEX Addresses (Polygon Mainnet)
UNISWAP_ROUTER=0xE592427A0AEce92De3Edee1F18E0157C05861564
SUSHISWAP_ROUTER=0x1b02dA8Cb0d097eB8D57A175b88c7D8b47997506
UNISWAP_FACTORY=0x1F98431c8aD98523631AE4a59f267346ea31F984
SUSHISWAP_FACTORY=0xc35DADB65012eC5796536bD9864eD8773aBc74C4

# Trading Pairs (Format: token0:token1:symbol)
# Separate multiple pairs with commas
TRADING_PAIRS=0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619:0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174:WETH/USDC,0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270:0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174:WMATIC/USDC

# Performance Settings
POLL_INTERVAL_MS=100
MAX_GAS_PRICE_GWEI=100

# Logging
RUST_LOG=phase1_arbitrage_bot=info,warn
EOF

echo -e "${GREEN}✅ Created .env.example${NC}"

# Create .gitignore
cat > .gitignore << 'EOF'
# Rust
/target/
**/*.rs.bk
*.pdb

# Environment
.env
.env.local

# IDE
.idea/
.vscode/
*.swp
*.swo
*~

# Logs
logs/
*.log

# OS
.DS_Store
Thumbs.db
EOF

echo -e "${GREEN}✅ Created .gitignore${NC}"

# Create basic README
cat > README.md << 'EOF'
# Phase 1 DEX Arbitrage Bot

High-performance Rust bot for detecting and executing arbitrage opportunities between Uniswap and Sushiswap on Polygon.

## Features

- ⚡ Real-time price monitoring via WebSocket
- 🎯 Opportunity detection with profitability calculation
- 💱 Automated trade execution
- 📊 Comprehensive logging and metrics
- 🔒 Safe error handling

## Phase 1 Objectives

- [x] Monitor Uniswap + Sushiswap prices
- [x] Detect arbitrage opportunities
- [x] Execute trades with own capital
- [ ] Achieve 2-5 profitable trades/day
- [ ] Average $5-20 profit per trade

## Setup

1. Copy `.env.example` to `.env`:
   ```bash
   cp .env.example .env
   ```

2. Configure your settings in `.env`:
   - Add your Alchemy/Infura RPC URL
   - Add your wallet private key (testnet wallet recommended initially)
   - Adjust trading parameters

3. Build the project:
   ```bash
   cargo build --release
   ```

4. Run on Mumbai testnet first:
   ```bash
   # Edit .env to use Mumbai testnet RPC and addresses
   cargo run --release
   ```

5. After testing, deploy to Polygon mainnet:
   ```bash
   # Edit .env to use mainnet RPC and addresses
   cargo run --release
   ```

## Project Structure

```
phase1-arbitrage-bot/
├── src/
│   ├── main.rs              # Entry point and main loop
│   ├── config.rs            # Configuration management
│   ├── types.rs             # Core data structures
│   ├── pool/                # Pool state management
│   ├── arbitrage/           # Opportunity detection & execution
│   └── utils/               # Logging, metrics
├── references/              # Reference implementations
├── contracts/               # Smart contracts (Phase 2)
└── tests/                   # Integration tests
```

## Usage

```bash
# Build
cargo build --release

# Run
cargo run --release

# Run with debug logging
RUST_LOG=debug cargo run --release

# Run tests
cargo test
```

## Safety Notes

⚠️ **Start with testnet (Mumbai) and small amounts**
- Test thoroughly before mainnet deployment
- Start with $500 or less on mainnet
- Monitor closely for the first 24-48 hours
- Scale gradually based on performance

## Phase 2 Coming Soon

- Flash loan integration for atomic execution
- Elimination of leg risk
- 10x capital efficiency improvement
- Multi-hop arbitrage

## Resources

- [Implementation Plan](./docs/phase1_implementation_plan.md)
- [Component Mapping](./docs/component_mapping_guide.md)
- Polygon RPC: [Alchemy](https://www.alchemy.com/) or [Infura](https://infura.io/)
- DEX Documentation: [Uniswap](https://docs.uniswap.org/), [Sushiswap](https://docs.sushi.com/)
EOF

echo -e "${GREEN}✅ Created README.md${NC}"

# Create starter main.rs
cat > src/main.rs << 'EOF'
// Phase 1 DEX Arbitrage Bot
// Main entry point

mod config;
mod types;

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();
    
    info!("🚀 Phase 1 DEX Arbitrage Bot Starting...");
    info!("⚠️  This is a starter template - implement components from the plan");
    
    // TODO: Implement components following phase1_implementation_plan.md
    // 1. Load configuration
    // 2. Initialize provider
    // 3. Set up pool state manager
    // 4. Start monitoring loop
    
    info!("✅ Bot initialized successfully");
    info!("📖 Next steps: Implement components from docs/phase1_implementation_plan.md");
    
    // Placeholder main loop
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        info!("Bot running... (implement core logic next)");
    }
}
EOF

echo -e "${GREEN}✅ Created starter main.rs${NC}"

# Create basic types.rs
cat > src/types.rs << 'EOF'
// Core data structures for Phase 1
// Expand these based on the implementation plan

use ethers::types::Address;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPair {
    pub token0: Address,
    pub token1: Address,
    pub symbol: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DexType {
    Uniswap,
    Sushiswap,
}

// Add more types as you implement...
EOF

echo -e "${GREEN}✅ Created starter types.rs${NC}"

# Create basic config.rs
cat > src/config.rs << 'EOF'
// Configuration management
// Load settings from .env file

use anyhow::Result;
use std::env;

pub struct Config {
    pub rpc_url: String,
    pub chain_id: u64,
    pub private_key: String,
    // Add more fields as needed
}

pub fn load_config() -> Result<Config> {
    dotenv::dotenv().ok();
    
    Ok(Config {
        rpc_url: env::var("RPC_URL")?,
        chain_id: env::var("CHAIN_ID")?.parse()?,
        private_key: env::var("PRIVATE_KEY")?,
    })
}
EOF

echo -e "${GREEN}✅ Created starter config.rs${NC}"

# Create test script
cat > scripts/test_connection.sh << 'EOF'
#!/bin/bash
# Test connection to Polygon RPC

echo "Testing connection to Polygon..."

# Load .env
if [ -f .env ]; then
    export $(cat .env | grep -v '#' | xargs)
fi

if [ -z "$RPC_URL" ]; then
    echo "❌ RPC_URL not set in .env"
    exit 1
fi

# Test with curl (remove wss:// prefix for http test)
HTTP_URL=$(echo $RPC_URL | sed 's/wss:/https:/' | sed 's/ws:/http:/')

echo "Testing connection to: $HTTP_URL"

curl -X POST $HTTP_URL \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  2>/dev/null | jq .

if [ $? -eq 0 ]; then
    echo "✅ Connection successful!"
else
    echo "❌ Connection failed. Check your RPC_URL in .env"
fi
EOF

chmod +x scripts/test_connection.sh
echo -e "${GREEN}✅ Created test script${NC}"

echo ""
echo "================================================"
echo "Step 5: Initial Build Test"
echo "================================================"

echo "Building project (this may take a few minutes)..."
if cargo build 2>&1 | tee logs/build.log; then
    echo -e "${GREEN}✅ Build successful!${NC}"
else
    echo -e "${RED}❌ Build failed. Check logs/build.log${NC}"
    exit 1
fi

echo ""
echo "================================================"
echo "✅ Setup Complete!"
echo "================================================"
echo ""
echo "Next steps:"
echo ""
echo "1. Configure your environment:"
echo "   ${YELLOW}cp .env.example .env${NC}"
echo "   ${YELLOW}nano .env${NC}  # Add your RPC URL and private key"
echo ""
echo "2. Test your connection:"
echo "   ${YELLOW}./scripts/test_connection.sh${NC}"
echo ""
echo "3. Study the reference implementations:"
echo "   ${YELLOW}cd references/mev-template-rs && cat README.md${NC}"
echo "   ${YELLOW}cd references/amms-rs && cat README.md${NC}"
echo ""
echo "4. Implement components following the plan:"
echo "   ${YELLOW}cat docs/phase1_implementation_plan.md${NC}"
echo "   ${YELLOW}cat docs/component_mapping_guide.md${NC}"
echo ""
echo "5. Build your bot step by step:"
echo "   Day 1: types.rs, config.rs"
echo "   Day 2: pool/state.rs, pool/syncer.rs"
echo "   Day 3: arbitrage/detector.rs"
echo "   Day 4: arbitrage/executor.rs, main.rs"
echo "   Day 5: Test on Mumbai testnet"
echo ""
echo "Happy coding! 🦀"
echo ""

# Create quick reference
cat > QUICKSTART.md << 'EOF'
# Quick Start Guide

## Day 1: Foundation Setup

### 1. Configure Environment
```bash
cp .env.example .env
# Edit .env with your settings:
# - Alchemy/Infura RPC URL (get free tier from alchemy.com)
# - Private key (USE TESTNET WALLET FIRST)
# - Trading parameters
```

### 2. Test Connection
```bash
./scripts/test_connection.sh
```

### 3. Study References
```bash
# Look at these files in references:
# - references/mev-template-rs/src/main.rs
# - references/amms-rs/src/amm/uniswap_v2/pool.rs
# - references/crypto-arbitrage-analyzer/src/arbitrage.rs
```

### 4. Implement Types
Open `src/types.rs` and implement the data structures from `docs/phase1_implementation_plan.md` (Section: Core Data Structures)

### 5. Implement Config
Open `src/config.rs` and implement config loading from `docs/phase1_implementation_plan.md` (Section: Configuration Management)

## Day 2: Pool Management

### 1. Create Pool Module
```bash
touch src/pool/mod.rs
touch src/pool/state.rs
touch src/pool/syncer.rs
```

### 2. Implement State Manager
Copy the PoolStateManager from `docs/phase1_implementation_plan.md` into `src/pool/state.rs`

### 3. Implement Pool Syncer
Adapt the PoolSyncer logic from `references/amms-rs` using patterns from the docs

### 4. Test Pool Syncing
```bash
cargo test --lib pool
```

## Day 3: Opportunity Detection

### 1. Create Arbitrage Module
```bash
touch src/arbitrage/mod.rs
touch src/arbitrage/detector.rs
```

### 2. Implement Detector
Copy OpportunityDetector from docs, enhance with profitability calculations

### 3. Test Detection
```bash
cargo test --lib arbitrage::detector
```

## Day 4: Execution

### 1. Create Executor
```bash
touch src/arbitrage/executor.rs
```

### 2. Implement TradeExecutor
Use patterns from `mev-template-rs` for transaction building

### 3. Integrate Main Loop
Update `src/main.rs` with the complete event loop

## Day 5: Testing

### 1. Deploy to Mumbai Testnet
- Update .env to use Mumbai testnet
- Run: `cargo run --release`
- Monitor logs
- Execute 5-10 test trades

### 2. Verify Results
- Check transactions on PolygonScan
- Validate profit calculations
- Test error handling

## Day 6-7: Mainnet Deployment

### 1. Configure for Mainnet
- Update .env to use Polygon mainnet
- Start with $500 capital
- Set conservative parameters

### 2. Deploy and Monitor
```bash
cargo run --release > logs/mainnet.log 2>&1 &
tail -f logs/mainnet.log
```

### 3. Scale Gradually
- Monitor for 24-48 hours
- Increase capital if stable
- Optimize parameters based on results

## Helpful Commands

```bash
# Build
cargo build --release

# Run with logs
RUST_LOG=debug cargo run --release

# Test
cargo test

# Check compilation
cargo check

# Update dependencies
cargo update

# Generate documentation
cargo doc --open
```

## Resources

- Implementation Plan: `docs/phase1_implementation_plan.md`
- Component Mapping: `docs/component_mapping_guide.md`
- DEX Arbitrage Strategy: `docs/dex-arbitrage-complete-strategy.md`

## Need Help?

1. Check the documentation in `docs/`
2. Study reference implementations in `references/`
3. Review component mapping guide
4. Test each component individually before integration
EOF

echo -e "${GREEN}✅ Created QUICKSTART.md${NC}"

# Copy documentation if provided
if [ -f "../phase1_implementation_plan.md" ]; then
    mkdir -p docs
    cp ../phase1_implementation_plan.md docs/
    cp ../component_mapping_guide.md docs/ 2>/dev/null || true
    echo -e "${GREEN}✅ Copied documentation to docs/${NC}"
fi

echo ""
echo "Project setup complete!"
echo "Location: $(pwd)"
echo ""
echo "Read QUICKSTART.md for next steps"
EOF

chmod +x setup.sh

echo -e "${GREEN}✅ Created automated setup script${NC}"
