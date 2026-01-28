# Tax Logging Implementation Plan

**Purpose:** Build comprehensive tax logging before any real trades occur.
**Created:** 2026-01-28
**Status:** ✅ ALL PHASES COMPLETE (2026-01-28)

---

## Implementation Status

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | TaxRecord struct (34+ fields) | ✅ Complete |
| Phase 2 | CSV + JSON logging | ✅ Complete |
| Phase 3 | RP2 export format | ✅ Complete |
| Phase 4 | Price oracle integration | ✅ Complete |
| Phase 5 | Wire into real executor | ✅ Complete |

### Files Created/Modified

```
src/rust-bot/src/tax/
├── mod.rs           # TaxRecord, TaxEventType, GainType, TaxSummary
├── csv_logger.rs    # CSV logging to data/tax/trades_YYYY.csv
├── json_logger.rs   # JSON backup + TaxLogger (combined)
├── rp2_export.rs    # RP2-compatible export for tax software
└── price_oracle.rs  # USD price lookups from pool_state.json

src/rust-bot/src/arbitrage/
└── executor.rs      # [Phase 5] Added TaxLogger + TaxRecordBuilder integration

src/rust-bot/src/types.rs        # [Phase 5] Added block_number, amount_in/out to TradeResult
                                 #           Added pool addresses to ArbitrageOpportunity

src/rust-bot/src/bin/
└── tax_export.rs    # CLI tool for exports
```

### Phase 5 Integration Details

The real trade executor now automatically logs tax records:

```rust
// Enable tax logging before executing trades
let mut executor = TradeExecutor::new(provider, wallet, config);
executor.enable_tax_logging("/home/botuser/bots/dexarb/data/tax")?;

// Every successful trade is automatically logged
let result = executor.execute(&opportunity).await?;
// Tax record written to data/tax/trades_YYYY.csv and .jsonl
```

**TradeResult** now includes:
- `block_number: Option<u64>` - For audit trail
- `gas_used_native: f64` - MATIC spent on gas
- `amount_in: Option<String>` - Raw input amount
- `amount_out: Option<String>` - Raw output amount

**ArbitrageOpportunity** now includes:
- `buy_pool_address: Option<Address>` - Pool where we buy
- `sell_pool_address: Option<Address>` - Pool where we sell

### Usage

```rust
// Create tax records with automatic price fetching
let builder = TaxRecordBuilder::new()?;
let record = builder.build_arbitrage_record(
    "USDC", amount_sent, "WMATIC", amount_received,
    gas_native, dex_fee_pct, tx_hash, block,
    wallet, dex_buy, dex_sell, pool_buy, pool_sell,
    spread_pct, is_paper_trade
)?;

// Log to CSV + JSON
let mut logger = TaxLogger::new("data/tax")?;
logger.log(&record)?;

// Export to RP2 format
export_to_rp2(&records, "rp2_2026.csv", "WalletName")?;
```

### CLI Commands

```bash
cargo run --bin tax-export -- --year 2026 --output rp2_2026.csv
cargo run --bin tax-export -- --summary --year 2026
cargo run --bin tax-export -- --validate rp2_2026.csv
cargo run --bin tax-export -- --config fifo
cargo run --bin tax-export -- --list
```

---

## Current State Assessment

### What Paper Trading Currently Logs

From `paper_trading/metrics.rs`:
```rust
pub struct SimulatedTradeResult {
    pub pair: String,           // ✅ Have
    pub success: bool,          // ✅ Have
    pub profit_usd: f64,        // ✅ Have (gross)
    pub gas_cost_usd: f64,      // ✅ Have
    pub net_profit_usd: f64,    // ✅ Have
    pub execution_time_ms: u64, // ✅ Have
    pub error: Option<String>,  // ✅ Have
    pub timestamp: DateTime<Utc>, // ✅ Have
}
```

### What's MISSING for Tax Compliance

| Field | Status | Impact |
|-------|--------|--------|
| `transaction_hash` | ❌ Missing | Required for IRS audit trail |
| `block_number` | ❌ Missing | Required for timestamp verification |
| `wallet_address` | ❌ Missing | Required for Rev. Proc. 2024-28 |
| `token_amounts` | ❌ Missing | Only USD values tracked |
| `token_decimals` | ❌ Missing | Needed for accurate amounts |
| `dex_fee_usd` | ❌ Missing | Separate from gas |
| `cost_basis_usd` | ❌ Missing | Required for capital gains |
| `lot_selection_method` | ❌ Missing | FIFO/LIFO/HIFO tracking |
| `asset_sent/received` | ❌ Missing | Only pair symbol tracked |

---

## Implementation Steps

### Phase 1: TaxRecord Struct (Week 1)

**File:** `src/tax/mod.rs`

```rust
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRecord {
    // === IDENTIFICATION ===
    pub trade_id: String,               // UUID for this trade
    pub timestamp: DateTime<Utc>,       // UTC timestamp
    pub tax_year: i16,                  // 2026, 2027, etc.

    // === TRANSACTION TYPE ===
    pub transaction_type: TaxEventType, // SWAP, BUY, SELL

    // === ASSETS ===
    pub asset_sent: String,             // "USDC", "WMATIC", etc.
    pub amount_sent: Decimal,           // Raw token amount
    pub asset_received: String,         // "WMATIC", "USDC", etc.
    pub amount_received: Decimal,       // Raw token amount
    pub token_sent_decimals: u8,        // 6 for USDC, 18 for WMATIC
    pub token_received_decimals: u8,    // Token decimals

    // === USD VALUATIONS (CRITICAL!) ===
    pub usd_value_sent: Decimal,        // FMV at time of trade
    pub usd_value_received: Decimal,    // FMV at time of trade
    pub spot_price_sent: Decimal,       // Price per unit in USD
    pub spot_price_received: Decimal,   // Price per unit in USD

    // === COST BASIS (For Capital Gains) ===
    pub cost_basis_usd: Decimal,        // What you paid for asset_sent
    pub proceeds_usd: Decimal,          // What you received
    pub capital_gain_loss: Decimal,     // proceeds - cost_basis
    pub holding_period_days: i32,       // Always 0 for arbitrage
    pub gain_type: GainType,            // SHORT_TERM (all arbitrage)

    // === FEES (Deductible) ===
    pub gas_fee_native: Decimal,        // Gas in MATIC
    pub gas_fee_usd: Decimal,           // Gas in USD
    pub dex_fee_percent: Decimal,       // 0.30% typically
    pub dex_fee_usd: Decimal,           // DEX fee in USD
    pub total_fees_usd: Decimal,        // gas + dex fees

    // === BLOCKCHAIN DATA ===
    pub blockchain: String,             // "Polygon"
    pub chain_id: u64,                  // 137
    pub transaction_hash: String,       // On-chain tx hash
    pub block_number: u64,              // Block number
    pub wallet_address: String,         // Your wallet

    // === DEX ROUTING ===
    pub dex_buy: String,                // "Uniswap", "Sushiswap"
    pub dex_sell: String,               // "Sushiswap", "Uniswap"
    pub pool_address_buy: String,       // Pool contract address
    pub pool_address_sell: String,      // Pool contract address

    // === ACCOUNTING ===
    pub lot_selection_method: String,   // "FIFO" (default for arbitrage)
    pub lot_id: Option<String>,         // For specific ID method

    // === METADATA ===
    pub spread_percent: Decimal,        // Arbitrage spread
    pub notes: Option<String>,          // Additional context
    pub is_paper_trade: bool,           // True = simulation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaxEventType {
    Swap,       // Crypto-to-crypto (most arbitrage)
    Buy,        // Fiat to crypto (initial funding)
    Sell,       // Crypto to fiat (withdrawal)
    Transfer,   // Between own wallets (not taxable)
    Fee,        // Gas-only transaction
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GainType {
    ShortTerm,  // Held <1 year (ALL arbitrage trades)
    LongTerm,   // Held ≥1 year (never for arbitrage)
}
```

### Phase 2: CSV Logging (Week 1)

**File:** `src/tax/csv_logger.rs`

```rust
use std::fs::{File, OpenOptions};
use std::path::Path;
use csv::Writer;
use super::TaxRecord;

pub struct TaxCsvLogger {
    path: String,
    headers_written: bool,
}

impl TaxCsvLogger {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            headers_written: Path::new(path).exists(),
        }
    }

    pub fn log(&mut self, record: &TaxRecord) -> Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        let mut writer = Writer::from_writer(file);

        if !self.headers_written {
            self.write_headers(&mut writer)?;
            self.headers_written = true;
        }

        self.write_record(&mut writer, record)?;
        writer.flush()?;

        Ok(())
    }

    fn write_headers(&self, writer: &mut Writer<File>) -> Result<()> {
        writer.write_record(&[
            "trade_id", "timestamp", "tax_year",
            "transaction_type", "asset_sent", "amount_sent",
            "asset_received", "amount_received",
            "usd_value_sent", "usd_value_received",
            "spot_price_sent", "spot_price_received",
            "cost_basis_usd", "proceeds_usd", "capital_gain_loss",
            "holding_period_days", "gain_type",
            "gas_fee_native", "gas_fee_usd", "dex_fee_usd", "total_fees_usd",
            "blockchain", "chain_id", "transaction_hash", "block_number",
            "wallet_address", "dex_buy", "dex_sell",
            "pool_address_buy", "pool_address_sell",
            "lot_selection_method", "spread_percent",
            "is_paper_trade", "notes"
        ])?;
        Ok(())
    }

    fn write_record(&self, writer: &mut Writer<File>, r: &TaxRecord) -> Result<()> {
        writer.write_record(&[
            &r.trade_id,
            &r.timestamp.to_rfc3339(),
            &r.tax_year.to_string(),
            &format!("{:?}", r.transaction_type),
            &r.asset_sent,
            &r.amount_sent.to_string(),
            &r.asset_received,
            &r.amount_received.to_string(),
            &r.usd_value_sent.to_string(),
            &r.usd_value_received.to_string(),
            &r.spot_price_sent.to_string(),
            &r.spot_price_received.to_string(),
            &r.cost_basis_usd.to_string(),
            &r.proceeds_usd.to_string(),
            &r.capital_gain_loss.to_string(),
            &r.holding_period_days.to_string(),
            &format!("{:?}", r.gain_type),
            &r.gas_fee_native.to_string(),
            &r.gas_fee_usd.to_string(),
            &r.dex_fee_usd.to_string(),
            &r.total_fees_usd.to_string(),
            &r.blockchain,
            &r.chain_id.to_string(),
            &r.transaction_hash,
            &r.block_number.to_string(),
            &r.wallet_address,
            &r.dex_buy,
            &r.dex_sell,
            &r.pool_address_buy,
            &r.pool_address_sell,
            &r.lot_selection_method,
            &r.spread_percent.to_string(),
            &r.is_paper_trade.to_string(),
            r.notes.as_deref().unwrap_or(""),
        ])?;
        Ok(())
    }
}
```

### Phase 3: RP2 Export Format (Week 2)

**File:** `src/tax/rp2_export.rs`

Export to RP2 format for tax calculation:

```rust
pub fn export_to_rp2(records: &[TaxRecord], output_path: &str) -> Result<()> {
    let mut wtr = Writer::from_path(output_path)?;

    // RP2 required headers
    wtr.write_record(&[
        "timestamp", "asset", "exchange", "holder",
        "transaction_type", "spot_price",
        "crypto_in", "crypto_out_no_fee", "crypto_fee",
        "fiat_in_no_fee", "fiat_in_with_fee",
        "fiat_out_no_fee", "fiat_fee", "notes"
    ])?;

    for record in records {
        // For arbitrage, each swap = SELL of asset_sent + BUY of asset_received
        // RP2 handles both sides

        // Record the SELL side (disposing of asset_sent)
        wtr.write_record(&[
            &record.timestamp.to_rfc3339(),
            &record.asset_sent,
            &record.dex_buy,  // Exchange where we sold
            &record.wallet_address,
            "SELL",
            &record.spot_price_sent.to_string(),
            "0",  // crypto_in
            &record.amount_sent.to_string(),  // crypto_out
            "0",  // crypto_fee (fees are fiat below)
            "0",  // fiat_in_no_fee
            "0",  // fiat_in_with_fee
            &record.usd_value_sent.to_string(),  // fiat_out
            &record.dex_fee_usd.to_string(),     // fiat_fee
            &format!("Arbitrage sell: {}", record.spread_percent),
        ])?;

        // Record the BUY side (acquiring asset_received)
        wtr.write_record(&[
            &record.timestamp.to_rfc3339(),
            &record.asset_received,
            &record.dex_sell,  // Exchange where we bought
            &record.wallet_address,
            "BUY",
            &record.spot_price_received.to_string(),
            &record.amount_received.to_string(),  // crypto_in
            "0",  // crypto_out
            "0",  // crypto_fee
            &record.usd_value_received.to_string(),  // fiat_in
            &(record.usd_value_received + record.gas_fee_usd).to_string(),  // with_fee
            "0",  // fiat_out
            &record.gas_fee_usd.to_string(),  // fiat_fee
            &format!("Arbitrage buy: {}", record.spread_percent),
        ])?;
    }

    wtr.flush()?;
    Ok(())
}
```

### Phase 4: Price Oracle Integration (Week 2)

**File:** `src/tax/price_oracle.rs`

Get USD prices at time of trade:

```rust
pub struct PriceOracle {
    // Use pool prices from shared state
    pool_state: Arc<SharedPoolState>,
    // Stablecoin addresses
    usdc_address: Address,
}

impl PriceOracle {
    /// Get USD price for a token at trade time
    pub fn get_price_usd(&self, token: &str, timestamp: DateTime<Utc>) -> Decimal {
        match token {
            "USDC" | "USDT" | "DAI" => Decimal::ONE,
            _ => {
                // Get from pool state (token vs USDC price)
                self.pool_state
                    .get_price_for_token(token)
                    .unwrap_or(Decimal::ZERO)
            }
        }
    }

    /// Get MATIC price for gas calculation
    pub fn get_matic_price_usd(&self) -> Decimal {
        self.pool_state
            .get_price_for_token("WMATIC")
            .unwrap_or_else(|| Decimal::from_str("0.90").unwrap())
    }
}
```

### Phase 5: Integration with Executor (Week 3)

**File:** `src/executor/mod.rs` (modify existing)

```rust
use crate::tax::{TaxRecord, TaxCsvLogger, TaxEventType, GainType};

impl Executor {
    pub async fn execute_arbitrage(&self, opportunity: &Opportunity) -> Result<TradeResult> {
        // Execute the trade
        let result = self.execute_swap(opportunity).await?;

        // Build tax record
        let tax_record = self.build_tax_record(opportunity, &result).await?;

        // Log to CSV
        self.tax_logger.log(&tax_record)?;

        // Also log to JSON (backup)
        self.json_logger.log(&tax_record)?;

        Ok(result)
    }

    async fn build_tax_record(
        &self,
        opp: &Opportunity,
        result: &TradeResult,
    ) -> Result<TaxRecord> {
        let now = Utc::now();
        let matic_price = self.price_oracle.get_matic_price_usd();

        Ok(TaxRecord {
            trade_id: uuid::Uuid::new_v4().to_string(),
            timestamp: now,
            tax_year: now.year() as i16,

            transaction_type: TaxEventType::Swap,

            asset_sent: opp.base_token.symbol.clone(),
            amount_sent: result.amount_in,
            asset_received: opp.quote_token.symbol.clone(),
            amount_received: result.amount_out,
            token_sent_decimals: opp.base_token.decimals,
            token_received_decimals: opp.quote_token.decimals,

            usd_value_sent: result.amount_in * opp.price_buy,
            usd_value_received: result.amount_out * opp.price_sell,
            spot_price_sent: opp.price_buy,
            spot_price_received: opp.price_sell,

            // For arbitrage, cost basis = what we just paid
            cost_basis_usd: result.amount_in * opp.price_buy,
            proceeds_usd: result.amount_out * opp.price_sell,
            capital_gain_loss: result.net_profit_usd,
            holding_period_days: 0,
            gain_type: GainType::ShortTerm,

            gas_fee_native: result.gas_used,
            gas_fee_usd: result.gas_used * matic_price,
            dex_fee_percent: Decimal::from_str("0.0030")?,
            dex_fee_usd: opp.dex_fee_usd,
            total_fees_usd: result.gas_used * matic_price + opp.dex_fee_usd,

            blockchain: "Polygon".to_string(),
            chain_id: 137,
            transaction_hash: result.tx_hash.clone(),
            block_number: result.block_number,
            wallet_address: self.wallet_address.clone(),

            dex_buy: opp.dex_buy.to_string(),
            dex_sell: opp.dex_sell.to_string(),
            pool_address_buy: opp.pool_buy.to_string(),
            pool_address_sell: opp.pool_sell.to_string(),

            lot_selection_method: "FIFO".to_string(),
            lot_id: None,

            spread_percent: opp.spread_percent,
            notes: Some(format!("Arbitrage: {} → {}", opp.dex_buy, opp.dex_sell)),
            is_paper_trade: false,
        })
    }
}
```

---

## File Structure

```
src/rust-bot/
├── src/
│   ├── tax/
│   │   ├── mod.rs           # TaxRecord struct, enums
│   │   ├── csv_logger.rs    # CSV file logging
│   │   ├── json_logger.rs   # JSON backup logging
│   │   ├── rp2_export.rs    # RP2 format export
│   │   └── price_oracle.rs  # USD price lookups
│   ├── executor/
│   │   └── mod.rs           # Trade execution (add tax logging)
│   └── bin/
│       ├── paper_trading.rs # Paper trading (add tax fields)
│       └── tax_export.rs    # CLI tool for exports
└── data/
    └── tax/
        ├── trades_2026.csv  # Annual trade log
        ├── trades_2026.json # JSON backup
        └── rp2_export_2026.csv  # RP2 format
```

---

## Storage Locations

| File | Purpose | Retention |
|------|---------|-----------|
| `data/tax/trades_YYYY.csv` | Primary tax log | 6 years |
| `data/tax/trades_YYYY.json` | JSON backup | 6 years |
| `data/tax/rp2_export_YYYY.csv` | RP2 import format | 6 years |

---

## Testing Checklist

Before deploying with real trades:

- [ ] TaxRecord captures all IRS-required fields
- [ ] CSV writes correctly (no truncation)
- [ ] JSON backup matches CSV
- [ ] RP2 export imports successfully into RP2
- [ ] Timestamps are UTC with timezone
- [ ] Decimal precision is sufficient (18 decimals for crypto)
- [ ] Gas fees convert correctly to USD
- [ ] DEX fees calculate correctly (0.30%)
- [ ] Capital gain/loss calculation is correct
- [ ] Paper trade flag distinguishes real from simulated

---

## Implementation Priority

1. **Week 1:** TaxRecord struct + CSV logging
2. **Week 2:** Price oracle + RP2 export
3. **Week 3:** Integration with real executor
4. **Week 4:** Testing + validation against RP2

---

## Critical Notes

### Rev. Proc. 2024-28 Compliance

Starting 2025 tax year, IRS requires per-wallet cost basis tracking.
- Each wallet is treated separately
- Cannot mix cost basis across wallets
- Our single-wallet setup simplifies this

### Flash Loan Consideration

For flash loan arbitrage:
- No actual asset holding (borrowed and returned in same block)
- Only profit is taxable (received as payment)
- Simplifies cost basis: always $0 for borrowed assets
- Net profit = taxable gain

### Paper Trade Records

Keep paper trade records separate:
- `is_paper_trade: true` flag
- Do NOT include in tax filings
- Use for strategy validation only
