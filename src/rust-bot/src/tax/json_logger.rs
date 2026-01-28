//! JSON Tax Logger
//!
//! Backup logging of tax records to JSON files.
//! Creates annual files: data/tax/trades_YYYY.json
//!
//! JSON format provides:
//! - Easy parsing for scripts and tools
//! - Human-readable backup
//! - Redundancy for CSV files
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use super::TaxRecord;
use anyhow::{Context, Result};
use chrono::Datelike;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// JSON logger for tax records (JSONL format - one record per line)
pub struct TaxJsonLogger {
    /// Base directory for tax files
    base_dir: PathBuf,
    /// Current tax year being logged
    current_year: i16,
}

impl TaxJsonLogger {
    /// Create a new JSON logger
    ///
    /// # Arguments
    /// * `base_dir` - Directory to store tax JSON files
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        fs::create_dir_all(&base_dir)
            .with_context(|| format!("Failed to create tax directory: {:?}", base_dir))?;

        let current_year = chrono::Utc::now().year() as i16;

        Ok(Self {
            base_dir,
            current_year,
        })
    }

    /// Get file path for a specific tax year
    fn file_path_for_year(base_dir: &Path, year: i16) -> PathBuf {
        base_dir.join(format!("trades_{}.jsonl", year))
    }

    /// Get the current file path
    fn current_file_path(&self) -> PathBuf {
        Self::file_path_for_year(&self.base_dir, self.current_year)
    }

    /// Log a tax record to JSON (JSONL format)
    pub fn log(&mut self, record: &TaxRecord) -> Result<()> {
        // Check if we need to switch to a new year's file
        if record.tax_year != self.current_year {
            self.current_year = record.tax_year;
        }

        let file_path = self.current_file_path();

        // Open file in append mode
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .with_context(|| format!("Failed to open tax JSON file: {:?}", file_path))?;

        // Serialize record to JSON and write as single line
        let json = serde_json::to_string(record)
            .context("Failed to serialize tax record to JSON")?;

        writeln!(file, "{}", json)?;

        Ok(())
    }

    /// Get the path to the current year's JSON file
    pub fn get_current_file_path(&self) -> PathBuf {
        self.current_file_path()
    }

    /// Get the path to a specific year's JSON file
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

        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        Ok(reader.lines().count())
    }

    /// Read all records from a specific year
    pub fn read_all(&self, year: i16) -> Result<Vec<TaxRecord>> {
        let path = Self::file_path_for_year(&self.base_dir, year);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&path)?;
        let reader = BufReader::new(file);

        let mut records = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                let record: TaxRecord = serde_json::from_str(&line)
                    .with_context(|| format!("Failed to parse JSON line: {}", line))?;
                records.push(record);
            }
        }

        Ok(records)
    }

    /// Read all records from current year
    pub fn read_current_year(&self) -> Result<Vec<TaxRecord>> {
        self.read_all(self.current_year)
    }
}

/// Combined tax logger that writes to both CSV and JSON
pub struct TaxLogger {
    csv_logger: super::csv_logger::TaxCsvLogger,
    json_logger: TaxJsonLogger,
}

impl TaxLogger {
    /// Create a new combined tax logger
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        let base_dir = base_dir.as_ref();

        Ok(Self {
            csv_logger: super::csv_logger::TaxCsvLogger::new(base_dir)?,
            json_logger: TaxJsonLogger::new(base_dir)?,
        })
    }

    /// Log a tax record to both CSV and JSON
    pub fn log(&mut self, record: &TaxRecord) -> Result<()> {
        // Log to CSV (primary)
        self.csv_logger.log(record)?;

        // Log to JSON (backup)
        self.json_logger.log(record)?;

        Ok(())
    }

    /// Get record count from CSV
    pub fn record_count(&self) -> Result<usize> {
        self.csv_logger.record_count()
    }

    /// Read all records from JSON for a year
    pub fn read_all(&self, year: i16) -> Result<Vec<TaxRecord>> {
        self.json_logger.read_all(year)
    }

    /// Get CSV file path
    pub fn csv_path(&self) -> PathBuf {
        self.csv_logger.get_current_file_path()
    }

    /// Get JSON file path
    pub fn json_path(&self) -> PathBuf {
        self.json_logger.get_current_file_path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::env;

    #[test]
    fn test_json_logger_creation() {
        let temp_dir = env::temp_dir().join("dexarb_tax_json_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let logger = TaxJsonLogger::new(&temp_dir);
        assert!(logger.is_ok());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_json_logging_and_reading() {
        let temp_dir = env::temp_dir().join("dexarb_tax_json_test_rw");
        let _ = fs::remove_dir_all(&temp_dir);

        let mut logger = TaxJsonLogger::new(&temp_dir).unwrap();

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

        // Log record
        let result = logger.log(&record);
        assert!(result.is_ok());
        assert!(logger.file_exists());
        assert_eq!(logger.record_count().unwrap(), 1);

        // Read back
        let records = logger.read_current_year().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].asset_sent, "USDC");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_combined_logger() {
        let temp_dir = env::temp_dir().join("dexarb_tax_combined_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let mut logger = TaxLogger::new(&temp_dir).unwrap();

        let record = crate::tax::TaxRecord::new_arbitrage(
            "USDC".to_string(),
            dec!(500),
            6,
            "WETH".to_string(),
            dec!(0.25),
            18,
            dec!(1.0),
            dec!(2000.0),
            dec!(0.001),
            dec!(0.90),
            dec!(0.30),
            "0xdef456".to_string(),
            12345679,
            "0xwallet".to_string(),
            "Sushiswap".to_string(),
            "Uniswap".to_string(),
            "0xpool3".to_string(),
            "0xpool4".to_string(),
            dec!(0.5),
            true, // Paper trade
        );

        let result = logger.log(&record);
        assert!(result.is_ok());

        // Both files should exist
        assert!(logger.csv_path().exists());
        assert!(logger.json_path().exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
