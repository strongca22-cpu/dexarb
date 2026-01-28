//! RP2 Export Format
//!
//! Exports tax records to RP2-compatible CSV format for tax calculation.
//! RP2 is a privacy-focused, open-source crypto tax calculator.
//!
//! Repository: https://github.com/eprbell/rp2
//!
//! RP2 CSV Format:
//! - timestamp: ISO 8601 format
//! - asset: Token symbol (e.g., "WMATIC")
//! - exchange: DEX name (e.g., "Uniswap")
//! - holder: Wallet identifier
//! - transaction_type: "BUY", "SELL", or "FEE"
//! - spot_price: USD price per unit
//! - crypto_in: Amount received (for BUY)
//! - crypto_out_no_fee: Amount sent (for SELL)
//! - crypto_fee: Fee in crypto
//! - fiat_in_no_fee: USD spent (for BUY)
//! - fiat_in_with_fee: USD spent including fees
//! - fiat_out_no_fee: USD received (for SELL)
//! - fiat_fee: Fee in USD
//! - notes: Additional context
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use super::{TaxEventType, TaxRecord};
use anyhow::{Context, Result};
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// RP2 CSV headers
const RP2_HEADERS: &[&str] = &[
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
    "notes",
];

/// Export tax records to RP2 CSV format
///
/// For arbitrage swaps, this creates TWO entries per TaxRecord:
/// 1. SELL of asset_sent (disposing the input token)
/// 2. BUY of asset_received (acquiring the output token)
///
/// # Arguments
/// * `records` - Tax records to export
/// * `output_path` - Path to output CSV file
/// * `holder_name` - Wallet identifier (e.g., "MainWallet" or wallet address)
pub fn export_to_rp2<P: AsRef<Path>>(
    records: &[TaxRecord],
    output_path: P,
    holder_name: &str,
) -> Result<usize> {
    let output_path = output_path.as_ref();

    let mut file = File::create(output_path)
        .with_context(|| format!("Failed to create RP2 export file: {:?}", output_path))?;

    // Write headers
    writeln!(file, "{}", RP2_HEADERS.join(","))?;

    let mut row_count = 0;

    for record in records {
        // Skip paper trades - they're not taxable
        if record.is_paper_trade {
            continue;
        }

        match record.transaction_type {
            TaxEventType::Swap => {
                // Arbitrage swap creates 2 entries: SELL input, BUY output
                write_sell_entry(&mut file, record, holder_name)?;
                write_buy_entry(&mut file, record, holder_name)?;
                row_count += 2;
            }
            TaxEventType::Buy => {
                write_buy_entry(&mut file, record, holder_name)?;
                row_count += 1;
            }
            TaxEventType::Sell => {
                write_sell_entry(&mut file, record, holder_name)?;
                row_count += 1;
            }
            TaxEventType::Fee => {
                write_fee_entry(&mut file, record, holder_name)?;
                row_count += 1;
            }
            TaxEventType::Transfer => {
                // Transfers are not taxable, skip
                continue;
            }
        }
    }

    Ok(row_count)
}

/// Write a SELL entry (disposing of asset_sent)
fn write_sell_entry(file: &mut File, record: &TaxRecord, holder: &str) -> Result<()> {
    let fields = vec![
        record.timestamp.to_rfc3339(),
        record.asset_sent.clone(),
        record.dex_buy.clone(), // Exchange where we sold (sent) the token
        holder.to_string(),
        "SELL".to_string(),
        record.spot_price_sent.to_string(),
        "".to_string(),                        // crypto_in (empty for SELL)
        record.amount_sent.to_string(),        // crypto_out_no_fee
        "".to_string(),                        // crypto_fee
        "".to_string(),                        // fiat_in_no_fee
        "".to_string(),                        // fiat_in_with_fee
        record.usd_value_sent.to_string(),     // fiat_out_no_fee
        record.dex_fee_usd.to_string(),        // fiat_fee
        escape_csv(&format!(
            "Arbitrage SELL: {} {} @ ${} on {}",
            record.amount_sent, record.asset_sent, record.spot_price_sent, record.dex_buy
        )),
    ];

    writeln!(file, "{}", fields.join(","))?;
    Ok(())
}

/// Write a BUY entry (acquiring asset_received)
fn write_buy_entry(file: &mut File, record: &TaxRecord, holder: &str) -> Result<()> {
    let total_cost = record.usd_value_received + record.gas_fee_usd;

    let fields = vec![
        record.timestamp.to_rfc3339(),
        record.asset_received.clone(),
        record.dex_sell.clone(), // Exchange where we bought (received) the token
        holder.to_string(),
        "BUY".to_string(),
        record.spot_price_received.to_string(),
        record.amount_received.to_string(),    // crypto_in
        "".to_string(),                        // crypto_out_no_fee (empty for BUY)
        "".to_string(),                        // crypto_fee
        record.usd_value_received.to_string(), // fiat_in_no_fee
        total_cost.to_string(),                // fiat_in_with_fee (includes gas)
        "".to_string(),                        // fiat_out_no_fee
        record.gas_fee_usd.to_string(),        // fiat_fee
        escape_csv(&format!(
            "Arbitrage BUY: {} {} @ ${} on {}",
            record.amount_received, record.asset_received, record.spot_price_received, record.dex_sell
        )),
    ];

    writeln!(file, "{}", fields.join(","))?;
    Ok(())
}

/// Write a FEE entry (gas-only transaction)
fn write_fee_entry(file: &mut File, record: &TaxRecord, holder: &str) -> Result<()> {
    let fields = vec![
        record.timestamp.to_rfc3339(),
        "MATIC".to_string(), // Gas is paid in MATIC on Polygon
        record.blockchain.clone(),
        holder.to_string(),
        "FEE".to_string(),
        "".to_string(),                   // spot_price
        "".to_string(),                   // crypto_in
        record.gas_fee_native.to_string(), // crypto_out_no_fee (gas spent)
        "".to_string(),                   // crypto_fee
        "".to_string(),                   // fiat_in_no_fee
        "".to_string(),                   // fiat_in_with_fee
        "".to_string(),                   // fiat_out_no_fee
        record.gas_fee_usd.to_string(),   // fiat_fee
        escape_csv(&format!(
            "Gas fee: {} MATIC (${}) - tx: {}",
            record.gas_fee_native, record.gas_fee_usd, record.transaction_hash
        )),
    ];

    writeln!(file, "{}", fields.join(","))?;
    Ok(())
}

/// Escape a CSV field that may contain special characters
fn escape_csv(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Export records for a specific tax year
pub fn export_year_to_rp2<P: AsRef<Path>>(
    records: &[TaxRecord],
    output_path: P,
    holder_name: &str,
    tax_year: i16,
) -> Result<usize> {
    let filtered: Vec<_> = records
        .iter()
        .filter(|r| r.tax_year == tax_year)
        .cloned()
        .collect();

    export_to_rp2(&filtered, output_path, holder_name)
}

/// Generate RP2 config.ini file
///
/// RP2 requires a config file specifying native currency and accounting method
pub fn generate_rp2_config<P: AsRef<Path>>(
    output_path: P,
    accounting_method: &str, // "fifo", "lifo", "hifo"
) -> Result<()> {
    let config = format!(
        r#"[rp2]
# RP2 Configuration for DEX Arbitrage Bot
# Generated: {}

# Native fiat currency
native_fiat = USD

# Accounting method for cost basis
# Options: fifo, lifo, hifo
accounting_method = {}

# Country-specific settings
country = us

# Input format
in_header = True

# Allow same-day sales (required for arbitrage)
allow_same_day_trades = True
"#,
        chrono::Utc::now().to_rfc3339(),
        accounting_method
    );

    let mut file = File::create(output_path.as_ref())?;
    write!(file, "{}", config)?;

    Ok(())
}

/// Validate RP2 export file format
pub fn validate_rp2_export<P: AsRef<Path>>(path: P) -> Result<ValidationResult> {
    let content = std::fs::read_to_string(path.as_ref())?;
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return Ok(ValidationResult {
            valid: false,
            row_count: 0,
            errors: vec!["File is empty".to_string()],
        });
    }

    let mut errors = Vec::new();

    // Check headers
    let headers: Vec<&str> = lines[0].split(',').collect();
    if headers.len() != RP2_HEADERS.len() {
        errors.push(format!(
            "Header count mismatch: expected {}, got {}",
            RP2_HEADERS.len(),
            headers.len()
        ));
    }

    for (i, (expected, actual)) in RP2_HEADERS.iter().zip(headers.iter()).enumerate() {
        if expected != actual {
            errors.push(format!(
                "Header mismatch at column {}: expected '{}', got '{}'",
                i, expected, actual
            ));
        }
    }

    // Check data rows
    let data_rows = lines.len() - 1;
    for (i, line) in lines.iter().skip(1).enumerate() {
        let fields: Vec<&str> = line.split(',').collect();

        // Basic field count check (may vary due to escaped commas in notes)
        if fields.len() < 10 {
            errors.push(format!(
                "Row {} has too few fields: {} (expected at least 10)",
                i + 1,
                fields.len()
            ));
        }

        // Check transaction_type is valid
        if fields.len() > 4 {
            let tx_type = fields[4];
            if !["BUY", "SELL", "FEE"].contains(&tx_type) {
                errors.push(format!(
                    "Row {} has invalid transaction_type: '{}'",
                    i + 1,
                    tx_type
                ));
            }
        }
    }

    Ok(ValidationResult {
        valid: errors.is_empty(),
        row_count: data_rows,
        errors,
    })
}

/// Result of RP2 export validation
#[derive(Debug)]
pub struct ValidationResult {
    pub valid: bool,
    pub row_count: usize,
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::env;
    use std::fs;

    fn create_test_record(is_paper: bool) -> TaxRecord {
        TaxRecord::new_arbitrage(
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
            is_paper,
        )
    }

    #[test]
    fn test_rp2_export() {
        let temp_dir = env::temp_dir().join("dexarb_rp2_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let records = vec![
            create_test_record(false), // Real trade
            create_test_record(true),  // Paper trade (should be skipped)
        ];

        let output_path = temp_dir.join("rp2_export.csv");
        let result = export_to_rp2(&records, &output_path, "TestWallet");

        assert!(result.is_ok());
        let row_count = result.unwrap();
        assert_eq!(row_count, 2); // 1 real trade = 2 rows (BUY + SELL)

        // Validate the export
        let validation = validate_rp2_export(&output_path).unwrap();
        assert!(validation.valid, "Errors: {:?}", validation.errors);
        assert_eq!(validation.row_count, 2);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_rp2_config_generation() {
        let temp_dir = env::temp_dir().join("dexarb_rp2_config_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let config_path = temp_dir.join("config.ini");
        let result = generate_rp2_config(&config_path, "fifo");
        assert!(result.is_ok());

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("accounting_method = fifo"));
        assert!(content.contains("native_fiat = USD"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_csv_escape() {
        assert_eq!(escape_csv("simple"), "simple");
        assert_eq!(escape_csv("has,comma"), "\"has,comma\"");
        assert_eq!(escape_csv("has\"quote"), "\"has\"\"quote\"");
    }
}
