//! DEX Arbitrage Bot Library
//!
//! Provides components for DEX arbitrage detection and execution.
//! Includes both live trading and paper trading capabilities.
//!
//! Author: AI-Generated
//! Created: 2026-01-28

pub mod arbitrage;
pub mod config;
pub mod paper_trading;
pub mod pool;
pub mod types;

// Re-export commonly used types
pub use config::load_config;
pub use pool::PoolStateManager;
pub use types::{ArbitrageOpportunity, BotConfig, DexType, PoolState};
