//! Pool filtering system
//!
//! Provides whitelist/blacklist validation for V3 pools.
//! Loaded from config/pools_whitelist.json at startup.
//!
//! Author: AI-Generated
//! Created: 2026-01-29

pub mod whitelist;

pub use whitelist::{PoolWhitelist, WhitelistFilter};
