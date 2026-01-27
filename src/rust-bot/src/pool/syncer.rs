//! Pool Synchronization
//!
//! Fetches pool addresses and reserves from blockchain.
//! Supports both initial sync and continuous updates.
//!
//! Author: AI-Generated
//! Created: 2026-01-27

use crate::pool::PoolStateManager;
use crate::types::{BotConfig, DexType, PoolState, TradingPair};
use anyhow::{Context, Result};
use ethers::prelude::*;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

// Contract ABIs for Uniswap V2 style DEXs
abigen!(
    IUniswapV2Factory,
    r#"[
        function getPair(address tokenA, address tokenB) external view returns (address pair)
        function allPairs(uint256) external view returns (address pair)
        function allPairsLength() external view returns (uint256)
    ]"#
);

abigen!(
    IUniswapV2Pair,
    r#"[
        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast)
        function token0() external view returns (address)
        function token1() external view returns (address)
    ]"#
);

/// Syncs pool reserves from blockchain
pub struct PoolSyncer<P> {
    provider: Arc<P>,
    config: BotConfig,
    state_manager: PoolStateManager,
}

impl<P: Middleware + 'static> PoolSyncer<P> {
    /// Create a new PoolSyncer
    pub fn new(provider: Arc<P>, config: BotConfig, state_manager: PoolStateManager) -> Self {
        Self {
            provider,
            config,
            state_manager,
        }
    }

    /// Initial sync: fetch all configured pool addresses and reserves
    pub async fn initial_sync(&self) -> Result<()> {
        info!("Starting initial pool sync...");

        for pair_config in &self.config.pairs {
            let token0: Address = pair_config
                .token0
                .parse()
                .context("Invalid token0 address")?;
            let token1: Address = pair_config
                .token1
                .parse()
                .context("Invalid token1 address")?;

            let pair = TradingPair::new(token0, token1, pair_config.symbol.clone());

            // Sync Uniswap pool
            match self.sync_pool(DexType::Uniswap, &pair).await {
                Ok(pool) => {
                    self.state_manager.update_pool(pool);
                    info!(
                        "Synced Uniswap pool: {} (reserves: {} / {})",
                        pair.symbol,
                        self.state_manager
                            .get_pool(DexType::Uniswap, &pair.symbol)
                            .map(|p| format!("{}", p.reserve0))
                            .unwrap_or_default(),
                        self.state_manager
                            .get_pool(DexType::Uniswap, &pair.symbol)
                            .map(|p| format!("{}", p.reserve1))
                            .unwrap_or_default()
                    );
                }
                Err(e) => {
                    warn!("Failed to sync Uniswap pool for {}: {}", pair.symbol, e);
                }
            }

            // Sync Sushiswap pool
            match self.sync_pool(DexType::Sushiswap, &pair).await {
                Ok(pool) => {
                    self.state_manager.update_pool(pool);
                    info!(
                        "Synced Sushiswap pool: {} (reserves: {} / {})",
                        pair.symbol,
                        self.state_manager
                            .get_pool(DexType::Sushiswap, &pair.symbol)
                            .map(|p| format!("{}", p.reserve0))
                            .unwrap_or_default(),
                        self.state_manager
                            .get_pool(DexType::Sushiswap, &pair.symbol)
                            .map(|p| format!("{}", p.reserve1))
                            .unwrap_or_default()
                    );
                }
                Err(e) => {
                    warn!("Failed to sync Sushiswap pool for {}: {}", pair.symbol, e);
                }
            }
        }

        let (count, min_block, max_block) = self.state_manager.stats();
        info!(
            "Initial sync complete: {} pools synced (blocks {} - {})",
            count, min_block, max_block
        );

        Ok(())
    }

    /// Sync a specific pool's reserves
    async fn sync_pool(&self, dex: DexType, pair: &TradingPair) -> Result<PoolState> {
        // Get pool address from factory
        let pool_address = self.get_pool_address(dex, pair).await?;

        if pool_address == Address::zero() {
            anyhow::bail!("Pool not found for pair {} on {:?}", pair.symbol, dex);
        }

        // Fetch reserves using getReserves()
        let (reserve0, reserve1, _block_timestamp_last) =
            self.get_reserves(pool_address).await?;

        // Get current block number
        let current_block = self
            .provider
            .get_block_number()
            .await
            .context("Failed to get block number")?
            .as_u64();

        debug!(
            "Pool {} on {:?}: address={:?}, reserves=({}, {}), block={}",
            pair.symbol, dex, pool_address, reserve0, reserve1, current_block
        );

        Ok(PoolState {
            address: pool_address,
            dex,
            pair: pair.clone(),
            reserve0: U256::from(reserve0),
            reserve1: U256::from(reserve1),
            last_updated: current_block,
        })
    }

    /// Get pool address from factory contract
    async fn get_pool_address(&self, dex: DexType, pair: &TradingPair) -> Result<Address> {
        let factory_address = match dex {
            DexType::Uniswap => self.config.uniswap_factory,
            DexType::Sushiswap => self.config.sushiswap_factory,
            DexType::Quickswap => anyhow::bail!("Quickswap not implemented in Phase 1"),
        };

        let factory = IUniswapV2Factory::new(factory_address, Arc::clone(&self.provider));

        let pool_address = factory
            .get_pair(pair.token0, pair.token1)
            .call()
            .await
            .context(format!(
                "Failed to get pair address from {:?} factory",
                dex
            ))?;

        Ok(pool_address)
    }

    /// Get reserves from pool contract
    async fn get_reserves(&self, pool_address: Address) -> Result<(u128, u128, u32)> {
        let pool = IUniswapV2Pair::new(pool_address, Arc::clone(&self.provider));

        let (reserve0, reserve1, block_timestamp_last) = pool
            .get_reserves()
            .call()
            .await
            .context("Failed to get reserves")?;

        Ok((reserve0, reserve1, block_timestamp_last))
    }

    /// Sync a single pool by its address (for event-driven updates)
    pub async fn sync_pool_by_address(
        &self,
        pool_address: Address,
        dex: DexType,
        pair: TradingPair,
    ) -> Result<PoolState> {
        let (reserve0, reserve1, _) = self.get_reserves(pool_address).await?;

        let current_block = self
            .provider
            .get_block_number()
            .await
            .context("Failed to get block number")?
            .as_u64();

        let pool_state = PoolState {
            address: pool_address,
            dex,
            pair,
            reserve0: U256::from(reserve0),
            reserve1: U256::from(reserve1),
            last_updated: current_block,
        };

        self.state_manager.update_pool(pool_state.clone());
        Ok(pool_state)
    }

    /// Get reference to state manager
    pub fn state_manager(&self) -> &PoolStateManager {
        &self.state_manager
    }
}

#[cfg(test)]
mod tests {
    // Integration tests would require a real provider
    // Unit tests for parsing/logic can be added here
}
