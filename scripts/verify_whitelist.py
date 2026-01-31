#!/usr/bin/env python3
"""
Whitelist Pool Verifier

Purpose:
    Verifies all pools in config/{chain}/pools_whitelist.json are valid on-chain.
    Runs 5 checks per whitelisted pool:
      1. Pool exists (eth_getCode)
      2. slot0 is valid (sqrtPriceX96 > 0)
      3. Liquidity meets threshold
      4. On-chain fee matches whitelist fee_tier
      5. Mini quote check ($1 USDC via Quoter)

    Also confirms blacklisted pools remain dead and checks observation pools.

Author: AI-Generated
Created: 2026-01-29
Modified: 2026-01-30 - Dual-quoter support (V1 for Uniswap, V2 for SushiSwap V3)
Modified: 2026-01-31 - Multi-chain: --chain polygon|base, chain-specific addresses/quoters

Dependencies:
    - python3 (standard library only — no pip packages)
    - curl (for JSON-RPC calls)

Usage:
    python3 scripts/verify_whitelist.py                          # Polygon (default)
    python3 scripts/verify_whitelist.py --chain base             # Base
    python3 scripts/verify_whitelist.py --chain base --verbose   # Base with debug output
    python3 scripts/verify_whitelist.py --update                 # + update timestamps
    python3 scripts/verify_whitelist.py --rpc https://...        # Custom RPC endpoint

Notes:
    - All RPC calls are read-only (eth_call / eth_getCode). Zero gas, zero risk.
    - Quote check uses quoteExactInputSingle — also read-only.
    - Exit code: 0 = all whitelist pools PASS, 1 = any FAIL, 2 = config error.
    - Supports: polygon (chain ID 137), base (chain ID 8453).
"""

import json
import os
import subprocess
import sys
import argparse
from datetime import datetime, timezone

# --- Chain Configuration ---

CHAIN_CONFIGS = {
    "polygon": {
        "usdc_address": "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174",
        "v3_quoter": "0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6",
        "v3_quoter_version": "v1",  # Polygon uses QuoterV1 for Uniswap V3
        "sushi_v3_quoter": "0xb1E835Dc2785b52265711e17fCCb0fd018226a6e",
        "default_rpc": "https://polygon-bor.publicnode.com",
        "token_addresses": {
            "USDC":   "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174",
            "WETH":   "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619",
            "WMATIC": "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270",
            "WBTC":   "0x1BFD67037B42Cf73acF2047067bd4F2C47D9BfD6",
            "USDT":   "0xc2132D05D31c914a87C6611C10748AEb04B58e8F",
            "DAI":    "0x8f3Cf7ad23Cd3CaDbD9735AFf958023239c6A063",
            "LINK":   "0x53E0bca35eC356BD5ddDFebbD1Fc0fD03FaBad39",
            "UNI":    "0xb33EaAd8d922B1083446DC23f610c2567fB5180f",
        },
        "token_decimals": {
            "USDC": 6, "USDT": 6, "DAI": 18, "WETH": 18,
            "WMATIC": 18, "WBTC": 8, "LINK": 18, "UNI": 18,
        },
    },
    "base": {
        "usdc_address": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
        "v3_quoter": "0x3d4e44Eb1374240CE5F1B871ab261CD16335B76a",
        "v3_quoter_version": "v2",  # Base uses QuoterV2 for Uniswap V3
        "sushi_v3_quoter": "0xb1E835Dc2785b52265711e17fCCb0fd018226a6e",
        "default_rpc": "https://mainnet.base.org",
        "token_addresses": {
            "USDC":   "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            "WETH":   "0x4200000000000000000000000000000000000006",
            "cbETH":  "0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22",
            "DAI":    "0x50c5725949A6F0c72E6C4a641F24049A917DB0Cb",
            "USDbC":  "0xd9aAEc86B65D86f6A7B5B1b0c42FFA531710b6CA",
        },
        "token_decimals": {
            "USDC": 6, "WETH": 18, "cbETH": 18, "DAI": 18, "USDbC": 6,
        },
    },
}

# --- Mutable globals (set by configure_chain()) ---

USDC_ADDRESS = ""
V3_QUOTER = ""
V3_QUOTER_VERSION = "v1"  # "v1" or "v2"
SUSHI_V3_QUOTER = ""
DEFAULT_RPC = ""
TOKEN_ADDRESSES = {}
TOKEN_DECIMALS = {}

# Function selectors (constant across all chains)
SELECTOR_SLOT0 = "0x3850c7bd"          # slot0()
SELECTOR_LIQUIDITY = "0x1a686502"      # liquidity()
SELECTOR_FEE = "0xddca3f43"            # fee()
SELECTOR_QUOTE_V1 = "0xf7729d43"       # QuoterV1: quoteExactInputSingle(address,address,uint24,uint256,uint160)
SELECTOR_QUOTE_V2 = "0xc6a5026a"       # QuoterV2: quoteExactInputSingle((address,address,uint256,uint24,uint160))
# Backwards compat alias
SELECTOR_QUOTE = SELECTOR_QUOTE_V1

FEE_TIER_NAMES = {
    100: "0.01%",
    500: "0.05%",
    3000: "0.30%",
    10000: "1.00%",
}


def configure_chain(chain: str):
    """Set module globals from chain config. Must be called before any verification."""
    global USDC_ADDRESS, V3_QUOTER, V3_QUOTER_VERSION, SUSHI_V3_QUOTER
    global DEFAULT_RPC, TOKEN_ADDRESSES, TOKEN_DECIMALS

    if chain not in CHAIN_CONFIGS:
        print(f"{RED}ERROR: Unknown chain '{chain}'. Supported: {', '.join(CHAIN_CONFIGS.keys())}{RESET}")
        sys.exit(2)

    cfg = CHAIN_CONFIGS[chain]
    USDC_ADDRESS = cfg["usdc_address"]
    V3_QUOTER = cfg["v3_quoter"]
    V3_QUOTER_VERSION = cfg["v3_quoter_version"]
    SUSHI_V3_QUOTER = cfg["sushi_v3_quoter"]
    DEFAULT_RPC = cfg["default_rpc"]
    TOKEN_ADDRESSES = cfg["token_addresses"]
    TOKEN_DECIMALS = cfg["token_decimals"]

# ANSI color codes
GREEN = "\033[92m"
RED = "\033[91m"
YELLOW = "\033[93m"
BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"


# --- RPC Helpers ---

def eth_call(rpc_url: str, to: str, data: str) -> str:
    """Execute eth_call and return hex result string. Returns empty string on error."""
    payload = json.dumps({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [{"to": to, "data": data}, "latest"],
        "id": 1,
    })
    try:
        result = subprocess.run(
            ["curl", "-s", "-X", "POST", rpc_url,
             "-H", "Content-Type: application/json",
             "-d", payload],
            capture_output=True, text=True, timeout=15,
        )
        resp = json.loads(result.stdout)
        if "result" in resp:
            return resp["result"]
        if "error" in resp:
            return ""
        return ""
    except Exception:
        return ""


def eth_get_code(rpc_url: str, address: str) -> str:
    """Execute eth_getCode and return hex bytecode string. Returns empty string on error."""
    payload = json.dumps({
        "jsonrpc": "2.0",
        "method": "eth_getCode",
        "params": [address, "latest"],
        "id": 1,
    })
    try:
        result = subprocess.run(
            ["curl", "-s", "-X", "POST", rpc_url,
             "-H", "Content-Type: application/json",
             "-d", payload],
            capture_output=True, text=True, timeout=15,
        )
        resp = json.loads(result.stdout)
        if "result" in resp:
            return resp["result"]
        return ""
    except Exception:
        return ""


def get_block_number(rpc_url: str) -> int:
    """Get current block number."""
    payload = json.dumps({
        "jsonrpc": "2.0",
        "method": "eth_blockNumber",
        "params": [],
        "id": 1,
    })
    try:
        result = subprocess.run(
            ["curl", "-s", "-X", "POST", rpc_url,
             "-H", "Content-Type: application/json",
             "-d", payload],
            capture_output=True, text=True, timeout=10,
        )
        resp = json.loads(result.stdout)
        return int(resp["result"], 16)
    except Exception:
        return 0


def pad_address(addr: str) -> str:
    """Pad address to 32-byte hex (64 chars, no 0x prefix)."""
    return addr.lower().replace("0x", "").zfill(64)


def pad_uint(val: int) -> str:
    """Pad uint to 32-byte hex (64 chars, no 0x prefix)."""
    return hex(val)[2:].zfill(64)


def hex_to_int(hex_str: str, signed: bool = False) -> int:
    """Convert hex string to integer."""
    if not hex_str or hex_str == "0x":
        return 0
    val = int(hex_str, 16)
    if signed and val >= 2**255:
        val -= 2**256
    return val


def format_big_num(val: int) -> str:
    """Format a large number with suffix (T/B/M/K)."""
    if val >= 1_000_000_000_000:
        return f"{val / 1_000_000_000_000:.1f}T"
    if val >= 1_000_000_000:
        return f"{val / 1_000_000_000:.1f}B"
    if val >= 1_000_000:
        return f"{val / 1_000_000:.1f}M"
    if val >= 1_000:
        return f"{val / 1_000:.1f}K"
    return str(val)


# --- Config Loading ---

def load_whitelist(path: str) -> dict:
    """Load and parse the whitelist JSON file."""
    if not os.path.exists(path):
        print(f"{RED}ERROR: Whitelist file not found: {path}{RESET}")
        sys.exit(2)
    try:
        with open(path, "r") as f:
            return json.load(f)
    except json.JSONDecodeError as e:
        print(f"{RED}ERROR: Invalid JSON in whitelist: {e}{RESET}")
        sys.exit(2)


def resolve_rpc_url(cli_rpc: str = None, chain: str = "polygon") -> str:
    """Resolve RPC URL from CLI arg, chain-specific .env file, or default."""
    if cli_rpc:
        return cli_rpc

    # Try to read RPC from chain-specific .env file
    # Only fall back to .env for polygon (the default); other chains use DEFAULT_RPC
    env_dir = os.path.join(os.path.dirname(__file__), "..", "src", "rust-bot")
    env_files = [os.path.join(env_dir, f".env.{chain}")]
    if chain == "polygon":
        env_files.append(os.path.join(env_dir, ".env"))

    for env_path in env_files:
        if os.path.exists(env_path):
            try:
                with open(env_path, "r") as f:
                    for line in f:
                        line = line.strip()
                        if line.startswith("RPC_URL=") and not line.startswith("#"):
                            url = line.split("=", 1)[1].strip()
                            # Skip placeholder URLs
                            if "YOUR_" in url:
                                continue
                            # Convert wss:// to https://
                            if url.startswith("wss://"):
                                url = "https://" + url[6:]
                            elif url.startswith("ws://"):
                                url = "http://" + url[5:]
                            return url
            except Exception:
                pass

    return DEFAULT_RPC


def parse_pair(pair_str: str) -> tuple:
    """Parse 'WETH/USDC' into (token0_symbol, token1_symbol)."""
    parts = pair_str.split("/")
    if len(parts) != 2:
        return (None, None)
    return (parts[0].strip(), parts[1].strip())


# --- Individual Checks ---

def check_exists(rpc_url: str, address: str, verbose: bool = False) -> dict:
    """Check 1: Pool has bytecode on-chain."""
    code = eth_get_code(rpc_url, address)
    if verbose and code:
        code_len = max(0, (len(code) - 2) // 2)  # subtract "0x", 2 hex chars = 1 byte
        print(f"    {DIM}eth_getCode: {code_len} bytes{RESET}")

    if not code or code in ("0x", "0x0", ""):
        return {"pass": False, "detail": "No bytecode (empty address)"}

    code_len = (len(code) - 2) // 2
    return {"pass": True, "detail": f"{code_len} bytes"}


def check_slot0(rpc_url: str, address: str, verbose: bool = False) -> dict:
    """Check 2: slot0() returns valid sqrtPriceX96."""
    resp = eth_call(rpc_url, address, SELECTOR_SLOT0)
    if not resp or len(resp) < 66:
        return {"pass": False, "detail": "slot0() call failed"}

    # sqrtPriceX96 is the first 32 bytes (chars 2..66 of "0x...")
    sqrt_price_hex = resp[2:66]
    sqrt_price = hex_to_int("0x" + sqrt_price_hex)

    # tick is second 32 bytes (chars 66..130), signed int24
    tick = 0
    if len(resp) >= 130:
        tick_hex = resp[66:130]
        tick = hex_to_int("0x" + tick_hex, signed=True)

    if verbose:
        print(f"    {DIM}sqrtPriceX96: {sqrt_price}, tick: {tick}{RESET}")

    if sqrt_price == 0:
        return {"pass": False, "detail": "sqrtPriceX96 = 0 (uninitialized)"}

    return {"pass": True, "detail": f"tick={tick}"}


def check_liquidity(rpc_url: str, address: str, min_liquidity: int, verbose: bool = False) -> dict:
    """Check 3: liquidity() meets threshold from whitelist config."""
    resp = eth_call(rpc_url, address, SELECTOR_LIQUIDITY)
    if not resp or len(resp) < 66:
        return {"pass": False, "detail": "liquidity() call failed"}

    liquidity_hex = resp[2:66]
    liquidity = hex_to_int("0x" + liquidity_hex)

    if verbose:
        print(f"    {DIM}liquidity: {liquidity} (min: {min_liquidity}){RESET}")

    if liquidity < min_liquidity:
        return {
            "pass": False,
            "detail": f"{format_big_num(liquidity)} < {format_big_num(min_liquidity)}"
        }

    return {"pass": True, "detail": format_big_num(liquidity)}


def check_fee(rpc_url: str, address: str, expected_fee: int, verbose: bool = False) -> dict:
    """Check 4: On-chain fee() matches whitelist fee_tier."""
    resp = eth_call(rpc_url, address, SELECTOR_FEE)
    if not resp or len(resp) < 66:
        return {"pass": False, "detail": "fee() call failed"}

    fee_hex = resp[2:66]
    fee = hex_to_int("0x" + fee_hex)

    if verbose:
        print(f"    {DIM}fee: {fee} (expected: {expected_fee}){RESET}")

    if fee != expected_fee:
        return {"pass": False, "detail": f"on-chain {fee} != expected {expected_fee}"}

    return {"pass": True, "detail": FEE_TIER_NAMES.get(fee, str(fee))}


def _resolve_quoter(dex: str) -> tuple:
    """Return (quoter_address, selector, param_order) for the given DEX.

    QuoterV1 (Uniswap on Polygon): params = tokenIn, tokenOut, fee, amountIn, sqrtPriceLimitX96
    QuoterV2 (SushiSwap; Uniswap on Base): params = tokenIn, tokenOut, amountIn, fee, sqrtPriceLimitX96
    """
    if dex and "sushi" in dex.lower():
        return (SUSHI_V3_QUOTER, SELECTOR_QUOTE_V2, "v2")
    # Uniswap V3: V1 on Polygon, V2 on Base (configured via V3_QUOTER_VERSION)
    if V3_QUOTER_VERSION == "v2":
        return (V3_QUOTER, SELECTOR_QUOTE_V2, "v2")
    return (V3_QUOTER, SELECTOR_QUOTE_V1, "v1")


def _build_quote_calldata(selector: str, param_order: str,
                          token_in: str, token_out: str,
                          fee_tier: int, amount_raw: int) -> str:
    """Build quoteExactInputSingle calldata for V1 or V2."""
    if param_order == "v2":
        # QuoterV2: (tokenIn, tokenOut, amountIn, fee, sqrtPriceLimitX96)
        return (
            selector
            + pad_address(token_in)
            + pad_address(token_out)
            + pad_uint(amount_raw)
            + pad_uint(fee_tier)
            + pad_uint(0)
        )
    else:
        # QuoterV1: (tokenIn, tokenOut, fee, amountIn, sqrtPriceLimitX96)
        return (
            selector
            + pad_address(token_in)
            + pad_address(token_out)
            + pad_uint(fee_tier)
            + pad_uint(amount_raw)
            + pad_uint(0)
        )


def check_quote(rpc_url: str, pair: str, fee_tier: int, amount_usdc_raw: int = 1_000_000,
                dex: str = "", verbose: bool = False) -> dict:
    """Check 5: quoteExactInputSingle with small USDC amount returns > 0."""
    token0_sym, token1_sym = parse_pair(pair)
    if not token0_sym or not token1_sym:
        return {"pass": False, "detail": f"Cannot parse pair: {pair}"}

    # Determine tokenIn (USDC) and tokenOut (the other token)
    # All pairs are TOKEN/USDC format
    if token1_sym == "USDC":
        token_in = TOKEN_ADDRESSES.get("USDC")
        token_out = TOKEN_ADDRESSES.get(token0_sym)
    elif token0_sym == "USDC":
        token_in = TOKEN_ADDRESSES.get("USDC")
        token_out = TOKEN_ADDRESSES.get(token1_sym)
    else:
        return {"pass": False, "detail": f"No USDC in pair: {pair}"}

    if not token_in or not token_out:
        unknown = token0_sym if not TOKEN_ADDRESSES.get(token0_sym) else token1_sym
        return {"pass": False, "detail": f"Unknown token: {unknown}"}

    # Route to correct quoter based on DEX type
    quoter_addr, selector, param_order = _resolve_quoter(dex)
    calldata = _build_quote_calldata(selector, param_order,
                                     token_in, token_out,
                                     fee_tier, amount_usdc_raw)

    resp = eth_call(rpc_url, quoter_addr, calldata)

    if not resp or len(resp) < 66:
        if verbose:
            print(f"    {DIM}quote: reverted or empty response{RESET}")
        return {"pass": False, "detail": "Quote reverted (pool cannot execute)"}

    output_hex = resp[2:66]
    output = hex_to_int("0x" + output_hex)

    if verbose:
        print(f"    {DIM}quote: {amount_usdc_raw} USDC raw -> {output} raw output{RESET}")

    if output == 0:
        return {"pass": False, "detail": "Quote returned 0"}

    # Format the output based on the token's decimals
    out_sym = token0_sym if token1_sym == "USDC" else token1_sym
    decimals = TOKEN_DECIMALS.get(out_sym, 18)
    human_out = output / (10 ** decimals)
    human_in = amount_usdc_raw / (10 ** 6)

    return {"pass": True, "detail": f"${human_in:.2f} -> {human_out:.6g} {out_sym}"}


# --- Blacklist Check ---

def _quote_raw(rpc_url: str, pair: str, fee_tier: int, amount_usdc_raw: int,
               dex: str = "") -> int:
    """Get raw quote output (integer) for price impact calculation."""
    token0_sym, token1_sym = parse_pair(pair)
    if not token0_sym or not token1_sym:
        return 0

    if token1_sym == "USDC":
        token_in = TOKEN_ADDRESSES.get("USDC")
        token_out = TOKEN_ADDRESSES.get(token0_sym)
    elif token0_sym == "USDC":
        token_in = TOKEN_ADDRESSES.get("USDC")
        token_out = TOKEN_ADDRESSES.get(token1_sym)
    else:
        return 0

    if not token_in or not token_out:
        return 0

    quoter_addr, selector, param_order = _resolve_quoter(dex)
    calldata = _build_quote_calldata(selector, param_order,
                                     token_in, token_out,
                                     fee_tier, amount_usdc_raw)

    resp = eth_call(rpc_url, quoter_addr, calldata)
    if not resp or len(resp) < 66:
        return 0

    return hex_to_int("0x" + resp[2:66])


def check_blacklist_pool(rpc_url: str, pool_entry: dict, verbose: bool = False) -> dict:
    """Verify a blacklisted pool is still problematic.

    Runs two quote checks:
      1. $1 USDC — can the pool execute at all?
      2. $140 USDC — can it handle the actual trade size?

    A blacklisted pool typically has some liquidity (small quotes work) but
    massive price impact at trade size. We check both to distinguish
    "truly dead" from "thin liquidity" (the more common case).
    """
    pair = pool_entry.get("pair", "")
    fee = pool_entry.get("fee_tier", 0)
    address = pool_entry.get("address", "")
    dex = pool_entry.get("dex", "")

    # Determine output token for formatting
    token0_sym, token1_sym = parse_pair(pair)
    out_sym = token0_sym if token1_sym == "USDC" else token1_sym
    out_decimals = TOKEN_DECIMALS.get(out_sym, 18)

    # Check 1: $1 USDC — basic functionality
    small_result = check_quote(rpc_url, pair, fee, amount_usdc_raw=1_000_000, dex=dex, verbose=verbose)

    # Check 2: $140 USDC — trade-size depth
    trade_result = check_quote(rpc_url, pair, fee, amount_usdc_raw=140_000_000, dex=dex, verbose=verbose)

    if not small_result["pass"] and not trade_result["pass"]:
        return {
            "still_dead": True,
            "address": address,
            "pair": pair,
            "fee_tier": fee,
            "detail": "Confirmed dead: both $1 and $140 quotes failed"
        }

    if not trade_result["pass"]:
        return {
            "still_dead": True,
            "address": address,
            "pair": pair,
            "fee_tier": fee,
            "detail": f"Thin: $1 works ({small_result['detail']}), $140 reverts"
        }

    # Both quotes succeeded — check price impact
    small_raw = _quote_raw(rpc_url, pair, fee, 1_000_000, dex=dex)
    trade_raw = _quote_raw(rpc_url, pair, fee, 140_000_000, dex=dex)

    if small_raw > 0 and trade_raw > 0:
        # Output per USDC at each size
        small_rate = small_raw / 1_000_000
        trade_rate = trade_raw / 140_000_000
        impact_pct = abs(1.0 - trade_rate / small_rate) * 100 if small_rate > 0 else 0

        trade_human = trade_raw / (10 ** out_decimals)

        if impact_pct > 5.0:
            return {
                "still_dead": True,
                "address": address,
                "pair": pair,
                "fee_tier": fee,
                "detail": f"High impact: {impact_pct:.1f}% at $140 -> {trade_human:.6g} {out_sym}"
            }
        else:
            return {
                "still_dead": False,
                "address": address,
                "pair": pair,
                "fee_tier": fee,
                "detail": f"Recovered! Impact only {impact_pct:.1f}% at $140 -> {trade_human:.6g} {out_sym}"
            }

    # Fallback
    return {
        "still_dead": False,
        "address": address,
        "pair": pair,
        "fee_tier": fee,
        "detail": f"Pool returned output at $140: {trade_result['detail']}"
    }


# --- Quote Depth Matrix ---

QUOTE_SIZES_USD = [1, 10, 100, 1000, 5000]


def run_quote_matrix(rpc_url: str, pool_entry: dict, label: str = "",
                     verbose: bool = False) -> dict:
    """Run quotes at $1, $10, $100, $1000, $5000 for a single pool.

    Returns dict with:
      - address, pair, fee_tier, label
      - quotes: list of {size_usd, raw_out, human_out, pass} per size
      - impact_pct: price impact at largest successful size vs $1
    """
    pair = pool_entry.get("pair", "")
    fee = pool_entry.get("fee_tier", 0)
    address = pool_entry.get("address", "")
    dex = pool_entry.get("dex", "")

    token0_sym, token1_sym = parse_pair(pair)
    out_sym = token0_sym if token1_sym == "USDC" else token1_sym
    out_decimals = TOKEN_DECIMALS.get(out_sym, 18)

    result = {
        "address": address,
        "pair": pair,
        "fee_tier": fee,
        "dex": dex,
        "label": label,
        "quotes": [],
        "impact_pct": None,
    }

    baseline_rate = None  # output-per-USDC at $1

    for size_usd in QUOTE_SIZES_USD:
        amount_raw = size_usd * 1_000_000  # USDC has 6 decimals
        raw_out = _quote_raw(rpc_url, pair, fee, amount_raw, dex=dex)

        passed = raw_out > 0
        human_out = raw_out / (10 ** out_decimals) if raw_out > 0 else 0.0
        # Value in USD (approximate: output_tokens * price_per_token)
        # price_per_token ~ size_usd / human_out inverted, but simpler:
        # value_usd = we can estimate from the $1 baseline
        usd_value = None
        if baseline_rate is not None and baseline_rate > 0 and raw_out > 0:
            rate = raw_out / amount_raw
            usd_value = size_usd * (rate / baseline_rate)

        entry = {
            "size_usd": size_usd,
            "raw_out": raw_out,
            "human_out": human_out,
            "pass": passed,
            "usd_value": usd_value,
        }
        result["quotes"].append(entry)

        # Track baseline from $1
        if size_usd == 1 and raw_out > 0:
            baseline_rate = raw_out / amount_raw

        if verbose and passed:
            print(f"    {DIM}${size_usd}: {human_out:.6g} {out_sym} (raw {raw_out}){RESET}")
        elif verbose:
            print(f"    {DIM}${size_usd}: REVERTED{RESET}")

    # Compute price impact: largest successful size vs $1
    if baseline_rate and baseline_rate > 0:
        # Find largest successful quote
        for q in reversed(result["quotes"]):
            if q["pass"] and q["size_usd"] > 1:
                trade_rate = q["raw_out"] / (q["size_usd"] * 1_000_000)
                impact = abs(1.0 - trade_rate / baseline_rate) * 100
                result["impact_pct"] = impact
                result["impact_size"] = q["size_usd"]
                break

    return result


def run_all_quote_matrices(rpc_url: str, whitelist_data: dict,
                           verbose: bool = False) -> list:
    """Run quote matrix for all pools (whitelist + blacklist + observation)."""
    all_results = []

    # Whitelist pools
    wl_pools = whitelist_data.get("whitelist", {}).get("pools", [])
    bl_pools = whitelist_data.get("blacklist", {}).get("pools", [])
    ob_pools = whitelist_data.get("observation", {}).get("pools", [])

    total = len(wl_pools) + len(bl_pools) + len(ob_pools)
    idx = 0

    for pool in wl_pools:
        idx += 1
        addr_short = pool.get("address", "")[:8] + ".." + pool.get("address", "")[-4:]
        pair = pool.get("pair", "?")
        fee_name = FEE_TIER_NAMES.get(pool.get("fee_tier", 0), "?")
        print(f"  [{idx}/{total}] {pair} {fee_name} ({addr_short})", flush=True)
        r = run_quote_matrix(rpc_url, pool, label="WL", verbose=verbose)
        all_results.append(r)

    for pool in bl_pools:
        idx += 1
        addr_short = pool.get("address", "")[:8] + ".." + pool.get("address", "")[-4:]
        pair = pool.get("pair", "?")
        fee_name = FEE_TIER_NAMES.get(pool.get("fee_tier", 0), "?")
        print(f"  [{idx}/{total}] {pair} {fee_name} ({addr_short}) [BL]", flush=True)
        r = run_quote_matrix(rpc_url, pool, label="BL", verbose=verbose)
        all_results.append(r)

    for pool in ob_pools:
        idx += 1
        addr_short = pool.get("address", "")[:8] + ".." + pool.get("address", "")[-4:]
        pair = pool.get("pair", "?")
        fee_name = FEE_TIER_NAMES.get(pool.get("fee_tier", 0), "?")
        dex_short = _short_dex(pool.get("dex", ""))
        print(f"  [{idx}/{total}] {pair} {fee_name} {dex_short} ({addr_short}) [OB]", flush=True)
        r = run_quote_matrix(rpc_url, pool, label="OB", verbose=verbose)
        all_results.append(r)

    return all_results


def _color_pad(text: str, color: str, width: int) -> str:
    """Right-align text to width, then wrap with ANSI color."""
    return color + text.rjust(width) + RESET


def _short_dex(dex: str) -> str:
    """Short DEX name for matrix display."""
    if not dex:
        return "Uni"
    if "sushi" in dex.lower():
        return "Sushi"
    if "uniswap" in dex.lower():
        return "Uni"
    return dex[:5]


def _format_usd_value(value: float) -> str:
    """Format a USD value for the matrix display."""
    if value < 0.01:
        return "~$0"
    elif value >= 1000:
        return f"${int(value)}"
    elif value >= 100:
        return f"${value:.0f}"
    else:
        return f"${value:.2f}"


def _usd_cell(value_est: float, input_usd: int, col_w: int) -> str:
    """Color-coded USD value cell. Green >= 90% of input, Yellow >= 50%, Red < 50%."""
    val_s = _format_usd_value(value_est)
    ratio = value_est / input_usd if input_usd > 0 else 0
    if ratio >= 0.90:
        return _color_pad(val_s, GREEN, col_w)
    elif ratio >= 0.50:
        return _color_pad(val_s, YELLOW, col_w)
    else:
        return _color_pad(val_s, RED, col_w)


def print_quote_matrix(matrix_results: list):
    """Print unified quote depth grid with dollar return values (color-coded)."""
    COL_W = 10  # width per size column

    print()
    print("=" * 120)
    print(f"  {BOLD}QUOTE DEPTH MATRIX — USD returned per trade size{RESET}")
    print(f"  {DIM}Green >= 90% | Yellow >= 50% | Red < 50% | FAIL = quote reverted{RESET}")
    print("=" * 120)

    size_headers = "".join(f"{'$'+str(s):>{COL_W}}" for s in QUOTE_SIZES_USD)
    print(f"\n  {'Pool':<14} {'Pair':<13} {'Fee':<7} {'DEX':<7} {'Tag':<5}{size_headers} {'Impact':>{COL_W}}")
    sep = "-" * 14 + " " + "-" * 12 + " " + "-" * 6 + " " + "-" * 6 + " " + "-" * 4
    sep += (" " + "-" * (COL_W - 1)) * len(QUOTE_SIZES_USD)
    sep += " " + "-" * (COL_W - 1)
    print(f"  {sep}")

    for r in matrix_results:
        addr = r["address"][:6] + ".." + r["address"][-4:]
        pair = r["pair"]
        fee = FEE_TIER_NAMES.get(r["fee_tier"], str(r["fee_tier"]))
        dex = _short_dex(r.get("dex", ""))
        label = f"[{r['label']}]" if r["label"] else ""

        cells = ""
        baseline_q = r["quotes"][0]  # $1 quote for baseline rate

        for q in r["quotes"]:
            if not q["pass"]:
                cells += _color_pad("FAIL", RED, COL_W)
            elif not baseline_q["pass"] or baseline_q["raw_out"] == 0:
                cells += _color_pad("?", YELLOW, COL_W)
            else:
                # Estimate USD value: compare rate at this size vs rate at $1
                tokens_per_dollar = baseline_q["raw_out"] / 1_000_000
                value_est = (q["raw_out"] / tokens_per_dollar) / 1_000_000 if tokens_per_dollar > 0 else 0
                cells += _usd_cell(value_est, q["size_usd"], COL_W)

        # Impact column
        if r["impact_pct"] is not None:
            impact = r["impact_pct"]
            impact_txt = f"{impact:.1f}%"
            if impact > 10:
                impact_s = _color_pad(impact_txt, RED, COL_W)
            elif impact > 2:
                impact_s = _color_pad(impact_txt, YELLOW, COL_W)
            else:
                impact_s = _color_pad(impact_txt, GREEN, COL_W)
        else:
            impact_s = "--".rjust(COL_W)

        print(f"  {addr:<14} {pair:<13} {fee:<7} {dex:<7} {label:<5}{cells} {impact_s}")

    print()


# --- Orchestration ---

def verify_pool(rpc_url: str, pool_entry: dict, verbose: bool = False) -> dict:
    """Run all 5 checks on a single pool. Short-circuits if pool doesn't exist."""
    address = pool_entry.get("address", "")
    pair = pool_entry.get("pair", "")
    fee_tier = pool_entry.get("fee_tier", 0)
    min_liq = pool_entry.get("min_liquidity", 1_000_000_000)

    result = {
        "address": address,
        "pair": pair,
        "fee_tier": fee_tier,
        "checks": {},
        "overall": False,
    }

    # Check 1: Exists
    exists = check_exists(rpc_url, address, verbose)
    result["checks"]["exists"] = exists
    if not exists["pass"]:
        # Short-circuit: no point checking further
        result["checks"]["slot0"] = {"pass": False, "detail": "Skipped (no bytecode)"}
        result["checks"]["liquidity"] = {"pass": False, "detail": "Skipped"}
        result["checks"]["fee"] = {"pass": False, "detail": "Skipped"}
        result["checks"]["quote"] = {"pass": False, "detail": "Skipped"}
        return result

    # Check 2: slot0
    result["checks"]["slot0"] = check_slot0(rpc_url, address, verbose)

    # Check 3: Liquidity
    result["checks"]["liquidity"] = check_liquidity(rpc_url, address, min_liq, verbose)

    # Check 4: Fee
    result["checks"]["fee"] = check_fee(rpc_url, address, fee_tier, verbose)

    # Check 5: Quote
    dex = pool_entry.get("dex", "")
    result["checks"]["quote"] = check_quote(rpc_url, pair, fee_tier, dex=dex, verbose=verbose)

    # Overall = all pass
    result["overall"] = all(c["pass"] for c in result["checks"].values())

    return result


def verify_all_pools(rpc_url: str, pools: list, label: str, verbose: bool = False) -> list:
    """Verify a list of pool entries with progress output."""
    results = []
    total = len(pools)
    for i, pool in enumerate(pools):
        addr_short = pool.get("address", "")[:8] + "..." + pool.get("address", "")[-4:]
        pair = pool.get("pair", "?")
        fee_name = FEE_TIER_NAMES.get(pool.get("fee_tier", 0), "?")
        print(f"  [{i+1}/{total}] {pair} {fee_name} ({addr_short}) ...", end="", flush=True)

        result = verify_pool(rpc_url, pool, verbose)
        results.append(result)

        if result["overall"]:
            print(f" {GREEN}PASS{RESET}")
        else:
            failed = [k for k, v in result["checks"].items() if not v["pass"]]
            print(f" {RED}FAIL{RESET} ({', '.join(failed)})")

    return results


# --- Output Formatting ---

def status_str(passed: bool) -> str:
    """Return colored PASS/FAIL string."""
    if passed:
        return f"{GREEN}PASS{RESET}"
    return f"{RED}FAIL{RESET}"


def print_report(whitelist_results: list, blacklist_results: list,
                 observation_results: list, fee_tier_entries: list,
                 block: int, rpc_url: str, whitelist_path: str):
    """Print the full verification report."""
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")

    print()
    print("=" * 70)
    print(f"  {BOLD}WHITELIST VERIFICATION REPORT{RESET}")
    print(f"  {now} | Block: {block}")
    print(f"  RPC: {rpc_url}")
    print(f"  Whitelist: {whitelist_path}")
    print("=" * 70)

    # Whitelisted pools table
    if whitelist_results:
        print(f"\n  {BOLD}WHITELISTED POOLS ({len(whitelist_results)}){RESET}\n")
        # Header
        print(f"  {'Address':<20} {'Pair':<12} {'Fee':<6} {'Exists':<7} {'slot0':<7} {'Liq':<14} {'Fee':<7} {'Quote':<28} {'Result':<6}")
        print(f"  {'-'*19} {'-'*11} {'-'*5} {'-'*6} {'-'*6} {'-'*13} {'-'*6} {'-'*27} {'-'*6}")

        for r in whitelist_results:
            addr = r["address"][:8] + ".." + r["address"][-4:]
            pair = r["pair"]
            fee = FEE_TIER_NAMES.get(r["fee_tier"], str(r["fee_tier"]))
            checks = r["checks"]

            exists_s = status_str(checks["exists"]["pass"])
            slot0_s = status_str(checks["slot0"]["pass"])
            liq_s = status_str(checks["liquidity"]["pass"])
            liq_detail = checks["liquidity"]["detail"] if checks["liquidity"]["pass"] else checks["liquidity"]["detail"]
            fee_s = status_str(checks["fee"]["pass"])
            quote_s = status_str(checks["quote"]["pass"])
            quote_detail = checks["quote"]["detail"]
            overall_s = status_str(r["overall"])

            # Truncate quote detail for table width
            if len(quote_detail) > 22:
                quote_detail = quote_detail[:22] + ".."

            print(f"  {addr:<20} {pair:<12} {fee:<6} {exists_s:<16} {slot0_s:<16} {liq_detail:<14} {fee_s:<16} {quote_detail:<28} {overall_s}")

    # Blacklisted pools
    if blacklist_results:
        print(f"\n  {BOLD}BLACKLISTED POOLS ({len(blacklist_results)}){RESET}\n")
        for br in blacklist_results:
            addr = br["address"][:8] + ".." + br["address"][-4:]
            pair = br["pair"]
            fee = FEE_TIER_NAMES.get(br["fee_tier"], str(br["fee_tier"]))
            if br["still_dead"]:
                status = f"{GREEN}Still dead{RESET}"
            else:
                status = f"{YELLOW}MAY HAVE RECOVERED{RESET}"
            print(f"  {addr} | {pair:<12} | {fee:<6} | {status} | {br['detail']}")

    # Blacklisted fee tiers
    if fee_tier_entries:
        print(f"\n  {BOLD}BLACKLISTED FEE TIERS ({len(fee_tier_entries)}){RESET}\n")
        for ft in fee_tier_entries:
            tier = ft.get("tier", "?")
            tier_name = FEE_TIER_NAMES.get(tier, str(tier))
            reason = ft.get("reason", "")
            print(f"  - {tier} ({tier_name}): {reason}")

    # Observation pools
    if observation_results:
        print(f"\n  {BOLD}OBSERVATION POOLS ({len(observation_results)}){RESET}\n")
        print(f"  {'Address':<20} {'Pair':<12} {'Fee':<6} {'Exists':<7} {'slot0':<7} {'Liq':<14} {'Fee':<7} {'Quote':<28} {'Result':<6}")
        print(f"  {'-'*19} {'-'*11} {'-'*5} {'-'*6} {'-'*6} {'-'*13} {'-'*6} {'-'*27} {'-'*6}")

        for r in observation_results:
            addr = r["address"][:8] + ".." + r["address"][-4:]
            pair = r["pair"]
            fee = FEE_TIER_NAMES.get(r["fee_tier"], str(r["fee_tier"]))
            checks = r["checks"]

            exists_s = status_str(checks["exists"]["pass"])
            slot0_s = status_str(checks["slot0"]["pass"])
            liq_detail = checks["liquidity"]["detail"]
            fee_s = status_str(checks["fee"]["pass"])
            quote_detail = checks["quote"]["detail"]
            overall_s = status_str(r["overall"])

            if len(quote_detail) > 22:
                quote_detail = quote_detail[:22] + ".."

            print(f"  {addr:<20} {pair:<12} {fee:<6} {exists_s:<16} {slot0_s:<16} {liq_detail:<14} {fee_s:<16} {quote_detail:<28} {overall_s}")


def print_summary(whitelist_results: list, blacklist_results: list, observation_results: list):
    """Print aggregated summary."""
    print()
    print("=" * 70)
    print(f"  {BOLD}SUMMARY{RESET}")
    print("=" * 70)

    # Whitelist
    wl_pass = sum(1 for r in whitelist_results if r["overall"])
    wl_total = len(whitelist_results)
    wl_color = GREEN if wl_pass == wl_total else RED
    print(f"  Whitelist:    {wl_color}{wl_pass}/{wl_total} PASS{RESET}")

    # Blacklist
    if blacklist_results:
        bl_dead = sum(1 for r in blacklist_results if r["still_dead"])
        bl_total = len(blacklist_results)
        bl_color = GREEN if bl_dead == bl_total else YELLOW
        print(f"  Blacklist:    {bl_color}{bl_dead}/{bl_total} confirmed dead{RESET}")

    # Observation
    if observation_results:
        ob_pass = sum(1 for r in observation_results if r["overall"])
        ob_total = len(observation_results)
        print(f"  Observation:  {ob_pass}/{ob_total} PASS")

    # List failures
    failures = [r for r in whitelist_results if not r["overall"]]
    if failures:
        print(f"\n  {RED}{BOLD}FAILED POOLS:{RESET}")
        for r in failures:
            addr = r["address"][:8] + ".." + r["address"][-4:]
            pair = r["pair"]
            fee = FEE_TIER_NAMES.get(r["fee_tier"], str(r["fee_tier"]))
            failed_checks = {k: v for k, v in r["checks"].items() if not v["pass"]}
            reasons = "; ".join(f"{k}: {v['detail']}" for k, v in failed_checks.items())
            print(f"    {RED}-{RESET} {addr} {pair} {fee} -- {reasons}")

    # Warnings (blacklist pools that recovered)
    warnings = [r for r in blacklist_results if not r["still_dead"]]
    if warnings:
        print(f"\n  {YELLOW}{BOLD}WARNINGS:{RESET}")
        for w in warnings:
            addr = w["address"][:8] + ".." + w["address"][-4:]
            print(f"    {YELLOW}-{RESET} {addr} {w['pair']} -- {w['detail']}")

    if not failures and not warnings:
        print(f"\n  {GREEN}All checks passed.{RESET}")

    print("=" * 70)
    print()

    return len(failures)


# --- Whitelist Timestamp Update ---

def update_timestamps(whitelist_path: str, whitelist_results: list):
    """Update last_verified for pools that passed all checks."""
    try:
        with open(whitelist_path, "r") as f:
            data = json.load(f)
    except Exception as e:
        print(f"{RED}ERROR: Cannot read whitelist for update: {e}{RESET}")
        return

    now_str = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    updated_count = 0

    # Build a set of passing addresses
    passing = set()
    for r in whitelist_results:
        if r["overall"]:
            passing.add(r["address"].lower())

    # Update whitelist pools
    for pool in data.get("whitelist", {}).get("pools", []):
        if pool.get("address", "").lower() in passing:
            pool["last_verified"] = now_str
            updated_count += 1

    # Write back
    try:
        with open(whitelist_path, "w") as f:
            json.dump(data, f, indent=2)
            f.write("\n")
        print(f"  Updated {updated_count} pool timestamps to {now_str}")
    except Exception as e:
        print(f"{RED}ERROR: Cannot write whitelist update: {e}{RESET}")


# --- Main ---

def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Verify whitelist pools are valid on-chain"
    )
    parser.add_argument(
        "--chain", "-c",
        default="polygon",
        choices=list(CHAIN_CONFIGS.keys()),
        help="Chain to verify (default: polygon)"
    )
    parser.add_argument(
        "--whitelist", "-w",
        default=None,
        help="Path to pools_whitelist.json (default: config/{chain}/pools_whitelist.json)"
    )
    parser.add_argument(
        "--rpc", "-r",
        default=None,
        help="RPC URL (default: read from .env or use chain default)"
    )
    parser.add_argument(
        "--update", "-u",
        action="store_true",
        help="Update last_verified timestamps for passing pools"
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Show raw hex values for debugging"
    )

    args = parser.parse_args()

    # Configure chain-specific constants
    configure_chain(args.chain)

    # Default whitelist path: config/{chain}/pools_whitelist.json
    if args.whitelist is None:
        args.whitelist = os.path.join(
            os.path.dirname(__file__), "..", "config", args.chain, "pools_whitelist.json"
        )

    # Resolve paths and config
    whitelist_path = os.path.abspath(args.whitelist)
    rpc_url = resolve_rpc_url(args.rpc, chain=args.chain)
    data = load_whitelist(whitelist_path)

    # Get block number
    block = get_block_number(rpc_url)
    if block == 0:
        print(f"{RED}ERROR: Cannot connect to RPC at {rpc_url}{RESET}")
        sys.exit(2)

    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    print(f"\n  Whitelist Verifier | {args.chain.upper()} | {now} | Block {block}")
    print(f"  RPC: {rpc_url}")
    print(f"  File: {whitelist_path}\n")

    # --- Whitelist pools ---
    whitelist_pools = data.get("whitelist", {}).get("pools", [])
    print(f"  {BOLD}Verifying {len(whitelist_pools)} whitelisted pools...{RESET}\n")
    whitelist_results = verify_all_pools(rpc_url, whitelist_pools, "whitelist", args.verbose)

    # --- Blacklisted pools ---
    blacklist_pools = data.get("blacklist", {}).get("pools", [])
    blacklist_results = []
    if blacklist_pools:
        print(f"\n  {BOLD}Verifying {len(blacklist_pools)} blacklisted pool(s)...{RESET}\n")
        for i, bp in enumerate(blacklist_pools):
            addr_short = bp.get("address", "")[:8] + "..." + bp.get("address", "")[-4:]
            pair = bp.get("pair", "?")
            fee_name = FEE_TIER_NAMES.get(bp.get("fee_tier", 0), "?")
            print(f"  [{i+1}/{len(blacklist_pools)}] {pair} {fee_name} ({addr_short}) ...", end="", flush=True)

            result = check_blacklist_pool(rpc_url, bp, args.verbose)
            blacklist_results.append(result)

            if result["still_dead"]:
                print(f" {GREEN}Confirmed dead{RESET}")
            else:
                print(f" {YELLOW}WARNING: May have recovered!{RESET}")

    # --- Blacklisted fee tiers ---
    fee_tier_entries = data.get("blacklist", {}).get("fee_tiers", [])

    # --- Observation pools ---
    observation_pools = data.get("observation", {}).get("pools", [])
    observation_results = []
    if observation_pools:
        print(f"\n  {BOLD}Verifying {len(observation_pools)} observation pool(s)...{RESET}\n")
        # Observation pools may lack min_liquidity; use config default
        default_min = data.get("config", {}).get("default_min_liquidity", 1_000_000_000)
        for op in observation_pools:
            if "min_liquidity" not in op:
                op["min_liquidity"] = default_min
        observation_results = verify_all_pools(rpc_url, observation_pools, "observation", args.verbose)

    # --- Quote Depth Matrix ---
    print(f"\n  {BOLD}Running quote depth matrix (${', $'.join(str(s) for s in QUOTE_SIZES_USD)})...{RESET}\n")
    matrix_results = run_all_quote_matrices(rpc_url, data, args.verbose)
    print_quote_matrix(matrix_results)

    # --- Report ---
    print_report(whitelist_results, blacklist_results, observation_results,
                 fee_tier_entries, block, rpc_url, whitelist_path)
    failure_count = print_summary(whitelist_results, blacklist_results, observation_results)

    # --- Update timestamps ---
    if args.update:
        print(f"  {BOLD}Updating whitelist timestamps...{RESET}")
        update_timestamps(whitelist_path, whitelist_results)

    # Exit code
    sys.exit(1 if failure_count > 0 else 0)


if __name__ == "__main__":
    main()
