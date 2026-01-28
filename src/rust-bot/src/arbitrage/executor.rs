//! Trade Executor
//!
//! Executes arbitrage trades across DEXs using Uniswap V2 Router interface.
//! Phase 1 implementation with two separate transactions (has leg risk).
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use crate::types::{ArbitrageOpportunity, BotConfig, DexType, TradeResult};
use anyhow::{anyhow, Result};
use ethers::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

// Uniswap V2 Router ABI (minimal interface for swaps)
abigen!(
    IUniswapV2Router02,
    r#"[
        function swapExactTokensForTokens(uint256 amountIn, uint256 amountOutMin, address[] calldata path, address to, uint256 deadline) external returns (uint256[] memory amounts)
        function getAmountsOut(uint256 amountIn, address[] calldata path) external view returns (uint256[] memory amounts)
    ]"#
);

// ERC20 ABI for token approvals
abigen!(
    IERC20,
    r#"[
        function approve(address spender, uint256 amount) external returns (bool)
        function allowance(address owner, address spender) external view returns (uint256)
        function balanceOf(address account) external view returns (uint256)
    ]"#
);

/// Trade executor for DEX arbitrage
pub struct TradeExecutor<M: Middleware> {
    provider: Arc<M>,
    wallet: LocalWallet,
    config: BotConfig,
    /// Dry run mode - simulates trades without executing
    dry_run: bool,
}

impl<M: Middleware + 'static> TradeExecutor<M> {
    /// Create a new TradeExecutor
    pub fn new(provider: Arc<M>, wallet: LocalWallet, config: BotConfig) -> Self {
        Self {
            provider,
            wallet,
            config,
            dry_run: true, // Default to dry run for safety
        }
    }

    /// Enable or disable dry run mode
    pub fn set_dry_run(&mut self, dry_run: bool) {
        self.dry_run = dry_run;
        if dry_run {
            info!("Executor in DRY RUN mode - trades will be simulated");
        } else {
            warn!("⚠️ Executor in LIVE mode - trades will be executed!");
        }
    }

    /// Execute an arbitrage opportunity
    pub async fn execute(&self, opportunity: &ArbitrageOpportunity) -> Result<TradeResult> {
        let start_time = Instant::now();
        let pair_symbol = &opportunity.pair.symbol;

        info!(
            "🚀 Executing arbitrage: {} | Buy {:?} @ {:.6} | Sell {:?} @ {:.6}",
            pair_symbol,
            opportunity.buy_dex,
            opportunity.buy_price,
            opportunity.sell_dex,
            opportunity.sell_price
        );

        // Get token addresses
        let token0 = opportunity.pair.token0;
        let token1 = opportunity.pair.token1;
        let trade_size = opportunity.trade_size;

        if self.dry_run {
            return self.simulate_execution(opportunity, start_time).await;
        }

        // Check gas price before executing
        let gas_price = self.provider.get_gas_price().await?;
        let max_gas_gwei = U256::from(self.config.max_gas_price_gwei) * U256::from(1_000_000_000u64);
        if gas_price > max_gas_gwei {
            return Ok(TradeResult {
                opportunity: pair_symbol.clone(),
                tx_hash: None,
                success: false,
                profit_usd: 0.0,
                gas_cost_usd: 0.0,
                net_profit_usd: 0.0,
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some(format!(
                    "Gas price too high: {} gwei > {} gwei max",
                    gas_price / U256::from(1_000_000_000u64),
                    self.config.max_gas_price_gwei
                )),
            });
        }

        // Step 1: Approve tokens for routers (if needed)
        self.ensure_approval(token0, opportunity.buy_dex, trade_size)
            .await?;

        // Step 2: Execute buy swap (token0 -> token1 on buy DEX)
        info!("📈 Buy: {} {} on {:?}", trade_size, pair_symbol, opportunity.buy_dex);
        let buy_result = self
            .swap(
                opportunity.buy_dex,
                token0,
                token1,
                trade_size,
                self.calculate_min_out(trade_size, opportunity.buy_price),
            )
            .await;

        let (buy_tx_hash, amount_received) = match buy_result {
            Ok((hash, amount)) => (hash, amount),
            Err(e) => {
                error!("Buy swap failed: {}", e);
                return Ok(TradeResult {
                    opportunity: pair_symbol.clone(),
                    tx_hash: None,
                    success: false,
                    profit_usd: 0.0,
                    gas_cost_usd: 0.0,
                    net_profit_usd: 0.0,
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(format!("Buy swap failed: {}", e)),
                });
            }
        };

        info!("✅ Buy complete: {} | Received: {}", buy_tx_hash, amount_received);

        // Step 3: Approve token1 for sell router
        self.ensure_approval(token1, opportunity.sell_dex, amount_received)
            .await?;

        // Step 4: Execute sell swap (token1 -> token0 on sell DEX)
        info!("📉 Sell: {} on {:?}", amount_received, opportunity.sell_dex);
        let sell_result = self
            .swap(
                opportunity.sell_dex,
                token1,
                token0,
                amount_received,
                self.calculate_min_out(amount_received, 1.0 / opportunity.sell_price),
            )
            .await;

        let (sell_tx_hash, final_amount) = match sell_result {
            Ok((hash, amount)) => (hash, amount),
            Err(e) => {
                error!("Sell swap failed: {}", e);
                return Ok(TradeResult {
                    opportunity: pair_symbol.clone(),
                    tx_hash: Some(buy_tx_hash.to_string()),
                    success: false,
                    profit_usd: 0.0,
                    gas_cost_usd: 0.0,
                    net_profit_usd: 0.0,
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(format!("Sell swap failed (buy succeeded): {}", e)),
                });
            }
        };

        info!("✅ Sell complete: {} | Final: {}", sell_tx_hash, final_amount);

        // Calculate profit
        let profit_wei = if final_amount > trade_size {
            final_amount - trade_size
        } else {
            U256::zero()
        };
        let profit_usd = self.wei_to_usd(profit_wei, pair_symbol);

        // Estimate gas cost (actual cost would require receipt analysis)
        let gas_cost_usd = 0.50; // Estimated
        let net_profit_usd = profit_usd - gas_cost_usd;

        let success = net_profit_usd > 0.0;

        if success {
            info!(
                "🎉 PROFIT: ${:.2} (gross: ${:.2}, gas: ${:.2})",
                net_profit_usd, profit_usd, gas_cost_usd
            );
        } else {
            warn!(
                "📉 LOSS: ${:.2} (gross: ${:.2}, gas: ${:.2})",
                net_profit_usd, profit_usd, gas_cost_usd
            );
        }

        Ok(TradeResult {
            opportunity: pair_symbol.clone(),
            tx_hash: Some(sell_tx_hash.to_string()),
            success,
            profit_usd,
            gas_cost_usd,
            net_profit_usd,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            error: None,
        })
    }

    /// Simulate execution without actual trades (dry run)
    async fn simulate_execution(
        &self,
        opportunity: &ArbitrageOpportunity,
        start_time: Instant,
    ) -> Result<TradeResult> {
        let pair_symbol = &opportunity.pair.symbol;

        info!("🔬 DRY RUN: Simulating arbitrage for {}", pair_symbol);
        info!(
            "   Would buy {} on {:?} @ {:.6}",
            opportunity.trade_size, opportunity.buy_dex, opportunity.buy_price
        );
        info!(
            "   Would sell on {:?} @ {:.6}",
            opportunity.sell_dex, opportunity.sell_price
        );
        info!(
            "   Estimated profit: ${:.2} (spread: {:.2}%)",
            opportunity.estimated_profit, opportunity.spread_percent
        );

        // Simulate some processing time
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(TradeResult {
            opportunity: pair_symbol.clone(),
            tx_hash: Some("DRY_RUN_NO_TX".to_string()),
            success: true,
            profit_usd: opportunity.estimated_profit + 0.50, // Add back gas for simulation
            gas_cost_usd: 0.50,
            net_profit_usd: opportunity.estimated_profit,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            error: None,
        })
    }

    /// Execute a single swap on a DEX
    async fn swap(
        &self,
        dex: DexType,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        min_amount_out: U256,
    ) -> Result<(TxHash, U256)> {
        let router_address = self.get_router_address(dex);

        // Create signer client
        let client = SignerMiddleware::new(
            self.provider.clone(),
            self.wallet.clone().with_chain_id(self.config.chain_id),
        );
        let client = Arc::new(client);

        let router = IUniswapV2Router02::new(router_address, client.clone());

        // Build swap path
        let path = vec![token_in, token_out];

        // Set deadline (current time + 5 minutes)
        let deadline = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 300;

        let wallet_address = self.wallet.address();

        debug!(
            "Swap: {} {} -> {} on {:?}",
            amount_in, token_in, token_out, dex
        );
        debug!("  Min out: {}, Deadline: {}", min_amount_out, deadline);

        // Execute swap
        let tx = router.swap_exact_tokens_for_tokens(
            amount_in,
            min_amount_out,
            path,
            wallet_address,
            U256::from(deadline),
        );

        let pending_tx = tx.send().await.map_err(|e| anyhow!("Send failed: {}", e))?;
        let tx_hash = pending_tx.tx_hash();

        info!("Swap tx submitted: {:?}", tx_hash);

        // Wait for confirmation
        let receipt = pending_tx
            .await
            .map_err(|e| anyhow!("Confirmation failed: {}", e))?
            .ok_or_else(|| anyhow!("No receipt returned"))?;

        if receipt.status != Some(U64::from(1)) {
            return Err(anyhow!("Transaction reverted"));
        }

        // Parse output amount from logs (simplified - assumes last log has amount)
        // In production, you'd parse the Swap event properly
        let amount_out = min_amount_out; // Placeholder - actual amount would come from logs

        Ok((tx_hash, amount_out))
    }

    /// Ensure token approval for router
    async fn ensure_approval(
        &self,
        token: Address,
        dex: DexType,
        amount: U256,
    ) -> Result<()> {
        let router_address = self.get_router_address(dex);

        // Create signer client
        let client = SignerMiddleware::new(
            self.provider.clone(),
            self.wallet.clone().with_chain_id(self.config.chain_id),
        );
        let client = Arc::new(client);

        let token_contract = IERC20::new(token, client.clone());
        let wallet_address = self.wallet.address();

        // Check current allowance
        let allowance = token_contract
            .allowance(wallet_address, router_address)
            .call()
            .await?;

        if allowance >= amount {
            debug!("Sufficient allowance: {} >= {}", allowance, amount);
            return Ok(());
        }

        // Approve max uint256 for future trades
        info!("Approving {} for {:?} router", token, dex);
        let max_approval = U256::MAX;
        let tx = token_contract.approve(router_address, max_approval);
        let pending_tx = tx.send().await?;

        let receipt = pending_tx.await?.ok_or_else(|| anyhow!("No approval receipt"))?;

        if receipt.status != Some(U64::from(1)) {
            return Err(anyhow!("Approval transaction reverted"));
        }

        info!("Approval confirmed: {:?}", receipt.transaction_hash);
        Ok(())
    }

    /// Get router address for a DEX
    fn get_router_address(&self, dex: DexType) -> Address {
        match dex {
            DexType::Uniswap | DexType::Quickswap => self.config.uniswap_router,
            DexType::Sushiswap => self.config.sushiswap_router,
        }
    }

    /// Calculate minimum output with slippage protection
    fn calculate_min_out(&self, amount_in: U256, price: f64) -> U256 {
        // Expected output based on price
        let expected_out = amount_in.as_u128() as f64 * price;

        // Apply slippage tolerance
        let slippage_factor = 1.0 - (self.config.max_slippage_percent / 100.0);
        let min_out = expected_out * slippage_factor;

        U256::from(min_out as u128)
    }

    /// Convert Wei to USD based on token pair
    fn wei_to_usd(&self, wei: U256, pair_symbol: &str) -> f64 {
        let wei_f = wei.as_u128() as f64;

        if pair_symbol.starts_with("WETH") {
            (wei_f / 1e18) * 3300.0
        } else if pair_symbol.starts_with("WMATIC") {
            (wei_f / 1e18) * 0.50
        } else {
            wei_f / 1e18
        }
    }
}
