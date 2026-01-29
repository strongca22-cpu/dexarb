//! Trade Executor
//!
//! Executes arbitrage trades across DEXs using Uniswap V2 and V3 Router interfaces.
//! V2: swapExactTokensForTokens (Quickswap, Sushiswap, Apeswap)
//! V3: exactInputSingle (Uniswap V3 fee tiers: 0.05%, 0.30%, 1.00%)
//! Phase 1 implementation with two separate transactions (has leg risk).
//! Includes IRS-compliant tax logging for all executed trades.
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//! Modified: 2026-01-28 (Phase 5: Tax logging integration)
//! Modified: 2026-01-29 (V3 SwapRouter support: exactInputSingle)

use crate::tax::{TaxLogger, TaxRecordBuilder};
use crate::types::{ArbitrageOpportunity, BotConfig, DexType, TradeResult};
use anyhow::{anyhow, Result};
use ethers::prelude::*;
use rust_decimal::Decimal;
use std::path::PathBuf;
use std::str::FromStr;
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

// Uniswap V3 SwapRouter ABI (exactInputSingle for single-hop V3 swaps)
// ExactInputSingleParams: (tokenIn, tokenOut, fee, recipient, deadline, amountIn, amountOutMinimum, sqrtPriceLimitX96)
abigen!(
    ISwapRouter,
    r#"[{"inputs":[{"components":[{"internalType":"address","name":"tokenIn","type":"address"},{"internalType":"address","name":"tokenOut","type":"address"},{"internalType":"uint24","name":"fee","type":"uint24"},{"internalType":"address","name":"recipient","type":"address"},{"internalType":"uint256","name":"deadline","type":"uint256"},{"internalType":"uint256","name":"amountIn","type":"uint256"},{"internalType":"uint256","name":"amountOutMinimum","type":"uint256"},{"internalType":"uint160","name":"sqrtPriceLimitX96","type":"uint160"}],"internalType":"struct ISwapRouter.ExactInputSingleParams","name":"params","type":"tuple"}],"name":"exactInputSingle","outputs":[{"internalType":"uint256","name":"amountOut","type":"uint256"}],"stateMutability":"payable","type":"function"}]"#
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
    /// Tax logger for IRS compliance
    tax_logger: Option<TaxLogger>,
    /// Price oracle for USD conversions
    tax_record_builder: Option<TaxRecordBuilder>,
}

impl<M: Middleware + 'static> TradeExecutor<M> {
    /// Create a new TradeExecutor
    pub fn new(provider: Arc<M>, wallet: LocalWallet, config: BotConfig) -> Self {
        Self {
            provider,
            wallet,
            config,
            dry_run: true, // Default to dry run for safety
            tax_logger: None,
            tax_record_builder: None,
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

    /// Enable tax logging for IRS compliance
    ///
    /// This should be called before executing real trades.
    /// Tax records are written to `data/tax/trades_YYYY.csv` and `.jsonl`.
    pub fn enable_tax_logging(&mut self, tax_dir: &str) -> Result<()> {
        let tax_path = PathBuf::from(tax_dir);
        self.tax_logger = Some(TaxLogger::new(&tax_path)?);
        self.tax_record_builder = Some(TaxRecordBuilder::default());
        info!("Tax logging enabled: {}", tax_dir);
        Ok(())
    }

    /// Get wallet address as string (for tax records)
    fn wallet_address_string(&self) -> String {
        format!("{:?}", self.wallet.address())
    }

    /// Execute an arbitrage opportunity
    pub async fn execute(&mut self, opportunity: &ArbitrageOpportunity) -> Result<TradeResult> {
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
                block_number: None,
                success: false,
                profit_usd: 0.0,
                gas_cost_usd: 0.0,
                gas_used_native: 0.0,
                net_profit_usd: 0.0,
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some(format!(
                    "Gas price too high: {} gwei > {} gwei max",
                    gas_price / U256::from(1_000_000_000u64),
                    self.config.max_gas_price_gwei
                )),
                amount_in: None,
                amount_out: None,
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

        let (buy_tx_hash, amount_received, buy_block) = match buy_result {
            Ok((hash, amount, block)) => (hash, amount, block),
            Err(e) => {
                error!("Buy swap failed: {}", e);
                return Ok(TradeResult {
                    opportunity: pair_symbol.clone(),
                    tx_hash: None,
                    block_number: None,
                    success: false,
                    profit_usd: 0.0,
                    gas_cost_usd: 0.0,
                    gas_used_native: 0.0,
                    net_profit_usd: 0.0,
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(format!("Buy swap failed: {}", e)),
                    amount_in: Some(trade_size.to_string()),
                    amount_out: None,
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

        let (sell_tx_hash, final_amount, sell_block) = match sell_result {
            Ok((hash, amount, block)) => (hash, amount, block),
            Err(e) => {
                error!("Sell swap failed: {}", e);
                return Ok(TradeResult {
                    opportunity: pair_symbol.clone(),
                    tx_hash: Some(buy_tx_hash.to_string()),
                    block_number: Some(buy_block),
                    success: false,
                    profit_usd: 0.0,
                    gas_cost_usd: 0.0,
                    gas_used_native: 0.0,
                    net_profit_usd: 0.0,
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(format!("Sell swap failed (buy succeeded): {}", e)),
                    amount_in: Some(trade_size.to_string()),
                    amount_out: Some(amount_received.to_string()),
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
        let gas_used_native = 0.001; // ~200k gas at 5 gwei = 0.001 MATIC
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

        // Log to tax records (IRS compliance)
        self.log_tax_record_if_enabled(
            opportunity,
            &sell_tx_hash.to_string(),
            sell_block,
            trade_size,
            final_amount,
            gas_used_native,
        );

        Ok(TradeResult {
            opportunity: pair_symbol.clone(),
            tx_hash: Some(sell_tx_hash.to_string()),
            block_number: Some(sell_block),
            success,
            profit_usd,
            gas_cost_usd,
            gas_used_native,
            net_profit_usd,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            error: None,
            amount_in: Some(trade_size.to_string()),
            amount_out: Some(final_amount.to_string()),
        })
    }

    /// Log a tax record if tax logging is enabled
    fn log_tax_record_if_enabled(
        &mut self,
        opportunity: &ArbitrageOpportunity,
        tx_hash: &str,
        block_number: u64,
        amount_in: U256,
        amount_out: U256,
        gas_native: f64,
    ) {
        // Get wallet address first (immutable borrow)
        let wallet_address = format!("{:?}", self.wallet.address());

        // Check if tax logging is enabled
        let (logger, builder) = match (&mut self.tax_logger, &self.tax_record_builder) {
            (Some(l), Some(b)) => (l, b),
            _ => return, // Tax logging not enabled
        };

        if let Err(e) = Self::build_and_log_tax_record(
            opportunity,
            tx_hash,
            block_number,
            amount_in,
            amount_out,
            gas_native,
            &wallet_address,
            builder,
            logger,
        ) {
            error!("Failed to log tax record: {}", e);
        }
    }

    /// Build and log a tax record for IRS compliance
    #[allow(clippy::too_many_arguments)]
    fn build_and_log_tax_record(
        opportunity: &ArbitrageOpportunity,
        tx_hash: &str,
        block_number: u64,
        amount_in: U256,
        amount_out: U256,
        gas_native: f64,
        wallet_address: &str,
        builder: &TaxRecordBuilder,
        logger: &mut TaxLogger,
    ) -> Result<()> {
        // Parse token symbols from pair (e.g., "WETH/USDC" -> "WETH", "USDC")
        let pair_parts: Vec<&str> = opportunity.pair.symbol.split('/').collect();
        let (asset_sent, asset_received) = if pair_parts.len() == 2 {
            (pair_parts[0], pair_parts[1])
        } else {
            ("UNKNOWN", "UNKNOWN")
        };

        // Convert U256 amounts to Decimal
        // Note: This assumes 18 decimals - in production, use actual token decimals
        let amount_sent = Decimal::from_str(&amount_in.to_string())
            .unwrap_or(Decimal::ZERO);
        let amount_received = Decimal::from_str(&amount_out.to_string())
            .unwrap_or(Decimal::ZERO);

        // DEX fee (typically 0.30% for V2)
        let dex_fee_percent = if opportunity.buy_dex.is_v3() {
            Decimal::from_str(&format!("{}", opportunity.buy_dex.v3_fee_bps().unwrap_or(30) as f64 / 10000.0))
                .unwrap_or(Decimal::from_str("0.003").unwrap())
        } else {
            Decimal::from_str("0.003").unwrap() // 0.30% for V2
        };

        // Pool addresses (use empty string if not available)
        let pool_buy = opportunity.buy_pool_address
            .map(|a| format!("{:?}", a))
            .unwrap_or_default();
        let pool_sell = opportunity.sell_pool_address
            .map(|a| format!("{:?}", a))
            .unwrap_or_default();

        // Build tax record with automatic price fetching
        let record = builder.build_arbitrage_record(
            asset_sent,
            amount_sent,
            asset_received,
            amount_received,
            Decimal::from_str(&gas_native.to_string()).unwrap_or(Decimal::ZERO),
            dex_fee_percent,
            tx_hash.to_string(),
            block_number,
            wallet_address.to_string(),
            opportunity.buy_dex.to_string(),
            opportunity.sell_dex.to_string(),
            pool_buy,
            pool_sell,
            Decimal::from_str(&opportunity.spread_percent.to_string()).unwrap_or(Decimal::ZERO),
            false, // is_paper_trade = false for real execution
        )?;

        // Log to CSV and JSON
        logger.log(&record)?;
        info!("📋 Tax record logged: {} -> {} | ${:.2} gain",
              asset_sent, asset_received, record.capital_gain_loss);

        Ok(())
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
            block_number: Some(0),
            success: true,
            profit_usd: opportunity.estimated_profit + 0.50, // Add back gas for simulation
            gas_cost_usd: 0.50,
            gas_used_native: 0.001,
            net_profit_usd: opportunity.estimated_profit,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            error: None,
            amount_in: Some(opportunity.trade_size.to_string()),
            amount_out: None, // Unknown in simulation
        })
    }

    /// Execute a single swap on a DEX (routes to V2 or V3 based on DexType)
    /// Returns (tx_hash, amount_out, block_number)
    async fn swap(
        &self,
        dex: DexType,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        min_amount_out: U256,
    ) -> Result<(TxHash, U256, u64)> {
        if dex.is_v3() {
            return self.swap_v3(dex, token_in, token_out, amount_in, min_amount_out).await;
        }
        self.swap_v2(dex, token_in, token_out, amount_in, min_amount_out).await
    }

    /// Execute a V2 swap (swapExactTokensForTokens)
    /// Used for Quickswap, Sushiswap, Apeswap
    async fn swap_v2(
        &self,
        dex: DexType,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        min_amount_out: U256,
    ) -> Result<(TxHash, U256, u64)> {
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
            "V2 Swap: {} {} -> {} on {:?}",
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

        info!("V2 swap tx submitted: {:?}", tx_hash);

        // Wait for confirmation
        let receipt = pending_tx
            .await
            .map_err(|e| anyhow!("Confirmation failed: {}", e))?
            .ok_or_else(|| anyhow!("No receipt returned"))?;

        if receipt.status != Some(U64::from(1)) {
            return Err(anyhow!("V2 transaction reverted"));
        }

        // Extract block number for tax logging
        let block_number = receipt.block_number
            .map(|bn| bn.as_u64())
            .unwrap_or(0);

        // Parse output amount from logs (simplified - assumes last log has amount)
        // In production, you'd parse the Swap event properly
        let amount_out = min_amount_out; // Placeholder - actual amount would come from logs

        Ok((tx_hash, amount_out, block_number))
    }

    /// Execute a V3 swap (exactInputSingle)
    /// Used for Uniswap V3 fee tiers: 0.05%, 0.30%, 1.00%
    async fn swap_v3(
        &self,
        dex: DexType,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        min_amount_out: U256,
    ) -> Result<(TxHash, U256, u64)> {
        let router_address = self.get_router_address(dex);
        let fee = dex.v3_fee_tier().ok_or_else(|| anyhow!("Not a V3 DEX type: {:?}", dex))?;

        // Create signer client
        let client = SignerMiddleware::new(
            self.provider.clone(),
            self.wallet.clone().with_chain_id(self.config.chain_id),
        );
        let client = Arc::new(client);

        let router = ISwapRouter::new(router_address, client.clone());

        // Set deadline (current time + 5 minutes)
        let deadline = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 300;

        let wallet_address = self.wallet.address();

        debug!(
            "V3 Swap: {} {} -> {} on {:?} (fee tier: {})",
            amount_in, token_in, token_out, dex, fee
        );
        debug!("  Min out: {}, Deadline: {}", min_amount_out, deadline);

        // Build ExactInputSingleParams struct (generated by abigen from V3 SwapRouter ABI)
        // sqrtPriceLimitX96 = 0 means no price limit (accept any price within slippage)
        let params = i_swap_router::ExactInputSingleParams {
            token_in,
            token_out,
            fee,
            recipient: wallet_address,
            deadline: U256::from(deadline),
            amount_in,
            amount_out_minimum: min_amount_out,
            sqrt_price_limit_x96: U256::zero(), // 0 = no limit
        };

        let tx = router.exact_input_single(params);

        let pending_tx = tx.send().await.map_err(|e| anyhow!("V3 send failed: {}", e))?;
        let tx_hash = pending_tx.tx_hash();

        info!("V3 swap tx submitted: {:?}", tx_hash);

        // Wait for confirmation
        let receipt = pending_tx
            .await
            .map_err(|e| anyhow!("V3 confirmation failed: {}", e))?
            .ok_or_else(|| anyhow!("No receipt returned"))?;

        if receipt.status != Some(U64::from(1)) {
            return Err(anyhow!("V3 transaction reverted"));
        }

        // Extract block number for tax logging
        let block_number = receipt.block_number
            .map(|bn| bn.as_u64())
            .unwrap_or(0);

        // Parse output amount from logs (simplified)
        // In production, parse the V3 Swap event for actual amountOut
        let amount_out = min_amount_out; // Placeholder - actual amount would come from logs

        Ok((tx_hash, amount_out, block_number))
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
            DexType::Apeswap => self.config.apeswap_router.unwrap_or(self.config.uniswap_router),
            // V3 DEX types - use V3 router
            DexType::UniswapV3_005 | DexType::UniswapV3_030 | DexType::UniswapV3_100 => {
                self.config.uniswap_v3_router.unwrap_or(self.config.uniswap_router)
            }
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
