//! Tax Export CLI
//!
//! Command-line tool for exporting tax records to RP2 format.
//!
//! Usage:
//!   cargo run --bin tax-export -- --year 2026 --output rp2_2026.csv
//!   cargo run --bin tax-export -- --year 2026 --summary
//!   cargo run --bin tax-export -- --validate rp2_2026.csv
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use anyhow::{Context, Result};
use chrono::Datelike;
use dexarb_bot::tax::{
    export_to_rp2, generate_rp2_config, validate_rp2_export, TaxJsonLogger, TaxSummary,
};
use std::env;
use std::path::PathBuf;
use tracing::{error, info};

/// Default paths
const DEFAULT_TAX_DIR: &str = "/home/botuser/bots/dexarb/data/tax";
const DEFAULT_HOLDER: &str = "DexArbBot";

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "--help" | "-h" => {
            print_usage();
        }
        "--year" | "-y" => {
            let year = parse_year(&args)?;
            let output = parse_output(&args)?;
            export_year(year, &output)?;
        }
        "--summary" | "-s" => {
            let year = parse_year(&args)?;
            print_summary(year)?;
        }
        "--validate" | "-v" => {
            let file = args.get(2).context("Missing file path for validation")?;
            validate_file(file)?;
        }
        "--config" | "-c" => {
            let method = args.get(2).unwrap_or(&"fifo".to_string()).clone();
            generate_config(&method)?;
        }
        "--list" | "-l" => {
            list_available_years()?;
        }
        _ => {
            error!("Unknown command: {}", args[1]);
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!(
        r#"
Tax Export CLI - DEX Arbitrage Bot

USAGE:
    tax-export [COMMAND] [OPTIONS]

COMMANDS:
    --year, -y <YEAR>      Export records for a specific tax year
                           Options: --output <PATH>  Output file path
                                    --holder <NAME>  Wallet holder name

    --summary, -s          Print tax summary for a year
                           Options: --year <YEAR>  Tax year (default: current)

    --validate, -v <FILE>  Validate an RP2 export file

    --config, -c [METHOD]  Generate RP2 config.ini file
                           Methods: fifo (default), lifo, hifo

    --list, -l             List available tax years with records

    --help, -h             Show this help message

EXAMPLES:
    # Export 2026 tax year to RP2 format
    tax-export --year 2026 --output rp2_2026.csv

    # Print summary for 2026
    tax-export --summary --year 2026

    # Validate an export file
    tax-export --validate rp2_2026.csv

    # Generate RP2 config with FIFO accounting
    tax-export --config fifo

    # List years with tax records
    tax-export --list

NOTES:
    - Tax records are read from: {}
    - RP2 exports create 2 rows per trade (BUY + SELL)
    - Paper trades are automatically excluded from exports
"#,
        DEFAULT_TAX_DIR
    );
}

fn parse_year(args: &[String]) -> Result<i16> {
    for i in 0..args.len() {
        if args[i] == "--year" || args[i] == "-y" {
            if let Some(year_str) = args.get(i + 1) {
                return year_str
                    .parse()
                    .with_context(|| format!("Invalid year: {}", year_str));
            }
        }
    }
    // Default to current year
    Ok(chrono::Utc::now().year() as i16)
}

fn parse_output(args: &[String]) -> Result<PathBuf> {
    for i in 0..args.len() {
        if args[i] == "--output" || args[i] == "-o" {
            if let Some(path) = args.get(i + 1) {
                return Ok(PathBuf::from(path));
            }
        }
    }
    // Default output path
    let year = parse_year(args)?;
    Ok(PathBuf::from(format!("{}/rp2_export_{}.csv", DEFAULT_TAX_DIR, year)))
}

fn parse_holder(args: &[String]) -> String {
    for i in 0..args.len() {
        if args[i] == "--holder" {
            if let Some(holder) = args.get(i + 1) {
                return holder.clone();
            }
        }
    }
    DEFAULT_HOLDER.to_string()
}

fn export_year(year: i16, output: &PathBuf) -> Result<()> {
    info!("Exporting tax year {} to RP2 format", year);

    let tax_dir = PathBuf::from(DEFAULT_TAX_DIR);
    let logger = TaxJsonLogger::new(&tax_dir)?;

    let records = logger.read_all(year)?;

    if records.is_empty() {
        info!("No records found for tax year {}", year);
        return Ok(());
    }

    info!("Found {} tax records for year {}", records.len(), year);

    // Filter out paper trades
    let real_records: Vec<_> = records.iter().filter(|r| !r.is_paper_trade).cloned().collect();
    info!(
        "Filtered to {} real trades ({} paper trades excluded)",
        real_records.len(),
        records.len() - real_records.len()
    );

    let holder = parse_holder(&env::args().collect::<Vec<_>>());
    let row_count = export_to_rp2(&real_records, output, &holder)?;

    info!("Exported {} rows to {:?}", row_count, output);
    info!("Holder name: {}", holder);

    // Validate the export
    let validation = validate_rp2_export(output)?;
    if validation.valid {
        info!("Export validation: PASSED");
    } else {
        error!("Export validation: FAILED");
        for err in &validation.errors {
            error!("  - {}", err);
        }
    }

    // Print summary
    let mut summary = TaxSummary::new(year);
    for record in &real_records {
        summary.add_record(record);
    }

    println!("\n{}", summary.report());

    Ok(())
}

fn print_summary(year: i16) -> Result<()> {
    let tax_dir = PathBuf::from(DEFAULT_TAX_DIR);
    let logger = TaxJsonLogger::new(&tax_dir)?;

    let records = logger.read_all(year)?;

    if records.is_empty() {
        info!("No records found for tax year {}", year);
        return Ok(());
    }

    // Filter out paper trades for tax summary
    let real_records: Vec<_> = records.iter().filter(|r| !r.is_paper_trade).collect();

    let mut summary = TaxSummary::new(year);
    for record in &real_records {
        summary.add_record(record);
    }

    println!("{}", summary.report());

    // Also show paper trade stats
    let paper_count = records.len() - real_records.len();
    if paper_count > 0 {
        println!("Note: {} paper trades excluded from summary", paper_count);
    }

    Ok(())
}

fn validate_file(file: &str) -> Result<()> {
    info!("Validating RP2 export file: {}", file);

    let result = validate_rp2_export(file)?;

    println!("\nValidation Results:");
    println!("─────────────────────────────────────");
    println!("File: {}", file);
    println!("Status: {}", if result.valid { "VALID ✓" } else { "INVALID ✗" });
    println!("Row count: {}", result.row_count);

    if !result.errors.is_empty() {
        println!("\nErrors:");
        for err in &result.errors {
            println!("  ✗ {}", err);
        }
    }

    Ok(())
}

fn generate_config(method: &str) -> Result<()> {
    let output = PathBuf::from(DEFAULT_TAX_DIR).join("rp2_config.ini");

    info!("Generating RP2 config with {} accounting method", method);
    generate_rp2_config(&output, method)?;

    info!("Config file created: {:?}", output);
    println!("\nRP2 config generated at: {:?}", output);
    println!("Accounting method: {}", method.to_uppercase());

    Ok(())
}

fn list_available_years() -> Result<()> {
    let tax_dir = PathBuf::from(DEFAULT_TAX_DIR);

    println!("\nAvailable Tax Years:");
    println!("─────────────────────────────────────");

    if !tax_dir.exists() {
        println!("No tax directory found at {:?}", tax_dir);
        return Ok(());
    }

    let mut years_found = Vec::new();

    for entry in std::fs::read_dir(&tax_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with("trades_") && name.ends_with(".jsonl") {
            if let Some(year_str) = name.strip_prefix("trades_").and_then(|s| s.strip_suffix(".jsonl")) {
                if let Ok(year) = year_str.parse::<i16>() {
                    // Count records in this year
                    let logger = TaxJsonLogger::new(&tax_dir)?;
                    let records = logger.read_all(year)?;
                    let real_count = records.iter().filter(|r| !r.is_paper_trade).count();
                    let paper_count = records.len() - real_count;

                    years_found.push((year, real_count, paper_count));
                }
            }
        }
    }

    if years_found.is_empty() {
        println!("No tax records found");
    } else {
        years_found.sort_by_key(|(y, _, _)| *y);
        for (year, real, paper) in years_found {
            println!(
                "  {} - {} real trades, {} paper trades",
                year, real, paper
            );
        }
    }

    Ok(())
}
