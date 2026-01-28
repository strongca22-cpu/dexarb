//! Paper Trading Engine
//!
//! Implements the Collector/Strategy/Executor pattern from Artemis.
//! Orchestrates data flow between pool state updates, trading strategies,
//! and simulated execution.
//!
//! Based on: https://github.com/paradigmxyz/artemis
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use anyhow::Result;
use async_trait::async_trait;
use std::pin::Pin;
use tokio::sync::broadcast::{self, Sender};
use tokio::task::JoinSet;
use tokio_stream::Stream;
use tokio_stream::StreamExt;
use tracing::{error, info};

/// A stream of events emitted by a Collector
pub type CollectorStream<'a, E> = Pin<Box<dyn Stream<Item = E> + Send + 'a>>;

/// Collector trait - produces a stream of events
///
/// Collectors take in external events (pool updates, new blocks, etc.)
/// and turn them into an internal event representation.
#[async_trait]
pub trait Collector<E>: Send + Sync {
    /// Returns the core event stream for the collector
    async fn get_event_stream(&self) -> Result<CollectorStream<'_, E>>;
}

/// Strategy trait - core trading logic
///
/// Strategies contain the logic for each trading opportunity.
/// They take in events as inputs and compute whether opportunities exist.
/// Strategies produce actions.
#[async_trait]
pub trait Strategy<E, A>: Send + Sync {
    /// Sync the initial state of the strategy if needed
    async fn sync_state(&mut self) -> Result<()>;

    /// Process an event and return actions if any opportunities found
    async fn process_event(&mut self, event: E) -> Vec<A>;

    /// Get the name of this strategy for logging
    fn name(&self) -> &str;
}

/// Executor trait - executes actions
///
/// Executors process actions and are responsible for executing them.
/// For paper trading, this means simulating execution and recording metrics.
#[async_trait]
pub trait Executor<A>: Send + Sync {
    /// Execute an action
    async fn execute(&self, action: A) -> Result<()>;
}

/// The main engine that orchestrates collectors, strategies, and executors
///
/// This implements the event processing pipeline:
/// Collectors -> Strategies -> Executors
pub struct Engine<E, A> {
    /// Collectors that produce events
    collectors: Vec<Box<dyn Collector<E>>>,
    /// Strategies that process events into actions
    strategies: Vec<Box<dyn Strategy<E, A>>>,
    /// Executors that handle actions
    executors: Vec<Box<dyn Executor<A>>>,
    /// Channel capacity for events
    event_channel_capacity: usize,
    /// Channel capacity for actions
    action_channel_capacity: usize,
}

impl<E, A> Engine<E, A> {
    pub fn new() -> Self {
        Self {
            collectors: vec![],
            strategies: vec![],
            executors: vec![],
            event_channel_capacity: 512,
            action_channel_capacity: 512,
        }
    }

    pub fn with_event_channel_capacity(mut self, capacity: usize) -> Self {
        self.event_channel_capacity = capacity;
        self
    }

    pub fn with_action_channel_capacity(mut self, capacity: usize) -> Self {
        self.action_channel_capacity = capacity;
        self
    }

    /// Add a collector to the engine
    pub fn add_collector(&mut self, collector: Box<dyn Collector<E>>) {
        self.collectors.push(collector);
    }

    /// Add a strategy to the engine
    pub fn add_strategy(&mut self, strategy: Box<dyn Strategy<E, A>>) {
        self.strategies.push(strategy);
    }

    /// Add an executor to the engine
    pub fn add_executor(&mut self, executor: Box<dyn Executor<A>>) {
        self.executors.push(executor);
    }
}

impl<E, A> Default for Engine<E, A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E, A> Engine<E, A>
where
    E: Send + Clone + 'static + std::fmt::Debug,
    A: Send + Clone + 'static + std::fmt::Debug,
{
    /// Run the engine - starts all collectors, strategies, and executors
    ///
    /// This spawns tasks for each component and orchestrates data flow:
    /// 1. Collectors produce events
    /// 2. Events are broadcast to all strategies
    /// 3. Strategies produce actions
    /// 4. Actions are broadcast to all executors
    pub async fn run(self) -> Result<JoinSet<()>> {
        let (event_sender, _): (Sender<E>, _) = broadcast::channel(self.event_channel_capacity);
        let (action_sender, _): (Sender<A>, _) = broadcast::channel(self.action_channel_capacity);

        let mut set = JoinSet::new();

        // Spawn executors
        for executor in self.executors {
            let mut receiver = action_sender.subscribe();
            set.spawn(async move {
                info!("Starting executor...");
                loop {
                    match receiver.recv().await {
                        Ok(action) => {
                            if let Err(e) = executor.execute(action).await {
                                error!("Error executing action: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Error receiving action: {}", e);
                        }
                    }
                }
            });
        }

        // Spawn strategies
        for mut strategy in self.strategies {
            let mut event_receiver = event_sender.subscribe();
            let action_sender = action_sender.clone();

            // Sync initial state
            if let Err(e) = strategy.sync_state().await {
                error!("Failed to sync strategy {}: {}", strategy.name(), e);
                continue;
            }

            let name = strategy.name().to_string();
            set.spawn(async move {
                info!("Starting strategy: {}", name);
                loop {
                    match event_receiver.recv().await {
                        Ok(event) => {
                            for action in strategy.process_event(event).await {
                                if let Err(e) = action_sender.send(action) {
                                    error!("Error sending action: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error receiving event in {}: {}", name, e);
                        }
                    }
                }
            });
        }

        // Spawn collectors
        for collector in self.collectors {
            let event_sender = event_sender.clone();
            set.spawn(async move {
                info!("Starting collector...");
                match collector.get_event_stream().await {
                    Ok(mut event_stream) => {
                        while let Some(event) = event_stream.next().await {
                            if let Err(e) = event_sender.send(event) {
                                error!("Error sending event: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to get event stream: {}", e);
                    }
                }
            });
        }

        Ok(set)
    }
}
