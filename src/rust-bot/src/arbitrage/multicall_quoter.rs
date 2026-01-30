//! Multicall3 Batch Quoter Pre-Screening
//!
//! Batch-verifies arbitrage opportunities by encoding buy+sell Quoter calls
//! into a single Multicall3 `aggregate3` RPC call. Filters out opportunities
//! where either leg cannot be filled, then ranks survivors by quoted profit.
//!
//! This is a PRE-SCREENING step only. The executor retains its own per-leg
//! Quoter safety checks during actual execution.
//!
//! Technical note: QuoterV1 returns data by reverting. Inside Multicall3 with
//! `allowFailure: true`, the revert payload is captured in `returnData`.
//! We decode the first 32 bytes as `uint256 amountOut`. Real failures are
//! distinguished by empty returnData or the Error(string) selector 0x08c379a2.
//!
//! Author: AI-Generated
//! Created: 2026-01-29
//! Modified: 2026-01-29

use crate::types::{ArbitrageOpportunity, BotConfig};
use anyhow::{anyhow, Context, Result};
use ethers::abi::{self, ParamType, Token};
use ethers::prelude::*;
use ethers::types::{Address, Bytes, U256};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Multicall3 deployed address (same on all EVM chains including Polygon)
const MULTICALL3_ADDRESS: &str = "0xcA11bde05977b3631167028862bE2a173976CA11";

/// QuoterV1 function selector: quoteExactInputSingle(address,address,uint24,uint256,uint160)
/// keccak256("quoteExactInputSingle(address,address,uint24,uint256,uint160)")[..4]
const QUOTER_SELECTOR: [u8; 4] = [0xf7, 0x72, 0x9d, 0x43];

/// Multicall3 aggregate3 function selector: aggregate3((address,bool,bytes)[])
/// keccak256("aggregate3((address,bool,bytes)[])")[..4]
const AGGREGATE3_SELECTOR: [u8; 4] = [0x82, 0xad, 0x56, 0xcb];

/// Error(string) selector — indicates an actual revert, not QuoterV1 data return
const ERROR_SELECTOR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa2];

/// Panic(uint256) selector — another form of actual failure
const PANIC_SELECTOR: [u8; 4] = [0x4e, 0x48, 0x7b, 0x71];

/// Conservative haircut applied to estimated buy output for sell-leg pre-screen.
/// The executor uses exact amounts from its own Quoter call.
const SELL_ESTIMATE_FACTOR: f64 = 0.95;

/// Result of batch verification for a single opportunity
#[derive(Debug, Clone)]
pub struct VerifiedOpportunity {
    /// Index into the original opportunities Vec from the detector
    pub original_index: usize,
    /// Quoted buy leg output (token1 amount received from buying)
    pub buy_quoted_out: U256,
    /// Quoted sell leg output (token0 amount received from selling)
    pub sell_quoted_out: U256,
    /// Net quoted profit in token0 raw units (sell_out - trade_size)
    pub quoted_profit_raw: i128,
    /// Whether both legs produced valid quotes
    pub both_legs_valid: bool,
    /// Error description if verification failed
    pub error: Option<String>,
}

impl VerifiedOpportunity {
    /// Create a passthrough entry (used when Multicall fails and we fall back)
    pub fn passthrough(index: usize) -> Self {
        Self {
            original_index: index,
            buy_quoted_out: U256::zero(),
            sell_quoted_out: U256::zero(),
            quoted_profit_raw: 0,
            both_legs_valid: true, // pass-through — executor will re-verify
            error: None,
        }
    }
}

/// Batch Quoter using Multicall3 for pre-screening opportunities
pub struct MulticallQuoter<M: Middleware> {
    provider: Arc<M>,
    multicall_address: Address,
    quoter_address: Address,
}

impl<M: Middleware + 'static> MulticallQuoter<M> {
    /// Create a new MulticallQuoter using the Quoter address from config
    pub fn new(provider: Arc<M>, config: &BotConfig) -> Result<Self> {
        let multicall_address: Address = MULTICALL3_ADDRESS
            .parse()
            .context("Invalid Multicall3 address constant")?;

        let quoter_address = config
            .uniswap_v3_quoter
            .ok_or_else(|| anyhow!("UNISWAP_V3_QUOTER not configured — required for Multicall batch verify"))?;

        info!(
            "MulticallQuoter initialized: Multicall3={:?}, Quoter={:?}",
            multicall_address, quoter_address
        );

        Ok(Self {
            provider,
            multicall_address,
            quoter_address,
        })
    }

    /// Batch verify all opportunities with a single RPC call.
    ///
    /// For each opportunity, encodes 2 Quoter sub-calls (buy leg + sell leg)
    /// into a Multicall3 `aggregate3` batch. Returns verification results
    /// in the same order as the input opportunities.
    ///
    /// The sell leg uses an estimated buy output (conservative 95% haircut)
    /// since we don't know the actual buy output until execution.
    pub async fn batch_verify(
        &self,
        opportunities: &[ArbitrageOpportunity],
        _config: &BotConfig,
    ) -> Result<Vec<VerifiedOpportunity>> {
        if opportunities.is_empty() {
            return Ok(Vec::new());
        }

        // Build all sub-calls: 2 per opportunity (buy leg + sell leg)
        let mut sub_calls: Vec<Vec<u8>> = Vec::with_capacity(opportunities.len() * 2);

        for opp in opportunities {
            let buy_fee = opp
                .buy_dex
                .v3_fee_tier()
                .ok_or_else(|| anyhow!("Buy DEX {:?} is not V3", opp.buy_dex))?;
            let sell_fee = opp
                .sell_dex
                .v3_fee_tier()
                .ok_or_else(|| anyhow!("Sell DEX {:?} is not V3", opp.sell_dex))?;

            // Buy leg: token0 → token1 on buy pool
            let buy_call = Self::encode_quoter_call(
                opp.pair.token0,
                opp.pair.token1,
                buy_fee,
                opp.trade_size,
            );

            // Sell leg: token1 → token0 on sell pool
            // Use estimated buy output since we don't have actual yet
            let estimated_buy_out = Self::estimate_buy_output(opp);
            let sell_call = Self::encode_quoter_call(
                opp.pair.token1,
                opp.pair.token0,
                sell_fee,
                estimated_buy_out,
            );

            sub_calls.push(buy_call);
            sub_calls.push(sell_call);
        }

        let num_subcalls = sub_calls.len();
        debug!(
            "Multicall batch: {} opportunities → {} sub-calls",
            opportunities.len(),
            num_subcalls
        );

        // Build Multicall3 aggregate3 calldata
        let calldata = self.build_aggregate3_calldata(&sub_calls);

        // Execute single eth_call to Multicall3
        let tx = TransactionRequest::new()
            .to(self.multicall_address)
            .data(calldata);

        let response = self
            .provider
            .call(&tx.into(), None)
            .await
            .context("Multicall3 aggregate3 eth_call failed")?;

        // Decode response
        let results = Self::decode_aggregate3_response(&response)
            .context("Failed to decode Multicall3 response")?;

        if results.len() != num_subcalls {
            return Err(anyhow!(
                "Multicall3 returned {} results, expected {}",
                results.len(),
                num_subcalls
            ));
        }

        // Process results in pairs (buy, sell) for each opportunity
        let mut verified = Vec::with_capacity(opportunities.len());

        for (i, opp) in opportunities.iter().enumerate() {
            let buy_idx = i * 2;
            let sell_idx = i * 2 + 1;

            let (buy_success, ref buy_data) = results[buy_idx];
            let (sell_success, ref sell_data) = results[sell_idx];

            let buy_result = Self::decode_quoter_result(buy_success, buy_data);
            let sell_result = Self::decode_quoter_result(sell_success, sell_data);

            match (buy_result, sell_result) {
                (Ok(buy_out), Ok(sell_out)) => {
                    // Both legs valid — calculate profit in token0 raw units
                    let trade_size_i128 = opp.trade_size.as_u128() as i128;
                    let sell_out_i128 = sell_out.as_u128() as i128;
                    let profit = sell_out_i128 - trade_size_i128;

                    debug!(
                        "Multicall verified [{}]: {} buy_out={} sell_out={} profit_raw={}",
                        i, opp.pair.symbol, buy_out, sell_out, profit
                    );

                    verified.push(VerifiedOpportunity {
                        original_index: i,
                        buy_quoted_out: buy_out,
                        sell_quoted_out: sell_out,
                        quoted_profit_raw: profit,
                        both_legs_valid: true,
                        error: None,
                    });
                }
                (Err(buy_err), _) => {
                    debug!(
                        "Multicall rejected [{}]: {} — buy leg failed: {}",
                        i, opp.pair.symbol, buy_err
                    );
                    verified.push(VerifiedOpportunity {
                        original_index: i,
                        buy_quoted_out: U256::zero(),
                        sell_quoted_out: U256::zero(),
                        quoted_profit_raw: 0,
                        both_legs_valid: false,
                        error: Some(format!("Buy leg: {}", buy_err)),
                    });
                }
                (Ok(buy_out), Err(sell_err)) => {
                    debug!(
                        "Multicall rejected [{}]: {} — sell leg failed: {} (buy_out={})",
                        i, opp.pair.symbol, sell_err, buy_out
                    );
                    verified.push(VerifiedOpportunity {
                        original_index: i,
                        buy_quoted_out: buy_out,
                        sell_quoted_out: U256::zero(),
                        quoted_profit_raw: 0,
                        both_legs_valid: false,
                        error: Some(format!("Sell leg: {}", sell_err)),
                    });
                }
            }
        }

        Ok(verified)
    }

    /// Encode a QuoterV1 `quoteExactInputSingle` call.
    ///
    /// Selector: 0xf7729d43
    /// Params: (address tokenIn, address tokenOut, uint24 fee, uint256 amountIn, uint160 sqrtPriceLimitX96)
    /// sqrtPriceLimitX96 = 0 (no price limit)
    fn encode_quoter_call(
        token_in: Address,
        token_out: Address,
        fee: u32,
        amount_in: U256,
    ) -> Vec<u8> {
        let mut data = Vec::with_capacity(164); // 4 selector + 5×32 params
        data.extend_from_slice(&QUOTER_SELECTOR);

        // ABI-encode 5 parameters as 32-byte words
        let encoded = abi::encode(&[
            Token::Address(token_in),
            Token::Address(token_out),
            Token::Uint(U256::from(fee)),
            Token::Uint(amount_in),
            Token::Uint(U256::zero()), // sqrtPriceLimitX96 = 0
        ]);
        data.extend_from_slice(&encoded);

        data
    }

    /// Build Multicall3 `aggregate3` calldata from a list of encoded sub-calls.
    ///
    /// Each sub-call is wrapped as: (target: quoter_address, allowFailure: true, callData: bytes)
    /// The aggregate3 function takes a single parameter: an array of Call3 structs.
    fn build_aggregate3_calldata(&self, sub_calls: &[Vec<u8>]) -> Bytes {
        let calls: Vec<Token> = sub_calls
            .iter()
            .map(|call_data| {
                Token::Tuple(vec![
                    Token::Address(self.quoter_address),
                    Token::Bool(true), // allowFailure — required for QuoterV1 revert pattern
                    Token::Bytes(call_data.clone()),
                ])
            })
            .collect();

        let mut data = Vec::new();
        data.extend_from_slice(&AGGREGATE3_SELECTOR);
        let encoded = abi::encode(&[Token::Array(calls)]);
        data.extend_from_slice(&encoded);

        Bytes::from(data)
    }

    /// Decode Multicall3 `aggregate3` response into individual (success, returnData) pairs.
    ///
    /// Response ABI: (bool success, bytes returnData)[]
    fn decode_aggregate3_response(response: &[u8]) -> Result<Vec<(bool, Vec<u8>)>> {
        // The response is ABI-encoded as: array of (bool, bytes) tuples
        let decoded = abi::decode(
            &[ParamType::Array(Box::new(ParamType::Tuple(vec![
                ParamType::Bool,
                ParamType::Bytes,
            ])))],
            response,
        )
        .context("ABI decode of aggregate3 response failed")?;

        let results_array = match decoded.into_iter().next() {
            Some(Token::Array(arr)) => arr,
            _ => return Err(anyhow!("Expected Array in aggregate3 response")),
        };

        let mut results = Vec::with_capacity(results_array.len());
        for token in results_array {
            match token {
                Token::Tuple(mut fields) if fields.len() == 2 => {
                    let return_data = match fields.pop() {
                        Some(Token::Bytes(b)) => b,
                        _ => return Err(anyhow!("Expected Bytes in result tuple")),
                    };
                    let success = match fields.pop() {
                        Some(Token::Bool(b)) => b,
                        _ => return Err(anyhow!("Expected Bool in result tuple")),
                    };
                    results.push((success, return_data));
                }
                _ => return Err(anyhow!("Unexpected token type in aggregate3 results")),
            }
        }

        Ok(results)
    }

    /// Decode a QuoterV1 result from Multicall3 sub-call response.
    ///
    /// QuoterV1 returns data by reverting — so inside Multicall3:
    /// - success=false, returnData=abi.encode(uint256 amountOut) → valid quote
    /// - success=false, returnData starts with 0x08c379a2 → Error(string), real failure
    /// - success=false, returnData is empty → real failure (no liquidity)
    fn decode_quoter_result(success: bool, return_data: &[u8]) -> Result<U256> {
        // QuoterV1 always "fails" (reverts to return data).
        // A successful sub-call would be unexpected but we handle it gracefully.
        if success && return_data.len() >= 32 {
            // Unexpected: the call succeeded normally. Decode as uint256 anyway.
            return Ok(U256::from_big_endian(&return_data[..32]));
        }

        // Normal QuoterV1 path: success=false, returnData contains the quote
        if return_data.len() < 32 {
            return Err(anyhow!(
                "Quoter returned insufficient data ({} bytes) — pool likely has no liquidity",
                return_data.len()
            ));
        }

        // Check for Error(string) selector — indicates a real revert
        if return_data.len() >= 4 && return_data[..4] == ERROR_SELECTOR {
            // Try to decode the error message
            let msg = if return_data.len() > 4 {
                abi::decode(&[ParamType::String], &return_data[4..])
                    .ok()
                    .and_then(|tokens| tokens.into_iter().next())
                    .and_then(|t| match t {
                        Token::String(s) => Some(s),
                        _ => None,
                    })
                    .unwrap_or_else(|| "unknown error".to_string())
            } else {
                "unknown error".to_string()
            };
            return Err(anyhow!("Quoter reverted with error: {}", msg));
        }

        // Check for Panic(uint256) selector
        if return_data.len() >= 4 && return_data[..4] == PANIC_SELECTOR {
            return Err(anyhow!("Quoter panicked (likely arithmetic overflow)"));
        }

        // Normal QuoterV1 response: first 32 bytes = uint256 amountOut
        let amount_out = U256::from_big_endian(&return_data[..32]);

        if amount_out.is_zero() {
            return Err(anyhow!("Quoter returned zero — pool has no executable depth"));
        }

        Ok(amount_out)
    }

    /// Estimate buy leg output for sell-leg pre-screening.
    ///
    /// Uses the detector's price with a conservative 5% haircut.
    /// The executor will use the actual Quoter amount during execution.
    fn estimate_buy_output(opp: &ArbitrageOpportunity) -> U256 {
        let amount_in_human =
            opp.trade_size.as_u128() as f64 / 10_f64.powi(opp.token0_decimals as i32);
        let expected_out = amount_in_human * opp.buy_price * SELL_ESTIMATE_FACTOR;
        let raw = expected_out * 10_f64.powi(opp.token1_decimals as i32);

        if raw <= 0.0 || !raw.is_finite() {
            warn!(
                "estimate_buy_output: invalid result {:.2} for {} (trade_size={}, buy_price={:.6})",
                raw, opp.pair.symbol, opp.trade_size, opp.buy_price
            );
            return U256::zero();
        }

        U256::from(raw as u128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_quoter_call() {
        let token_in: Address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
            .parse()
            .unwrap(); // USDC
        let token_out: Address = "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619"
            .parse()
            .unwrap(); // WETH on Polygon

        let fee = 500u32; // 0.05%
        let amount_in = U256::from(1_000_000u64); // 1 USDC (6 decimals)

        let encoded = MulticallQuoter::<Provider<Ws>>::encode_quoter_call(
            token_in, token_out, fee, amount_in,
        );

        // Should be 4 (selector) + 5*32 (params) = 164 bytes
        assert_eq!(encoded.len(), 164);
        // First 4 bytes = QuoterV1 selector
        assert_eq!(&encoded[..4], &QUOTER_SELECTOR);
    }

    #[test]
    fn test_decode_quoter_result_valid() {
        // Simulate QuoterV1 returning 1e18 (1 token with 18 decimals)
        let mut return_data = vec![0u8; 32];
        let amount = U256::from(1_000_000_000_000_000_000u64); // 1e18
        amount.to_big_endian(&mut return_data);

        let result =
            MulticallQuoter::<Provider<Ws>>::decode_quoter_result(false, &return_data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), amount);
    }

    #[test]
    fn test_decode_quoter_result_error_selector() {
        // Simulate an Error(string) revert
        let mut return_data = Vec::new();
        return_data.extend_from_slice(&ERROR_SELECTOR);
        // Add ABI-encoded string "insufficient liquidity"
        let encoded_msg = abi::encode(&[Token::String("insufficient liquidity".to_string())]);
        return_data.extend_from_slice(&encoded_msg);

        let result =
            MulticallQuoter::<Provider<Ws>>::decode_quoter_result(false, &return_data);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("insufficient liquidity"));
    }

    #[test]
    fn test_decode_quoter_result_empty() {
        let result =
            MulticallQuoter::<Provider<Ws>>::decode_quoter_result(false, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("insufficient data"));
    }

    #[test]
    fn test_decode_quoter_result_zero_amount() {
        let return_data = vec![0u8; 32]; // All zeros = amount 0
        let result =
            MulticallQuoter::<Provider<Ws>>::decode_quoter_result(false, &return_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("zero"));
    }

    #[test]
    fn test_decode_quoter_result_panic() {
        let mut return_data = Vec::new();
        return_data.extend_from_slice(&PANIC_SELECTOR);
        return_data.extend_from_slice(&[0u8; 32]); // panic code 0

        let result =
            MulticallQuoter::<Provider<Ws>>::decode_quoter_result(false, &return_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("panicked"));
    }

    #[test]
    fn test_decode_quoter_result_success_true() {
        // Unexpected but handled: success=true with valid data
        let mut return_data = vec![0u8; 32];
        let amount = U256::from(500_000u64);
        amount.to_big_endian(&mut return_data);

        let result =
            MulticallQuoter::<Provider<Ws>>::decode_quoter_result(true, &return_data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), amount);
    }
}
