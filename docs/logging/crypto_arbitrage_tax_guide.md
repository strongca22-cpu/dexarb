# Crypto Arbitrage Tax Logging Guide
## US Federal Requirements (Washington State - No State Income Tax)

**Status**: Washington State has no income tax, so **only federal taxes apply**.

---

## Part 1: What You MUST Track for IRS Compliance

### **Critical Understanding: EVERY Trade is a Taxable Event**

```
Your arbitrage bot makes 2 trades per opportunity:

Trade 1: USDC → WMATIC (Buy)
├─ This is a taxable event (acquiring property)
└─ Need to record cost basis

Trade 2: WMATIC → USDC (Sell)
├─ This is a taxable event (disposing property)
├─ Need to calculate capital gain/loss
└─ Must report on Form 8949

Even though the entire cycle takes 10 seconds, BOTH trades
are separate taxable events in the eyes of the IRS!
```

### **Required Data Per Trade**

For **EVERY single trade**, you must log:

```rust
pub struct TaxRecord {
    // Transaction Identification
    pub trade_id: String,               // Unique identifier
    pub timestamp: DateTime<Utc>,       // Exact time (UTC or local with timezone)
    pub tax_year: u16,                  // 2026, 2027, etc.
    
    // Transaction Type
    pub transaction_type: TaxEventType, // SWAP, BUY, SELL
    
    // Assets Involved
    pub asset_sent: String,             // "USDC", "WMATIC", etc.
    pub amount_sent: Decimal,           // Amount disposed
    pub asset_received: String,         // "WMATIC", "USDC", etc.
    pub amount_received: Decimal,       // Amount acquired
    
    // USD Valuations (CRITICAL!)
    pub usd_value_sent: Decimal,        // Fair market value in USD at time of trade
    pub usd_value_received: Decimal,    // Fair market value in USD at time of trade
    
    // Cost Basis Tracking
    pub cost_basis_usd: Decimal,        // What you originally paid for asset_sent
    pub proceeds_usd: Decimal,          // What you received (usd_value_received)
    
    // Capital Gains Calculation
    pub capital_gain_loss: Decimal,     // proceeds - cost_basis
    pub holding_period_days: u32,       // Days held (almost always 0 for arbitrage)
    pub gain_type: GainType,            // SHORT_TERM or LONG_TERM
    
    // Transaction Costs
    pub gas_fee_usd: Decimal,           // Gas cost in USD
    pub dex_fee_usd: Decimal,           // DEX fee in USD (0.30% × trade value)
    pub total_fees_usd: Decimal,        // Total costs (deductible)
    
    // Source Information
    pub dex_from: String,               // "Uniswap", "Sushiswap", etc.
    pub dex_to: String,                 // "Sushiswap", "Quickswap", etc.
    pub wallet_address: String,         // Your wallet address
    pub blockchain: String,             // "Polygon"
    pub transaction_hash: String,       // On-chain tx hash
    pub block_number: u64,              // Block number
    
    // Accounting Method
    pub lot_selection_method: String,   // "FIFO", "LIFO", "HIFO", "SpecID"
    pub lot_id: Option<String>,         // If using specific ID
    
    // Notes
    pub notes: Option<String>,          // Any additional context
}

pub enum TaxEventType {
    SWAP,       // Crypto-to-crypto (most common for arbitrage)
    BUY,        // Fiat to crypto
    SELL,       // Crypto to fiat
    TRANSFER,   // Between own wallets (not taxable but track anyway)
    FEE,        // Just gas fee (no trade)
}

pub enum GainType {
    SHORT_TERM,  // Held <1 year (all arbitrage trades)
    LONG_TERM,   // Held ≥1 year
}
```

### **Example: Single Arbitrage Opportunity**

```
Opportunity: WMATIC spread of 1.0%

Trade 1: Buy WMATIC
──────────────────────────────────────────────────
timestamp:           2026-01-28 14:32:15 UTC
transaction_type:    SWAP
asset_sent:          "USDC"
amount_sent:         5000.00 USDC
asset_received:      "WMATIC"
amount_received:     5000.00 WMATIC (assume $1 each)
usd_value_sent:      $5,000.00
usd_value_received:  $5,000.00
cost_basis_usd:      $5,000.00 (what we paid for USDC)
proceeds_usd:        $5,000.00 (value of WMATIC received)
capital_gain_loss:   $0.00 (break-even on this leg)
holding_period_days: 0
gain_type:           SHORT_TERM
gas_fee_usd:         $0.25
dex_fee_usd:         $15.00 (0.30% of $5,000)
total_fees_usd:      $15.25
dex_from:            "Uniswap"
wallet_address:      "0x1234..."
blockchain:          "Polygon"
transaction_hash:    "0xabc..."
block_number:        54123456
lot_selection_method: "FIFO"

Trade 2: Sell WMATIC (10 seconds later)
──────────────────────────────────────────────────
timestamp:           2026-01-28 14:32:25 UTC
transaction_type:    SWAP
asset_sent:          "WMATIC"
amount_sent:         5000.00 WMATIC
asset_received:      "USDC"
amount_received:     5050.00 USDC (1% profit!)
usd_value_sent:      $5,050.00 (current value of WMATIC)
usd_value_received:  $5,050.00
cost_basis_usd:      $5,000.00 (what we paid in Trade 1)
proceeds_usd:        $5,050.00
capital_gain_loss:   $50.00 ✅ TAXABLE GAIN!
holding_period_days: 0 (held for 10 seconds)
gain_type:           SHORT_TERM
gas_fee_usd:         $0.25
dex_fee_usd:         $15.15 (0.30% of $5,050)
total_fees_usd:      $15.40
dex_from:            "Sushiswap"
wallet_address:      "0x1234..."
blockchain:          "Polygon"
transaction_hash:    "0xdef..."
block_number:        54123457
lot_selection_method: "FIFO"

Summary for IRS Form 8949:
──────────────────────────────────────────────────
Description:         5000 WMATIC
Date Acquired:       01/28/2026 14:32:15
Date Sold:           01/28/2026 14:32:25
Proceeds:            $5,050.00
Cost Basis:          $5,000.00
Adjustment:          -$15.40 (fees)
Gain/Loss:           $34.60 (short-term)

Note: Net gain after fees = $50.00 - $15.40 = $34.60
```

---

## Part 2: Best Existing GitHub Tools (Python)

### **🏆 Recommended: RP2 (Privacy-Focused, Python)** 

**Repository**: https://github.com/eprbell/rp2

**Why it's best for you**:
```
✅ US-specific (Form 8949, Schedule D)
✅ Privacy-focused (local only, no cloud)
✅ Supports FIFO, LIFO, HIFO accounting methods
✅ Handles high-frequency trading
✅ Transparent computation (audit trail)
✅ Free, open-source (GPL)
✅ Active development
✅ Handles fractional amounts (common in crypto)

⚠️ IMPORTANT NOTE: Rev. Proc. 2024-28
Starting 2025 tax year, IRS requires "per-wallet application"
RP2 is adding support (issue #135), but not ready yet.
For 2026, verify compliance before using.
```

**Input Format**: CSV or ODS (LibreOffice Calc)

**Example Input** (crypto_trades.csv):
```csv
timestamp,asset,exchange,holder,transaction_type,spot_price,crypto_in,crypto_out_no_fee,crypto_fee,fiat_in_no_fee,fiat_in_with_fee,fiat_out_no_fee,fiat_fee,notes
2026-01-28T14:32:15Z,WMATIC,Uniswap,MyWallet,BUY,1.00,5000.00,0,15.00,5000.00,5015.00,0,0,Arbitrage buy
2026-01-28T14:32:25Z,WMATIC,Sushiswap,MyWallet,SELL,1.01,0,5000.00,0,0,0,5050.00,15.40,Arbitrage sell
```

**Running RP2**:
```bash
# Install
pip install rp2

# Generate US tax report (FIFO method)
rp2_us -m fifo -o output_dir config.ini trades.csv

# Output files:
# - output_dir/rp2_full_report.txt (detailed breakdown)
# - output_dir/8949.ods (ready for Form 8949)
# - output_dir/tax_report_us.txt (summary)
```

**Integration with Your Bot**:
```rust
// Export to RP2 format
pub fn export_to_rp2(trades: &[Trade], output_path: &str) -> Result<()> {
    let mut wtr = csv::Writer::from_path(output_path)?;
    
    wtr.write_record(&[
        "timestamp",
        "asset", 
        "exchange",
        "holder",
        "transaction_type",
        "spot_price",
        "crypto_in",
        "crypto_out_no_fee",
        "crypto_fee",
        "fiat_in_no_fee",
        "fiat_in_with_fee",
        "fiat_out_no_fee",
        "fiat_fee",
        "notes"
    ])?;
    
    for trade in trades {
        wtr.write_record(&[
            &trade.timestamp.to_rfc3339(),
            &trade.asset,
            &trade.dex,
            "MyWallet",
            &trade.tx_type.to_string(),
            &trade.price_usd.to_string(),
            &trade.crypto_in.to_string(),
            &trade.crypto_out.to_string(),
            &trade.fee_crypto.to_string(),
            &trade.fiat_in.to_string(),
            &trade.fiat_in_with_fee.to_string(),
            &trade.fiat_out.to_string(),
            &trade.fiat_fee.to_string(),
            &trade.notes,
        ])?;
    }
    
    wtr.flush()?;
    Ok(())
}
```

---

### **Alternative: Bitcoin-Taxes (Python, Older but Works)**

**Repository**: https://github.com/robertwb/bitcoin-taxes

**Pros**:
- Simpler than RP2
- Supports major exchanges
- FIFO/LIFO/HIFO

**Cons**:
- Less actively maintained
- Requires more manual work

---

### **Alternative: Taxcount (Rust!)** ⭐

**Repository**: https://github.com/dcdpr/taxcount

**Why interesting**:
```
✅ Written in Rust!
✅ Airgapped (privacy-focused)
✅ Designed for Bitcoiners
✅ Form 8949 output
✅ Production-ready

⚠️ Bitcoin-focused (may need adaptation for EVM)
⚠️ Less comprehensive than RP2
```

**Worth exploring** if you want Rust-native solution, but may require modifications for Polygon arbitrage.

---

## Part 3: Implementing Tax Logging in Your Bot

### **Schema Design**

**File**: `src/tax/mod.rs`

```rust
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool; // Or sqlite

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRecord {
    pub id: i64,
    pub trade_id: String,
    pub timestamp: DateTime<Utc>,
    pub tax_year: i16,
    
    // Transaction details
    pub transaction_type: String,  // "SWAP", "BUY", "SELL"
    pub asset_sent: String,
    pub amount_sent: Decimal,
    pub asset_received: String,
    pub amount_received: Decimal,
    
    // USD valuations
    pub usd_value_sent: Decimal,
    pub usd_value_received: Decimal,
    pub cost_basis_usd: Decimal,
    pub proceeds_usd: Decimal,
    
    // Gain/loss calculation
    pub capital_gain_loss: Decimal,
    pub holding_period_days: i32,
    pub gain_type: String,  // "SHORT_TERM", "LONG_TERM"
    
    // Fees
    pub gas_fee_usd: Decimal,
    pub dex_fee_usd: Decimal,
    pub total_fees_usd: Decimal,
    
    // Source info
    pub dex_from: String,
    pub dex_to: Option<String>,
    pub wallet_address: String,
    pub blockchain: String,
    pub transaction_hash: String,
    pub block_number: i64,
    
    // Accounting
    pub lot_selection_method: String,  // "FIFO", "LIFO", "HIFO"
    pub lot_id: Option<String>,
    
    pub notes: Option<String>,
}

pub struct TaxLogger {
    db: PgPool,  // Or sqlite
}

impl TaxLogger {
    pub async fn new(database_url: &str) -> Result<Self> {
        let db = PgPool::connect(database_url).await?;
        Ok(Self { db })
    }
    
    pub async fn log_trade(
        &self,
        trade: &ExecutedTrade,
        price_oracle: &PriceOracle,
    ) -> Result<TaxRecord> {
        // Get USD prices at time of trade
        let usd_price_sent = price_oracle.get_price_usd(&trade.asset_sent, trade.timestamp).await?;
        let usd_price_received = price_oracle.get_price_usd(&trade.asset_received, trade.timestamp).await?;
        
        // Calculate USD values
        let usd_value_sent = trade.amount_sent * usd_price_sent;
        let usd_value_received = trade.amount_received * usd_price_received;
        
        // Get cost basis (from previous purchase)
        let cost_basis = self.calculate_cost_basis(&trade.asset_sent, trade.amount_sent).await?;
        
        // Calculate gain/loss
        let capital_gain_loss = usd_value_received - cost_basis;
        
        // Calculate fees in USD
        let gas_fee_usd = trade.gas_fee_wei.to_decimal() * price_oracle.get_gas_price_usd().await?;
        let dex_fee_usd = usd_value_sent * Decimal::from_str("0.003")?; // 0.30%
        
        let tax_record = TaxRecord {
            id: 0,  // Auto-generated
            trade_id: trade.id.clone(),
            timestamp: trade.timestamp,
            tax_year: trade.timestamp.year() as i16,
            transaction_type: "SWAP".to_string(),
            asset_sent: trade.asset_sent.clone(),
            amount_sent: trade.amount_sent,
            asset_received: trade.asset_received.clone(),
            amount_received: trade.amount_received,
            usd_value_sent,
            usd_value_received,
            cost_basis_usd: cost_basis,
            proceeds_usd: usd_value_received,
            capital_gain_loss,
            holding_period_days: 0,  // Arbitrage is same-day
            gain_type: "SHORT_TERM".to_string(),
            gas_fee_usd,
            dex_fee_usd,
            total_fees_usd: gas_fee_usd + dex_fee_usd,
            dex_from: trade.dex_from.clone(),
            dex_to: Some(trade.dex_to.clone()),
            wallet_address: trade.wallet.clone(),
            blockchain: "Polygon".to_string(),
            transaction_hash: trade.tx_hash.clone(),
            block_number: trade.block_number,
            lot_selection_method: "FIFO".to_string(),
            lot_id: None,
            notes: Some(format!("Arbitrage: {} spread", trade.spread_percent)),
        };
        
        // Insert into database
        sqlx::query!(
            r#"
            INSERT INTO tax_records (
                trade_id, timestamp, tax_year,
                transaction_type, asset_sent, amount_sent,
                asset_received, amount_received,
                usd_value_sent, usd_value_received,
                cost_basis_usd, proceeds_usd,
                capital_gain_loss, holding_period_days, gain_type,
                gas_fee_usd, dex_fee_usd, total_fees_usd,
                dex_from, dex_to, wallet_address,
                blockchain, transaction_hash, block_number,
                lot_selection_method, lot_id, notes
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18,
                $19, $20, $21, $22, $23, $24, $25, $26, $27
            )
            "#,
            tax_record.trade_id,
            tax_record.timestamp,
            tax_record.tax_year,
            tax_record.transaction_type,
            tax_record.asset_sent,
            tax_record.amount_sent,
            tax_record.asset_received,
            tax_record.amount_received,
            tax_record.usd_value_sent,
            tax_record.usd_value_received,
            tax_record.cost_basis_usd,
            tax_record.proceeds_usd,
            tax_record.capital_gain_loss,
            tax_record.holding_period_days,
            tax_record.gain_type,
            tax_record.gas_fee_usd,
            tax_record.dex_fee_usd,
            tax_record.total_fees_usd,
            tax_record.dex_from,
            tax_record.dex_to,
            tax_record.wallet_address,
            tax_record.blockchain,
            tax_record.transaction_hash,
            tax_record.block_number,
            tax_record.lot_selection_method,
            tax_record.lot_id,
            tax_record.notes,
        )
        .execute(&self.db)
        .await?;
        
        Ok(tax_record)
    }
    
    // Calculate cost basis using FIFO
    async fn calculate_cost_basis(&self, asset: &str, amount: Decimal) -> Result<Decimal> {
        // Query previous purchases of this asset
        let purchases = sqlx::query!(
            r#"
            SELECT amount_received, usd_value_received
            FROM tax_records
            WHERE asset_received = $1
            AND timestamp <= NOW()
            ORDER BY timestamp ASC
            "#,
            asset
        )
        .fetch_all(&self.db)
        .await?;
        
        // FIFO: Use oldest purchases first
        let mut remaining = amount;
        let mut total_cost = Decimal::ZERO;
        
        for purchase in purchases {
            if remaining <= Decimal::ZERO {
                break;
            }
            
            let available = purchase.amount_received;
            let used = remaining.min(available);
            let unit_cost = purchase.usd_value_received / available;
            
            total_cost += used * unit_cost;
            remaining -= used;
        }
        
        Ok(total_cost)
    }
    
    // Export to RP2 format
    pub async fn export_to_rp2(&self, tax_year: i16, output_path: &str) -> Result<()> {
        let records = sqlx::query_as!(
            TaxRecord,
            r#"
            SELECT * FROM tax_records
            WHERE tax_year = $1
            ORDER BY timestamp ASC
            "#,
            tax_year
        )
        .fetch_all(&self.db)
        .await?;
        
        let mut wtr = csv::Writer::from_path(output_path)?;
        
        wtr.write_record(&[
            "timestamp", "asset", "exchange", "holder",
            "transaction_type", "spot_price",
            "crypto_in", "crypto_out_no_fee", "crypto_fee",
            "fiat_in_no_fee", "fiat_in_with_fee",
            "fiat_out_no_fee", "fiat_fee", "notes"
        ])?;
        
        for record in records {
            // Convert to RP2 format
            // This is simplified - you'll need to handle
            // both sides of each swap properly
            wtr.write_record(&[
                &record.timestamp.to_rfc3339(),
                &record.asset_sent,
                &record.dex_from,
                &record.wallet_address,
                "SELL",  // Disposing of asset
                &(record.usd_value_sent / record.amount_sent).to_string(),
                "0",
                &record.amount_sent.to_string(),
                &(record.dex_fee_usd / record.usd_value_sent * record.amount_sent).to_string(),
                "0",
                "0",
                &record.proceeds_usd.to_string(),
                &record.dex_fee_usd.to_string(),
                &record.notes.as_ref().unwrap_or(&String::new()),
            ])?;
        }
        
        wtr.flush()?;
        Ok(())
    }
    
    // Generate annual summary
    pub async fn generate_annual_summary(&self, tax_year: i16) -> Result<TaxSummary> {
        let summary = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total_trades,
                SUM(capital_gain_loss) as total_gain_loss,
                SUM(CASE WHEN capital_gain_loss > 0 THEN capital_gain_loss ELSE 0 END) as total_gains,
                SUM(CASE WHEN capital_gain_loss < 0 THEN capital_gain_loss ELSE 0 END) as total_losses,
                SUM(total_fees_usd) as total_fees
            FROM tax_records
            WHERE tax_year = $1
            "#,
            tax_year
        )
        .fetch_one(&self.db)
        .await?;
        
        Ok(TaxSummary {
            tax_year,
            total_trades: summary.total_trades.unwrap_or(0) as u32,
            total_gain_loss: Decimal::from_str(&summary.total_gain_loss.unwrap_or(0.0).to_string())?,
            total_gains: Decimal::from_str(&summary.total_gains.unwrap_or(0.0).to_string())?,
            total_losses: Decimal::from_str(&summary.total_losses.unwrap_or(0.0).to_string())?,
            total_fees: Decimal::from_str(&summary.total_fees.unwrap_or(0.0).to_string())?,
        })
    }
}

#[derive(Debug)]
pub struct TaxSummary {
    pub tax_year: i16,
    pub total_trades: u32,
    pub total_gain_loss: Decimal,
    pub total_gains: Decimal,
    pub total_losses: Decimal,
    pub total_fees: Decimal,
}
```

### **Database Schema (PostgreSQL)**

```sql
CREATE TABLE tax_records (
    id BIGSERIAL PRIMARY KEY,
    trade_id VARCHAR(255) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    tax_year SMALLINT NOT NULL,
    
    -- Transaction details
    transaction_type VARCHAR(50) NOT NULL,
    asset_sent VARCHAR(50) NOT NULL,
    amount_sent NUMERIC(38, 18) NOT NULL,
    asset_received VARCHAR(50) NOT NULL,
    amount_received NUMERIC(38, 18) NOT NULL,
    
    -- USD valuations
    usd_value_sent NUMERIC(38, 2) NOT NULL,
    usd_value_received NUMERIC(38, 2) NOT NULL,
    cost_basis_usd NUMERIC(38, 2) NOT NULL,
    proceeds_usd NUMERIC(38, 2) NOT NULL,
    
    -- Gain/loss
    capital_gain_loss NUMERIC(38, 2) NOT NULL,
    holding_period_days INTEGER NOT NULL,
    gain_type VARCHAR(20) NOT NULL,
    
    -- Fees
    gas_fee_usd NUMERIC(38, 2) NOT NULL,
    dex_fee_usd NUMERIC(38, 2) NOT NULL,
    total_fees_usd NUMERIC(38, 2) NOT NULL,
    
    -- Source info
    dex_from VARCHAR(100) NOT NULL,
    dex_to VARCHAR(100),
    wallet_address VARCHAR(100) NOT NULL,
    blockchain VARCHAR(50) NOT NULL,
    transaction_hash VARCHAR(100) NOT NULL,
    block_number BIGINT NOT NULL,
    
    -- Accounting
    lot_selection_method VARCHAR(50) NOT NULL,
    lot_id VARCHAR(255),
    
    notes TEXT,
    
    created_at TIMESTAMPTZ DEFAULT NOW(),
    
    UNIQUE(trade_id)
);

-- Indexes for common queries
CREATE INDEX idx_tax_records_tax_year ON tax_records(tax_year);
CREATE INDEX idx_tax_records_timestamp ON tax_records(timestamp);
CREATE INDEX idx_tax_records_asset_sent ON tax_records(asset_sent);
CREATE INDEX idx_tax_records_asset_received ON tax_records(asset_received);
CREATE INDEX idx_tax_records_wallet ON tax_records(wallet_address);
```

---

## Part 4: IRS Forms You'll File

### **Form 8949: Sales and Other Dispositions of Capital Assets**

Every single trade goes here (or a summary if >1000 trades).

**Example Entry**:
```
(a) Description of property:    5000 WMATIC
(b) Date acquired:              01/28/2026
(c) Date sold:                  01/28/2026
(d) Proceeds:                   $5,050.00
(e) Cost basis:                 $5,000.00
(f) Adjustment (fees):          -$15.40
(g) Gain or loss:               $34.60

Gain type: Short-term (held <1 year)
```

For **high-frequency arbitrage** (hundreds of trades), IRS allows:
- Consolidated summary (totals only)
- Attach detailed records separately
- Use crypto tax software to generate

### **Schedule D: Capital Gains and Losses**

Summary from Form 8949:

```
Part I: Short-Term Capital Gains and Losses (assets held ≤1 year)

Total short-term gains:     $12,450.00
Total short-term losses:    -$2,340.00
Net short-term gain:        $10,110.00

Part II: Long-Term Capital Gains and Losses (assets held >1 year)

(None for arbitrage - everything is short-term)

Summary:
Net short-term capital gain:  $10,110.00
Net long-term capital gain:   $0.00
Total capital gain:           $10,110.00

This amount goes to Form 1040, Line 7.
```

### **Form 1040: Individual Tax Return**

Line 7: Capital gains from Schedule D → $10,110.00

**Tax Rate** (Short-term capital gains = ordinary income):

```
Your tax bracket determines rate:

2026 Federal Tax Brackets (Single):
$0 - $11,600:        10%
$11,600 - $47,150:   12%
$47,150 - $100,525:  22%
$100,525 - $191,950: 24%
... (higher brackets)

Example:
If you're in 22% bracket:
$10,110 × 22% = $2,224.20 federal tax owed

Washington State: $0 (no income tax)
```

---

## Part 5: Important IRS Rules & Best Practices

### **1. Wash Sale Rule (Doesn't Apply to Crypto Yet!)**

```
Stock world: Can't claim loss if you rebuy within 30 days.
Crypto world: Wash sale rule does NOT apply (as of 2026).

This means:
- You CAN sell WMATIC at a loss
- Buy it back immediately
- Still claim the loss

However: This could change! Proposed legislation may change this.
```

### **2. Cost Basis Methods**

```
FIFO (First In, First Out):
├─ Default method
├─ Use oldest purchase price as cost basis
└─ Simplest, but may result in higher taxes

LIFO (Last In, First Out):
├─ Use newest purchase price
└─ Can reduce taxes in rising markets

HIFO (Highest In, First Out):
├─ Use highest purchase price
└─ Minimizes capital gains (max tax savings)

Specific ID:
├─ Manually select which lot to sell
└─ Most flexible, but requires documentation

For arbitrage (holding <1 minute):
→ FIFO is simplest since all trades are same-day
```

### **3. Rev. Proc. 2024-28 (CRITICAL NEW RULE!)**

**Starting 2025 tax year**, IRS requires:

```
Old way (Universal Application):
├─ Pool all same-asset holdings together
└─ Calculate cost basis across all wallets/exchanges

New way (Per-Wallet Application):
├─ Each wallet/exchange is separate
├─ Must track cost basis PER WALLET
└─ Transfers between wallets = taxable events!

Impact on arbitrage:
If you use multiple wallets, you MUST track each separately.

Example:
Wallet A: Buy WMATIC for $1.00
Wallet B: Buy WMATIC for $1.05
Sell from Wallet A: Use $1.00 cost basis
Sell from Wallet B: Use $1.05 cost basis

Cannot mix/match anymore!
```

### **4. Form 1099-DA (Starting 2025)**

```
Exchanges will send you Form 1099-DA:
├─ Reports your trades to IRS
├─ Lists gross proceeds
├─ May show cost basis (if available)

You MUST reconcile:
├─ Your records vs 1099-DA
├─ Adjust for any errors
└─ Report correctly on Form 8949

For DEX trading (Uniswap, Sushiswap):
├─ DEXs don't issue 1099-DA (they're not brokers)
├─ But your wallet provider might
└─ You're still responsible for all records
```

### **5. Record Retention**

```
IRS requires:
├─ Keep records for 6 years minimum
├─ Include transaction receipts
├─ Include cost basis documentation
└─ Include all supporting calculations

For crypto:
├─ On-chain transaction hashes
├─ Exchange records
├─ Price data sources
└─ Wallet addresses

Your bot should:
├─ Log to database (permanent)
├─ Export annual CSV backups
└─ Save to multiple locations
```

---

## Part 6: Practical Implementation Checklist

### **Week 1: Setup Tax Logging**

```bash
# 1. Create tax database
createdb dexarb_tax

# 2. Run schema migration
psql dexarb_tax < migrations/001_tax_records.sql

# 3. Add tax logging to executor
# In src/executor/mod.rs:

use crate::tax::TaxLogger;

pub struct Executor {
    // ... existing fields
    tax_logger: Arc<TaxLogger>,
}

impl Executor {
    pub async fn execute_arbitrage(&self, opportunity: Opportunity) -> Result<()> {
        // Execute trades
        let result = self.execute_trades(&opportunity).await?;
        
        // Log for taxes
        for trade in &result.trades {
            self.tax_logger.log_trade(trade, &self.price_oracle).await?;
        }
        
        Ok(())
    }
}
```

### **Monthly: Review Tax Records**

```bash
# Generate monthly summary
cargo run --bin tax-summary -- --month 2026-01

# Export to RP2 format for review
cargo run --bin tax-export -- --year 2026 --output rp2_2026.csv
```

### **Annually: File Taxes**

```bash
# 1. Export full year to RP2
cargo run --bin tax-export -- --year 2026 --output tax_2026.csv

# 2. Process with RP2
rp2_us -m fifo -o output_2026 config.ini tax_2026.csv

# 3. Review generated Form 8949
open output_2026/8949.ods

# 4. Import to TurboTax or give to accountant

# 5. File before April 15, 2027
```

---

## Part 7: Cost Basis Tracking for Arbitrage

### **Simple Case: Pure USDC Arbitrage**

```
All trades start and end with USDC:

Trade 1: $5,000 USDC → WMATIC
├─ Cost basis of WMATIC: $5,000
└─ (This is what you paid)

Trade 2: WMATIC → $5,050 USDC
├─ Proceeds: $5,050
├─ Cost basis: $5,000 (from Trade 1)
├─ Capital gain: $50
└─ Tax owed: $50 × your tax rate

USDC itself:
├─ $1 = $1 (stablecoin)
├─ No gain/loss when buying WMATIC with USDC
└─ All gain happens on the sell
```

### **Complex Case: Holding Inventory**

```
If you sometimes hold WMATIC overnight:

Day 1: Buy 5,000 WMATIC @ $1.00 = $5,000 cost basis
Day 2: Buy 3,000 WMATIC @ $1.05 = $3,150 cost basis
Day 3: Sell 6,000 WMATIC @ $1.10

FIFO calculation:
├─ First 5,000 from Day 1 @ $1.00 = $5,000 cost
├─ Next 1,000 from Day 2 @ $1.05 = $1,050 cost
├─ Total cost basis: $6,050
├─ Proceeds: 6,000 × $1.10 = $6,600
├─ Capital gain: $6,600 - $6,050 = $550

Your tax logger must:
├─ Track all purchases (FIFO queue)
├─ Consume oldest first when selling
└─ Calculate weighted cost basis
```

---

## Part 8: Summary & Action Items

### **What You MUST Do**

1. ✅ **Log every single trade** to database
2. ✅ **Record USD values** at time of trade
3. ✅ **Calculate cost basis** using FIFO (or chosen method)
4. ✅ **Track all fees** (gas + DEX fees)
5. ✅ **Keep records for 6 years**
6. ✅ **Export to RP2 format** annually
7. ✅ **File Form 8949 & Schedule D** by April 15

### **Recommended Tools**

**Primary**: RP2 (Python)
- https://github.com/eprbell/rp2
- Best for US taxes
- Free, open-source
- Generates Form 8949

**Alternative**: Taxcount (Rust)
- https://github.com/dcdpr/taxcount
- Native Rust
- May need EVM adaptation

**Commercial** (if you hate coding):
- CoinTracker.io
- Koinly.io
- TaxBit.com

### **Washington State Advantage**

```
✅ No state income tax
✅ Only federal taxes apply
✅ Simpler than most states

Total tax rate = Federal rate only
(22% bracket = 22% total, not 22% + state)
```

### **Expected Tax Liability**

```
Scenario: $10,000 profit from arbitrage in 2026

Federal tax (22% bracket):
$10,000 × 22% = $2,200

Washington State tax:
$0

Total tax owed: $2,200

Net profit after tax: $7,800
```

---

## Part 9: Integration Example

```rust
// src/main.rs

mod tax;

use tax::TaxLogger;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tax logger
    let tax_logger = Arc::new(
        TaxLogger::new("postgresql://localhost/dexarb_tax").await?
    );
    
    // Create executor with tax logging
    let executor = Executor::new(
        config,
        web3_provider,
        price_oracle,
        tax_logger.clone(),
    );
    
    // Run arbitrage bot
    // Every trade automatically logged to tax database
    executor.run().await?;
    
    Ok(())
}
```

---

## Bottom Line

**For US/Washington State arbitrage trading**:

1. Use **RP2** (Python) for tax calculation
2. Implement **TaxLogger** in your bot (Rust)
3. Export to RP2 format annually
4. File **Form 8949** & **Schedule D**
5. Pay taxes by April 15

**No state income tax = simpler than most states!**

Your effective tax rate = your federal bracket (12-37% depending on total income).

For $50K arbitrage profit → expect ~$11K-13K in federal taxes (22-24% bracket).

Keep detailed records, use RP2, and you're golden. 🎯
