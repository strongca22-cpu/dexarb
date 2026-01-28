//! Simulated Executor for Paper Trading
//!
//! Simulates trade execution with realistic conditions:
//! - Slippage modeling
//! - Gas cost variance
//! - Competition simulation
//! - Execution delays
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use super::config::PaperTradingConfig;
use super::engine::Executor;
use super::metrics::{SimulatedTradeResult, TraderMetrics};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::info;

/// Action to execute a simulated trade
#[derive(Debug, Clone)]
pub struct SimulatedTradeAction {
    /// Trading pair symbol
    pub pair: String,
    /// Configuration name that generated this action
    pub config_name: String,
    /// Estimated profit before costs
    pub estimated_profit: f64,
    /// Trade size in USD
    pub trade_size: f64,
    /// Buy DEX name
    pub buy_dex: String,
    /// Sell DEX name
    pub sell_dex: String,
    /// Whether this was lost to competition (pre-determined by strategy)
    pub lost_to_competition: bool,
}

/// Simulated executor that models realistic trade outcomes
pub struct SimulatedExecutor {
    config: PaperTradingConfig,
    metrics: Arc<RwLock<TraderMetrics>>,
}

impl SimulatedExecutor {
    pub fn new(config: PaperTradingConfig, metrics: Arc<RwLock<TraderMetrics>>) -> Self {
        Self { config, metrics }
    }

    /// Simulate slippage loss as a percentage of expected profit
    fn simulate_slippage_loss(&self, estimated_profit: f64) -> f64 {
        if !self.config.simulate_slippage {
            return 0.0;
        }

        // Slippage typically eats 10-30% of expected profit
        // Use a simple deterministic model based on trade characteristics
        let base_slippage = 0.15; // 15% base slippage
        let variance = 0.10; // +/- 10% variance

        // Use timestamp-based pseudo-randomness for reproducibility
        let seed = Utc::now().timestamp_nanos_opt().unwrap_or(0) as f64;
        let random_factor = ((seed % 1000.0) / 1000.0) * variance * 2.0 - variance;

        let slippage_rate = base_slippage + random_factor;
        estimated_profit * slippage_rate.max(0.05).min(0.40)
    }

    /// Simulate gas cost with variance
    fn simulate_gas_cost(&self) -> f64 {
        if !self.config.simulate_gas_variance {
            return 0.50; // Fixed estimate for Polygon
        }

        // Gas cost on Polygon: $0.30 - $1.00 depending on network congestion
        let base_gas = 0.50;
        let variance = 0.25;

        // Use timestamp-based pseudo-randomness
        let seed = Utc::now().timestamp_nanos_opt().unwrap_or(0) as f64;
        let random_factor = ((seed % 1000.0) / 1000.0) * variance * 2.0 - variance;

        (base_gas + random_factor).max(0.20).min(1.50)
    }

    /// Simulate execution delay (network latency, etc.)
    async fn simulate_execution_delay(&self) {
        // Simulate 10-50ms execution delay
        let seed = Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
        let delay_ms = 10 + (seed % 40);
        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
    }

    /// Execute a simulated trade
    pub async fn simulate_trade(&self, action: &SimulatedTradeAction) -> SimulatedTradeResult {
        let start = Instant::now();

        // If lost to competition, record as missed opportunity
        if action.lost_to_competition {
            let mut metrics = self.metrics.write().await;
            metrics.record_missed_opportunity(action.estimated_profit);

            return SimulatedTradeResult {
                pair: action.pair.clone(),
                success: false,
                profit_usd: 0.0,
                gas_cost_usd: 0.0,
                net_profit_usd: 0.0,
                execution_time_ms: start.elapsed().as_millis() as u64,
                error: Some("Lost to competition".to_string()),
                timestamp: Utc::now(),
            };
        }

        // Calculate profit after slippage
        let slippage_loss = self.simulate_slippage_loss(action.estimated_profit);
        let profit_after_slippage = action.estimated_profit - slippage_loss;

        // Calculate gas cost
        let gas_cost = self.simulate_gas_cost();

        // Simulate execution delay
        self.simulate_execution_delay().await;

        // Calculate net profit
        let net_profit = profit_after_slippage - gas_cost;
        let success = net_profit > 0.0;

        let result = SimulatedTradeResult {
            pair: action.pair.clone(),
            success,
            profit_usd: profit_after_slippage,
            gas_cost_usd: gas_cost,
            net_profit_usd: net_profit,
            execution_time_ms: start.elapsed().as_millis() as u64,
            error: if success {
                None
            } else {
                Some("Unprofitable after costs".to_string())
            },
            timestamp: Utc::now(),
        };

        // Record in metrics
        let mut metrics = self.metrics.write().await;
        metrics.record_trade(result.clone());

        result
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> TraderMetrics {
        self.metrics.read().await.clone()
    }
}

#[async_trait]
impl Executor<SimulatedTradeAction> for SimulatedExecutor {
    async fn execute(&self, action: SimulatedTradeAction) -> Result<()> {
        let result = self.simulate_trade(&action).await;

        if result.success {
            info!(
                "[{}] Trade executed: {} | Net: ${:.2} | Time: {}ms",
                action.config_name, action.pair, result.net_profit_usd, result.execution_time_ms
            );
        } else {
            info!(
                "[{}] Trade failed: {} | Reason: {}",
                action.config_name,
                action.pair,
                result.error.unwrap_or_else(|| "Unknown".to_string())
            );
        }

        Ok(())
    }
}

/// Multi-executor that routes actions to the correct executor by config name
pub struct MultiExecutor {
    executors: Vec<(String, Arc<SimulatedExecutor>)>,
}

impl MultiExecutor {
    pub fn new() -> Self {
        Self {
            executors: Vec::new(),
        }
    }

    pub fn add_executor(&mut self, config_name: String, executor: Arc<SimulatedExecutor>) {
        self.executors.push((config_name, executor));
    }

    pub fn get_executor(&self, config_name: &str) -> Option<Arc<SimulatedExecutor>> {
        self.executors
            .iter()
            .find(|(name, _)| name == config_name)
            .map(|(_, exec)| Arc::clone(exec))
    }
}

impl Default for MultiExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Executor<SimulatedTradeAction> for MultiExecutor {
    async fn execute(&self, action: SimulatedTradeAction) -> Result<()> {
        if let Some(executor) = self.get_executor(&action.config_name) {
            executor.execute(action).await
        } else {
            tracing::warn!("No executor found for config: {}", action.config_name);
            Ok(())
        }
    }
}
