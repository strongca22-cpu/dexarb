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
//!     types.rs   — PendingSwap, DecodedSwap, MempoolMode, ConfirmationTracker
//!     decoder.rs — Calldata → DecodedSwap (V2/V3/Algebra router functions)
//!     monitor.rs — WS subscription loop, CSV logging, cross-reference tracking
//!
//! Usage:
//!     Spawned as an async task from main.rs when MEMPOOL_MONITOR=observe.
//!     Self-contained: creates its own WS connections, independent of the block loop.

pub mod decoder;
pub mod monitor;
pub mod types;

pub use monitor::run_observation;
pub use types::MempoolMode;
