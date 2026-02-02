//! Paper Trading Strategy
//!
//! Implements the Strategy trait for paper trading.
//! Processes pool state updates and generates simulated trade actions.
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use super::config::PaperTradingConfig;
use super::engine::Strategy;
use super::executor::SimulatedTradeAction;
use super::metrics::TraderMetrics;
use crate::pool::PoolStateManager;
use crate::types::ArbitrageOpportunity;
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Event representing a pool state update
#[derive(Debug, Clone)]
pub struct PoolUpdateEvent {
    /// Block number
    pub block_number: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Paper trading strategy for a single configuration
pub struct PaperTradingStrategy {
    /// Configuration for this strategy
    config: PaperTradingConfig,
    /// Shared pool state manager (read-only access)
    state_manager: PoolStateManager,
    /// Metrics tracker for this strategy
    metrics: Arc<RwLock<TraderMetrics>>,
    /// Iteration counter
    iteration: u64,
}

impl PaperTradingStrategy {
    pub fn new(
        config: PaperTradingConfig,
        state_manager: PoolStateManager,
        metrics: Arc<RwLock<TraderMetrics>>,
    ) -> Self {
        Self {
            config,
            state_manager,
            metrics,
            iteration: 0,
        }
    }

    /// Check if we should stop trading due to limits
    async fn should_stop_trading(&self) -> bool {
        let metrics = self.metrics.read().await;

        // Daily trade limit
        if let Some(max_trades) = self.config.max_daily_trades {
            if metrics.daily_trades() >= max_trades {
                return true;
            }
        }

        // Daily loss limit
        if let Some(max_loss) = self.config.daily_loss_limit_usd {
            if metrics.daily_loss() >= max_loss {
                return true;
            }
        }

        // Consecutive losses
        if let Some(max_losses) = self.config.max_consecutive_losses {
            if metrics.consecutive_losses() >= max_losses {
                return true;
            }
        }

        false
    }

    /// Scan for arbitrage opportunities across configured pairs
    fn scan_opportunities(&self) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();

        for pair_symbol in &self.config.pairs {
            // Get pools for this pair across all DEXs
            let pools = self.state_manager.get_pools_for_pair(pair_symbol);

            if pools.len() < 2 {
                continue;
            }

            // Compare each pair of pools
            for i in 0..pools.len() {
                for j in (i + 1)..pools.len() {
                    let pool_a = &pools[i];
                    let pool_b = &pools[j];

                    let price_a = pool_a.price();
                    let price_b = pool_b.price();

                    if price_a == 0.0 || price_b == 0.0 {
                        continue;
                    }

                    // Check if A -> B arbitrage exists
                    if price_b > price_a {
                        let spread = (price_b - price_a) / price_a;
                        if spread > self.config.max_slippage_percent / 100.0 {
                            let estimated_profit = self.estimate_profit(spread);

                            if estimated_profit >= self.config.min_profit_usd {
                                opportunities.push(ArbitrageOpportunity::new(
                                    pool_a.pair.clone(),
                                    pool_a.dex,
                                    pool_b.dex,
                                    price_a,
                                    price_b,
                                    alloy::primitives::U256::from(
                                        (self.config.max_trade_size_usd * 1e18) as u128,
                                    ),
                                ));
                            }
                        }
                    }

                    // Check if B -> A arbitrage exists
                    if price_a > price_b {
                        let spread = (price_a - price_b) / price_b;
                        if spread > self.config.max_slippage_percent / 100.0 {
                            let estimated_profit = self.estimate_profit(spread);

                            if estimated_profit >= self.config.min_profit_usd {
                                opportunities.push(ArbitrageOpportunity::new(
                                    pool_b.pair.clone(),
                                    pool_b.dex,
                                    pool_a.dex,
                                    price_b,
                                    price_a,
                                    alloy::primitives::U256::from(
                                        (self.config.max_trade_size_usd * 1e18) as u128,
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Sort by estimated profit descending
        opportunities.sort_by(|a, b| {
            b.estimated_profit
                .partial_cmp(&a.estimated_profit)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        opportunities
    }

    /// Estimate profit based on spread and trade size
    fn estimate_profit(&self, spread: f64) -> f64 {
        // Simple estimation: spread * trade_size - estimated_costs
        let gross = spread * self.config.max_trade_size_usd;
        let estimated_gas = 0.50; // Polygon gas estimate
        let estimated_slippage = gross * 0.15; // 15% slippage estimate

        (gross - estimated_gas - estimated_slippage).max(0.0)
    }

    /// Simulate competition - returns true if opportunity is lost to others
    fn lost_to_competition(&self) -> bool {
        if !self.config.simulate_competition {
            return false;
        }

        // Use timestamp-based pseudo-randomness
        let seed = Utc::now().timestamp_nanos_opt().unwrap_or(0) as f64;
        let roll = (seed % 1000.0) / 1000.0;

        roll < self.config.competition_rate
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> TraderMetrics {
        self.metrics.read().await.clone()
    }
}

#[async_trait]
impl Strategy<PoolUpdateEvent, SimulatedTradeAction> for PaperTradingStrategy {
    async fn sync_state(&mut self) -> Result<()> {
        // Reset daily counters if needed
        let mut metrics = self.metrics.write().await;
        metrics.check_daily_reset();

        info!(
            "[{}] Strategy initialized with {} pairs",
            self.config.name,
            self.config.pairs.len()
        );

        Ok(())
    }

    async fn process_event(&mut self, _event: PoolUpdateEvent) -> Vec<SimulatedTradeAction> {
        self.iteration += 1;

        // Check if we should stop trading
        if self.should_stop_trading().await {
            if self.iteration % 100 == 0 {
                debug!(
                    "[{}] Trading paused due to limits",
                    self.config.name
                );
            }
            return vec![];
        }

        // Scan for opportunities
        let opportunities = self.scan_opportunities();

        if opportunities.is_empty() {
            if self.iteration % 1000 == 0 {
                debug!(
                    "[{}] No opportunities (iteration {})",
                    self.config.name, self.iteration
                );
            }
            return vec![];
        }

        // Record detected opportunities
        {
            let mut metrics = self.metrics.write().await;
            for _ in &opportunities {
                metrics.record_detected_opportunity();
            }
        }

        // Take the best opportunity
        let mut actions = Vec::new();

        if let Some(best) = opportunities.first() {
            let lost = self.lost_to_competition();

            let action = SimulatedTradeAction {
                pair: best.pair.symbol.clone(),
                config_name: self.config.name.clone(),
                estimated_profit: self.estimate_profit(best.spread_percent / 100.0),
                trade_size: self.config.max_trade_size_usd,
                buy_dex: best.buy_dex.to_string(),
                sell_dex: best.sell_dex.to_string(),
                lost_to_competition: lost,
            };

            if !lost {
                info!(
                    "[{}] Opportunity: {} | Spread: {:.2}% | Est. Profit: ${:.2}",
                    self.config.name,
                    best.pair.symbol,
                    best.spread_percent,
                    action.estimated_profit
                );
            }

            actions.push(action);
        }

        actions
    }

    fn name(&self) -> &str {
        &self.config.name
    }
}

/// Factory for creating strategies from configurations
pub struct StrategyFactory;

impl StrategyFactory {
    /// Create all strategies from preset configurations
    pub fn create_all_strategies(
        state_manager: PoolStateManager,
    ) -> Vec<(PaperTradingStrategy, Arc<RwLock<TraderMetrics>>)> {
        PaperTradingConfig::all_presets()
            .into_iter()
            .filter(|config| config.enabled)
            .map(|config| {
                let metrics = Arc::new(RwLock::new(TraderMetrics::new(config.name.clone())));
                let strategy = PaperTradingStrategy::new(
                    config,
                    state_manager.clone(),
                    Arc::clone(&metrics),
                );
                (strategy, metrics)
            })
            .collect()
    }

    /// Create a single strategy from configuration
    pub fn create_strategy(
        config: PaperTradingConfig,
        state_manager: PoolStateManager,
    ) -> (PaperTradingStrategy, Arc<RwLock<TraderMetrics>>) {
        let metrics = Arc::new(RwLock::new(TraderMetrics::new(config.name.clone())));
        let strategy = PaperTradingStrategy::new(
            config,
            state_manager,
            Arc::clone(&metrics),
        );
        (strategy, metrics)
    }
}
