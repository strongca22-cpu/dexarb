//! A4 Mempool Monitor — Calldata Decoder
//!
//! Purpose:
//!     Decode DEX swap calldata from pending transaction input bytes.
//!     Supports Uniswap V3, SushiSwap V3, QuickSwap V3 (Algebra), and V2 router functions.
//!
//! Author: AI-Generated
//! Created: 2026-02-01
//! Modified: 2026-02-01
//!
//! Dependencies:
//!     - ethers (abi decoding)
//!
//! Supported Function Selectors:
//!     V3 SwapRouter:
//!       0x414bf389 — exactInputSingle(ExactInputSingleParams)
//!       0xc04b8d59 — exactInput(ExactInputParams)
//!       0xdb3e2198 — exactOutputSingle(ExactOutputSingleParams)
//!       0xf28c0498 — exactOutput(ExactOutputParams)
//!       0x5ae401dc — multicall(uint256,bytes[])
//!       0xac9650d8 — multicall(bytes[])
//!     Algebra (QuickSwap V3):
//!       0xbc651188 — exactInputSingle (no fee field)
//!     V2 Router:
//!       0x38ed1739 — swapExactTokensForTokens
//!       0x8803dbee — swapTokensForExactTokens
//!       0x7ff36ab5 — swapExactETHForTokens
//!       0x18cbafe5 — swapExactTokensForETH

use ethers::abi::{decode, ParamType, Token};
use ethers::types::{Address, U256};
use tracing::trace;

use super::types::DecodedSwap;

// ── V3 SwapRouter selectors ─────────────────────────────────────────
const EXACT_INPUT_SINGLE: [u8; 4] = [0x41, 0x4b, 0xf3, 0x89];
const EXACT_INPUT: [u8; 4] = [0xc0, 0x4b, 0x8d, 0x59];
const EXACT_OUTPUT_SINGLE: [u8; 4] = [0xdb, 0x3e, 0x21, 0x98];
const EXACT_OUTPUT: [u8; 4] = [0xf2, 0x8c, 0x04, 0x98];
const MULTICALL_DEADLINE: [u8; 4] = [0x5a, 0xe4, 0x01, 0xdc];
const MULTICALL_NO_DEADLINE: [u8; 4] = [0xac, 0x96, 0x50, 0xd8];

// ── Algebra (QuickSwap V3) selectors ────────────────────────────────
const ALGEBRA_EXACT_INPUT_SINGLE: [u8; 4] = [0xbc, 0x65, 0x11, 0x88];

// ── V2 Router selectors ────────────────────────────────────────────
const SWAP_EXACT_TOKENS_FOR_TOKENS: [u8; 4] = [0x38, 0xed, 0x17, 0x39];
const SWAP_TOKENS_FOR_EXACT_TOKENS: [u8; 4] = [0x88, 0x03, 0xdb, 0xee];
const SWAP_EXACT_ETH_FOR_TOKENS: [u8; 4] = [0x7f, 0xf3, 0x6a, 0xb5];
const SWAP_EXACT_TOKENS_FOR_ETH: [u8; 4] = [0x18, 0xcb, 0xaf, 0xe5];

/// Decode swap calldata from transaction input bytes.
/// Returns None if the selector is unknown or decoding fails.
pub fn decode_calldata(input: &[u8]) -> Option<DecodedSwap> {
    if input.len() < 4 {
        return None;
    }

    let selector: [u8; 4] = input[..4].try_into().ok()?;
    let data = &input[4..];

    let result = match selector {
        // V3 SwapRouter
        EXACT_INPUT_SINGLE => decode_v3_exact_input_single(data),
        EXACT_INPUT => decode_v3_exact_input(data),
        EXACT_OUTPUT_SINGLE => decode_v3_exact_output_single(data),
        EXACT_OUTPUT => decode_v3_exact_output(data),
        MULTICALL_DEADLINE => decode_multicall(data, true),
        MULTICALL_NO_DEADLINE => decode_multicall(data, false),
        // Algebra (QuickSwap V3)
        ALGEBRA_EXACT_INPUT_SINGLE => decode_algebra_exact_input_single(data),
        // V2 Router
        SWAP_EXACT_TOKENS_FOR_TOKENS => decode_v2_swap_exact_in(data, "swapExactTokensForTokens"),
        SWAP_TOKENS_FOR_EXACT_TOKENS => decode_v2_swap_exact_out(data),
        SWAP_EXACT_ETH_FOR_TOKENS => decode_v2_swap_eth_in(data),
        SWAP_EXACT_TOKENS_FOR_ETH => decode_v2_swap_exact_in(data, "swapExactTokensForETH"),
        _ => {
            trace!(
                "Unknown selector: 0x{:02x}{:02x}{:02x}{:02x}",
                selector[0], selector[1], selector[2], selector[3]
            );
            None
        }
    };

    result
}

/// Return the 4-byte selector as a hex string for logging
pub fn selector_hex(input: &[u8]) -> String {
    if input.len() < 4 {
        return "0x????".to_string();
    }
    format!("0x{:02x}{:02x}{:02x}{:02x}", input[0], input[1], input[2], input[3])
}

// ── V3 SwapRouter Decoders ──────────────────────────────────────────

/// Decode exactInputSingle(ExactInputSingleParams)
/// Params: (address tokenIn, address tokenOut, uint24 fee, address recipient,
///          uint256 deadline, uint256 amountIn, uint256 amountOutMinimum, uint160 sqrtPriceLimitX96)
fn decode_v3_exact_input_single(data: &[u8]) -> Option<DecodedSwap> {
    let params = vec![
        ParamType::Address,  // tokenIn
        ParamType::Address,  // tokenOut
        ParamType::Uint(24), // fee
        ParamType::Address,  // recipient
        ParamType::Uint(256), // deadline
        ParamType::Uint(256), // amountIn
        ParamType::Uint(256), // amountOutMinimum
        ParamType::Uint(160), // sqrtPriceLimitX96
    ];

    let tokens = decode(&params, data).ok()?;

    Some(DecodedSwap {
        function_name: "exactInputSingle".to_string(),
        token_in: token_to_address(&tokens[0]),
        token_out: token_to_address(&tokens[1]),
        fee_tier: token_to_u32(&tokens[2]),
        amount_in: token_to_u256(&tokens[5]),
        amount_out_min: token_to_u256(&tokens[6]),
    })
}

/// Decode exactInput(ExactInputParams)
/// Params struct: (bytes path, address recipient, uint256 deadline,
///                 uint256 amountIn, uint256 amountOutMinimum)
/// Path encoding: tokenA(20) | fee(3) | tokenB(20) [| fee(3) | tokenC(20) ...]
fn decode_v3_exact_input(data: &[u8]) -> Option<DecodedSwap> {
    // ExactInputParams is a struct with a dynamic field (bytes path),
    // so it's encoded with an offset pointer.
    let params = vec![ParamType::Tuple(vec![
        ParamType::Bytes,     // path
        ParamType::Address,   // recipient
        ParamType::Uint(256), // deadline
        ParamType::Uint(256), // amountIn
        ParamType::Uint(256), // amountOutMinimum
    ])];

    let tokens = decode(&params, data).ok()?;

    if let Token::Tuple(inner) = &tokens[0] {
        let path = token_to_bytes(&inner[0])?;
        let (token_in, token_out, fee) = decode_v3_path(&path)?;

        Some(DecodedSwap {
            function_name: "exactInput".to_string(),
            token_in: Some(token_in),
            token_out: Some(token_out),
            fee_tier: Some(fee),
            amount_in: token_to_u256(&inner[3]),
            amount_out_min: token_to_u256(&inner[4]),
        })
    } else {
        None
    }
}

/// Decode exactOutputSingle(ExactOutputSingleParams)
/// Params: (address tokenIn, address tokenOut, uint24 fee, address recipient,
///          uint256 deadline, uint256 amountOut, uint256 amountInMaximum, uint160 sqrtPriceLimitX96)
fn decode_v3_exact_output_single(data: &[u8]) -> Option<DecodedSwap> {
    let params = vec![
        ParamType::Address,  // tokenIn
        ParamType::Address,  // tokenOut
        ParamType::Uint(24), // fee
        ParamType::Address,  // recipient
        ParamType::Uint(256), // deadline
        ParamType::Uint(256), // amountOut
        ParamType::Uint(256), // amountInMaximum
        ParamType::Uint(160), // sqrtPriceLimitX96
    ];

    let tokens = decode(&params, data).ok()?;

    Some(DecodedSwap {
        function_name: "exactOutputSingle".to_string(),
        token_in: token_to_address(&tokens[0]),
        token_out: token_to_address(&tokens[1]),
        fee_tier: token_to_u32(&tokens[2]),
        // For exactOutput, amountOut is the target; amountInMaximum is the cap.
        // We log amountInMaximum as amount_in (max the user is willing to spend)
        // and amountOut as amount_out_min (the exact output they want).
        amount_in: token_to_u256(&tokens[6]),      // amountInMaximum
        amount_out_min: token_to_u256(&tokens[5]),  // amountOut (exact target)
    })
}

/// Decode exactOutput(ExactOutputParams)
/// Params struct: (bytes path, address recipient, uint256 deadline,
///                 uint256 amountOut, uint256 amountInMaximum)
/// Path is REVERSED: tokenOut | fee | ... | tokenIn
fn decode_v3_exact_output(data: &[u8]) -> Option<DecodedSwap> {
    let params = vec![ParamType::Tuple(vec![
        ParamType::Bytes,     // path (reversed!)
        ParamType::Address,   // recipient
        ParamType::Uint(256), // deadline
        ParamType::Uint(256), // amountOut
        ParamType::Uint(256), // amountInMaximum
    ])];

    let tokens = decode(&params, data).ok()?;

    if let Token::Tuple(inner) = &tokens[0] {
        let path = token_to_bytes(&inner[0])?;
        // Path is reversed for exactOutput: first token is tokenOut, last is tokenIn
        let (first_token, last_token, fee) = decode_v3_path(&path)?;

        Some(DecodedSwap {
            function_name: "exactOutput".to_string(),
            token_in: Some(last_token),   // reversed
            token_out: Some(first_token), // reversed
            fee_tier: Some(fee),
            amount_in: token_to_u256(&inner[4]),    // amountInMaximum
            amount_out_min: token_to_u256(&inner[3]), // amountOut (exact target)
        })
    } else {
        None
    }
}

/// Decode multicall(uint256 deadline, bytes[] data) or multicall(bytes[] data)
/// Recursively decodes the first recognized swap call within the multicall.
fn decode_multicall(data: &[u8], has_deadline: bool) -> Option<DecodedSwap> {
    let params = if has_deadline {
        vec![
            ParamType::Uint(256),
            ParamType::Array(Box::new(ParamType::Bytes)),
        ]
    } else {
        vec![ParamType::Array(Box::new(ParamType::Bytes))]
    };

    let tokens = decode(&params, data).ok()?;

    // Inner calls array is the last element
    let calls_token = tokens.last()?;
    if let Token::Array(inner_calls) = calls_token {
        // Try to decode each inner call; return the first recognized swap
        for call in inner_calls {
            if let Token::Bytes(call_data) = call {
                if let Some(mut swap) = decode_calldata(call_data) {
                    swap.function_name = format!("multicall>{}", swap.function_name);
                    return Some(swap);
                }
            }
        }

        // Recognized as multicall but no decodable inner swap
        Some(DecodedSwap {
            function_name: "multicall(opaque)".to_string(),
            token_in: None,
            token_out: None,
            amount_in: None,
            amount_out_min: None,
            fee_tier: None,
        })
    } else {
        None
    }
}

// ── Algebra (QuickSwap V3) Decoders ─────────────────────────────────

/// Decode Algebra exactInputSingle (no fee field)
/// Params: (address tokenIn, address tokenOut, address recipient,
///          uint256 deadline, uint256 amountIn, uint256 amountOutMinimum, uint160 limitSqrtPrice)
fn decode_algebra_exact_input_single(data: &[u8]) -> Option<DecodedSwap> {
    let params = vec![
        ParamType::Address,  // tokenIn
        ParamType::Address,  // tokenOut
        ParamType::Address,  // recipient
        ParamType::Uint(256), // deadline
        ParamType::Uint(256), // amountIn
        ParamType::Uint(256), // amountOutMinimum
        ParamType::Uint(160), // limitSqrtPrice
    ];

    let tokens = decode(&params, data).ok()?;

    Some(DecodedSwap {
        function_name: "algebraExactInputSingle".to_string(),
        token_in: token_to_address(&tokens[0]),
        token_out: token_to_address(&tokens[1]),
        fee_tier: None, // Algebra uses dynamic fees
        amount_in: token_to_u256(&tokens[4]),
        amount_out_min: token_to_u256(&tokens[5]),
    })
}

// ── V2 Router Decoders ──────────────────────────────────────────────

/// Decode swapExactTokensForTokens / swapExactTokensForETH
/// Params: (uint256 amountIn, uint256 amountOutMin, address[] path, address to, uint256 deadline)
fn decode_v2_swap_exact_in(data: &[u8], fn_name: &str) -> Option<DecodedSwap> {
    let params = vec![
        ParamType::Uint(256),  // amountIn
        ParamType::Uint(256),  // amountOutMin
        ParamType::Array(Box::new(ParamType::Address)), // path
        ParamType::Address,    // to
        ParamType::Uint(256),  // deadline
    ];

    let tokens = decode(&params, data).ok()?;

    let (token_in, token_out) = extract_v2_path(&tokens[2]);

    Some(DecodedSwap {
        function_name: fn_name.to_string(),
        token_in,
        token_out,
        fee_tier: None, // V2 always 0.30%
        amount_in: token_to_u256(&tokens[0]),
        amount_out_min: token_to_u256(&tokens[1]),
    })
}

/// Decode swapTokensForExactTokens
/// Params: (uint256 amountOut, uint256 amountInMax, address[] path, address to, uint256 deadline)
fn decode_v2_swap_exact_out(data: &[u8]) -> Option<DecodedSwap> {
    let params = vec![
        ParamType::Uint(256),  // amountOut
        ParamType::Uint(256),  // amountInMax
        ParamType::Array(Box::new(ParamType::Address)), // path
        ParamType::Address,    // to
        ParamType::Uint(256),  // deadline
    ];

    let tokens = decode(&params, data).ok()?;

    let (token_in, token_out) = extract_v2_path(&tokens[2]);

    Some(DecodedSwap {
        function_name: "swapTokensForExactTokens".to_string(),
        token_in,
        token_out,
        fee_tier: None,
        amount_in: token_to_u256(&tokens[1]),    // amountInMax
        amount_out_min: token_to_u256(&tokens[0]), // amountOut (exact)
    })
}

/// Decode swapExactETHForTokens (msg.value is amountIn, not in calldata)
/// Params: (uint256 amountOutMin, address[] path, address to, uint256 deadline)
fn decode_v2_swap_eth_in(data: &[u8]) -> Option<DecodedSwap> {
    let params = vec![
        ParamType::Uint(256),  // amountOutMin
        ParamType::Array(Box::new(ParamType::Address)), // path
        ParamType::Address,    // to
        ParamType::Uint(256),  // deadline
    ];

    let tokens = decode(&params, data).ok()?;

    let (token_in, token_out) = extract_v2_path(&tokens[1]);

    Some(DecodedSwap {
        function_name: "swapExactETHForTokens".to_string(),
        token_in,
        token_out,
        fee_tier: None,
        amount_in: None, // amountIn is msg.value, not in calldata
        amount_out_min: token_to_u256(&tokens[0]),
    })
}

// ── Path Decoders ───────────────────────────────────────────────────

/// Decode V3 packed path: token(20) | fee(3) | token(20) [| fee(3) | token(20) ...]
/// Returns (first_token, last_token, first_fee)
fn decode_v3_path(path: &[u8]) -> Option<(Address, Address, u32)> {
    // Minimum path: token(20) + fee(3) + token(20) = 43 bytes
    if path.len() < 43 {
        return None;
    }

    let token_in = Address::from_slice(&path[0..20]);
    let fee = u32::from(path[20]) << 16 | u32::from(path[21]) << 8 | u32::from(path[22]);

    // Last token starts 20 bytes from the end
    let token_out = Address::from_slice(&path[path.len() - 20..]);

    Some((token_in, token_out, fee))
}

/// Extract first and last tokens from V2 address[] path
fn extract_v2_path(token: &Token) -> (Option<Address>, Option<Address>) {
    if let Token::Array(addresses) = token {
        let first = addresses.first().and_then(|t| token_to_address(t));
        let last = addresses.last().and_then(|t| token_to_address(t));
        (first, last)
    } else {
        (None, None)
    }
}

// ── Token → Rust Type Helpers ───────────────────────────────────────

fn token_to_address(token: &Token) -> Option<Address> {
    match token {
        Token::Address(addr) => Some(*addr),
        _ => None,
    }
}

fn token_to_u256(token: &Token) -> Option<U256> {
    match token {
        Token::Uint(val) => Some(*val),
        _ => None,
    }
}

fn token_to_u32(token: &Token) -> Option<u32> {
    match token {
        Token::Uint(val) => Some(val.low_u32()),
        _ => None,
    }
}

fn token_to_bytes(token: &Token) -> Option<Vec<u8>> {
    match token {
        Token::Bytes(bytes) => Some(bytes.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::utils::hex;

    #[test]
    fn test_selector_hex() {
        let data = vec![0x41, 0x4b, 0xf3, 0x89, 0x00];
        assert_eq!(selector_hex(&data), "0x414bf389");
    }

    #[test]
    fn test_selector_hex_short() {
        let data = vec![0x41, 0x4b];
        assert_eq!(selector_hex(&data), "0x????");
    }

    #[test]
    fn test_decode_v3_path() {
        // Construct a simple 2-hop path: WETH -> 500bps -> USDC
        let mut path = Vec::new();
        // token_in: 0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619 (WETH on Polygon)
        path.extend_from_slice(
            &hex::decode("7ceB23fD6bC0adD59E62ac25578270cFf1b9f619").unwrap(),
        );
        // fee: 500 (0x0001F4)
        path.push(0x00);
        path.push(0x01);
        path.push(0xf4);
        // token_out: 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174 (USDC.e)
        path.extend_from_slice(
            &hex::decode("2791Bca1f2de4661ED88A30C99A7a9449Aa84174").unwrap(),
        );

        let (token_in, token_out, fee) = decode_v3_path(&path).unwrap();
        assert_eq!(fee, 500);
        assert_eq!(
            format!("{:?}", token_in).to_lowercase(),
            "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619"
        );
        assert_eq!(
            format!("{:?}", token_out).to_lowercase(),
            "0x2791bca1f2de4661ed88a30c99a7a9449aa84174"
        );
    }
}
