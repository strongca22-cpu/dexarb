//! Pool State Collector
//!
//! Produces a stream of pool state update events.
//! Wraps the existing PoolStateManager and PoolSyncer.
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use super::engine::{Collector, CollectorStream};
use super::strategy::PoolUpdateEvent;
use crate::pool::{PoolStateManager, PoolSyncer};
use crate::types::BotConfig;
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;

/// Collector that syncs pool state and produces update events
pub struct PoolStateCollector<M> {
    /// Ethers provider
    provider: Arc<M>,
    /// Bot configuration
    config: BotConfig,
    /// Pool state manager (shared with strategies)
    state_manager: PoolStateManager,
    /// Polling interval
    poll_interval: Duration,
}

impl<M> PoolStateCollector<M>
where
    M: Middleware + 'static,
{
    pub fn new(
        provider: Arc<M>,
        config: BotConfig,
        state_manager: PoolStateManager,
    ) -> Self {
        let poll_interval = Duration::from_millis(config.poll_interval_ms);
        Self {
            provider,
            config,
            state_manager,
            poll_interval,
        }
    }

    /// Get the state manager for sharing with strategies
    pub fn state_manager(&self) -> PoolStateManager {
        self.state_manager.clone()
    }
}

#[async_trait]
impl<M> Collector<PoolUpdateEvent> for PoolStateCollector<M>
where
    M: Middleware + 'static,
    M::Error: 'static,
{
    async fn get_event_stream(&self) -> Result<CollectorStream<'_, PoolUpdateEvent>> {
        // Create a syncer for this collector
        let syncer = PoolSyncer::new(
            Arc::clone(&self.provider),
            self.config.clone(),
            self.state_manager.clone(),
        );

        // Perform initial sync
        syncer.initial_sync().await?;

        // Create an interval stream
        let interval = tokio::time::interval(self.poll_interval);
        let stream = IntervalStream::new(interval);

        // Clone what we need for the async block
        let state_manager = self.state_manager.clone();
        let provider = Arc::clone(&self.provider);
        let config = self.config.clone();

        // Map interval ticks to pool update events
        let event_stream = stream.then(move |_| {
            let syncer = PoolSyncer::new(
                Arc::clone(&provider),
                config.clone(),
                state_manager.clone(),
            );
            let provider = Arc::clone(&provider);

            async move {
                // Re-sync pools
                if let Err(e) = syncer.initial_sync().await {
                    tracing::warn!("Pool sync error: {}", e);
                }

                // Get current block number
                let block_number = provider
                    .get_block_number()
                    .await
                    .map(|b| b.as_u64())
                    .unwrap_or(0);

                PoolUpdateEvent {
                    block_number,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                }
            }
        });

        Ok(Box::pin(event_stream))
    }
}

/// Simple block-based collector that doesn't sync pools
/// (for testing or when pools are synced elsewhere)
pub struct SimpleBlockCollector<M> {
    provider: Arc<M>,
    poll_interval: Duration,
}

impl<M> SimpleBlockCollector<M>
where
    M: Middleware + 'static,
{
    pub fn new(provider: Arc<M>, poll_interval_ms: u64) -> Self {
        Self {
            provider,
            poll_interval: Duration::from_millis(poll_interval_ms),
        }
    }
}

#[async_trait]
impl<M> Collector<PoolUpdateEvent> for SimpleBlockCollector<M>
where
    M: Middleware + 'static,
    M::Error: 'static,
{
    async fn get_event_stream(&self) -> Result<CollectorStream<'_, PoolUpdateEvent>> {
        let interval = tokio::time::interval(self.poll_interval);
        let stream = IntervalStream::new(interval);

        let provider = Arc::clone(&self.provider);

        let event_stream = stream.then(move |_| {
            let provider = Arc::clone(&provider);
            async move {
                let block_number = provider
                    .get_block_number()
                    .await
                    .map(|b| b.as_u64())
                    .unwrap_or(0);

                PoolUpdateEvent {
                    block_number,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                }
            }
        });

        Ok(Box::pin(event_stream))
    }
}
