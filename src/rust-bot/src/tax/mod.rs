//! Tax Logging Module
//!
//! Comprehensive tax record logging for IRS compliance.
//! Captures all data required for Form 8949 and Schedule D.
//!
//! Key features:
//! - TaxRecord with all 34+ IRS-required fields
//! - CSV logging for permanent audit trail
//! - JSON backup for redundancy
//! - RP2 export format for tax calculation software
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//!
//! References:
//! - IRS Form 8949: Sales and Other Dispositions of Capital Assets
//! - Rev. Proc. 2024-28: Per-wallet cost basis tracking
//! - RP2 tax software: https://github.com/eprbell/rp2

pub mod csv_logger;
pub mod json_logger;
pub mod price_oracle;
pub mod rp2_export;

pub use csv_logger::TaxCsvLogger;
pub use json_logger::{TaxJsonLogger, TaxLogger};
pub use price_oracle::{PriceOracle, TaxRecordBuilder, TOKEN_DECIMALS};
pub use rp2_export::{export_to_rp2, export_year_to_rp2, generate_rp2_config, validate_rp2_export};

use chrono::{DateTime, Datelike, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Tax event type for IRS classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaxEventType {
    /// Crypto-to-crypto swap (most arbitrage trades)
    Swap,
    /// Fiat to crypto purchase (initial funding)
    Buy,
    /// Crypto to fiat sale (withdrawal)
    Sell,
    /// Transfer between own wallets (not taxable, but track for audit)
    Transfer,
    /// Gas-only transaction (failed trade, etc.)
    Fee,
}

impl fmt::Display for TaxEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaxEventType::Swap => write!(f, "SWAP"),
            TaxEventType::Buy => write!(f, "BUY"),
            TaxEventType::Sell => write!(f, "SELL"),
            TaxEventType::Transfer => write!(f, "TRANSFER"),
            TaxEventType::Fee => write!(f, "FEE"),
        }
    }
}

/// Capital gain type for IRS classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GainType {
    /// Held less than 1 year (all arbitrage trades)
    ShortTerm,
    /// Held 1 year or more (never for arbitrage)
    LongTerm,
}

impl fmt::Display for GainType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GainType::ShortTerm => write!(f, "SHORT_TERM"),
            GainType::LongTerm => write!(f, "LONG_TERM"),
        }
    }
}

/// Comprehensive tax record for IRS compliance
///
/// Contains all fields required for:
/// - Form 8949 (Sales and Other Dispositions of Capital Assets)
/// - Schedule D (Capital Gains and Losses)
/// - Rev. Proc. 2024-28 (per-wallet cost basis tracking)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRecord {
    // === IDENTIFICATION ===
    /// Unique identifier for this trade (UUID)
    pub trade_id: String,
    /// UTC timestamp of the trade
    pub timestamp: DateTime<Utc>,
    /// Tax year (2026, 2027, etc.)
    pub tax_year: i16,

    // === TRANSACTION TYPE ===
    /// Type of tax event
    pub transaction_type: TaxEventType,

    // === ASSETS SENT ===
    /// Symbol of asset disposed (e.g., "USDC", "WMATIC")
    pub asset_sent: String,
    /// Raw token amount sent (before decimals)
    pub amount_sent: Decimal,
    /// Token decimals for asset_sent
    pub token_sent_decimals: u8,

    // === ASSETS RECEIVED ===
    /// Symbol of asset acquired (e.g., "WMATIC", "USDC")
    pub asset_received: String,
    /// Raw token amount received (before decimals)
    pub amount_received: Decimal,
    /// Token decimals for asset_received
    pub token_received_decimals: u8,

    // === USD VALUATIONS (CRITICAL FOR IRS!) ===
    /// Fair market value of asset_sent in USD at time of trade
    pub usd_value_sent: Decimal,
    /// Fair market value of asset_received in USD at time of trade
    pub usd_value_received: Decimal,
    /// Spot price of asset_sent per unit in USD
    pub spot_price_sent: Decimal,
    /// Spot price of asset_received per unit in USD
    pub spot_price_received: Decimal,

    // === COST BASIS (For Capital Gains Calculation) ===
    /// Original cost basis of asset_sent in USD
    pub cost_basis_usd: Decimal,
    /// Proceeds from disposing asset_sent (= usd_value_received)
    pub proceeds_usd: Decimal,
    /// Capital gain or loss (proceeds - cost_basis)
    pub capital_gain_loss: Decimal,
    /// Days held (always 0 for same-block arbitrage)
    pub holding_period_days: i32,
    /// Short-term or long-term gain
    pub gain_type: GainType,

    // === FEES (Deductible) ===
    /// Gas fee in native token (MATIC)
    pub gas_fee_native: Decimal,
    /// Gas fee converted to USD
    pub gas_fee_usd: Decimal,
    /// DEX fee percentage (e.g., 0.30 for 0.30%)
    pub dex_fee_percent: Decimal,
    /// DEX fee in USD
    pub dex_fee_usd: Decimal,
    /// Total fees (gas + DEX) in USD
    pub total_fees_usd: Decimal,

    // === BLOCKCHAIN DATA ===
    /// Blockchain name (e.g., "Polygon")
    pub blockchain: String,
    /// Chain ID (e.g., 137 for Polygon)
    pub chain_id: u64,
    /// On-chain transaction hash
    pub transaction_hash: String,
    /// Block number
    pub block_number: u64,
    /// Wallet address (required for Rev. Proc. 2024-28)
    pub wallet_address: String,

    // === DEX ROUTING ===
    /// DEX where we bought (e.g., "Uniswap", "Sushiswap")
    pub dex_buy: String,
    /// DEX where we sold
    pub dex_sell: String,
    /// Pool contract address for buy
    pub pool_address_buy: String,
    /// Pool contract address for sell
    pub pool_address_sell: String,

    // === ACCOUNTING ===
    /// Cost basis method (FIFO, LIFO, HIFO, SpecID)
    pub lot_selection_method: String,
    /// Specific lot ID if using SpecID method
    pub lot_id: Option<String>,

    // === METADATA ===
    /// Arbitrage spread percentage
    pub spread_percent: Decimal,
    /// Additional notes
    pub notes: Option<String>,
    /// Whether this is a paper trade (not taxable)
    pub is_paper_trade: bool,
}

impl TaxRecord {
    /// Create a new tax record for an arbitrage trade
    #[allow(clippy::too_many_arguments)]
    pub fn new_arbitrage(
        asset_sent: String,
        amount_sent: Decimal,
        token_sent_decimals: u8,
        asset_received: String,
        amount_received: Decimal,
        token_received_decimals: u8,
        spot_price_sent: Decimal,
        spot_price_received: Decimal,
        gas_fee_native: Decimal,
        gas_price_usd: Decimal,
        dex_fee_percent: Decimal,
        transaction_hash: String,
        block_number: u64,
        wallet_address: String,
        dex_buy: String,
        dex_sell: String,
        pool_address_buy: String,
        pool_address_sell: String,
        spread_percent: Decimal,
        is_paper_trade: bool,
    ) -> Self {
        let now = Utc::now();

        // Calculate USD values
        let usd_value_sent = amount_sent * spot_price_sent;
        let usd_value_received = amount_received * spot_price_received;

        // Calculate fees
        let gas_fee_usd = gas_fee_native * gas_price_usd;
        let dex_fee_usd = usd_value_sent * dex_fee_percent / Decimal::from(100);
        let total_fees_usd = gas_fee_usd + dex_fee_usd;

        // For arbitrage: cost basis = what we paid (usd_value_sent)
        // proceeds = what we got back (usd_value_received)
        let cost_basis_usd = usd_value_sent;
        let proceeds_usd = usd_value_received;
        let capital_gain_loss = proceeds_usd - cost_basis_usd - total_fees_usd;

        Self {
            trade_id: generate_trade_id(),
            timestamp: now,
            tax_year: now.year() as i16,

            transaction_type: TaxEventType::Swap,

            asset_sent,
            amount_sent,
            token_sent_decimals,

            asset_received,
            amount_received,
            token_received_decimals,

            usd_value_sent,
            usd_value_received,
            spot_price_sent,
            spot_price_received,

            cost_basis_usd,
            proceeds_usd,
            capital_gain_loss,
            holding_period_days: 0, // Same-block arbitrage
            gain_type: GainType::ShortTerm,

            gas_fee_native,
            gas_fee_usd,
            dex_fee_percent,
            dex_fee_usd,
            total_fees_usd,

            blockchain: "Polygon".to_string(),
            chain_id: 137,
            transaction_hash,
            block_number,
            wallet_address,

            dex_buy,
            dex_sell,
            pool_address_buy,
            pool_address_sell,

            lot_selection_method: "FIFO".to_string(),
            lot_id: None,

            spread_percent,
            notes: None,
            is_paper_trade,
        }
    }

    /// Add a note to the tax record
    pub fn with_note(mut self, note: &str) -> Self {
        self.notes = Some(note.to_string());
        self
    }

    /// Set the lot selection method
    pub fn with_lot_method(mut self, method: &str) -> Self {
        self.lot_selection_method = method.to_string();
        self
    }

    /// Check if this record represents a taxable event
    pub fn is_taxable(&self) -> bool {
        !self.is_paper_trade && self.transaction_type != TaxEventType::Transfer
    }

    /// Get net profit after all fees
    pub fn net_profit(&self) -> Decimal {
        self.capital_gain_loss
    }
}

/// Generate a unique trade ID using timestamp + random suffix
fn generate_trade_id() -> String {
    let now = Utc::now();
    let timestamp = now.format("%Y%m%d%H%M%S%3f").to_string();
    let random: u32 = (now.timestamp_nanos_opt().unwrap_or(0) % 10000) as u32;
    format!("TX-{}-{:04}", timestamp, random)
}

/// Annual tax summary for quick reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxSummary {
    pub tax_year: i16,
    pub total_trades: u32,
    pub total_proceeds: Decimal,
    pub total_cost_basis: Decimal,
    pub total_fees: Decimal,
    pub total_gain_loss: Decimal,
    pub short_term_gain: Decimal,
    pub short_term_loss: Decimal,
    pub long_term_gain: Decimal,
    pub long_term_loss: Decimal,
}

impl TaxSummary {
    /// Create a new empty summary for a tax year
    pub fn new(tax_year: i16) -> Self {
        Self {
            tax_year,
            total_trades: 0,
            total_proceeds: Decimal::ZERO,
            total_cost_basis: Decimal::ZERO,
            total_fees: Decimal::ZERO,
            total_gain_loss: Decimal::ZERO,
            short_term_gain: Decimal::ZERO,
            short_term_loss: Decimal::ZERO,
            long_term_gain: Decimal::ZERO,
            long_term_loss: Decimal::ZERO,
        }
    }

    /// Add a tax record to the summary
    pub fn add_record(&mut self, record: &TaxRecord) {
        if !record.is_taxable() {
            return;
        }

        self.total_trades += 1;
        self.total_proceeds += record.proceeds_usd;
        self.total_cost_basis += record.cost_basis_usd;
        self.total_fees += record.total_fees_usd;
        self.total_gain_loss += record.capital_gain_loss;

        match record.gain_type {
            GainType::ShortTerm => {
                if record.capital_gain_loss > Decimal::ZERO {
                    self.short_term_gain += record.capital_gain_loss;
                } else {
                    self.short_term_loss += record.capital_gain_loss.abs();
                }
            }
            GainType::LongTerm => {
                if record.capital_gain_loss > Decimal::ZERO {
                    self.long_term_gain += record.capital_gain_loss;
                } else {
                    self.long_term_loss += record.capital_gain_loss.abs();
                }
            }
        }
    }

    /// Generate summary report string
    pub fn report(&self) -> String {
        format!(
            r#"
═══════════════════════════════════════════════════════
           TAX YEAR {} SUMMARY
═══════════════════════════════════════════════════════

Total Trades:        {}
Total Proceeds:      ${:.2}
Total Cost Basis:    ${:.2}
Total Fees:          ${:.2}

SHORT-TERM CAPITAL GAINS/LOSSES:
  Gains:             ${:.2}
  Losses:            ${:.2}
  Net:               ${:.2}

LONG-TERM CAPITAL GAINS/LOSSES:
  Gains:             ${:.2}
  Losses:            ${:.2}
  Net:               ${:.2}

TOTAL NET GAIN/LOSS: ${:.2}
═══════════════════════════════════════════════════════
"#,
            self.tax_year,
            self.total_trades,
            self.total_proceeds,
            self.total_cost_basis,
            self.total_fees,
            self.short_term_gain,
            self.short_term_loss,
            self.short_term_gain - self.short_term_loss,
            self.long_term_gain,
            self.long_term_loss,
            self.long_term_gain - self.long_term_loss,
            self.total_gain_loss,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_tax_record_creation() {
        let record = TaxRecord::new_arbitrage(
            "USDC".to_string(),
            dec!(1000),
            6,
            "WMATIC".to_string(),
            dec!(1010),
            18,
            dec!(1.0),    // USDC price
            dec!(1.0),    // WMATIC price
            dec!(0.001),  // Gas in MATIC
            dec!(0.90),   // MATIC price in USD
            dec!(0.30),   // DEX fee %
            "0xabc123".to_string(),
            12345678,
            "0xwallet".to_string(),
            "Uniswap".to_string(),
            "Sushiswap".to_string(),
            "0xpool1".to_string(),
            "0xpool2".to_string(),
            dec!(1.0),
            false,
        );

        assert_eq!(record.asset_sent, "USDC");
        assert_eq!(record.asset_received, "WMATIC");
        assert_eq!(record.gain_type, GainType::ShortTerm);
        assert_eq!(record.holding_period_days, 0);
        assert!(record.is_taxable());
    }

    #[test]
    fn test_paper_trade_not_taxable() {
        let record = TaxRecord::new_arbitrage(
            "USDC".to_string(),
            dec!(1000),
            6,
            "WMATIC".to_string(),
            dec!(1010),
            18,
            dec!(1.0),
            dec!(1.0),
            dec!(0.001),
            dec!(0.90),
            dec!(0.30),
            "0xabc123".to_string(),
            12345678,
            "0xwallet".to_string(),
            "Uniswap".to_string(),
            "Sushiswap".to_string(),
            "0xpool1".to_string(),
            "0xpool2".to_string(),
            dec!(1.0),
            true, // Paper trade
        );

        assert!(!record.is_taxable());
    }

    #[test]
    fn test_tax_summary() {
        let mut summary = TaxSummary::new(2026);

        let record = TaxRecord::new_arbitrage(
            "USDC".to_string(),
            dec!(1000),
            6,
            "WMATIC".to_string(),
            dec!(1050),
            18,
            dec!(1.0),
            dec!(1.0),
            dec!(0.001),
            dec!(0.90),
            dec!(0.30),
            "0xabc123".to_string(),
            12345678,
            "0xwallet".to_string(),
            "Uniswap".to_string(),
            "Sushiswap".to_string(),
            "0xpool1".to_string(),
            "0xpool2".to_string(),
            dec!(5.0),
            false,
        );

        summary.add_record(&record);

        assert_eq!(summary.total_trades, 1);
        assert!(summary.total_gain_loss > Decimal::ZERO);
    }
}
