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

            // Sync ApeSwap pool (if configured)
            if self.config.apeswap_factory.is_some() {
                match self.sync_pool(DexType::Apeswap, &pair).await {
                    Ok(pool) => {
                        self.state_manager.update_pool(pool);
                        info!(
                            "Synced Apeswap pool: {} (reserves: {} / {})",
                            pair.symbol,
                            self.state_manager
                                .get_pool(DexType::Apeswap, &pair.symbol)
                                .map(|p| format!("{}", p.reserve0))
                                .unwrap_or_default(),
                            self.state_manager
                                .get_pool(DexType::Apeswap, &pair.symbol)
                                .map(|p| format!("{}", p.reserve1))
                                .unwrap_or_default()
                        );
                    }
                    Err(e) => {
                        warn!("Failed to sync Apeswap pool for {}: {}", pair.symbol, e);
                    }
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
    ///
    /// IMPORTANT: V2 pools sort tokens by address (token0 < token1).
    /// We must get the actual token0/token1 from the pool contract, not from config.
    /// This matches the fix applied to V3 syncer.
    async fn sync_pool(&self, dex: DexType, pair: &TradingPair) -> Result<PoolState> {
        // Get pool address from factory
        let pool_address = self.get_pool_address(dex, pair).await?;

        if pool_address == Address::zero() {
            anyhow::bail!("Pool not found for pair {} on {:?}", pair.symbol, dex);
        }

        let pool = IUniswapV2Pair::new(pool_address, Arc::clone(&self.provider));

        // Fetch reserves using getReserves()
        let (reserve0, reserve1, _block_timestamp_last) = pool
            .get_reserves()
            .call()
            .await
            .context("Failed to get reserves")?;

        // CRITICAL: Get the ACTUAL token0/token1 from the pool contract
        // V2 pools sort tokens by address, which may differ from config order
        let actual_token0 = pool.token_0().call().await.context("Failed to get token0")?;
        let actual_token1 = pool.token_1().call().await.context("Failed to get token1")?;

        // Get current block number
        let current_block = self
            .provider
            .get_block_number()
            .await
            .context("Failed to get block number")?
            .as_u64();

        // Create pair with actual pool ordering (preserves symbol from config)
        let actual_pair = TradingPair {
            token0: actual_token0,
            token1: actual_token1,
            symbol: pair.symbol.clone(),
        };

        debug!(
            "Pool {} on {:?}: address={:?}, token0={:?}, token1={:?}, reserves=({}, {}), block={}",
            pair.symbol, dex, pool_address, actual_token0, actual_token1, reserve0, reserve1, current_block
        );

        Ok(PoolState {
            address: pool_address,
            dex,
            pair: actual_pair,
            reserve0: U256::from(reserve0),
            reserve1: U256::from(reserve1),
            last_updated: current_block,
        })
    }

    /// Get pool address from factory contract (V2 pools only)
    async fn get_pool_address(&self, dex: DexType, pair: &TradingPair) -> Result<Address> {
        let factory_address = match dex {
            DexType::Uniswap => self.config.uniswap_factory,
            DexType::Sushiswap => self.config.sushiswap_factory,
            DexType::Quickswap => self.config.uniswap_factory, // Quickswap == Uniswap slot on Polygon
            DexType::Apeswap => {
                self.config.apeswap_factory
                    .ok_or_else(|| anyhow::anyhow!("ApeSwap factory not configured"))?
            }
            // V3 types are handled by V3PoolSyncer, not this V2 syncer
            DexType::UniswapV3_005 | DexType::UniswapV3_030 | DexType::UniswapV3_100 => {
                anyhow::bail!("V3 pools should be synced using V3PoolSyncer")
            }
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
        let pool = IUniswapV2Pair::new(pool_address, Arc::clone(&self.provider));

        let (reserve0, reserve1, _) = pool
            .get_reserves()
            .call()
            .await
            .context("Failed to get reserves")?;

        // Get actual token ordering from pool contract
        let actual_token0 = pool.token_0().call().await.context("Failed to get token0")?;
        let actual_token1 = pool.token_1().call().await.context("Failed to get token1")?;

        let current_block = self
            .provider
            .get_block_number()
            .await
            .context("Failed to get block number")?
            .as_u64();

        let actual_pair = TradingPair {
            token0: actual_token0,
            token1: actual_token1,
            symbol: pair.symbol.clone(),
        };

        let pool_state = PoolState {
            address: pool_address,
            dex,
            pair: actual_pair,
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
