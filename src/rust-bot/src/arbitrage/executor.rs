//! Trade Executor
//!
//! Executes arbitrage trades across DEXs using Uniswap V2, V3, and Algebra Router interfaces.
//! V2: swapExactTokensForTokens (Quickswap, Sushiswap, Apeswap)
//! V3: exactInputSingle (Uniswap V3, SushiSwap V3 fee tiers)
//! Algebra: exactInputSingle (QuickSwap V3 — no fee param, dynamic fees)
//! Includes IRS-compliant tax logging for all executed trades.
//!
//! Execution modes:
//!   - Atomic (preferred): Single tx via ArbExecutor.sol contract. Both swaps
//!     execute atomically — reverts on loss. Zero leg risk.
//!     Supports V3↔V3, V2↔V3, and V2↔V2 via fee sentinel routing:
//!       fee=0 → Algebra (QuickSwap V3), fee=1..65535 → standard V3,
//!       fee=16777215 (type(uint24).max) → V2 swapExactTokensForTokens.
//!   - Legacy two-tx: Separate buy + sell transactions (has leg risk).
//!     Used as fallback if ARB_EXECUTOR_ADDRESS is not configured.
//!
//! Post-incident fixes (2026-01-29 $500 loss):
//!   1. calculate_min_out now handles token decimal conversion (6 vs 18 dec)
//!   2. V3 Quoter pre-trade simulation (quoteExactInputSingle) before execution
//!   3. Actual amountOut parsed from ERC20 Transfer events in receipt logs
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//! Modified: 2026-01-28 (Phase 5: Tax logging integration)
//! Modified: 2026-01-29 (V3 SwapRouter support: exactInputSingle)
//! Modified: 2026-01-29 (Post-incident: decimal fix, quoter, event parsing)
//! Modified: 2026-01-30 (Atomic execution via ArbExecutor.sol contract)
//! Modified: 2026-01-30 (QuickSwap V3 / Algebra router + quoter support)

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

// Uniswap V3 QuoterV1 ABI (pre-trade simulation)
// quoteExactInputSingle simulates a swap and returns the expected output
// Note: This is not a view function — it reverts internally after computing. Use .call() only.
abigen!(
    IQuoter,
    r#"[
        function quoteExactInputSingle(address tokenIn, address tokenOut, uint24 fee, uint256 amountIn, uint160 sqrtPriceLimitX96) external returns (uint256 amountOut)
    ]"#
);

// SushiSwap V3 QuoterV2 ABI (different function signature from V1)
// Takes a struct param instead of individual args; returns (amountOut, sqrtPriceX96After, initializedTicksCrossed, gasEstimate)
abigen!(
    IQuoterV2,
    r#"[{"inputs":[{"components":[{"internalType":"address","name":"tokenIn","type":"address"},{"internalType":"address","name":"tokenOut","type":"address"},{"internalType":"uint256","name":"amountIn","type":"uint256"},{"internalType":"uint24","name":"fee","type":"uint24"},{"internalType":"uint160","name":"sqrtPriceLimitX96","type":"uint160"}],"internalType":"struct IQuoterV2.QuoteExactInputSingleParams","name":"params","type":"tuple"}],"name":"quoteExactInputSingle","outputs":[{"internalType":"uint256","name":"amountOut","type":"uint256"},{"internalType":"uint160","name":"sqrtPriceX96After","type":"uint160"},{"internalType":"uint32","name":"initializedTicksCrossed","type":"uint32"},{"internalType":"uint256","name":"gasEstimate","type":"uint256"}],"stateMutability":"nonpayable","type":"function"}]"#
);

// QuickSwap V3 (Algebra) SwapRouter ABI — no fee parameter, uses limitSqrtPrice
// ExactInputSingleParams: (tokenIn, tokenOut, recipient, deadline, amountIn, amountOutMinimum, limitSqrtPrice)
abigen!(
    IAlgebraSwapRouter,
    r#"[{"inputs":[{"components":[{"internalType":"address","name":"tokenIn","type":"address"},{"internalType":"address","name":"tokenOut","type":"address"},{"internalType":"address","name":"recipient","type":"address"},{"internalType":"uint256","name":"deadline","type":"uint256"},{"internalType":"uint256","name":"amountIn","type":"uint256"},{"internalType":"uint256","name":"amountOutMinimum","type":"uint256"},{"internalType":"uint160","name":"limitSqrtPrice","type":"uint160"}],"internalType":"struct ISwapRouter.ExactInputSingleParams","name":"params","type":"tuple"}],"name":"exactInputSingle","outputs":[{"internalType":"uint256","name":"amountOut","type":"uint256"}],"stateMutability":"payable","type":"function"}]"#
);

// QuickSwap V3 (Algebra) Quoter ABI — no fee parameter
// quoteExactInputSingle(address,address,uint256,uint160) → (uint256,uint16)
abigen!(
    IAlgebraQuoter,
    r#"[
        function quoteExactInputSingle(address tokenIn, address tokenOut, uint256 amountIn, uint160 limitSqrtPrice) external returns (uint256 amountOut, uint16 fee)
    ]"#
);

// ERC20 ABI for token approvals
abigen!(
    IERC20,
    r#"[
        function approve(address spender, uint256 amount) external returns (bool)
        function allowance(address owner, address spender) external view returns (uint256)
        function balanceOf(address account) external view returns (uint256)
        function decimals() external view returns (uint8)
    ]"#
);

// ArbExecutor contract ABI (atomic two-leg arbitrage)
// Executes both V3 swaps in a single transaction. Reverts if profit < minProfit.
abigen!(
    IArbExecutor,
    r#"[
        function executeArb(address token0, address token1, address routerBuy, address routerSell, uint24 feeBuy, uint24 feeSell, uint256 amountIn, uint256 minProfit) external returns (uint256 profit)
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

        // Get token addresses — orient so token0 = quote (USDC), token1 = base
        let (token0, token1) = if opportunity.quote_token_is_token0 {
            (opportunity.pair.token0, opportunity.pair.token1)
        } else {
            (opportunity.pair.token1, opportunity.pair.token0)
        };
        let trade_size = opportunity.trade_size;

        if self.dry_run {
            return self.simulate_execution(opportunity, start_time).await;
        }

        // Route to atomic execution if ArbExecutor contract is configured.
        // Supports V3↔V3, V2↔V3, and V2↔V2 — all via fee sentinel routing in the contract.
        if self.config.arb_executor_address.is_some() {
            return self.execute_atomic(opportunity, start_time).await;
        }

        // Legacy two-tx execution (fallback — has leg risk)
        info!("Using legacy two-tx execution (no atomic contract configured)");

        // Gas cap removed: on Polygon, gas costs fractions of a penny even at
        // high gwei values. Profitability is already gated by the detector
        // (ESTIMATED_GAS_COST_USD) and the post-trade net profit check.

        // Token decimals for correct slippage calculation
        let t0_dec = opportunity.token0_decimals;
        let t1_dec = opportunity.token1_decimals;

        // Pre-trade safety: V3 Quoter simulation (buy leg)
        // Verifies buy pool can fill order before committing capital
        // Sell leg is Quoter-checked separately after buy succeeds
        if opportunity.buy_dex.is_v3() {
            if let Err(e) = self.v3_quoter_check(
                token0, token1, opportunity.buy_dex, trade_size,
                self.calculate_min_out(trade_size, opportunity.buy_price, t0_dec, t1_dec),
            ).await {
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
                    error: Some(format!("V3 Quoter pre-check failed: {}", e)),
                    amount_in: Some(trade_size.to_string()),
                    amount_out: None,
                });
            }
        }

        // Step 1: Approve tokens for routers (if needed)
        self.ensure_approval(token0, opportunity.buy_dex, trade_size)
            .await?;

        // Step 2: Execute buy swap (token0 -> token1 on buy DEX)
        // buy_dex has the HIGHER V3 price (more token1 per token0 = better entry)
        let buy_min_out = self.calculate_min_out(
            trade_size, opportunity.buy_price, t0_dec, t1_dec,
        );
        info!(
            "📈 Buy: {} token0 on {:?} | min_out: {} token1",
            trade_size, opportunity.buy_dex, buy_min_out
        );
        let buy_result = self
            .swap(
                opportunity.buy_dex,
                token0,
                token1,
                trade_size,
                buy_min_out,
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

        // Pre-sell safety: V3 Quoter simulation (sell leg)
        // Buy has executed — we're holding token1. Verify sell pool can return expected token0
        // before sending the sell tx. If rejected, bot stops (capital committed, manual exit needed).
        if opportunity.sell_dex.is_v3() {
            let sell_quote_min = self.calculate_min_out(
                amount_received, 1.0 / opportunity.sell_price, t1_dec, t0_dec,
            );
            if let Err(e) = self.v3_quoter_check(
                token1, token0, opportunity.sell_dex, amount_received,
                sell_quote_min,
            ).await {
                error!("Sell swap failed: V3 Quoter rejected sell leg: {}", e);
                return Ok(TradeResult {
                    opportunity: pair_symbol.clone(),
                    tx_hash: Some(format!("{:?}", buy_tx_hash)),
                    block_number: Some(buy_block),
                    success: false,
                    profit_usd: 0.0,
                    gas_cost_usd: 0.0,
                    gas_used_native: 0.0,
                    net_profit_usd: 0.0,
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(format!(
                        "Sell Quoter rejected after buy executed (holding token1, manual sell needed): {}",
                        e
                    )),
                    amount_in: Some(trade_size.to_string()),
                    amount_out: Some(amount_received.to_string()),
                });
            }
        }

        // Step 3: Approve token1 for sell router
        self.ensure_approval(token1, opportunity.sell_dex, amount_received)
            .await?;

        // Step 4: Execute sell swap (token1 -> token0 on sell DEX)
        // sell_dex has the LOWER V3 price (1/price is higher = more token0 per token1 = better exit)
        let sell_min_out = self.calculate_min_out(
            amount_received, 1.0 / opportunity.sell_price, t1_dec, t0_dec,
        );
        info!(
            "📉 Sell: {} token1 on {:?} | min_out: {} token0",
            amount_received, opportunity.sell_dex, sell_min_out
        );
        let sell_result = self
            .swap(
                opportunity.sell_dex,
                token1,
                token0,
                amount_received,
                sell_min_out,
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
        // Polygon: ~400k gas for two V3 swaps, ~50 gwei avg = 0.02 MATIC = ~$0.01
        let gas_used_native = 0.02; // ~400k gas at 50 gwei = 0.02 MATIC
        let gas_cost_usd = 0.01; // ~$0.01 at MATIC ~$0.50
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

    /// Execute an atomic arbitrage via the ArbExecutor contract.
    ///
    /// Both swap legs execute in a single transaction. If the second leg fails
    /// or net profit < minProfit, the entire tx reverts — zero risk.
    ///
    /// Token flow: wallet → contract → routerBuy(token0→token1) → routerSell(token1→token0) → wallet
    async fn execute_atomic(
        &mut self,
        opportunity: &ArbitrageOpportunity,
        start_time: Instant,
    ) -> Result<TradeResult> {
        let pair_symbol = &opportunity.pair.symbol;
        let arb_address = self.config.arb_executor_address.unwrap();

        let mode = if opportunity.buy_dex.is_v2() || opportunity.sell_dex.is_v2() {
            "V2↔V3"
        } else {
            "V3↔V3"
        };
        info!(
            "⚡ ATOMIC {} execution: {} | Buy {:?} → Sell {:?} via ArbExecutor {:?}",
            mode, pair_symbol, opportunity.buy_dex, opportunity.sell_dex, arb_address
        );

        // Gas cap removed: on Polygon, gas costs fractions of a penny even at
        // high gwei values. The atomic contract reverts unprofitable trades,
        // and the detector already filters by profit-after-gas.
        let gas_price = self.provider.get_gas_price().await?;

        // ArbExecutor.sol token0 = "base token" (start & end) = USDC (quote token)
        // ArbExecutor.sol token1 = "intermediate token" (bought & sold)
        // Map from V3 pool ordering to contract ordering based on quote_token_is_token0
        let (token0, token1) = if opportunity.quote_token_is_token0 {
            (opportunity.pair.token0, opportunity.pair.token1)
        } else {
            (opportunity.pair.token1, opportunity.pair.token0)
        };
        let trade_size = opportunity.trade_size;

        // Get router addresses and fee sentinels.
        // atomic_fee() returns: V2 → 16777215 (V2 sentinel), Algebra → 0, V3 → fee tier
        let router_buy = self.get_router_address(opportunity.buy_dex);
        let router_sell = self.get_router_address(opportunity.sell_dex);
        let fee_buy = opportunity.buy_dex.atomic_fee();
        let fee_sell = opportunity.sell_dex.atomic_fee();

        // minProfit in token0 raw units
        // Convert min_profit_usd to token0 units (USDC = 6 dec, 1 USDC = 1e6)
        // For non-stablecoin base tokens this would need a price oracle
        let min_profit_raw = U256::from((self.config.min_profit_usd * 1e6) as u64);

        info!(
            "  routerBuy={:?} feeBuy={} | routerSell={:?} feeSell={} | amountIn={} | minProfit={}",
            router_buy, fee_buy, router_sell, fee_sell, trade_size, min_profit_raw
        );

        // Create signer client
        let client = SignerMiddleware::new(
            self.provider.clone(),
            self.wallet.clone().with_chain_id(self.config.chain_id),
        );
        let client = Arc::new(client);

        // Call ArbExecutor.executeArb()
        let arb_contract = IArbExecutor::new(arb_address, client.clone());
        let tx = arb_contract.execute_arb(
            token0,
            token1,
            router_buy,
            router_sell,
            fee_buy.into(),  // u32 → u32, but abigen expects the right type
            fee_sell.into(),
            trade_size,
            min_profit_raw,
        );

        let pending_tx = match tx.send().await {
            Ok(pt) => pt,
            Err(e) => {
                let err_msg = format!("Atomic tx send failed: {}", e);
                // Check if this is an InsufficientProfit revert
                if err_msg.contains("InsufficientProfit") || err_msg.contains("execution reverted") {
                    info!("Atomic arb reverted (expected: insufficient profit or pool conditions changed)");
                } else {
                    error!("{}", err_msg);
                }
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
                    error: Some(err_msg),
                    amount_in: Some(trade_size.to_string()),
                    amount_out: None,
                });
            }
        };

        let tx_hash = pending_tx.tx_hash();
        info!("⚡ Atomic arb tx submitted: {:?}", tx_hash);

        // Wait for confirmation
        let receipt = pending_tx
            .await
            .map_err(|e| anyhow!("Atomic tx confirmation failed: {}", e))?
            .ok_or_else(|| anyhow!("No receipt returned for atomic tx"))?;

        let block_number = receipt.block_number.map(|bn| bn.as_u64()).unwrap_or(0);

        if receipt.status != Some(U64::from(1)) {
            warn!("Atomic arb tx reverted on-chain (tx confirmed but failed)");
            return Ok(TradeResult {
                opportunity: pair_symbol.clone(),
                tx_hash: Some(format!("{:?}", tx_hash)),
                block_number: Some(block_number),
                success: false,
                profit_usd: 0.0,
                gas_cost_usd: 0.0,
                gas_used_native: 0.0,
                net_profit_usd: 0.0,
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some("Atomic tx reverted on-chain".to_string()),
                amount_in: Some(trade_size.to_string()),
                amount_out: None,
            });
        }

        // Parse profit from ArbExecuted event
        // event ArbExecuted(token0, token1, amountIn, amountOut, profit, routerBuy, routerSell)
        // topic0 = keccak256("ArbExecuted(address,address,uint256,uint256,uint256,address,address)")
        let mut profit_raw = U256::zero();
        let mut amount_out = trade_size; // fallback
        let arb_executed_topic: H256 = ethers::utils::keccak256(
            b"ArbExecuted(address,address,uint256,uint256,uint256,address,address)"
        ).into();

        for log in &receipt.logs {
            if log.address == arb_address && !log.topics.is_empty() && log.topics[0] == arb_executed_topic {
                // data layout: amountIn (32) | amountOut (32) | profit (32) | routerBuy (32) | routerSell (32)
                if log.data.len() >= 96 {
                    amount_out = U256::from_big_endian(&log.data[32..64]);
                    profit_raw = U256::from_big_endian(&log.data[64..96]);
                    debug!("Parsed ArbExecuted: amountOut={}, profit={}", amount_out, profit_raw);
                }
                break;
            }
        }

        // profit_raw is in token0 (quote token = USDC) raw units.
        // Use actual quote token decimals instead of wei_to_usd() which assumes 18-dec WETH.
        let quote_decimals = if opportunity.quote_token_is_token0 {
            opportunity.token0_decimals
        } else {
            opportunity.token1_decimals
        };
        let profit_usd = profit_raw.low_u128() as f64 / 10_f64.powi(quote_decimals as i32);
        // Actual gas from receipt
        let gas_used = receipt.gas_used.unwrap_or(U256::from(400_000u64));
        let effective_gas_price = receipt.effective_gas_price.unwrap_or(gas_price);
        let gas_cost_wei = gas_used * effective_gas_price;
        let gas_used_native = gas_cost_wei.low_u128() as f64 / 1e18;
        let gas_cost_usd = gas_used_native * 0.50; // MATIC ~$0.50
        let net_profit_usd = profit_usd - gas_cost_usd;

        let success = net_profit_usd > 0.0;

        if success {
            info!(
                "🎉 ATOMIC PROFIT: ${:.4} (gross: ${:.4}, gas: ${:.4}) | tx: {:?}",
                net_profit_usd, profit_usd, gas_cost_usd, tx_hash
            );
        } else {
            warn!(
                "📉 ATOMIC LOSS: ${:.4} (gross: ${:.4}, gas: ${:.4}) | tx: {:?}",
                net_profit_usd, profit_usd, gas_cost_usd, tx_hash
            );
        }

        // Log tax record
        self.log_tax_record_if_enabled(
            opportunity,
            &format!("{:?}", tx_hash),
            block_number,
            trade_size,
            amount_out,
            gas_used_native,
        );

        Ok(TradeResult {
            opportunity: pair_symbol.clone(),
            tx_hash: Some(format!("{:?}", tx_hash)),
            block_number: Some(block_number),
            success,
            profit_usd,
            gas_cost_usd,
            gas_used_native,
            net_profit_usd,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            error: None,
            amount_in: Some(trade_size.to_string()),
            amount_out: Some(amount_out.to_string()),
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

        // Parse actual amountOut from ERC20 Transfer events in receipt
        let amount_out = self
            .parse_amount_out_from_receipt(&receipt, token_out, wallet_address)
            .unwrap_or_else(|| {
                warn!("V2: falling back to min_amount_out as output amount");
                min_amount_out
            });

        info!("V2 swap confirmed: block={}, amount_out={}", block_number, amount_out);

        Ok((tx_hash, amount_out, block_number))
    }

    /// Execute a V3 swap (exactInputSingle)
    /// Routes to Algebra SwapRouter (no fee) for QuickSwap V3,
    /// or standard ISwapRouter (with fee) for Uniswap/SushiSwap V3.
    async fn swap_v3(
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

        // Set deadline (current time + 5 minutes)
        let deadline = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 300;

        let wallet_address = self.wallet.address();

        // Route to correct router and wait for receipt
        // Each branch handles the full send+confirm flow to avoid lifetime issues
        // with PendingTransaction borrowing from the router contract instance.
        if dex.is_quickswap_v3() {
            // QuickSwap V3 (Algebra): no fee parameter, uses limitSqrtPrice
            debug!(
                "Algebra Swap: {} {} -> {} on {:?} (dynamic fee)",
                amount_in, token_in, token_out, dex
            );
            debug!("  Min out: {}, Deadline: {}", min_amount_out, deadline);

            let router = IAlgebraSwapRouter::new(router_address, client.clone());
            let params = i_algebra_swap_router::ExactInputSingleParams {
                token_in,
                token_out,
                recipient: wallet_address,
                deadline: U256::from(deadline),
                amount_in,
                amount_out_minimum: min_amount_out,
                limit_sqrt_price: U256::zero(), // 0 = no limit
            };
            let tx = router.exact_input_single(params);
            let pending_tx = tx.send().await.map_err(|e| anyhow!("Algebra V3 send failed: {}", e))?;
            let tx_hash = pending_tx.tx_hash();
            info!("V3 swap tx submitted: {:?} ({:?})", tx_hash, dex);

            let receipt = pending_tx
                .await
                .map_err(|e| anyhow!("V3 confirmation failed: {}", e))?
                .ok_or_else(|| anyhow!("No receipt returned"))?;

            if receipt.status != Some(U64::from(1)) {
                return Err(anyhow!("V3 transaction reverted"));
            }
            let block_number = receipt.block_number.map(|bn| bn.as_u64()).unwrap_or(0);
            let amount_out = self
                .parse_amount_out_from_receipt(&receipt, token_out, wallet_address)
                .unwrap_or_else(|| { warn!("V3: falling back to min_amount_out"); min_amount_out });
            info!("V3 swap confirmed: block={}, amount_out={}", block_number, amount_out);
            Ok((tx_hash, amount_out, block_number))
        } else {
            // Uniswap V3 / SushiSwap V3: standard ISwapRouter with fee
            let fee = dex.v3_fee_tier().ok_or_else(|| anyhow!("Not a V3 DEX type: {:?}", dex))?;
            debug!(
                "V3 Swap: {} {} -> {} on {:?} (fee tier: {})",
                amount_in, token_in, token_out, dex, fee
            );
            debug!("  Min out: {}, Deadline: {}", min_amount_out, deadline);

            let router = ISwapRouter::new(router_address, client.clone());
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
            info!("V3 swap tx submitted: {:?} ({:?})", tx_hash, dex);

            let receipt = pending_tx
                .await
                .map_err(|e| anyhow!("V3 confirmation failed: {}", e))?
                .ok_or_else(|| anyhow!("No receipt returned"))?;

            if receipt.status != Some(U64::from(1)) {
                return Err(anyhow!("V3 transaction reverted"));
            }
            let block_number = receipt.block_number.map(|bn| bn.as_u64()).unwrap_or(0);
            let amount_out = self
                .parse_amount_out_from_receipt(&receipt, token_out, wallet_address)
                .unwrap_or_else(|| { warn!("V3: falling back to min_amount_out"); min_amount_out });
            info!("V3 swap confirmed: block={}, amount_out={}", block_number, amount_out);
            Ok((tx_hash, amount_out, block_number))
        }
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
            // V2 DEX types — QuickSwapV2 uses same router as legacy Quickswap/Uniswap on Polygon
            DexType::Uniswap | DexType::Quickswap | DexType::QuickSwapV2 => self.config.uniswap_router,
            DexType::Sushiswap | DexType::SushiSwapV2 => self.config.sushiswap_router,
            DexType::Apeswap => self.config.apeswap_router.unwrap_or(self.config.uniswap_router),
            // Uniswap V3 DEX types
            DexType::UniswapV3_001 | DexType::UniswapV3_005 | DexType::UniswapV3_030 | DexType::UniswapV3_100 => {
                self.config.uniswap_v3_router.unwrap_or(self.config.uniswap_router)
            }
            // SushiSwap V3 DEX types (same ABI, different router address)
            DexType::SushiV3_001 | DexType::SushiV3_005 | DexType::SushiV3_030 => {
                self.config.sushiswap_v3_router.unwrap_or(self.config.sushiswap_router)
            }
            // QuickSwap V3 (Algebra) — different ABI, different router
            DexType::QuickswapV3 => {
                self.config.quickswap_v3_router.unwrap_or(self.config.uniswap_router)
            }
        }
    }

    /// Calculate minimum output with slippage protection and decimal conversion.
    ///
    /// CRITICAL FIX (2026-01-29 $500 loss incident):
    ///   Previously: amount_in_raw * price → produced garbage when tokens have different decimals.
    ///   Example: 500 USDC (500_000_000 at 6 dec) * 0.2056 = 102_789_500
    ///            In UNI's 18-dec format: 0.0000000001 UNI → zero slippage protection!
    ///
    ///   Now: convert raw→human, multiply by price, convert human→raw.
    ///   Correct: (500_000_000 / 1e6) * 0.2056 * 1e18 = 1.028e20 → ~102.8 UNI ✓
    ///
    /// Parameters:
    ///   amount_in: raw token amount (with input token's decimals)
    ///   price: expected output per input in human-readable units
    ///          (e.g., 0.2056 UNI per USDC, or 4.864 USDC per UNI)
    ///   in_decimals: input token's decimal places (e.g., 6 for USDC)
    ///   out_decimals: output token's decimal places (e.g., 18 for UNI)
    fn calculate_min_out(
        &self,
        amount_in: U256,
        price: f64,
        in_decimals: u8,
        out_decimals: u8,
    ) -> U256 {
        // Step 1: Convert raw input to human-readable
        let amount_in_human = amount_in.low_u128() as f64 / 10_f64.powi(in_decimals as i32);

        // Step 2: Calculate expected output in human-readable units
        let expected_out_human = amount_in_human * price;

        // Step 3: Apply slippage tolerance
        let slippage_factor = 1.0 - (self.config.max_slippage_percent / 100.0);
        let min_out_human = expected_out_human * slippage_factor;

        // Step 4: Convert back to raw output units
        let min_out_raw = min_out_human * 10_f64.powi(out_decimals as i32);

        // Safety: ensure min_out is positive and fits in u128
        if min_out_raw <= 0.0 || !min_out_raw.is_finite() {
            warn!(
                "calculate_min_out: invalid result {:.2} (in={}, price={:.6}, dec={}->{})",
                min_out_raw, amount_in, price, in_decimals, out_decimals
            );
            return U256::zero();
        }

        debug!(
            "calculate_min_out: {:.6} human_in * {:.6} price * {:.4} slippage = {:.6} human_out → {} raw ({}→{} dec)",
            amount_in_human, price, slippage_factor, min_out_human, min_out_raw as u128,
            in_decimals, out_decimals
        );

        U256::from(min_out_raw as u128)
    }

    /// V3 Quoter pre-trade simulation.
    /// Calls quoteExactInputSingle to verify the swap will produce expected output
    /// BEFORE committing any capital on-chain.
    ///
    /// This is a read-only simulation (uses .call()) — no gas spent.
    /// Routes to correct quoter: Algebra (no fee), SushiV3 (QuoterV2), UniV3 (QuoterV1).
    async fn v3_quoter_check(
        &self,
        token_in: Address,
        token_out: Address,
        dex: DexType,
        amount_in: U256,
        expected_min_out: U256,
    ) -> Result<U256> {
        let fee = dex.v3_fee_tier()
            .ok_or_else(|| anyhow!("Not a V3 DEX type: {:?}", dex))?;

        // Route to correct quoter based on DEX type
        let quoted_out = if dex.is_quickswap_v3() {
            // QuickSwap V3 (Algebra): no fee parameter
            let quoter_address = self.config.quickswap_v3_quoter
                .ok_or_else(|| anyhow!("QuickSwap V3 Quoter not configured (QUICKSWAP_V3_QUOTER)"))?;
            let quoter = IAlgebraQuoter::new(quoter_address, self.provider.clone());
            let (amount_out, _fee) = quoter
                .quote_exact_input_single(
                    token_in,
                    token_out,
                    amount_in,
                    U256::zero(), // limitSqrtPrice = 0 (no limit)
                )
                .call()
                .await
                .map_err(|e| anyhow!("Algebra Quoter simulation failed: {} — pool may lack liquidity", e))?;
            amount_out
        } else if dex.is_sushi_v3() {
            // SushiSwap V3: use QuoterV2 (struct param, tuple return)
            let quoter_address = self.config.sushiswap_v3_quoter
                .ok_or_else(|| anyhow!("SushiSwap V3 Quoter not configured (SUSHISWAP_V3_QUOTER)"))?;
            let quoter = IQuoterV2::new(quoter_address, self.provider.clone());
            let params = QuoteExactInputSingleParams {
                token_in,
                token_out,
                amount_in,
                fee: fee.into(),
                sqrt_price_limit_x96: U256::zero(),
            };
            let (amount_out, _, _, _) = quoter
                .quote_exact_input_single(params)
                .call()
                .await
                .map_err(|e| anyhow!("SushiV3 QuoterV2 simulation failed: {} — pool may lack liquidity", e))?;
            amount_out
        } else if self.config.uniswap_v3_quoter_is_v2 {
            // Uniswap V3 with QuoterV2 (Base): struct params, tuple return
            let quoter_address = self.config.uniswap_v3_quoter
                .ok_or_else(|| anyhow!("V3 Quoter address not configured (UNISWAP_V3_QUOTER)"))?;
            let quoter = IQuoterV2::new(quoter_address, self.provider.clone());
            let params = QuoteExactInputSingleParams {
                token_in,
                token_out,
                amount_in,
                fee: fee.into(),
                sqrt_price_limit_x96: U256::zero(),
            };
            let (amount_out, _, _, _) = quoter
                .quote_exact_input_single(params)
                .call()
                .await
                .map_err(|e| anyhow!("V3 QuoterV2 simulation failed: {} — pool may lack liquidity", e))?;
            amount_out
        } else {
            // Uniswap V3: use QuoterV1 (flat params, single return) — Polygon
            let quoter_address = self.config.uniswap_v3_quoter
                .ok_or_else(|| anyhow!("V3 Quoter address not configured (UNISWAP_V3_QUOTER)"))?;
            let quoter = IQuoter::new(quoter_address, self.provider.clone());
            quoter
                .quote_exact_input_single(
                    token_in,
                    token_out,
                    fee,
                    amount_in,
                    U256::zero(), // sqrtPriceLimitX96 = 0 (no limit)
                )
                .call()
                .await
                .map_err(|e| anyhow!("V3 Quoter simulation failed: {} — pool may lack liquidity", e))?
        };

        info!(
            "V3 Quoter ({:?}): {} in → {} out (expected min: {})",
            dex, amount_in, quoted_out, expected_min_out
        );

        // Safety check: quoted output must meet our minimum
        if quoted_out < expected_min_out {
            return Err(anyhow!(
                "V3 Quoter: output {} < min_out {} — pool likely has insufficient liquidity or price moved",
                quoted_out, expected_min_out
            ));
        }

        Ok(quoted_out)
    }

    /// Parse actual amountOut from transaction receipt logs.
    ///
    /// Looks for ERC20 Transfer events where `to = recipient` for the output token.
    /// Works for both V2 and V3 swaps.
    ///
    /// ERC20 Transfer event:
    ///   event Transfer(address indexed from, address indexed to, uint256 value)
    ///   topic0 = 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef
    fn parse_amount_out_from_receipt(
        &self,
        receipt: &TransactionReceipt,
        token_out: Address,
        recipient: Address,
    ) -> Option<U256> {
        // ERC20 Transfer event topic
        let transfer_topic: H256 = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
            .parse()
            .unwrap();

        let recipient_topic = H256::from(recipient);

        // Find Transfer events for the output token to our wallet
        for log in &receipt.logs {
            // Must be from the output token contract
            if log.address != token_out {
                continue;
            }
            // Must have Transfer event signature
            if log.topics.is_empty() || log.topics[0] != transfer_topic {
                continue;
            }
            // topic[2] = `to` address (indexed)
            if log.topics.len() < 3 || log.topics[2] != recipient_topic {
                continue;
            }
            // data = uint256 value
            if log.data.len() >= 32 {
                let amount = U256::from_big_endian(&log.data[..32]);
                debug!("Parsed amountOut from Transfer event: {}", amount);
                return Some(amount);
            }
        }

        warn!("Could not parse amountOut from receipt logs — using min_amount_out as fallback");
        None
    }

    /// Convert Wei to USD based on token pair
    fn wei_to_usd(&self, wei: U256, pair_symbol: &str) -> f64 {
        let wei_f = wei.low_u128() as f64;

        if pair_symbol.starts_with("WETH") {
            (wei_f / 1e18) * 3300.0
        } else if pair_symbol.starts_with("WMATIC") {
            (wei_f / 1e18) * 0.50
        } else {
            wei_f / 1e18
        }
    }
}
