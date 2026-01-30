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
//! Cross-DEX support: Routes Uniswap V3 legs to QuoterV1 and SushiSwap V3
//! legs to QuoterV2. Both revert-return amountOut as the first 32 bytes.
//! Multicall3 aggregate3 supports mixed target addresses per sub-call.
//!
//! Author: AI-Generated
//! Created: 2026-01-29
//! Modified: 2026-01-30 - Cross-DEX: dual-quoter (V1 for Uni, V2 for Sushi)

use crate::types::{ArbitrageOpportunity, BotConfig, DexType};
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
const QUOTER_V1_SELECTOR: [u8; 4] = [0xf7, 0x72, 0x9d, 0x43];

/// QuoterV2 function selector: quoteExactInputSingle((address,address,uint256,uint24,uint160))
/// keccak256("quoteExactInputSingle((address,address,uint256,uint24,uint160))")[..4]
/// Note: V2 wraps params in a tuple struct — different selector and param order from V1.
const QUOTER_V2_SELECTOR: [u8; 4] = [0xc6, 0xa5, 0x02, 0x6a];

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

/// Batch Quoter using Multicall3 for pre-screening opportunities.
/// Supports dual-quoter: QuoterV1 for Uniswap V3, QuoterV2 for SushiSwap V3.
pub struct MulticallQuoter<M: Middleware> {
    provider: Arc<M>,
    multicall_address: Address,
    uniswap_quoter_address: Address,
    sushiswap_quoter_address: Option<Address>,
}

impl<M: Middleware + 'static> MulticallQuoter<M> {
    /// Create a new MulticallQuoter using Quoter addresses from config.
    /// Uniswap V3 QuoterV1 is required; SushiSwap V3 QuoterV2 is optional.
    pub fn new(provider: Arc<M>, config: &BotConfig) -> Result<Self> {
        let multicall_address: Address = MULTICALL3_ADDRESS
            .parse()
            .context("Invalid Multicall3 address constant")?;

        let uniswap_quoter_address = config
            .uniswap_v3_quoter
            .ok_or_else(|| anyhow!("UNISWAP_V3_QUOTER not configured — required for Multicall batch verify"))?;

        let sushiswap_quoter_address = config.sushiswap_v3_quoter;

        info!(
            "MulticallQuoter initialized: Multicall3={:?}, UniQuoter={:?}, SushiQuoter={:?}",
            multicall_address, uniswap_quoter_address, sushiswap_quoter_address
        );

        Ok(Self {
            provider,
            multicall_address,
            uniswap_quoter_address,
            sushiswap_quoter_address,
        })
    }

    /// Get the correct quoter address for a DexType.
    /// SushiSwap V3 → SushiSwap QuoterV2; all else → Uniswap QuoterV1.
    fn quoter_for_dex(&self, dex: DexType) -> Address {
        if dex.is_sushi_v3() {
            self.sushiswap_quoter_address.unwrap_or(self.uniswap_quoter_address)
        } else {
            self.uniswap_quoter_address
        }
    }

    /// Batch verify all opportunities with a single RPC call.
    ///
    /// For each opportunity, encodes 2 Quoter sub-calls (buy leg + sell leg)
    /// into a Multicall3 `aggregate3` batch. Returns verification results
    /// in the same order as the input opportunities.
    ///
    /// Cross-DEX: routes each leg to the correct quoter contract (V1 or V2)
    /// based on the leg's DexType. Multicall3 supports mixed target addresses.
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
        // Each sub-call is (target_address, encoded_calldata)
        let mut sub_calls: Vec<(Address, Vec<u8>)> = Vec::with_capacity(opportunities.len() * 2);

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
            let buy_quoter = self.quoter_for_dex(opp.buy_dex);
            let buy_call = Self::encode_quoter_for_dex(
                opp.buy_dex,
                opp.pair.token0,
                opp.pair.token1,
                buy_fee,
                opp.trade_size,
            );

            // Sell leg: token1 → token0 on sell pool
            // Use estimated buy output since we don't have actual yet
            let sell_quoter = self.quoter_for_dex(opp.sell_dex);
            let estimated_buy_out = Self::estimate_buy_output(opp);
            let sell_call = Self::encode_quoter_for_dex(
                opp.sell_dex,
                opp.pair.token1,
                opp.pair.token0,
                sell_fee,
                estimated_buy_out,
            );

            sub_calls.push((buy_quoter, buy_call));
            sub_calls.push((sell_quoter, sell_call));
        }

        let num_subcalls = sub_calls.len();
        debug!(
            "Multicall batch: {} opportunities → {} sub-calls",
            opportunities.len(),
            num_subcalls
        );

        // Build Multicall3 aggregate3 calldata
        let calldata = Self::build_aggregate3_calldata(&sub_calls);

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

    /// Route to the correct quoter encoding based on DexType.
    /// SushiSwap V3 → QuoterV2 (tuple struct param), all else → QuoterV1 (flat params).
    fn encode_quoter_for_dex(
        dex: DexType,
        token_in: Address,
        token_out: Address,
        fee: u32,
        amount_in: U256,
    ) -> Vec<u8> {
        if dex.is_sushi_v3() {
            Self::encode_quoter_v2_call(token_in, token_out, fee, amount_in)
        } else {
            Self::encode_quoter_v1_call(token_in, token_out, fee, amount_in)
        }
    }

    /// Encode a QuoterV1 `quoteExactInputSingle` call (Uniswap V3).
    ///
    /// Selector: 0xf7729d43
    /// Params: (address tokenIn, address tokenOut, uint24 fee, uint256 amountIn, uint160 sqrtPriceLimitX96)
    /// sqrtPriceLimitX96 = 0 (no price limit)
    fn encode_quoter_v1_call(
        token_in: Address,
        token_out: Address,
        fee: u32,
        amount_in: U256,
    ) -> Vec<u8> {
        let mut data = Vec::with_capacity(164); // 4 selector + 5×32 params
        data.extend_from_slice(&QUOTER_V1_SELECTOR);

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

    /// Encode a QuoterV2 `quoteExactInputSingle` call (SushiSwap V3).
    ///
    /// Selector: 0xc6a5026a
    /// Param: tuple (address tokenIn, address tokenOut, uint256 amountIn, uint24 fee, uint160 sqrtPriceLimitX96)
    /// Note different param order from V1: amountIn comes before fee in V2.
    /// sqrtPriceLimitX96 = 0 (no price limit)
    ///
    /// Return: (uint256 amountOut, uint160 sqrtPriceX96After, uint32 initializedTicksCrossed, uint256 gasEstimate)
    /// First 32 bytes of returnData = amountOut (same as V1), so decode_quoter_result works for both.
    fn encode_quoter_v2_call(
        token_in: Address,
        token_out: Address,
        fee: u32,
        amount_in: U256,
    ) -> Vec<u8> {
        let mut data = Vec::with_capacity(164); // 4 selector + 5×32 params (encoded as tuple)
        data.extend_from_slice(&QUOTER_V2_SELECTOR);

        // ABI-encode as a single tuple parameter (note: order differs from V1)
        let encoded = abi::encode(&[Token::Tuple(vec![
            Token::Address(token_in),
            Token::Address(token_out),
            Token::Uint(amount_in),            // amountIn before fee in V2
            Token::Uint(U256::from(fee)),
            Token::Uint(U256::zero()),         // sqrtPriceLimitX96 = 0
        ])]);
        data.extend_from_slice(&encoded);

        data
    }

    /// Build Multicall3 `aggregate3` calldata from a list of (target, calldata) pairs.
    ///
    /// Each sub-call is wrapped as: (target: address, allowFailure: true, callData: bytes)
    /// The aggregate3 function takes a single parameter: an array of Call3 structs.
    /// Cross-DEX: each sub-call can target a different quoter contract.
    fn build_aggregate3_calldata(sub_calls: &[(Address, Vec<u8>)]) -> Bytes {
        let calls: Vec<Token> = sub_calls
            .iter()
            .map(|(target, call_data)| {
                Token::Tuple(vec![
                    Token::Address(*target),
                    Token::Bool(true), // allowFailure — required for Quoter revert pattern
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
    fn test_encode_quoter_v1_call() {
        let token_in: Address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
            .parse()
            .unwrap(); // USDC
        let token_out: Address = "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619"
            .parse()
            .unwrap(); // WETH on Polygon

        let fee = 500u32; // 0.05%
        let amount_in = U256::from(1_000_000u64); // 1 USDC (6 decimals)

        let encoded = MulticallQuoter::<Provider<Ws>>::encode_quoter_v1_call(
            token_in, token_out, fee, amount_in,
        );

        // Should be 4 (selector) + 5*32 (params) = 164 bytes
        assert_eq!(encoded.len(), 164);
        // First 4 bytes = QuoterV1 selector
        assert_eq!(&encoded[..4], &QUOTER_V1_SELECTOR);
    }

    #[test]
    fn test_encode_quoter_v2_call() {
        let token_in: Address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
            .parse()
            .unwrap(); // USDC
        let token_out: Address = "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619"
            .parse()
            .unwrap(); // WETH on Polygon

        let fee = 500u32; // 0.05%
        let amount_in = U256::from(1_000_000u64); // 1 USDC (6 decimals)

        let encoded = MulticallQuoter::<Provider<Ws>>::encode_quoter_v2_call(
            token_in, token_out, fee, amount_in,
        );

        // First 4 bytes = QuoterV2 selector
        assert_eq!(&encoded[..4], &QUOTER_V2_SELECTOR);
        // V2 encodes params as a tuple — different from V1 flat params
        // but still produces valid ABI-encoded calldata
        assert!(encoded.len() > 4);
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
