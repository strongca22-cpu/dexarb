#!/usr/bin/env python3
"""
Enhanced Whitelist Pool Verifier with Liquidity Analysis

Purpose:
    Advanced pool verification with automated categorization:
      - WHITELIST: Passes all checks, handles large trades ($5k+) with <5% impact
      - MARGINAL: Works for small trades ($1-$100) but fails or has high impact at large sizes
      - BLACKLIST: Fails basic checks or has >20% impact even at small sizes
    
    Runs comprehensive analysis:
      1. Pool existence & state (slot0, liquidity)
      2. Quote depth matrix ($1, $10, $100, $1000, $5000)
      3. Price impact analysis at each size
      4. Liquidity depth metrics
      5. Automated categorization with recommendations

Author: Enhanced version
Created: 2026-01-30
Based on: Original verify_whitelist.py

Dependencies:
    - python3 (standard library only)
    - curl (for JSON-RPC calls)

Usage:
    python3 scripts/verify_whitelist_enhanced.py                    # Full analysis
    python3 scripts/verify_whitelist_enhanced.py --categorize       # Show recommendations
    python3 scripts/verify_whitelist_enhanced.py --rpc https://...  # Custom RPC
    python3 scripts/verify_whitelist_enhanced.py --verbose          # Detailed output

Exit Codes:
    0 = All checks pass
    1 = Some pools failed or need recategorization
    2 = Configuration error
"""

import json
import os
import subprocess
import sys
import argparse
from datetime import datetime, timezone
from typing import Dict, List, Tuple, Optional

# --- Constants ---

USDC_ADDRESS = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"
V3_FACTORY = "0x1F98431c8aD98523631AE4a59f267346ea31F984"
V3_QUOTER = "0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6"
SUSHI_V3_QUOTER = "0xb1E835Dc2785b52265711e17fCCb0fd018226a6e"
DEFAULT_RPC = "https://polygon-bor.publicnode.com"

# Quote sizes for depth analysis
QUOTE_SIZES_USD = [1, 10, 100, 1000, 5000]

# Categorization thresholds
THRESHOLDS = {
    "marginal_max_size": 100,      # Marginal pools work up to $100
    "whitelist_min_size": 1000,    # Whitelist pools must handle $1000+
    "impact_whitelist": 5.0,       # Max impact % for whitelist
    "impact_marginal": 10.0,       # Max impact % for marginal at small sizes
    "impact_blacklist": 20.0,      # Blacklist if impact > 20% even at $1
    "min_liquidity_base": 1_000_000_000,  # Minimum raw liquidity
}

# Function selectors
SELECTOR_SLOT0 = "0x3850c7bd"
SELECTOR_LIQUIDITY = "0x1a686502"
SELECTOR_FEE = "0xddca3f43"
SELECTOR_QUOTE_V1 = "0xf7729d43"
SELECTOR_QUOTE_V2 = "0xc6a5026a"

FEE_TIER_NAMES = {
    100: "0.01%",
    500: "0.05%",
    3000: "0.30%",
    10000: "1.00%",
}

TOKEN_ADDRESSES = {
    "USDC":   "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174",
    "WETH":   "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619",
    "WMATIC": "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270",
    "WBTC":   "0x1BFD67037B42Cf73acF2047067bd4F2C47D9BfD6",
    "USDT":   "0xc2132D05D31c914a87C6611C10748AEb04B58e8F",
    "DAI":    "0x8f3Cf7ad23Cd3CaDbD9735AFf958023239c6A063",
    "LINK":   "0x53E0bca35eC356BD5ddDFebbD1Fc0fD03FaBad39",
}

TOKEN_DECIMALS = {
    "USDC": 6, "USDT": 6, "DAI": 18, "WETH": 18,
    "WMATIC": 18, "WBTC": 8, "LINK": 18,
}

# ANSI colors
GREEN = "\033[92m"
RED = "\033[91m"
YELLOW = "\033[93m"
BLUE = "\033[94m"
CYAN = "\033[96m"
MAGENTA = "\033[95m"
BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"


# --- RPC Helpers ---

def eth_call(rpc_url: str, to: str, data: str) -> str:
    """Execute eth_call and return hex result."""
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
        return resp.get("result", "")
    except Exception:
        return ""


def eth_get_code(rpc_url: str, address: str) -> str:
    """Execute eth_getCode and return hex bytecode."""
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
        return resp.get("result", "")
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
    """Pad address to 32-byte hex."""
    return addr.lower().replace("0x", "").zfill(64)


def pad_uint(val: int) -> str:
    """Pad uint to 32-byte hex."""
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
    """Format large number with suffix."""
    if val >= 1_000_000_000_000:
        return f"{val / 1_000_000_000_000:.2f}T"
    if val >= 1_000_000_000:
        return f"{val / 1_000_000_000:.2f}B"
    if val >= 1_000_000:
        return f"{val / 1_000_000:.2f}M"
    if val >= 1_000:
        return f"{val / 1_000:.2f}K"
    return str(val)


# --- Config Loading ---

def load_whitelist(path: str) -> dict:
    """Load whitelist JSON file."""
    if not os.path.exists(path):
        print(f"{RED}ERROR: Whitelist file not found: {path}{RESET}")
        sys.exit(2)
    try:
        with open(path, "r") as f:
            return json.load(f)
    except json.JSONDecodeError as e:
        print(f"{RED}ERROR: Invalid JSON: {e}{RESET}")
        sys.exit(2)


def resolve_rpc_url(cli_rpc: str = None) -> str:
    """Resolve RPC URL from CLI, .env, or default."""
    if cli_rpc:
        return cli_rpc

    env_path = os.path.join(os.path.dirname(__file__), "..", "src", "rust-bot", ".env")
    if os.path.exists(env_path):
        try:
            with open(env_path, "r") as f:
                for line in f:
                    line = line.strip()
                    if line.startswith("RPC_URL=") and not line.startswith("#"):
                        url = line.split("=", 1)[1].strip()
                        if url.startswith("wss://"):
                            url = "https://" + url[6:]
                        elif url.startswith("ws://"):
                            url = "http://" + url[5:]
                        return url
        except Exception:
            pass

    return DEFAULT_RPC


def parse_pair(pair_str: str) -> tuple:
    """Parse 'WETH/USDC' into (token0_sym, token1_sym)."""
    parts = pair_str.split("/")
    if len(parts) != 2:
        return (None, None)
    return (parts[0].strip(), parts[1].strip())


# --- Pool State Analysis ---

def get_pool_state(rpc_url: str, address: str, verbose: bool = False) -> dict:
    """Get comprehensive pool state: exists, slot0, liquidity, fee."""
    state = {
        "exists": False,
        "sqrt_price": 0,
        "tick": 0,
        "liquidity": 0,
        "fee": 0,
        "initialized": False,
    }

    # Check exists
    code = eth_get_code(rpc_url, address)
    if not code or code in ("0x", "0x0"):
        return state
    state["exists"] = True

    # slot0
    resp = eth_call(rpc_url, address, SELECTOR_SLOT0)
    if resp and len(resp) >= 66:
        sqrt_price = hex_to_int("0x" + resp[2:66])
        state["sqrt_price"] = sqrt_price
        if len(resp) >= 130:
            tick = hex_to_int("0x" + resp[66:130], signed=True)
            state["tick"] = tick
        if sqrt_price > 0:
            state["initialized"] = True

    # liquidity
    resp = eth_call(rpc_url, address, SELECTOR_LIQUIDITY)
    if resp and len(resp) >= 66:
        liquidity = hex_to_int("0x" + resp[2:66])
        state["liquidity"] = liquidity

    # fee
    resp = eth_call(rpc_url, address, SELECTOR_FEE)
    if resp and len(resp) >= 66:
        fee = hex_to_int("0x" + resp[2:66])
        state["fee"] = fee

    if verbose:
        print(f"    {DIM}Pool state: sqrt={state['sqrt_price']}, tick={state['tick']}, " +
              f"liq={format_big_num(state['liquidity'])}, fee={state['fee']}{RESET}")

    return state


# --- Quoter Functions ---

def _resolve_quoter(dex: str) -> tuple:
    """Return (quoter_address, selector, param_order)."""
    if dex and "sushi" in dex.lower():
        return (SUSHI_V3_QUOTER, SELECTOR_QUOTE_V2, "v2")
    return (V3_QUOTER, SELECTOR_QUOTE_V1, "v1")


def _build_quote_calldata(selector: str, param_order: str,
                          token_in: str, token_out: str,
                          fee_tier: int, amount_raw: int) -> str:
    """Build quoteExactInputSingle calldata."""
    if param_order == "v2":
        return (
            selector
            + pad_address(token_in)
            + pad_address(token_out)
            + pad_uint(amount_raw)
            + pad_uint(fee_tier)
            + pad_uint(0)
        )
    else:
        return (
            selector
            + pad_address(token_in)
            + pad_address(token_out)
            + pad_uint(fee_tier)
            + pad_uint(amount_raw)
            + pad_uint(0)
        )


def quote_raw(rpc_url: str, pair: str, fee_tier: int, amount_usdc_raw: int,
              dex: str = "") -> int:
    """Get raw quote output (integer)."""
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


# --- Quote Depth Matrix ---

def run_quote_matrix(rpc_url: str, pool_entry: dict,
                     verbose: bool = False) -> dict:
    """Run quotes at all QUOTE_SIZES_USD and calculate metrics.
    
    Returns:
        - quotes: list of {size_usd, raw_out, human_out, pass, impact_pct, value_usd}
        - max_working_size: largest size that succeeds
        - impact_at_max: price impact at max_working_size
        - liquidity_score: 0-100 based on how well it handles depth
    """
    pair = pool_entry.get("pair", "")
    fee = pool_entry.get("fee_tier", 0)
    address = pool_entry.get("address", "")
    dex = pool_entry.get("dex", "")

    token0_sym, token1_sym = parse_pair(pair)
    out_sym = token0_sym if token1_sym == "USDC" else token1_sym
    out_decimals = TOKEN_DECIMALS.get(out_sym, 18)

    # Get pool state
    pool_state = get_pool_state(rpc_url, address, verbose)

    result = {
        "address": address,
        "pair": pair,
        "fee_tier": fee,
        "dex": dex,
        "pool_state": pool_state,
        "quotes": [],
        "max_working_size": 0,
        "impact_at_max": None,
        "liquidity_score": 0,
        "category_suggestion": "blacklist",
        "category_reason": "",
    }

    if not pool_state["exists"] or not pool_state["initialized"]:
        result["category_reason"] = "Pool doesn't exist or not initialized"
        return result

    baseline_rate = None  # output-per-USDC at $1

    for size_usd in QUOTE_SIZES_USD:
        amount_raw = size_usd * 1_000_000  # USDC has 6 decimals
        raw_out = quote_raw(rpc_url, pair, fee, amount_raw, dex=dex)

        passed = raw_out > 0
        human_out = raw_out / (10 ** out_decimals) if raw_out > 0 else 0.0

        # Calculate price impact vs $1 baseline
        impact_pct = None
        if baseline_rate and baseline_rate > 0 and raw_out > 0:
            current_rate = raw_out / amount_raw
            impact_pct = abs(1.0 - current_rate / baseline_rate) * 100

        # Estimate USD value returned
        value_usd = None
        if baseline_rate and baseline_rate > 0 and raw_out > 0:
            rate = raw_out / amount_raw
            value_usd = size_usd * (rate / baseline_rate)

        entry = {
            "size_usd": size_usd,
            "raw_out": raw_out,
            "human_out": human_out,
            "pass": passed,
            "impact_pct": impact_pct,
            "value_usd": value_usd,
        }
        result["quotes"].append(entry)

        # Track baseline from $1
        if size_usd == 1 and raw_out > 0:
            baseline_rate = raw_out / amount_raw

        if passed:
            result["max_working_size"] = size_usd
            if impact_pct is not None:
                result["impact_at_max"] = impact_pct

        if verbose and passed:
            impact_str = f" (impact: {impact_pct:.1f}%)" if impact_pct else ""
            print(f"    {DIM}${size_usd}: {human_out:.6g} {out_sym}{impact_str}{RESET}")
        elif verbose:
            print(f"    {DIM}${size_usd}: REVERTED{RESET}")

    # Calculate liquidity score (0-100)
    result["liquidity_score"] = calculate_liquidity_score(result)

    # Categorize pool
    result["category_suggestion"], result["category_reason"] = categorize_pool(result)

    return result


def calculate_liquidity_score(matrix_result: dict) -> int:
    """Calculate 0-100 liquidity score based on quote matrix performance.
    
    Scoring:
      - 100: Handles $5k with <2% impact
      - 80-99: Handles $5k with 2-5% impact
      - 60-79: Handles $1k with <5% impact
      - 40-59: Handles $1k with 5-10% impact
      - 20-39: Handles $100 with <10% impact
      - 0-19: Fails at $100 or only works at tiny sizes
    """
    max_size = matrix_result["max_working_size"]
    impact = matrix_result["impact_at_max"]

    if max_size == 0:
        return 0

    if max_size >= 5000:
        if impact is None or impact < 2:
            return 100
        elif impact < 5:
            return 90
        elif impact < 10:
            return 75
        else:
            return 60

    if max_size >= 1000:
        if impact is None or impact < 5:
            return 70
        elif impact < 10:
            return 50
        else:
            return 30

    if max_size >= 100:
        if impact is None or impact < 10:
            return 35
        else:
            return 20

    if max_size >= 10:
        return 10

    return 5  # Only works at $1


def categorize_pool(matrix_result: dict) -> Tuple[str, str]:
    """Categorize pool into whitelist/marginal/blacklist with reason.
    
    WHITELIST criteria:
      - Works at $1000+ with <5% impact
      - Liquidity score >= 60
    
    MARGINAL criteria:
      - Works at $10-$100 range with <10% impact
      - Fails or high impact at $1000+
      - Liquidity score 20-59
    
    BLACKLIST criteria:
      - Fails at $100 or below
      - >20% impact even at small sizes
      - Liquidity score < 20
    """
    max_size = matrix_result["max_working_size"]
    impact = matrix_result["impact_at_max"]
    liq_score = matrix_result["liquidity_score"]
    pool_state = matrix_result["pool_state"]

    # Check basic pool validity
    if not pool_state["exists"]:
        return ("blacklist", "Pool doesn't exist")
    if not pool_state["initialized"]:
        return ("blacklist", "Pool not initialized (sqrtPrice = 0)")
    if pool_state["liquidity"] < THRESHOLDS["min_liquidity_base"]:
        return ("blacklist", f"Liquidity too low: {format_big_num(pool_state['liquidity'])}")

    # Check if completely non-functional
    if max_size == 0:
        return ("blacklist", "All quotes failed")

    # WHITELIST: Great depth, handles large trades
    if max_size >= THRESHOLDS["whitelist_min_size"]:
        if impact is None or impact <= THRESHOLDS["impact_whitelist"]:
            return ("whitelist", f"Excellent: ${max_size} @ {impact:.1f}% impact, score={liq_score}")
        elif impact <= THRESHOLDS["impact_marginal"]:
            return ("marginal", f"Works at ${max_size} but high impact ({impact:.1f}%)")
        else:
            return ("blacklist", f"Unacceptable impact: {impact:.1f}% at ${max_size}")

    # MARGINAL: Works for small/medium trades only
    if max_size >= 10 and max_size <= THRESHOLDS["marginal_max_size"]:
        if impact is None or impact <= THRESHOLDS["impact_marginal"]:
            return ("marginal", f"Small trade pool: max ${max_size} @ {impact:.1f}% impact")
        else:
            return ("blacklist", f"High impact even at small size: {impact:.1f}% @ ${max_size}")

    # BLACKLIST: Only works at $1 or fails completely
    if max_size < 10:
        return ("blacklist", f"Insufficient depth: only works up to ${max_size}")

    # Fallback
    return ("blacklist", f"Unknown issue: max_size=${max_size}, impact={impact}")


# --- Matrix Display ---

def print_enhanced_matrix(matrix_results: list, show_categorization: bool = False):
    """Print enhanced quote depth matrix with categorization."""
    COL_W = 10

    print()
    print("=" * 140)
    print(f"  {BOLD}ENHANCED QUOTE DEPTH MATRIX{RESET}")
    print(f"  {DIM}Green: <5% impact | Yellow: 5-10% | Red: >10% or FAIL{RESET}")
    print("=" * 140)

    # Headers
    size_headers = "".join(f"{'$'+str(s):>{COL_W}}" for s in QUOTE_SIZES_USD)
    print(f"\n  {'Pool':<14} {'Pair':<13} {'Fee':<7} {'Max$':<6} {size_headers} " +
          f"{'Impact':>{COL_W}} {'Score':<6} {'Category':<12}")
    
    sep = ("-" * 14 + " " + "-" * 12 + " " + "-" * 6 + " " + "-" * 5 +
           (" " + "-" * (COL_W - 1)) * len(QUOTE_SIZES_USD) +
           " " + "-" * (COL_W - 1) + " " + "-" * 5 + " " + "-" * 11)
    print(f"  {sep}")

    for r in matrix_results:
        addr = r["address"][:6] + ".." + r["address"][-4:]
        pair = r["pair"]
        fee = FEE_TIER_NAMES.get(r["fee_tier"], str(r["fee_tier"]))
        max_size = f"${r['max_working_size']}"
        
        cells = ""
        for q in r["quotes"]:
            if not q["pass"]:
                cells += _color_pad("FAIL", RED, COL_W)
            else:
                impact = q.get("impact_pct")
                if impact is None:
                    cells += _color_pad("OK", GREEN, COL_W)
                elif impact <= 5:
                    cells += _color_pad(f"{impact:.1f}%", GREEN, COL_W)
                elif impact <= 10:
                    cells += _color_pad(f"{impact:.1f}%", YELLOW, COL_W)
                else:
                    cells += _color_pad(f"{impact:.1f}%", RED, COL_W)

        # Impact at max
        if r["impact_at_max"] is not None:
            impact = r["impact_at_max"]
            impact_txt = f"{impact:.1f}%"
            if impact > 10:
                impact_s = _color_pad(impact_txt, RED, COL_W)
            elif impact > 5:
                impact_s = _color_pad(impact_txt, YELLOW, COL_W)
            else:
                impact_s = _color_pad(impact_txt, GREEN, COL_W)
        else:
            impact_s = "--".rjust(COL_W)

        # Liquidity score
        score = r["liquidity_score"]
        score_txt = str(score)
        
        # Category
        cat = r["category_suggestion"]
        if cat == "whitelist":
            cat_s = f"{GREEN}{cat.upper():<12}{RESET}"
        elif cat == "marginal":
            cat_s = f"{YELLOW}{cat.upper():<12}{RESET}"
        else:
            cat_s = f"{RED}{cat.upper():<12}{RESET}"

        print(f"  {addr:<14} {pair:<13} {fee:<7} {max_size:<6} {cells} " +
              f"{impact_s} {score_txt:<6} {cat_s}")

    print()


def _color_pad(text: str, color: str, width: int) -> str:
    """Right-align text with color."""
    return color + text.rjust(width) + RESET


# --- Categorization Summary ---

def print_categorization_summary(matrix_results: list):
    """Print summary of pool categorizations with recommendations."""
    whitelist = [r for r in matrix_results if r["category_suggestion"] == "whitelist"]
    marginal = [r for r in matrix_results if r["category_suggestion"] == "marginal"]
    blacklist = [r for r in matrix_results if r["category_suggestion"] == "blacklist"]

    print()
    print("=" * 140)
    print(f"  {BOLD}POOL CATEGORIZATION SUMMARY{RESET}")
    print("=" * 140)

    print(f"\n  {GREEN}{BOLD}WHITELIST ({len(whitelist)} pools){RESET} - Safe for all trade sizes")
    if whitelist:
        print(f"  {'Pool':<14} {'Pair':<13} {'Fee':<7} {'Max Size':<10} {'Impact':<10} {'Reason'}")
        print(f"  {'-'*13} {'-'*12} {'-'*6} {'-'*9} {'-'*9} {'-'*40}")
        for r in whitelist:
            addr = r["address"][:6] + ".." + r["address"][-4:]
            pair = r["pair"]
            fee = FEE_TIER_NAMES.get(r["fee_tier"], str(r["fee_tier"]))
            max_s = f"${r['max_working_size']}"
            impact = f"{r['impact_at_max']:.1f}%" if r["impact_at_max"] else "N/A"
            reason = r["category_reason"][:60]
            print(f"  {addr:<14} {pair:<13} {fee:<7} {max_s:<10} {impact:<10} {reason}")

    print(f"\n  {YELLOW}{BOLD}MARGINAL ({len(marginal)} pools){RESET} - Good for small trades only ($1-$100)")
    if marginal:
        print(f"  {'Pool':<14} {'Pair':<13} {'Fee':<7} {'Max Size':<10} {'Impact':<10} {'Reason'}")
        print(f"  {'-'*13} {'-'*12} {'-'*6} {'-'*9} {'-'*9} {'-'*40}")
        for r in marginal:
            addr = r["address"][:6] + ".." + r["address"][-4:]
            pair = r["pair"]
            fee = FEE_TIER_NAMES.get(r["fee_tier"], str(r["fee_tier"]))
            max_s = f"${r['max_working_size']}"
            impact = f"{r['impact_at_max']:.1f}%" if r["impact_at_max"] else "N/A"
            reason = r["category_reason"][:60]
            print(f"  {addr:<14} {pair:<13} {fee:<7} {max_s:<10} {impact:<10} {reason}")

    print(f"\n  {RED}{BOLD}BLACKLIST ({len(blacklist)} pools){RESET} - Should not be used")
    if blacklist:
        print(f"  {'Pool':<14} {'Pair':<13} {'Fee':<7} {'Max Size':<10} {'Reason'}")
        print(f"  {'-'*13} {'-'*12} {'-'*6} {'-'*9} {'-'*60}")
        for r in blacklist:
            addr = r["address"][:6] + ".." + r["address"][-4:]
            pair = r["pair"]
            fee = FEE_TIER_NAMES.get(r["fee_tier"], str(r["fee_tier"]))
            max_s = f"${r['max_working_size']}" if r['max_working_size'] > 0 else "FAIL"
            reason = r["category_reason"][:60]
            print(f"  {addr:<14} {pair:<13} {fee:<7} {max_s:<10} {reason}")

    print()


def print_recommendations(matrix_results: list, current_config: dict):
    """Print specific recommendations for config changes."""
    print("=" * 140)
    print(f"  {BOLD}RECOMMENDATIONS{RESET}")
    print("=" * 140)

    current_whitelist = set(p["address"].lower() for p in current_config.get("whitelist", {}).get("pools", []))
    current_blacklist = set(p["address"].lower() for p in current_config.get("blacklist", {}).get("pools", []))
    current_marginal = set(p["address"].lower() for p in current_config.get("marginal", {}).get("pools", []))

    # Find mismatches
    to_whitelist = []
    to_marginal = []
    to_blacklist = []

    for r in matrix_results:
        addr = r["address"].lower()
        suggested = r["category_suggestion"]

        if suggested == "whitelist" and addr not in current_whitelist:
            to_whitelist.append(r)
        elif suggested == "marginal" and addr not in current_marginal:
            to_marginal.append(r)
        elif suggested == "blacklist" and addr not in current_blacklist:
            to_blacklist.append(r)

    if to_whitelist:
        print(f"\n  {GREEN}► PROMOTE TO WHITELIST:{RESET}")
        for r in to_whitelist:
            print(f"    - {r['pair']} {FEE_TIER_NAMES.get(r['fee_tier'])} ({r['address'][:10]}...)")
            print(f"      Reason: {r['category_reason']}")

    if to_marginal:
        print(f"\n  {YELLOW}► ADD TO MARGINAL (soft blacklist for large trades):{RESET}")
        for r in to_marginal:
            print(f"    - {r['pair']} {FEE_TIER_NAMES.get(r['fee_tier'])} ({r['address'][:10]}...)")
            print(f"      Reason: {r['category_reason']}")

    if to_blacklist:
        print(f"\n  {RED}► MOVE TO BLACKLIST:{RESET}")
        for r in to_blacklist:
            print(f"    - {r['pair']} {FEE_TIER_NAMES.get(r['fee_tier'])} ({r['address'][:10]}...)")
            print(f"      Reason: {r['category_reason']}")

    if not to_whitelist and not to_marginal and not to_blacklist:
        print(f"\n  {GREEN}✓ All pools are correctly categorized{RESET}")

    print()


# --- Main ---

def main():
    parser = argparse.ArgumentParser(
        description="Enhanced whitelist verifier with automated categorization"
    )
    parser.add_argument(
        "--whitelist", "-w",
        default=os.path.join(os.path.dirname(__file__), "..", "config", "pools_whitelist.json"),
        help="Path to pools_whitelist.json"
    )
    parser.add_argument(
        "--rpc", "-r",
        default=None,
        help="RPC URL"
    )
    parser.add_argument(
        "--categorize", "-c",
        action="store_true",
        help="Show detailed categorization and recommendations"
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Show detailed output"
    )

    args = parser.parse_args()

    # Load config
    whitelist_path = os.path.abspath(args.whitelist)
    rpc_url = resolve_rpc_url(args.rpc)
    data = load_whitelist(whitelist_path)

    # Get block
    block = get_block_number(rpc_url)
    if block == 0:
        print(f"{RED}ERROR: Cannot connect to RPC at {rpc_url}{RESET}")
        sys.exit(2)

    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    print(f"\n  {BOLD}Enhanced Whitelist Verifier{RESET} | {now} | Block {block}")
    print(f"  RPC: {rpc_url}")
    print(f"  File: {whitelist_path}\n")

    # Collect all pools
    all_pools = []
    all_pools.extend(data.get("whitelist", {}).get("pools", []))
    all_pools.extend(data.get("blacklist", {}).get("pools", []))
    all_pools.extend(data.get("marginal", {}).get("pools", []))
    all_pools.extend(data.get("observation", {}).get("pools", []))

    print(f"  {BOLD}Analyzing {len(all_pools)} pools across all categories...{RESET}\n")

    # Run matrix analysis
    matrix_results = []
    for i, pool in enumerate(all_pools):
        addr_short = pool.get("address", "")[:8] + ".." + pool.get("address", "")[-4:]
        pair = pool.get("pair", "?")
        fee_name = FEE_TIER_NAMES.get(pool.get("fee_tier", 0), "?")
        print(f"  [{i+1}/{len(all_pools)}] {pair} {fee_name} ({addr_short})", flush=True)
        
        result = run_quote_matrix(rpc_url, pool, verbose=args.verbose)
        matrix_results.append(result)

    # Print matrix
    print_enhanced_matrix(matrix_results, show_categorization=True)

    # Categorization summary
    if args.categorize:
        print_categorization_summary(matrix_results)
        print_recommendations(matrix_results, data)

    # Exit code based on mismatches
    current_whitelist = set(p["address"].lower() for p in data.get("whitelist", {}).get("pools", []))
    mismatches = sum(1 for r in matrix_results 
                     if r["category_suggestion"] == "blacklist" and 
                     r["address"].lower() in current_whitelist)

    sys.exit(1 if mismatches > 0 else 0)


if __name__ == "__main__":
    main()
