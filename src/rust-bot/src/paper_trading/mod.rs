//! Multi-Configuration Paper Trading Module
//!
//! This module implements a paper trading system that runs multiple
//! trading configurations in parallel against live market data.
//!
//! Architecture (based on Artemis pattern):
//! - Collector: Syncs pool state and produces update events
//! - Strategy: Processes events using specific configuration parameters
//! - Executor: Simulates trade execution and records metrics
//! - Engine: Orchestrates data flow between components
//!
//! Usage:
//! ```ignore
//! use paper_trading::{run_paper_trading, PaperTradingConfig};
//!
//! // Run with all 12 preset configurations
//! run_paper_trading(provider, bot_config).await?;
//! ```
//!
//! Author: AI-Generated
//! Created: 2026-01-28

pub mod collector;
pub mod config;
pub mod discord_alerts;
pub mod engine;
pub mod executor;
pub mod metrics;
pub mod strategy;
pub mod toml_config;

// Re-exports for convenience
pub use collector::{PoolStateCollector, SimpleBlockCollector};
pub use config::PaperTradingConfig;
pub use discord_alerts::{
    AggregatedOpportunity, DiscordAlerter, StrategyMatch, DailySummary, StrategyStats,
    OpportunityBatcher, BatchedOpportunitySummary,
};
pub use engine::{Collector, Engine, Executor, Strategy};
pub use executor::{MultiExecutor, SimulatedExecutor, SimulatedTradeAction};
pub use metrics::{MetricsAggregator, SimulatedTradeResult, TraderMetrics};
pub use strategy::{PaperTradingStrategy, PoolUpdateEvent, StrategyFactory};
pub use toml_config::{TomlConfig, GeneralConfig, StrategyConfig};

use crate::pool::PoolStateManager;
use crate::types::BotConfig;
use anyhow::Result;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

/// Run the multi-configuration paper trading system
///
/// This sets up:
/// 1. A single PoolStateCollector (one data source)
/// 2. 12 PaperTradingStrategies (one per configuration)
/// 3. A MultiExecutor that routes actions to the correct metrics tracker
pub async fn run_paper_trading<M>(
    provider: Arc<M>,
    bot_config: BotConfig,
) -> Result<()>
where
    M: Middleware + 'static,
    M::Error: 'static,
{
    info!("Starting Multi-Configuration Paper Trading System");

    // Create shared pool state manager
    let state_manager = PoolStateManager::new();

    // Create the collector
    let collector = PoolStateCollector::new(
        Arc::clone(&provider),
        bot_config.clone(),
        state_manager.clone(),
    );

    // Create all strategies and their metrics
    let strategies_and_metrics = StrategyFactory::create_all_strategies(state_manager.clone());

    info!(
        "Created {} paper trading configurations",
        strategies_and_metrics.len()
    );

    // Create multi-executor
    let mut multi_executor = MultiExecutor::new();
    let mut all_metrics: Vec<Arc<RwLock<TraderMetrics>>> = Vec::new();

    for (strategy, metrics) in &strategies_and_metrics {
        let executor = Arc::new(SimulatedExecutor::new(
            PaperTradingConfig::all_presets()
                .into_iter()
                .find(|c| c.name == strategy.name())
                .unwrap_or_default(),
            Arc::clone(metrics),
        ));
        multi_executor.add_executor(strategy.name().to_string(), executor);
        all_metrics.push(Arc::clone(metrics));
    }

    // Build the engine
    let mut engine: Engine<PoolUpdateEvent, SimulatedTradeAction> = Engine::new()
        .with_event_channel_capacity(1024)
        .with_action_channel_capacity(1024);

    // Add collector
    engine.add_collector(Box::new(collector));

    // Add strategies
    for (strategy, _) in strategies_and_metrics {
        engine.add_strategy(Box::new(strategy));
    }

    // Add executor
    engine.add_executor(Box::new(multi_executor));

    // Spawn metrics reporting task
    let metrics_for_reporting = all_metrics.clone();
    tokio::spawn(async move {
        report_metrics_loop(metrics_for_reporting).await;
    });

    // Run the engine
    info!("Starting paper trading engine...");
    let mut tasks = engine.run().await?;

    // Wait for all tasks (runs indefinitely)
    while let Some(result) = tasks.join_next().await {
        if let Err(e) = result {
            tracing::error!("Task error: {}", e);
        }
    }

    Ok(())
}

/// Periodically report metrics for all configurations
async fn report_metrics_loop(metrics: Vec<Arc<RwLock<TraderMetrics>>>) {
    let report_interval = Duration::from_secs(300); // Every 5 minutes

    loop {
        tokio::time::sleep(report_interval).await;

        let mut aggregator = MetricsAggregator::new();

        for m in &metrics {
            let metrics_snapshot = m.read().await.clone();
            aggregator.add(metrics_snapshot);
        }

        let report = aggregator.generate_report();
        info!("\n{}", report);
    }
}

/// Run paper trading with custom configurations
pub async fn run_paper_trading_custom<M>(
    provider: Arc<M>,
    bot_config: BotConfig,
    configs: Vec<PaperTradingConfig>,
) -> Result<()>
where
    M: Middleware + 'static,
    M::Error: 'static,
{
    info!(
        "Starting Custom Paper Trading with {} configurations",
        configs.len()
    );

    let state_manager = PoolStateManager::new();

    let collector = PoolStateCollector::new(
        Arc::clone(&provider),
        bot_config.clone(),
        state_manager.clone(),
    );

    let mut engine: Engine<PoolUpdateEvent, SimulatedTradeAction> = Engine::new();
    let mut multi_executor = MultiExecutor::new();
    let mut all_metrics: Vec<Arc<RwLock<TraderMetrics>>> = Vec::new();

    for config in configs {
        if !config.enabled {
            continue;
        }

        let (strategy, metrics) =
            StrategyFactory::create_strategy(config.clone(), state_manager.clone());

        let executor = Arc::new(SimulatedExecutor::new(config, Arc::clone(&metrics)));

        multi_executor.add_executor(strategy.name().to_string(), executor);
        all_metrics.push(Arc::clone(&metrics));

        engine.add_strategy(Box::new(strategy));
    }

    engine.add_collector(Box::new(collector));
    engine.add_executor(Box::new(multi_executor));

    // Spawn metrics reporting
    let metrics_for_reporting = all_metrics.clone();
    tokio::spawn(async move {
        report_metrics_loop(metrics_for_reporting).await;
    });

    info!("Starting custom paper trading engine...");
    let mut tasks = engine.run().await?;

    while let Some(result) = tasks.join_next().await {
        if let Err(e) = result {
            tracing::error!("Task error: {}", e);
        }
    }

    Ok(())
}
