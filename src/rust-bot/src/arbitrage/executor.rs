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
//! Modified: 2026-02-01 (Migrated from ethers-rs to alloy)

use crate::contracts::{
    IArbExecutor, IAlgebraQuoter, IAlgebraSwapRouter, IERC20, IQuoter, IQuoterV2,
    ISwapRouter, IUniswapV2Router02,
};
use crate::tax::{TaxLogger, TaxRecordBuilder};
use crate::types::{ArbitrageOpportunity, BotConfig, DexType, TradeResult};
use alloy::eips::Encodable2718;
use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::{Address, B256, U256, keccak256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use alloy::sol_types::SolCall;
use anyhow::{anyhow, Result};
use rust_decimal::Decimal;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

/// Helper: convert u32 fee tier to alloy U24 (same as in v3_syncer.rs)
fn fee_to_u24(fee: u32) -> alloy::primitives::Uint<24, 1> {
    alloy::primitives::Uint::from(fee as u16)
}

/// Result of a successful atomic tx submission (before receipt).
/// Used by parallel execution: submit N txs fast, then wait for all receipts.
#[derive(Debug, Clone)]
pub struct SubmitResult {
    /// Hash of the submitted transaction
    pub tx_hash: B256,
    /// Nonce used for this transaction
    pub nonce: u64,
    /// When submission started (for execution time measurement)
    pub start_time: Instant,
    /// Pair symbol for logging and cooldown tracking
    pub pair_symbol: String,
    /// Trade size in raw token units
    pub trade_size: U256,
    /// Quote token decimals (for profit parsing)
    pub quote_decimals: u8,
    /// ArbExecutor contract address (for event parsing)
    pub arb_address: Address,
    /// Native token price for gas cost calculation
    pub native_token_price_usd: f64,
}

/// Trade executor for DEX arbitrage
pub struct TradeExecutor<P: Provider> {
    provider: Arc<P>,
    wallet: PrivateKeySigner,
    config: BotConfig,
    /// Dry run mode - simulates trades without executing
    dry_run: bool,
    /// Tax logger for IRS compliance
    tax_logger: Option<TaxLogger>,
    /// Price oracle for USD conversions
    tax_record_builder: Option<TaxRecordBuilder>,
    /// Optional URL for private mempool tx submission.
    /// When set, atomic arb transactions are signed locally, then ONLY the raw
    /// signed bytes are sent through this provider. This avoids burning rate limits on reads.
    tx_client_url: Option<String>,
    /// Cached base_fee_per_gas from latest block header (A1: eliminates get_gas_price RPC).
    /// Set by set_base_fee() from main.rs on each new block.
    /// alloy gas prices are u128.
    cached_base_fee: Option<u128>,
    /// Locally tracked nonce (A2: eliminates nonce lookup from fill_transaction).
    /// Initialized on first use, incremented after each successful send.
    cached_nonce: Arc<AtomicU64>,
    nonce_initialized: bool,
}

impl<P: Provider + 'static> TradeExecutor<P> {
    /// Create a new TradeExecutor
    pub fn new(provider: Arc<P>, wallet: PrivateKeySigner, config: BotConfig) -> Self {
        Self {
            provider,
            wallet,
            config,
            dry_run: true, // Default to dry run for safety
            tax_logger: None,
            tax_record_builder: None,
            tx_client_url: None,
            cached_base_fee: None,
            cached_nonce: Arc::new(AtomicU64::new(0)),
            nonce_initialized: false,
        }
    }

    /// Enable private mempool for transaction submission.
    /// Stores the URL. A bare HTTP provider is created on demand for each send.
    /// Only eth_sendRawTransaction goes through this — all reads (estimateGas,
    /// nonce, gas price) stay on the main WS provider to avoid rate limits.
    pub fn set_private_rpc(&mut self, url: &str) -> Result<()> {
        self.tx_client_url = Some(url.to_string());
        info!("Private mempool enabled: {}", url);
        Ok(())
    }

    /// Update cached base fee from latest block header (A1).
    /// Called from main.rs on each new block. Eliminates get_gas_price() RPC.
    /// In alloy, block.base_fee_per_gas is u128.
    pub fn set_base_fee(&mut self, base_fee: u128) {
        self.cached_base_fee = Some(base_fee);
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

        info!("✅ Buy complete: {:?} | Received: {}", buy_tx_hash, amount_received);

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
                    tx_hash: Some(format!("{:?}", buy_tx_hash)),
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

        info!("✅ Sell complete: {:?} | Final: {}", sell_tx_hash, final_amount);

        // Calculate profit
        let profit_wei = if final_amount > trade_size {
            final_amount - trade_size
        } else {
            U256::ZERO
        };
        let profit_usd = self.wei_to_usd(profit_wei, pair_symbol);

        // Estimate gas cost (actual cost would require receipt analysis)
        // Polygon: ~400k gas for two V3 swaps, ~50 gwei avg = 0.02 MATIC = ~$0.01
        let gas_used_native = 0.02; // ~400k gas at 50 gwei = 0.02 native
        let gas_cost_usd = gas_used_native * self.config.native_token_price_usd;
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
            &format!("{:?}", sell_tx_hash),
            sell_block,
            trade_size,
            final_amount,
            gas_used_native,
        );

        Ok(TradeResult {
            opportunity: pair_symbol.clone(),
            tx_hash: Some(format!("{:?}", sell_tx_hash)),
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

        // A0+A1: Gas priority bump + cached base fee from block header.
        // On Polygon, gas is negligible (~$0.01 even at 5000 gwei). Atomic reverts protect capital.
        // Priority fee of 5000 gwei targets top ~30 block position (median is ~2134 gwei).
        let base_fee: u128 = match self.cached_base_fee {
            Some(bf) => bf,
            None => {
                // Fallback: fetch from RPC (only on first call before any block arrives)
                self.provider.get_gas_price().await?
            }
        };
        let priority_fee: u128 = 5_000_000_000_000; // 5000 gwei
        let max_fee: u128 = base_fee + priority_fee;

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

        // minProfit in raw quote token units (on-chain revert threshold).
        // Phase D: use pre-computed min_profit_raw (dynamic decimals, no hot-path conversion).
        // Backwards compat: if not pre-computed (U256::ZERO), fall back to legacy 1e6 path.
        let min_profit_raw = if opportunity.min_profit_raw > U256::ZERO {
            opportunity.min_profit_raw
        } else {
            let effective_min_profit = if opportunity.min_profit_usd > 0.0 {
                opportunity.min_profit_usd
            } else {
                self.config.min_profit_usd
            };
            U256::from((effective_min_profit * 1e6) as u64)
        };

        info!(
            "  routerBuy={:?} feeBuy={} | routerSell={:?} feeSell={} | amountIn={} | minProfit={}",
            router_buy, fee_buy, router_sell, fee_sell, trade_size, min_profit_raw
        );

        // A2: Initialize nonce on first use, then track locally
        if !self.nonce_initialized {
            let nonce = self.provider.get_transaction_count(self.wallet.address()).await?;
            self.cached_nonce.store(nonce, Ordering::SeqCst);
            self.nonce_initialized = true;
            info!("Nonce initialized: {}", nonce);
        }
        let current_nonce = self.cached_nonce.load(Ordering::SeqCst);

        // Build + sign transaction. Gas price and nonce are pre-set (A0-A2).
        // If private RPC is configured, send only the raw signed bytes through it.
        let send_result: Result<B256, String> = if let Some(ref url) = self.tx_client_url {
            // Private RPC path: encode calldata manually, build tx, sign, send raw.
            info!("📡 Sending via private mempool (priority=5000gwei, nonce={})", current_nonce);

            let call_data = IArbExecutor::executeArbCall {
                token0, token1,
                routerBuy: router_buy, routerSell: router_sell,
                feeBuy: fee_to_u24(fee_buy), feeSell: fee_to_u24(fee_sell),
                amountIn: trade_size, minProfit: min_profit_raw,
            }.abi_encode();

            let mut tx = alloy::rpc::types::TransactionRequest::default()
                .with_to(arb_address)
                .with_input(call_data)
                .with_nonce(current_nonce)
                .with_chain_id(self.config.chain_id)
                .with_max_fee_per_gas(max_fee)
                .with_max_priority_fee_per_gas(priority_fee);

            // Estimate gas via main provider
            let gas_estimate = self.provider.estimate_gas(tx.clone()).await
                .map_err(|e| anyhow!("Atomic tx gas estimate failed: {}", e))?;
            tx.set_gas_limit(gas_estimate);

            // Sign
            let wallet = EthereumWallet::from(self.wallet.clone());
            let signed = tx.build(&wallet).await
                .map_err(|e| anyhow!("Atomic tx sign failed: {}", e))?;
            let raw_tx = signed.encoded_2718();

            // Send via private provider
            let http_provider = ProviderBuilder::new().connect_http(url.parse().unwrap());
            match http_provider.send_raw_transaction(&raw_tx).await {
                Ok(pending) => {
                    self.cached_nonce.fetch_add(1, Ordering::SeqCst);
                    Ok(*pending.tx_hash())
                }
                Err(e) => Err(format!("Atomic tx send failed (private): {}", e))
            }
        } else {
            // Public path: use signing provider with contract call builder.
            let wallet = EthereumWallet::from(self.wallet.clone());
            let signer_provider = ProviderBuilder::new()
                .wallet(wallet)
                .connect_http(self.config.rpc_url.parse().unwrap());
            let contract = IArbExecutor::new(arb_address, &signer_provider);
            contract.executeArb(
                token0, token1, router_buy, router_sell,
                fee_to_u24(fee_buy), fee_to_u24(fee_sell), trade_size, min_profit_raw,
            )
            .nonce(current_nonce)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .send()
            .await
            .map(|builder| {
                self.cached_nonce.fetch_add(1, Ordering::SeqCst);
                *builder.tx_hash()
            })
            .map_err(|e| format!("Atomic tx send failed: {}", e))
        };

        let tx_hash = match send_result {
            Ok(hash) => hash,
            Err(err_msg) => {
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

        info!("⚡ Atomic arb tx submitted: {:?}", tx_hash);

        // Wait for receipt using main provider (WS — fast block notifications).
        // Polls get_transaction_receipt since PendingTransaction types differ
        // between WS and HTTP providers (Rust generics constraint).
        // Timeout after 30s (~15 Polygon blocks) to avoid blocking the main loop.
        let receipt_deadline = Instant::now() + Duration::from_secs(30);
        let receipt = loop {
            match self.provider.get_transaction_receipt(tx_hash).await {
                Ok(Some(r)) => break r,
                Ok(None) => {
                    if Instant::now() > receipt_deadline {
                        error!("Receipt timeout (30s) for tx {:?} — tx may still confirm later", tx_hash);
                        return Ok(TradeResult {
                            opportunity: pair_symbol.clone(),
                            tx_hash: Some(format!("{:?}", tx_hash)),
                            block_number: None,
                            success: false,
                            profit_usd: 0.0,
                            gas_cost_usd: 0.0,
                            gas_used_native: 0.0,
                            net_profit_usd: 0.0,
                            execution_time_ms: start_time.elapsed().as_millis() as u64,
                            error: Some("Receipt timeout — tx submitted but unconfirmed".to_string()),
                            amount_in: Some(trade_size.to_string()),
                            amount_out: None,
                        });
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                }
                Err(e) => return Err(anyhow!("Failed to fetch receipt for {:?}: {}", tx_hash, e)),
            }
        };

        let block_number = receipt.block_number.unwrap_or(0);

        if !receipt.status() {
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
        let mut profit_raw = U256::ZERO;
        let mut amount_out = trade_size; // fallback
        let arb_executed_topic: B256 = keccak256(
            b"ArbExecuted(address,address,uint256,uint256,uint256,address,address)"
        );

        for log in receipt.inner.logs() {
            if log.inner.address == arb_address
                && !log.inner.data.topics().is_empty()
                && log.inner.data.topics()[0] == arb_executed_topic
            {
                // data layout: amountIn (32) | amountOut (32) | profit (32) | routerBuy (32) | routerSell (32)
                if log.inner.data.data.len() >= 96 {
                    amount_out = U256::from_be_slice(&log.inner.data.data[32..64]);
                    profit_raw = U256::from_be_slice(&log.inner.data.data[64..96]);
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
        let profit_usd = profit_raw.to::<u128>() as f64 / 10_f64.powi(quote_decimals as i32);
        // Actual gas from receipt
        let gas_used = receipt.gas_used as u128;
        let effective_gas_price = receipt.effective_gas_price;
        let gas_cost_wei = gas_used * effective_gas_price;
        let gas_used_native = gas_cost_wei as f64 / 1e18;
        let gas_cost_usd = gas_used_native * self.config.native_token_price_usd;
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

    /// Submit an atomic arb tx and return immediately (no receipt wait).
    ///
    /// Returns SubmitResult on successful send, or TradeResult error on pre-trade rejection.
    /// Used by parallel submission: submit N txs sequentially (fast, ~5ms each),
    /// then wait for all receipts in parallel via wait_for_atomic_receipt().
    ///
    /// The existing execute() and execute_atomic() methods are unchanged for backwards
    /// compatibility. Use submit_atomic() + wait_for_atomic_receipt() only when
    /// MAX_PARALLEL_SUBMISSIONS > 1.
    pub async fn submit_atomic(
        &mut self,
        opportunity: &ArbitrageOpportunity,
    ) -> Result<SubmitResult, TradeResult> {
        let start_time = Instant::now();
        let pair_symbol = &opportunity.pair.symbol;
        let arb_address = match self.config.arb_executor_address {
            Some(addr) => addr,
            None => {
                return Err(TradeResult {
                    opportunity: pair_symbol.clone(),
                    tx_hash: None, block_number: None, success: false,
                    profit_usd: 0.0, gas_cost_usd: 0.0, gas_used_native: 0.0, net_profit_usd: 0.0,
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some("No ARB_EXECUTOR_ADDRESS configured".to_string()),
                    amount_in: Some(opportunity.trade_size.to_string()), amount_out: None,
                });
            }
        };

        if self.dry_run {
            return Err(TradeResult {
                opportunity: pair_symbol.clone(),
                tx_hash: None, block_number: None, success: false,
                profit_usd: 0.0, gas_cost_usd: 0.0, gas_used_native: 0.0, net_profit_usd: 0.0,
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some("submit_atomic not available in dry_run mode".to_string()),
                amount_in: Some(opportunity.trade_size.to_string()), amount_out: None,
            });
        }

        let mode = if opportunity.buy_dex.is_v2() || opportunity.sell_dex.is_v2() {
            "V2↔V3"
        } else {
            "V3↔V3"
        };
        info!(
            "⚡ SUBMIT ATOMIC {} : {} | Buy {:?} → Sell {:?} via ArbExecutor {:?}",
            mode, pair_symbol, opportunity.buy_dex, opportunity.sell_dex, arb_address
        );

        // Gas pricing (same as execute_atomic)
        let base_fee: u128 = match self.cached_base_fee {
            Some(bf) => bf,
            None => self.provider.get_gas_price().await
                .map_err(|e| TradeResult {
                    opportunity: pair_symbol.clone(),
                    tx_hash: None, block_number: None, success: false,
                    profit_usd: 0.0, gas_cost_usd: 0.0, gas_used_native: 0.0, net_profit_usd: 0.0,
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(format!("Gas price fetch failed: {}", e)),
                    amount_in: Some(opportunity.trade_size.to_string()), amount_out: None,
                })?,
        };
        let priority_fee: u128 = 5_000_000_000_000; // 5000 gwei
        let max_fee: u128 = base_fee + priority_fee;

        // Token ordering
        let (token0, token1) = if opportunity.quote_token_is_token0 {
            (opportunity.pair.token0, opportunity.pair.token1)
        } else {
            (opportunity.pair.token1, opportunity.pair.token0)
        };
        let trade_size = opportunity.trade_size;

        let router_buy = self.get_router_address(opportunity.buy_dex);
        let router_sell = self.get_router_address(opportunity.sell_dex);
        let fee_buy = opportunity.buy_dex.atomic_fee();
        let fee_sell = opportunity.sell_dex.atomic_fee();

        // minProfit
        let min_profit_raw = if opportunity.min_profit_raw > U256::ZERO {
            opportunity.min_profit_raw
        } else {
            let effective_min_profit = if opportunity.min_profit_usd > 0.0 {
                opportunity.min_profit_usd
            } else {
                self.config.min_profit_usd
            };
            U256::from((effective_min_profit * 1e6) as u64)
        };

        // Nonce
        if !self.nonce_initialized {
            let nonce = self.provider.get_transaction_count(self.wallet.address()).await
                .map_err(|e| TradeResult {
                    opportunity: pair_symbol.clone(),
                    tx_hash: None, block_number: None, success: false,
                    profit_usd: 0.0, gas_cost_usd: 0.0, gas_used_native: 0.0, net_profit_usd: 0.0,
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(format!("Nonce fetch failed: {}", e)),
                    amount_in: Some(trade_size.to_string()), amount_out: None,
                })?;
            self.cached_nonce.store(nonce, Ordering::SeqCst);
            self.nonce_initialized = true;
            info!("Nonce initialized: {}", nonce);
        }
        let current_nonce = self.cached_nonce.load(Ordering::SeqCst);

        // Build and send transaction
        let send_result: Result<B256, String> = if let Some(ref url) = self.tx_client_url {
            info!("📡 PARALLEL SUBMIT via private mempool (nonce={})", current_nonce);

            let call_data = IArbExecutor::executeArbCall {
                token0, token1,
                routerBuy: router_buy, routerSell: router_sell,
                feeBuy: fee_to_u24(fee_buy), feeSell: fee_to_u24(fee_sell),
                amountIn: trade_size, minProfit: min_profit_raw,
            }.abi_encode();

            let mut tx = alloy::rpc::types::TransactionRequest::default()
                .with_to(arb_address)
                .with_input(call_data)
                .with_nonce(current_nonce)
                .with_chain_id(self.config.chain_id)
                .with_max_fee_per_gas(max_fee)
                .with_max_priority_fee_per_gas(priority_fee);

            let gas_estimate = self.provider.estimate_gas(tx.clone()).await
                .map_err(|e| format!("Gas estimate failed: {}", e));
            let gas_estimate = match gas_estimate {
                Ok(g) => g,
                Err(msg) => return Err(self.submit_error(&pair_symbol, &msg, start_time, trade_size)),
            };
            tx.set_gas_limit(gas_estimate);

            let wallet = EthereumWallet::from(self.wallet.clone());
            let signed = tx.build(&wallet).await
                .map_err(|e| format!("Sign failed: {}", e));
            let signed = match signed {
                Ok(s) => s,
                Err(msg) => return Err(self.submit_error(&pair_symbol, &msg, start_time, trade_size)),
            };
            let raw_tx = signed.encoded_2718();

            let http_provider = ProviderBuilder::new().connect_http(url.parse().unwrap());
            match http_provider.send_raw_transaction(&raw_tx).await {
                Ok(pending) => {
                    self.cached_nonce.fetch_add(1, Ordering::SeqCst);
                    Ok(*pending.tx_hash())
                }
                Err(e) => Err(format!("Send failed (private): {}", e))
            }
        } else {
            let wallet = EthereumWallet::from(self.wallet.clone());
            let signer_provider = ProviderBuilder::new()
                .wallet(wallet)
                .connect_http(self.config.rpc_url.parse().unwrap());
            let contract = IArbExecutor::new(arb_address, &signer_provider);
            contract.executeArb(
                token0, token1, router_buy, router_sell,
                fee_to_u24(fee_buy), fee_to_u24(fee_sell), trade_size, min_profit_raw,
            )
            .nonce(current_nonce)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .send()
            .await
            .map(|builder| {
                self.cached_nonce.fetch_add(1, Ordering::SeqCst);
                *builder.tx_hash()
            })
            .map_err(|e| format!("Send failed: {}", e))
        };

        let tx_hash = match send_result {
            Ok(hash) => hash,
            Err(err_msg) => {
                if err_msg.contains("InsufficientProfit") || err_msg.contains("execution reverted") {
                    info!("Atomic arb reverted at submission (insufficient profit)");
                } else {
                    error!("{}", err_msg);
                }
                return Err(self.submit_error(&pair_symbol, &err_msg, start_time, trade_size));
            }
        };

        info!("⚡ Atomic arb submitted: {:?} (nonce={})", tx_hash, current_nonce);

        let quote_decimals = if opportunity.quote_token_is_token0 {
            opportunity.token0_decimals
        } else {
            opportunity.token1_decimals
        };

        Ok(SubmitResult {
            tx_hash,
            nonce: current_nonce,
            start_time,
            pair_symbol: pair_symbol.clone(),
            trade_size,
            quote_decimals,
            arb_address,
            native_token_price_usd: self.config.native_token_price_usd,
        })
    }

    /// Check if executor is in dry run mode (parallel path needs to know).
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Helper to build a TradeResult error for submit_atomic() failure paths.
    fn submit_error(&self, pair: &str, msg: &str, start: Instant, trade_size: U256) -> TradeResult {
        TradeResult {
            opportunity: pair.to_string(),
            tx_hash: None, block_number: None, success: false,
            profit_usd: 0.0, gas_cost_usd: 0.0, gas_used_native: 0.0, net_profit_usd: 0.0,
            execution_time_ms: start.elapsed().as_millis() as u64,
            error: Some(msg.to_string()),
            amount_in: Some(trade_size.to_string()), amount_out: None,
        }
    }

    /// Expose the provider Arc for use by wait_for_atomic_receipt().
    /// Called from main.rs to get a cloneable provider handle for spawning receipt tasks.
    pub fn provider_arc(&self) -> Arc<P> {
        self.provider.clone()
    }

    /// Execute an atomic arbitrage from a mempool signal (Phase 3).
    ///
    /// Structurally identical to execute_atomic() with three key differences:
    /// 1. Skips estimateGas — uses fixed gas limit (500K) for speed
    /// 2. Dynamic gas pricing — priority fee scales with trigger tx + expected profit
    /// 3. Lower minProfit threshold — mempool signals have higher conviction
    ///
    /// Safety: ArbExecutor.sol reverts if profit < minProfit. Only gas (~$0.01) at risk.
    ///
    /// Note: trigger_gas_price and trigger_max_priority_fee are U256 from MempoolSignal.
    /// They are converted to u128 internally for gas arithmetic.
    pub async fn execute_from_mempool(
        &mut self,
        opportunity: &ArbitrageOpportunity,
        trigger_gas_price: U256,
        trigger_max_priority_fee: Option<U256>,
        mempool_min_profit_usd: f64,
    ) -> Result<TradeResult> {
        let start_time = Instant::now();
        let pair_symbol = &opportunity.pair.symbol;
        let arb_address = match self.config.arb_executor_address {
            Some(addr) => addr,
            None => {
                return Ok(TradeResult {
                    opportunity: pair_symbol.clone(),
                    tx_hash: None, block_number: None, success: false,
                    profit_usd: 0.0, gas_cost_usd: 0.0, gas_used_native: 0.0,
                    net_profit_usd: 0.0,
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some("No ARB_EXECUTOR_ADDRESS configured".to_string()),
                    amount_in: None, amount_out: None,
                });
            }
        };

        if self.dry_run {
            return self.simulate_execution(opportunity, start_time).await;
        }

        let mode = if opportunity.buy_dex.is_v2() || opportunity.sell_dex.is_v2() {
            "V2↔V3"
        } else {
            "V3↔V3"
        };
        info!(
            "⚡ MEMPOOL {} exec: {} | Buy {:?} → Sell {:?} | est=${:.2}",
            mode, pair_symbol, opportunity.buy_dex, opportunity.sell_dex,
            opportunity.estimated_profit
        );

        // Dynamic gas pricing (Phase 3: profit-aware bidding)
        // Convert U256 trigger values to u128 for gas arithmetic
        let trigger_gas_price_u128: u128 = trigger_gas_price.to::<u128>();
        let trigger_max_priority_fee_u128: Option<u128> = trigger_max_priority_fee
            .map(|v| v.to::<u128>());

        let base_fee: u128 = match self.cached_base_fee {
            Some(bf) => bf,
            None => self.provider.get_gas_price().await?,
        };
        let (priority_fee, max_fee) = self.calculate_mempool_gas(
            trigger_gas_price_u128,
            trigger_max_priority_fee_u128,
            opportunity.estimated_profit,
            base_fee,
        );

        // Token ordering: token0 = quote (USDC), token1 = base
        let (token0, token1) = if opportunity.quote_token_is_token0 {
            (opportunity.pair.token0, opportunity.pair.token1)
        } else {
            (opportunity.pair.token1, opportunity.pair.token0)
        };
        let trade_size = opportunity.trade_size;

        let router_buy = self.get_router_address(opportunity.buy_dex);
        let router_sell = self.get_router_address(opportunity.sell_dex);
        let fee_buy = opportunity.buy_dex.atomic_fee();
        let fee_sell = opportunity.sell_dex.atomic_fee();

        // minProfit: use pre-computed raw value (dynamic decimals) if available,
        // else fall back to legacy 1e6 path for backwards compat.
        let min_profit_raw = if opportunity.min_profit_raw > U256::ZERO {
            opportunity.min_profit_raw
        } else {
            U256::from((mempool_min_profit_usd * 1e6) as u64)
        };

        let gas_limit: u64 = self.config.mempool_gas_limit;

        info!(
            "  MEMPOOL TX: routerBuy={:?} feeBuy={} | routerSell={:?} feeSell={} | amt={} | minProfit={} | gas={}K priority={:.0}gwei",
            router_buy, fee_buy, router_sell, fee_sell, trade_size, min_profit_raw,
            gas_limit / 1000,
            priority_fee as f64 / 1e9,
        );

        // Initialize nonce if needed
        if !self.nonce_initialized {
            let nonce = self.provider.get_transaction_count(self.wallet.address()).await?;
            self.cached_nonce.store(nonce, Ordering::SeqCst);
            self.nonce_initialized = true;
            info!("Nonce initialized: {}", nonce);
        }
        let current_nonce = self.cached_nonce.load(Ordering::SeqCst);

        // Encode calldata manually for both paths
        let call_data = IArbExecutor::executeArbCall {
            token0, token1,
            routerBuy: router_buy, routerSell: router_sell,
            feeBuy: fee_to_u24(fee_buy), feeSell: fee_to_u24(fee_sell),
            amountIn: trade_size, minProfit: min_profit_raw,
        }.abi_encode();

        // Build tx manually — skip estimateGas for speed
        let send_result: Result<B256, String> = if let Some(ref url) = self.tx_client_url {
            // Private RPC path: pre-set all fields, sign, send raw
            info!("📡 MEMPOOL: private RPC (priority={:.0}gwei, nonce={}, gas={}K)",
                  priority_fee as f64 / 1e9, current_nonce, gas_limit / 1000);

            let mut tx = alloy::rpc::types::TransactionRequest::default()
                .with_to(arb_address)
                .with_input(call_data.clone())
                .with_nonce(current_nonce)
                .with_chain_id(self.config.chain_id)
                .with_max_fee_per_gas(max_fee)
                .with_max_priority_fee_per_gas(priority_fee);
            tx.set_gas_limit(gas_limit);

            // Sign directly — no estimateGas (fixed gas limit for speed)
            let wallet = EthereumWallet::from(self.wallet.clone());
            let signed = tx.build(&wallet).await
                .map_err(|e| anyhow!("Mempool tx sign failed: {}", e))?;
            let raw_tx = signed.encoded_2718();

            let http_provider = ProviderBuilder::new().connect_http(url.parse().unwrap());
            match http_provider.send_raw_transaction(&raw_tx).await {
                Ok(pending) => {
                    self.cached_nonce.fetch_add(1, Ordering::SeqCst);
                    Ok(*pending.tx_hash())
                }
                Err(e) => Err(format!("Mempool tx send failed (private): {}", e))
            }
        } else {
            // Public path: sign and send raw via main provider
            info!("📡 MEMPOOL: public RPC (priority={:.0}gwei, nonce={})",
                  priority_fee as f64 / 1e9, current_nonce);

            let mut tx = alloy::rpc::types::TransactionRequest::default()
                .with_to(arb_address)
                .with_input(call_data)
                .with_nonce(current_nonce)
                .with_chain_id(self.config.chain_id)
                .with_max_fee_per_gas(max_fee)
                .with_max_priority_fee_per_gas(priority_fee);
            tx.set_gas_limit(gas_limit);

            let wallet = EthereumWallet::from(self.wallet.clone());
            let signed = tx.build(&wallet).await
                .map_err(|e| anyhow!("Mempool tx sign failed: {}", e))?;
            let raw_tx = signed.encoded_2718();

            match self.provider.send_raw_transaction(&raw_tx).await {
                Ok(pending) => {
                    self.cached_nonce.fetch_add(1, Ordering::SeqCst);
                    Ok(*pending.tx_hash())
                }
                Err(e) => Err(format!("Mempool tx send failed: {}", e))
            }
        };

        let tx_hash = match send_result {
            Ok(hash) => hash,
            Err(err_msg) => {
                if err_msg.contains("InsufficientProfit") || err_msg.contains("execution reverted") {
                    info!("MEMPOOL: atomic revert (expected — pool conditions changed)");
                } else {
                    error!("MEMPOOL: {}", err_msg);
                }
                return Ok(TradeResult {
                    opportunity: pair_symbol.clone(),
                    tx_hash: None, block_number: None, success: false,
                    profit_usd: 0.0, gas_cost_usd: 0.0, gas_used_native: 0.0,
                    net_profit_usd: 0.0,
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(err_msg),
                    amount_in: Some(trade_size.to_string()), amount_out: None,
                });
            }
        };

        info!("⚡ MEMPOOL tx submitted: {:?} ({}ms from signal)", tx_hash, start_time.elapsed().as_millis());

        // Wait for receipt (identical to execute_atomic)
        let receipt_deadline = Instant::now() + Duration::from_secs(30);
        let receipt = loop {
            match self.provider.get_transaction_receipt(tx_hash).await {
                Ok(Some(r)) => break r,
                Ok(None) => {
                    if Instant::now() > receipt_deadline {
                        error!("MEMPOOL: receipt timeout (30s) for {:?}", tx_hash);
                        return Ok(TradeResult {
                            opportunity: pair_symbol.clone(),
                            tx_hash: Some(format!("{:?}", tx_hash)),
                            block_number: None, success: false,
                            profit_usd: 0.0, gas_cost_usd: 0.0, gas_used_native: 0.0,
                            net_profit_usd: 0.0,
                            execution_time_ms: start_time.elapsed().as_millis() as u64,
                            error: Some("Receipt timeout — tx submitted but unconfirmed".to_string()),
                            amount_in: Some(trade_size.to_string()), amount_out: None,
                        });
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                }
                Err(e) => return Err(anyhow!("Failed to fetch receipt for {:?}: {}", tx_hash, e)),
            }
        };

        let block_number = receipt.block_number.unwrap_or(0);

        if !receipt.status() {
            warn!("MEMPOOL: tx reverted on-chain (gas burned, no capital loss)");
            return Ok(TradeResult {
                opportunity: pair_symbol.clone(),
                tx_hash: Some(format!("{:?}", tx_hash)),
                block_number: Some(block_number), success: false,
                profit_usd: 0.0, gas_cost_usd: 0.0, gas_used_native: 0.0,
                net_profit_usd: 0.0,
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some("Mempool tx reverted on-chain".to_string()),
                amount_in: Some(trade_size.to_string()), amount_out: None,
            });
        }

        // Parse profit from ArbExecuted event (identical to execute_atomic)
        let mut profit_raw = U256::ZERO;
        let mut amount_out = trade_size;
        let arb_executed_topic: B256 = keccak256(
            b"ArbExecuted(address,address,uint256,uint256,uint256,address,address)"
        );

        for log in receipt.inner.logs() {
            if log.inner.address == arb_address
                && !log.inner.data.topics().is_empty()
                && log.inner.data.topics()[0] == arb_executed_topic
            {
                if log.inner.data.data.len() >= 96 {
                    amount_out = U256::from_be_slice(&log.inner.data.data[32..64]);
                    profit_raw = U256::from_be_slice(&log.inner.data.data[64..96]);
                    debug!("MEMPOOL: ArbExecuted amountOut={}, profit={}", amount_out, profit_raw);
                }
                break;
            }
        }

        let quote_decimals = if opportunity.quote_token_is_token0 {
            opportunity.token0_decimals
        } else {
            opportunity.token1_decimals
        };
        let profit_usd = profit_raw.to::<u128>() as f64 / 10_f64.powi(quote_decimals as i32);
        let gas_used = receipt.gas_used as u128;
        let effective_gas_price = receipt.effective_gas_price;
        let gas_cost_wei = gas_used * effective_gas_price;
        let gas_used_native = gas_cost_wei as f64 / 1e18;
        let gas_cost_usd = gas_used_native * self.config.native_token_price_usd;
        let net_profit_usd = profit_usd - gas_cost_usd;

        let success = net_profit_usd > 0.0;

        if success {
            info!(
                "🎉 MEMPOOL PROFIT: ${:.4} (gross: ${:.4}, gas: ${:.4}) | tx: {:?}",
                net_profit_usd, profit_usd, gas_cost_usd, tx_hash
            );
        } else {
            warn!(
                "📉 MEMPOOL LOSS: ${:.4} (gross: ${:.4}, gas: ${:.4}) | tx: {:?}",
                net_profit_usd, profit_usd, gas_cost_usd, tx_hash
            );
        }

        // Tax logging
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

    /// Calculate dynamic gas pricing for mempool-sourced transactions.
    ///
    /// Strategy:
    /// - match_trigger: trigger_priority * 1.05 (land right after trigger tx)
    /// - profit_cap: never spend > gas_profit_cap fraction of expected profit
    /// - min_priority: competitive floor (configurable, default 1000 gwei on Polygon)
    /// - final = min(profit_cap, max(match_trigger, min_priority))
    ///
    /// Returns (priority_fee, max_fee) both in wei as u128.
    fn calculate_mempool_gas(
        &self,
        trigger_gas_price: u128,
        trigger_max_priority_fee: Option<u128>,
        est_profit_usd: f64,
        base_fee: u128,
    ) -> (u128, u128) {
        let gwei: u128 = 1_000_000_000;
        let min_priority_gwei = self.config.mempool_min_priority_gwei as u128;
        let gas_profit_cap = self.config.mempool_gas_profit_cap;
        let gas_limit = self.config.mempool_gas_limit as u128;

        // 1. Match trigger: trigger's priority fee * 1.05
        let trigger_priority = trigger_max_priority_fee.unwrap_or_else(|| {
            // Legacy tx: estimate priority as gas_price - base_fee
            if trigger_gas_price > base_fee {
                trigger_gas_price - base_fee
            } else {
                gwei * min_priority_gwei // fallback to floor
            }
        });
        // * 1.05 = * 105 / 100
        let match_trigger = trigger_priority * 105 / 100;

        // 2. Profit cap: max gas spend = est_profit * gas_profit_cap / native_token_price / gas_limit
        // Convert: profit_usd * cap → max_gas_cost_usd → max_gas_cost_native → max_gas_per_unit
        let native_price = self.config.native_token_price_usd;
        let max_gas_budget_usd = est_profit_usd * gas_profit_cap;
        let max_gas_budget_matic = max_gas_budget_usd / native_price;
        let max_gas_budget_wei = (max_gas_budget_matic * 1e18) as u128;
        let profit_cap = if gas_limit > 0 {
            max_gas_budget_wei / gas_limit
        } else {
            gwei * min_priority_gwei
        };

        // 3. Floor
        let floor = gwei * min_priority_gwei;

        // 4. Final: min(profit_cap, max(match_trigger, floor))
        let candidate = std::cmp::max(match_trigger, floor);
        let priority_fee = std::cmp::min(profit_cap, candidate);

        // Max fee = base_fee + priority_fee
        let max_fee = base_fee + priority_fee;

        debug!(
            "MEMPOOL GAS: trigger_priority={:.0}gwei match={:.0}gwei cap={:.0}gwei floor={:.0}gwei → final={:.0}gwei maxfee={:.0}gwei",
            trigger_priority as f64 / 1e9,
            match_trigger as f64 / 1e9,
            profit_cap as f64 / 1e9,
            floor as f64 / 1e9,
            priority_fee as f64 / 1e9,
            max_fee as f64 / 1e9,
        );

        (priority_fee, max_fee)
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
            profit_usd: opportunity.estimated_profit + self.config.native_token_price_usd, // Add back gas for simulation
            gas_cost_usd: self.config.native_token_price_usd,
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
    ) -> Result<(B256, U256, u64)> {
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
    ) -> Result<(B256, U256, u64)> {
        let router_address = self.get_router_address(dex);

        // Create signing provider
        let wallet = EthereumWallet::from(self.wallet.clone());
        let signer_provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.config.rpc_url.parse().unwrap());

        let router = IUniswapV2Router02::new(router_address, &signer_provider);

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
        let call = router.swapExactTokensForTokens(
            amount_in,
            min_amount_out,
            path,
            wallet_address,
            U256::from(deadline),
        );

        let pending = call.send().await.map_err(|e| anyhow!("Send failed: {}", e))?;
        let tx_hash = *pending.tx_hash();

        info!("V2 swap tx submitted: {:?}", tx_hash);

        // Wait for confirmation
        let receipt = pending
            .get_receipt()
            .await
            .map_err(|e| anyhow!("Confirmation failed: {}", e))?;

        if !receipt.status() {
            return Err(anyhow!("V2 transaction reverted"));
        }

        // Extract block number for tax logging
        let block_number = receipt.block_number.unwrap_or(0);

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
    ) -> Result<(B256, U256, u64)> {
        let router_address = self.get_router_address(dex);

        // Create signing provider
        let wallet = EthereumWallet::from(self.wallet.clone());
        let signer_provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.config.rpc_url.parse().unwrap());

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

            let router = IAlgebraSwapRouter::new(router_address, &signer_provider);
            let params = IAlgebraSwapRouter::ExactInputSingleParams {
                tokenIn: token_in,
                tokenOut: token_out,
                recipient: wallet_address,
                deadline: U256::from(deadline),
                amountIn: amount_in,
                amountOutMinimum: min_amount_out,
                limitSqrtPrice: alloy::primitives::Uint::<160, 3>::ZERO,
            };
            let call = router.exactInputSingle(params);
            let pending = call.send().await.map_err(|e| anyhow!("Algebra V3 send failed: {}", e))?;
            let tx_hash = *pending.tx_hash();
            info!("V3 swap tx submitted: {:?} ({:?})", tx_hash, dex);

            let receipt = pending
                .get_receipt()
                .await
                .map_err(|e| anyhow!("V3 confirmation failed: {}", e))?;

            if !receipt.status() {
                return Err(anyhow!("V3 transaction reverted"));
            }
            let block_number = receipt.block_number.unwrap_or(0);
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

            let router = ISwapRouter::new(router_address, &signer_provider);
            let params = ISwapRouter::ExactInputSingleParams {
                tokenIn: token_in,
                tokenOut: token_out,
                fee: fee_to_u24(fee),
                recipient: wallet_address,
                deadline: U256::from(deadline),
                amountIn: amount_in,
                amountOutMinimum: min_amount_out,
                sqrtPriceLimitX96: alloy::primitives::Uint::<160, 3>::ZERO,
            };
            let call = router.exactInputSingle(params);
            let pending = call.send().await.map_err(|e| anyhow!("V3 send failed: {}", e))?;
            let tx_hash = *pending.tx_hash();
            info!("V3 swap tx submitted: {:?} ({:?})", tx_hash, dex);

            let receipt = pending
                .get_receipt()
                .await
                .map_err(|e| anyhow!("V3 confirmation failed: {}", e))?;

            if !receipt.status() {
                return Err(anyhow!("V3 transaction reverted"));
            }
            let block_number = receipt.block_number.unwrap_or(0);
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

        // Create signing provider
        let wallet = EthereumWallet::from(self.wallet.clone());
        let signer_provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.config.rpc_url.parse().unwrap());

        let token_contract = IERC20::new(token, &signer_provider);
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
        let call = token_contract.approve(router_address, max_approval);
        let pending = call.send().await.map_err(|e| anyhow!("Approval send failed: {}", e))?;

        let receipt = pending.get_receipt().await.map_err(|e| anyhow!("Approval confirmation failed: {}", e))?;

        if !receipt.status() {
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
        let amount_in_human = amount_in.to::<u128>() as f64 / 10_f64.powi(in_decimals as i32);

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
            return U256::ZERO;
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
            let result = quoter
                .quoteExactInputSingle(
                    token_in,
                    token_out,
                    amount_in,
                    alloy::primitives::Uint::<160, 3>::ZERO, // limitSqrtPrice = 0 (no limit)
                )
                .call()
                .await
                .map_err(|e| anyhow!("Algebra Quoter simulation failed: {} — pool may lack liquidity", e))?;
            result.amountOut
        } else if dex.is_sushi_v3() {
            // SushiSwap V3: use QuoterV2 (struct param, tuple return)
            let quoter_address = self.config.sushiswap_v3_quoter
                .ok_or_else(|| anyhow!("SushiSwap V3 Quoter not configured (SUSHISWAP_V3_QUOTER)"))?;
            let quoter = IQuoterV2::new(quoter_address, self.provider.clone());
            let params = IQuoterV2::QuoteExactInputSingleParams {
                tokenIn: token_in,
                tokenOut: token_out,
                amountIn: amount_in,
                fee: fee_to_u24(fee),
                sqrtPriceLimitX96: alloy::primitives::Uint::<160, 3>::ZERO,
            };
            let result = quoter
                .quoteExactInputSingle(params)
                .call()
                .await
                .map_err(|e| anyhow!("SushiV3 QuoterV2 simulation failed: {} — pool may lack liquidity", e))?;
            result.amountOut
        } else if self.config.uniswap_v3_quoter_is_v2 {
            // Uniswap V3 with QuoterV2 (Base): struct params, tuple return
            let quoter_address = self.config.uniswap_v3_quoter
                .ok_or_else(|| anyhow!("V3 Quoter address not configured (UNISWAP_V3_QUOTER)"))?;
            let quoter = IQuoterV2::new(quoter_address, self.provider.clone());
            let params = IQuoterV2::QuoteExactInputSingleParams {
                tokenIn: token_in,
                tokenOut: token_out,
                amountIn: amount_in,
                fee: fee_to_u24(fee),
                sqrtPriceLimitX96: alloy::primitives::Uint::<160, 3>::ZERO,
            };
            let result = quoter
                .quoteExactInputSingle(params)
                .call()
                .await
                .map_err(|e| anyhow!("V3 QuoterV2 simulation failed: {} — pool may lack liquidity", e))?;
            result.amountOut
        } else {
            // Uniswap V3: use QuoterV1 (flat params, single return) — Polygon
            let quoter_address = self.config.uniswap_v3_quoter
                .ok_or_else(|| anyhow!("V3 Quoter address not configured (UNISWAP_V3_QUOTER)"))?;
            let quoter = IQuoter::new(quoter_address, self.provider.clone());
            quoter
                .quoteExactInputSingle(
                    token_in,
                    token_out,
                    fee_to_u24(fee),
                    amount_in,
                    alloy::primitives::Uint::<160, 3>::ZERO, // sqrtPriceLimitX96 = 0 (no limit)
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
        receipt: &alloy::rpc::types::TransactionReceipt,
        token_out: Address,
        recipient: Address,
    ) -> Option<U256> {
        // ERC20 Transfer event topic
        let transfer_topic: B256 = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
            .parse()
            .unwrap();

        let recipient_topic: B256 = recipient.into_word();

        // Find Transfer events for the output token to our wallet
        for log in receipt.inner.logs() {
            // Must be from the output token contract
            if log.inner.address != token_out {
                continue;
            }
            // Must have Transfer event signature
            if log.inner.data.topics().is_empty() || log.inner.data.topics()[0] != transfer_topic {
                continue;
            }
            // topic[2] = `to` address (indexed)
            if log.inner.data.topics().len() < 3 || log.inner.data.topics()[2] != recipient_topic {
                continue;
            }
            // data = uint256 value
            if log.inner.data.data.len() >= 32 {
                let amount = U256::from_be_slice(&log.inner.data.data[..32]);
                debug!("Parsed amountOut from Transfer event: {}", amount);
                return Some(amount);
            }
        }

        warn!("Could not parse amountOut from receipt logs — using min_amount_out as fallback");
        None
    }

    /// Convert Wei to USD based on token pair
    fn wei_to_usd(&self, wei: U256, pair_symbol: &str) -> f64 {
        let wei_f = wei.to::<u128>() as f64;

        if pair_symbol.starts_with("WETH") {
            (wei_f / 1e18) * 3300.0
        } else if pair_symbol.starts_with("WMATIC") {
            (wei_f / 1e18) * self.config.native_token_price_usd
        } else {
            wei_f / 1e18
        }
    }
}

/// Wait for an atomic arb transaction receipt and parse the result.
///
/// Free function (not a method) — takes an Arc<Provider> so it can be spawned
/// in a JoinSet for parallel receipt waiting. Does NOT do tax logging (caller's
/// responsibility via the returned TradeResult data).
///
/// Used by the parallel submission path (A10): submit_atomic() returns SubmitResult,
/// then this function is spawned per pending tx to wait for receipts concurrently.
pub async fn wait_for_atomic_receipt<P: Provider>(
    provider: Arc<P>,
    submit: SubmitResult,
) -> TradeResult {
    let receipt_deadline = Instant::now() + Duration::from_secs(30);
    let receipt = loop {
        match provider.get_transaction_receipt(submit.tx_hash).await {
            Ok(Some(r)) => break r,
            Ok(None) => {
                if Instant::now() > receipt_deadline {
                    error!("Receipt timeout (30s) for tx {:?} — tx may still confirm later", submit.tx_hash);
                    return TradeResult {
                        opportunity: submit.pair_symbol.clone(),
                        tx_hash: Some(format!("{:?}", submit.tx_hash)),
                        block_number: None,
                        success: false,
                        profit_usd: 0.0,
                        gas_cost_usd: 0.0,
                        gas_used_native: 0.0,
                        net_profit_usd: 0.0,
                        execution_time_ms: submit.start_time.elapsed().as_millis() as u64,
                        error: Some("Receipt timeout — tx submitted but unconfirmed".to_string()),
                        amount_in: Some(submit.trade_size.to_string()),
                        amount_out: None,
                    };
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
            }
            Err(e) => {
                return TradeResult {
                    opportunity: submit.pair_symbol.clone(),
                    tx_hash: Some(format!("{:?}", submit.tx_hash)),
                    block_number: None,
                    success: false,
                    profit_usd: 0.0,
                    gas_cost_usd: 0.0,
                    gas_used_native: 0.0,
                    net_profit_usd: 0.0,
                    execution_time_ms: submit.start_time.elapsed().as_millis() as u64,
                    error: Some(format!("Receipt fetch failed: {}", e)),
                    amount_in: Some(submit.trade_size.to_string()),
                    amount_out: None,
                };
            }
        }
    };

    let block_number = receipt.block_number.unwrap_or(0);

    if !receipt.status() {
        warn!("Parallel atomic tx reverted on-chain: {:?}", submit.tx_hash);
        return TradeResult {
            opportunity: submit.pair_symbol.clone(),
            tx_hash: Some(format!("{:?}", submit.tx_hash)),
            block_number: Some(block_number),
            success: false,
            profit_usd: 0.0,
            gas_cost_usd: 0.0,
            gas_used_native: 0.0,
            net_profit_usd: 0.0,
            execution_time_ms: submit.start_time.elapsed().as_millis() as u64,
            error: Some("Atomic tx reverted on-chain".to_string()),
            amount_in: Some(submit.trade_size.to_string()),
            amount_out: None,
        };
    }

    // Parse profit from ArbExecuted event
    let mut profit_raw = U256::ZERO;
    let mut amount_out = submit.trade_size; // fallback
    let arb_executed_topic: B256 = keccak256(
        b"ArbExecuted(address,address,uint256,uint256,uint256,address,address)"
    );

    for log in receipt.inner.logs() {
        if log.inner.address == submit.arb_address
            && !log.inner.data.topics().is_empty()
            && log.inner.data.topics()[0] == arb_executed_topic
        {
            if log.inner.data.data.len() >= 96 {
                amount_out = U256::from_be_slice(&log.inner.data.data[32..64]);
                profit_raw = U256::from_be_slice(&log.inner.data.data[64..96]);
                debug!("Parsed ArbExecuted: amountOut={}, profit={}", amount_out, profit_raw);
            }
            break;
        }
    }

    let profit_usd = profit_raw.to::<u128>() as f64 / 10_f64.powi(submit.quote_decimals as i32);
    let gas_used = receipt.gas_used as u128;
    let effective_gas_price = receipt.effective_gas_price;
    let gas_cost_wei = gas_used * effective_gas_price;
    let gas_used_native = gas_cost_wei as f64 / 1e18;
    let gas_cost_usd = gas_used_native * submit.native_token_price_usd;
    let net_profit_usd = profit_usd - gas_cost_usd;
    let success = net_profit_usd > 0.0;

    if success {
        info!(
            "🎉 PARALLEL PROFIT: {} | ${:.4} (gross: ${:.4}, gas: ${:.4}) | tx: {:?}",
            submit.pair_symbol, net_profit_usd, profit_usd, gas_cost_usd, submit.tx_hash
        );
    } else {
        warn!(
            "📉 PARALLEL LOSS: {} | ${:.4} (gross: ${:.4}, gas: ${:.4}) | tx: {:?}",
            submit.pair_symbol, net_profit_usd, profit_usd, gas_cost_usd, submit.tx_hash
        );
    }

    TradeResult {
        opportunity: submit.pair_symbol,
        tx_hash: Some(format!("{:?}", submit.tx_hash)),
        block_number: Some(block_number),
        success,
        profit_usd,
        gas_cost_usd,
        gas_used_native,
        net_profit_usd,
        execution_time_ms: submit.start_time.elapsed().as_millis() as u64,
        error: None,
        amount_in: Some(submit.trade_size.to_string()),
        amount_out: Some(amount_out.to_string()),
    }
}
