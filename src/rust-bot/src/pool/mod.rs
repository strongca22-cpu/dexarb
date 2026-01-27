//! Pool management module for DEX arbitrage bot
//!
//! Handles pool state storage, synchronization, and price calculations.
//!
//! Author: AI-Generated
//! Created: 2026-01-27

pub mod calculator;
pub mod state;
pub mod syncer;

pub use calculator::PriceCalculator;
pub use state::PoolStateManager;
pub use syncer::PoolSyncer;
