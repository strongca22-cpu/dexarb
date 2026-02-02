//! DEX Arbitrage Bot Library
//!
//! Provides components for DEX arbitrage detection and execution.
//! Includes both live trading and paper trading capabilities.
//!
//! Author: AI-Generated
//! Created: 2026-01-28

pub mod arbitrage;
pub mod config;
pub mod contracts;
pub mod data_collector;
pub mod filters;
pub mod mempool;
pub mod paper_trading;
pub mod pool;
pub mod price_logger;
pub mod tax;
pub mod types;

// Re-export commonly used types
pub use config::load_config;
pub use pool::PoolStateManager;
pub use tax::{
    export_to_rp2, generate_rp2_config, validate_rp2_export, PriceOracle, TaxCsvLogger,
    TaxJsonLogger, TaxLogger, TaxRecord, TaxRecordBuilder, TaxSummary,
};
pub use types::{ArbitrageOpportunity, BotConfig, DexType, PoolState};
