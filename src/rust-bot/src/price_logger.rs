//! Historical Price Logger
//!
//! Logs V3 pool price snapshots to CSV files for offline research and analysis.
//! One row per pool per block, rotated daily.
//!
//! Output format (CSV):
//!   timestamp, block, pair, dex, fee, price, tick, liquidity, sqrt_price_x96, address
//!
//! File naming: prices_YYYYMMDD.csv (auto-rotated at midnight UTC)
//!
//! Author: AI-Generated
//! Created: 2026-01-30
//! Modified: 2026-01-30

use crate::types::V3PoolState;
use chrono::{NaiveDate, Utc};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// CSV header for price log files
const CSV_HEADER: &str = "timestamp,block,pair,dex,fee,price,tick,liquidity,sqrt_price_x96,address";

/// Historical price logger — appends V3 pool snapshots to daily CSV files.
pub struct PriceLogger {
    /// Directory for price log files
    log_dir: PathBuf,
    /// Currently open file date (for rotation detection)
    current_date: Option<NaiveDate>,
    /// Currently open file handle
    file: Option<File>,
}

impl PriceLogger {
    /// Create a new PriceLogger. Creates the log directory if it doesn't exist.
    pub fn new(log_dir: &str) -> Self {
        let path = PathBuf::from(log_dir);
        if let Err(e) = fs::create_dir_all(&path) {
            warn!("Failed to create price log directory {}: {}", log_dir, e);
        }
        info!("PriceLogger initialized: {}", log_dir);

        Self {
            log_dir: path,
            current_date: None,
            file: None,
        }
    }

    /// Log price snapshots for all V3 pools at a given block.
    /// Appends one CSV row per pool. Rotates file daily.
    pub fn log_prices(&mut self, block_number: u64, pools: &[V3PoolState]) {
        let now = Utc::now();
        let today = now.date_naive();

        // Rotate file if date changed
        if self.current_date != Some(today) {
            self.rotate_file(today);
        }

        let file = match self.file.as_mut() {
            Some(f) => f,
            None => {
                debug!("PriceLogger: no open file, skipping");
                return;
            }
        };

        let timestamp = now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

        for pool in pools {
            let line = format!(
                "{},{},{},{},{},{:.10},{},{},{},{:?}\n",
                timestamp,
                block_number,
                pool.pair.symbol,
                pool.dex,
                pool.fee,
                pool.price(),
                pool.tick,
                pool.liquidity,
                pool.sqrt_price_x96,
                pool.address,
            );

            if let Err(e) = file.write_all(line.as_bytes()) {
                warn!("PriceLogger write error: {}", e);
                return;
            }
        }

        // Flush to ensure data is on disk
        if let Err(e) = file.flush() {
            debug!("PriceLogger flush error: {}", e);
        }
    }

    /// Rotate to a new daily file
    fn rotate_file(&mut self, date: NaiveDate) {
        // Close existing file
        self.file = None;
        self.current_date = None;

        let filename = format!("prices_{}.csv", date.format("%Y%m%d"));
        let filepath = self.log_dir.join(&filename);

        let file_exists = filepath.exists();

        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&filepath)
        {
            Ok(mut f) => {
                // Write header if new file
                if !file_exists {
                    if let Err(e) = writeln!(f, "{}", CSV_HEADER) {
                        warn!("PriceLogger: failed to write header to {}: {}", filename, e);
                        return;
                    }
                    info!("PriceLogger: created new file {}", filename);
                } else {
                    info!("PriceLogger: appending to existing {}", filename);
                }
                self.file = Some(f);
                self.current_date = Some(date);
            }
            Err(e) => {
                warn!("PriceLogger: failed to open {}: {}", filename, e);
            }
        }
    }
}
