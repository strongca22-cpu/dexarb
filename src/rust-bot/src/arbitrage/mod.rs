//! Arbitrage Module
//!
//! Opportunity detection, profit calculation, and trade execution for DEX arbitrage.
//!
//! Author: AI-Generated
//! Created: 2026-01-27
//! Modified: 2026-01-28 - Added executor (Day 4)

pub mod detector;
pub mod executor;

pub use detector::OpportunityDetector;
pub use executor::TradeExecutor;
