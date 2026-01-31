//! Pool management module for DEX arbitrage bot
//!
//! Handles pool state storage, synchronization, and price calculations.
//! Supports both V2 (constant product) and V3 (concentrated liquidity) pools.
//!
//! Author: AI-Generated
//! Created: 2026-01-27
//! Modified: 2026-01-28 (added V3 support)

pub mod calculator;
pub mod state;
pub mod syncer;
pub mod v2_syncer;
pub mod v3_syncer;

pub use calculator::PriceCalculator;
pub use state::PoolStateManager;
pub use syncer::PoolSyncer;
pub use v2_syncer::V2PoolSyncer;
pub use v3_syncer::{V3PoolSyncer, SUSHI_V3_FEE_TIERS, V3_FEE_TIERS};
