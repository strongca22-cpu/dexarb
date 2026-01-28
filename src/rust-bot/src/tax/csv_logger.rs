//! CSV Tax Logger
//!
//! Logs tax records to CSV files for IRS compliance and audit trail.
//! Creates annual files: data/tax/trades_YYYY.csv
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use super::TaxRecord;
use anyhow::{Context, Result};
use chrono::Datelike;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// CSV logger for tax records
pub struct TaxCsvLogger {
    /// Base directory for tax files
    base_dir: PathBuf,
    /// Current tax year being logged
    current_year: i16,
    /// Whether headers have been written for current year
    headers_written: bool,
}

impl TaxCsvLogger {
    /// CSV headers matching TaxRecord fields
    const HEADERS: &'static [&'static str] = &[
        "trade_id",
        "timestamp",
        "tax_year",
        "transaction_type",
        "asset_sent",
        "amount_sent",
        "token_sent_decimals",
        "asset_received",
        "amount_received",
        "token_received_decimals",
        "usd_value_sent",
        "usd_value_received",
        "spot_price_sent",
        "spot_price_received",
        "cost_basis_usd",
        "proceeds_usd",
        "capital_gain_loss",
        "holding_period_days",
        "gain_type",
        "gas_fee_native",
        "gas_fee_usd",
        "dex_fee_percent",
        "dex_fee_usd",
        "total_fees_usd",
        "blockchain",
        "chain_id",
        "transaction_hash",
        "block_number",
        "wallet_address",
        "dex_buy",
        "dex_sell",
        "pool_address_buy",
        "pool_address_sell",
        "lot_selection_method",
        "lot_id",
        "spread_percent",
        "is_paper_trade",
        "notes",
    ];

    /// Create a new CSV logger
    ///
    /// # Arguments
    /// * `base_dir` - Directory to store tax CSV files
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        fs::create_dir_all(&base_dir)
            .with_context(|| format!("Failed to create tax directory: {:?}", base_dir))?;

        let current_year = chrono::Utc::now().year() as i16;

        // Check if file exists (headers already written)
        let file_path = Self::file_path_for_year(&base_dir, current_year);
        let headers_written = file_path.exists();

        Ok(Self {
            base_dir,
            current_year,
            headers_written,
        })
    }

    /// Get file path for a specific tax year
    fn file_path_for_year(base_dir: &Path, year: i16) -> PathBuf {
        base_dir.join(format!("trades_{}.csv", year))
    }

    /// Get the current file path
    fn current_file_path(&self) -> PathBuf {
        Self::file_path_for_year(&self.base_dir, self.current_year)
    }

    /// Log a tax record to CSV
    pub fn log(&mut self, record: &TaxRecord) -> Result<()> {
        // Check if we need to switch to a new year's file
        if record.tax_year != self.current_year {
            self.current_year = record.tax_year;
            let file_path = self.current_file_path();
            self.headers_written = file_path.exists();
        }

        let file_path = self.current_file_path();

        // Open file in append mode
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .with_context(|| format!("Failed to open tax CSV file: {:?}", file_path))?;

        // Write headers if needed
        if !self.headers_written {
            self.write_headers(&mut file)?;
            self.headers_written = true;
        }

        // Write the record
        self.write_record(&mut file, record)?;

        Ok(())
    }

    /// Write CSV headers
    fn write_headers(&self, file: &mut File) -> Result<()> {
        let header_line = Self::HEADERS.join(",");
        writeln!(file, "{}", header_line)?;
        Ok(())
    }

    /// Write a single record as CSV line
    fn write_record(&self, file: &mut File, record: &TaxRecord) -> Result<()> {
        let fields = vec![
            record.trade_id.clone(),
            record.timestamp.to_rfc3339(),
            record.tax_year.to_string(),
            record.transaction_type.to_string(),
            record.asset_sent.clone(),
            record.amount_sent.to_string(),
            record.token_sent_decimals.to_string(),
            record.asset_received.clone(),
            record.amount_received.to_string(),
            record.token_received_decimals.to_string(),
            record.usd_value_sent.to_string(),
            record.usd_value_received.to_string(),
            record.spot_price_sent.to_string(),
            record.spot_price_received.to_string(),
            record.cost_basis_usd.to_string(),
            record.proceeds_usd.to_string(),
            record.capital_gain_loss.to_string(),
            record.holding_period_days.to_string(),
            record.gain_type.to_string(),
            record.gas_fee_native.to_string(),
            record.gas_fee_usd.to_string(),
            record.dex_fee_percent.to_string(),
            record.dex_fee_usd.to_string(),
            record.total_fees_usd.to_string(),
            record.blockchain.clone(),
            record.chain_id.to_string(),
            record.transaction_hash.clone(),
            record.block_number.to_string(),
            record.wallet_address.clone(),
            record.dex_buy.clone(),
            record.dex_sell.clone(),
            record.pool_address_buy.clone(),
            record.pool_address_sell.clone(),
            record.lot_selection_method.clone(),
            record.lot_id.clone().unwrap_or_default(),
            record.spread_percent.to_string(),
            record.is_paper_trade.to_string(),
            // Escape notes field (may contain commas)
            escape_csv_field(&record.notes.clone().unwrap_or_default()),
        ];

        let line = fields.join(",");
        writeln!(file, "{}", line)?;

        Ok(())
    }

    /// Get the path to the current year's CSV file
    pub fn get_current_file_path(&self) -> PathBuf {
        self.current_file_path()
    }

    /// Get the path to a specific year's CSV file
    pub fn get_file_path_for_year(&self, year: i16) -> PathBuf {
        Self::file_path_for_year(&self.base_dir, year)
    }

    /// Check if the current year's file exists
    pub fn file_exists(&self) -> bool {
        self.current_file_path().exists()
    }

    /// Count records in the current year's file
    pub fn record_count(&self) -> Result<usize> {
        let path = self.current_file_path();
        if !path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(&path)?;
        let line_count = content.lines().count();

        // Subtract 1 for header line
        Ok(line_count.saturating_sub(1))
    }
}

/// Escape a CSV field that may contain special characters
fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        // Wrap in quotes and escape any quotes
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::env;

    #[test]
    fn test_csv_escape() {
        assert_eq!(escape_csv_field("simple"), "simple");
        assert_eq!(escape_csv_field("has,comma"), "\"has,comma\"");
        assert_eq!(escape_csv_field("has\"quote"), "\"has\"\"quote\"");
    }

    #[test]
    fn test_csv_logger_creation() {
        let temp_dir = env::temp_dir().join("dexarb_tax_test");
        let _ = fs::remove_dir_all(&temp_dir); // Clean up from previous runs

        let logger = TaxCsvLogger::new(&temp_dir);
        assert!(logger.is_ok());

        let logger = logger.unwrap();
        assert!(!logger.headers_written);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_csv_logging() {
        let temp_dir = env::temp_dir().join("dexarb_tax_test_log");
        let _ = fs::remove_dir_all(&temp_dir);

        let mut logger = TaxCsvLogger::new(&temp_dir).unwrap();

        let record = crate::tax::TaxRecord::new_arbitrage(
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
            false,
        );

        let result = logger.log(&record);
        assert!(result.is_ok());
        assert!(logger.file_exists());
        assert_eq!(logger.record_count().unwrap(), 1);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
