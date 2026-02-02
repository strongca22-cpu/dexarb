//! A4 Mempool Monitor Module
//!
//! Purpose:
//!     Observe pending DEX swap transactions in the mempool, decode calldata,
//!     and measure mempool visibility + lead time before block confirmation.
//!
//! Author: AI-Generated
//! Created: 2026-02-01
//! Modified: 2026-02-01
//!
//! Architecture:
//!     types.rs      — PendingSwap, DecodedSwap, MempoolMode, ConfirmationTracker, SimulationTracker
//!     decoder.rs    — Calldata → DecodedSwap (V2/V3/Algebra router functions)
//!     monitor.rs    — WS subscription loop, CSV logging, cross-reference tracking
//!     simulator.rs  — Phase 2: AMM state simulation (V2 constant product, V3 sqrtPrice)
//!
//! Usage:
//!     Spawned as an async task from main.rs when MEMPOOL_MONITOR=observe.
//!     Receives PoolStateManager (Arc-cloned) for Phase 2 simulation access.

pub mod decoder;
pub mod monitor;
pub mod simulator;
pub mod types;

pub use monitor::{run_observation, run_execution};
pub use types::{CachedOpportunity, HybridCache, MempoolMode, MempoolSignal};
