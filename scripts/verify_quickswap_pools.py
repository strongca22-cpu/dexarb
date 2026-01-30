#!/usr/bin/env python3
"""
QuickSwap V3 (Algebra) Enhanced Pool Verifier

Purpose:
    Runs the same enhanced verification as verify_whitelist_enhanced.py
    but using the Algebra QuoterV2 selector (no fee parameter).

    Categorizes pools as:
      - WHITELIST: Handles $1000+ with <5% impact
      - MARGINAL: Works for $10-$100 but fails/high impact at $1000+
      - BLACKLIST: Fails at $100 or has >20% impact

Author: AI-Generated
Created: 2026-01-30
Based on: verify_whitelist_enhanced.py

Dependencies:
    - python3 (standard library only)
    - curl (for JSON-RPC calls)

Usage:
    python3 scripts/verify_quickswap_pools.py
"""

import json
import subprocess
import sys
from datetime import datetime, timezone


# --- Constants ---

# Algebra QuoterV2 on Polygon
ALGEBRA_QUOTER = "0xa15F0D7377B2A0C0c10db057f641beD21028FC89"

# Algebra QuoterV2: quoteExactInputSingle(address,address,uint256,uint160)
# No fee parameter — Algebra uses dynamic fees, quoter finds pool automatically
SELECTOR_ALGEBRA_QUOTE = "0x2d9ebd1d"

# Algebra globalState selector
SELECTOR_GLOBAL_STATE = "0xe76c01e4"
SELECTOR_LIQUIDITY = "0x1a686502"

RPC_URL = "https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"

USDC_E = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"

TOKEN_ADDRESSES = {
    "WETH":   "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619",
    "WMATIC": "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270",
    "WBTC":   "0x1BFD67037B42Cf73acF2047067bd4F2C47D9BfD6",
    "USDT":   "0xc2132D05D31c914a87C6611C10748AEb04B58e8F",
    "DAI":    "0x8f3Cf7ad23Cd3CaDbD9735AFf958023239c6A063",
    "LINK":   "0x53E0bca35eC356BD5ddDFebbD1Fc0fD03FaBad39",
}

TOKEN_DECIMALS = {
    "WETH": 18, "WMATIC": 18, "WBTC": 8, "USDT": 6, "DAI": 18, "LINK": 18,
    "USDC": 6,
}

# QuickSwap V3 pools (USDC.e — cross-DEX compatible with existing bot)
POOLS = [
    {"address": "0x55CAaBB0d2b704FD0eF8192A7E35D8837e678207", "pair": "WETH/USDC",   "dynamic_fee": 895,  "out_token": "WETH"},
    {"address": "0xAE81FAc689A1b4b1e06e7ef4a2ab4CD8aC0A087D", "pair": "WMATIC/USDC", "dynamic_fee": 900,  "out_token": "WMATIC"},
    {"address": "0xA5CD8351Cbf30B531C7b11B0D9d3Ff38eA2E280f", "pair": "WBTC/USDC",   "dynamic_fee": 876,  "out_token": "WBTC"},
    {"address": "0x7B925e617aefd7FB3a93Abe3a701135D7a1Ba710", "pair": "USDT/USDC",   "dynamic_fee": 10,   "out_token": "USDT"},
    {"address": "0xe7E0eB9F6bCcCfe847fDf62a3628319a092F11a2", "pair": "DAI/USDC",    "dynamic_fee": 10,   "out_token": "DAI"},
    {"address": "0xEFdC563F99310A5Dd189eaaA91A1bf28034dA94C", "pair": "LINK/USDC",   "dynamic_fee": 1998, "out_token": "LINK"},
]

QUOTE_SIZES_USD = [1, 10, 100, 1000, 5000]

# Categorization thresholds (same as verify_whitelist_enhanced.py)
THRESHOLDS = {
    "whitelist_min_size": 1000,
    "impact_whitelist": 5.0,
    "impact_marginal": 10.0,
    "impact_blacklist": 20.0,
    "min_liquidity_base": 1_000_000_000,
}

# ANSI colors
GREEN = "\033[92m"
RED = "\033[91m"
YELLOW = "\033[93m"
BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"


# --- RPC Helpers ---

def eth_call(to, data):
    """Execute eth_call and return hex result."""
    payload = json.dumps({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [{"to": to, "data": data}, "latest"],
        "id": 1,
    })
    try:
        result = subprocess.run(
            ["curl", "-s", "-X", "POST", RPC_URL,
             "-H", "Content-Type: application/json",
             "-d", payload],
            capture_output=True, text=True, timeout=15,
        )
        resp = json.loads(result.stdout)
        if "error" in resp:
            return ""
        return resp.get("result", "")
    except Exception as e:
        print(f"  {RED}RPC error: {e}{RESET}")
        return ""


def eth_get_code(address):
    """Check if contract exists."""
    payload = json.dumps({
        "jsonrpc": "2.0",
        "method": "eth_getCode",
        "params": [address, "latest"],
        "id": 1,
    })
    try:
        result = subprocess.run(
            ["curl", "-s", "-X", "POST", RPC_URL,
             "-H", "Content-Type: application/json",
             "-d", payload],
            capture_output=True, text=True, timeout=15,
        )
        resp = json.loads(result.stdout)
        return resp.get("result", "")
    except Exception:
        return ""


def pad_address(addr):
    return addr.lower().replace("0x", "").zfill(64)


def pad_uint(val):
    return hex(val)[2:].zfill(64)


def hex_to_int(hex_str, signed=False):
    if not hex_str or hex_str == "0x":
        return 0
    val = int(hex_str, 16)
    if signed and val >= 2**255:
        val -= 2**256
    return val


def format_big_num(val):
    if val >= 1_000_000_000_000:
        return f"{val / 1_000_000_000_000:.2f}T"
    if val >= 1_000_000_000:
        return f"{val / 1_000_000_000:.2f}B"
    if val >= 1_000_000:
        return f"{val / 1_000_000:.2f}M"
    if val >= 1_000:
        return f"{val / 1_000:.2f}K"
    return str(val)


# --- Pool State ---

def get_algebra_pool_state(address):
    """Get Algebra pool state via globalState() and liquidity()."""
    state = {
        "exists": False,
        "sqrt_price": 0,
        "tick": 0,
        "fee": 0,
        "liquidity": 0,
        "initialized": False,
    }

    code = eth_get_code(address)
    if not code or code in ("0x", "0x0"):
        return state
    state["exists"] = True

    # globalState() returns (uint160 price, int24 tick, uint16 fee, ...)
    resp = eth_call(address, SELECTOR_GLOBAL_STATE)
    if resp and len(resp) >= 194:  # 0x + 3*64 = 194 min
        state["sqrt_price"] = hex_to_int("0x" + resp[2:66])
        state["tick"] = hex_to_int("0x" + resp[66:130], signed=True)
        state["fee"] = hex_to_int("0x" + resp[130:194])
        if state["sqrt_price"] > 0:
            state["initialized"] = True

    # liquidity()
    resp = eth_call(address, SELECTOR_LIQUIDITY)
    if resp and len(resp) >= 66:
        state["liquidity"] = hex_to_int("0x" + resp[2:66])

    return state


# --- Algebra Quoter ---

def algebra_quote(token_in, token_out, amount_raw):
    """Call Algebra QuoterV2.quoteExactInputSingle(address,address,uint256,uint160).

    Note: No fee parameter — Algebra quoter finds the pool automatically.
    sqrtPriceLimitX96 = 0 means no limit.
    """
    calldata = (
        SELECTOR_ALGEBRA_QUOTE
        + pad_address(token_in)
        + pad_address(token_out)
        + pad_uint(amount_raw)
        + pad_uint(0)  # sqrtPriceLimitX96 = 0
    )

    resp = eth_call(ALGEBRA_QUOTER, calldata)
    if not resp or len(resp) < 66:
        return 0

    # First 32 bytes = amountOut (both V1 and V2 return this first)
    return hex_to_int("0x" + resp[2:66])


# --- Assessment ---

def assess_pool(pool):
    """Run full depth assessment on a single pool."""
    pair = pool["pair"]
    out_token = pool["out_token"]
    out_decimals = TOKEN_DECIMALS[out_token]
    address = pool["address"]

    # Get pool state
    state = get_algebra_pool_state(address)

    result = {
        "address": address,
        "pair": pair,
        "dynamic_fee": pool["dynamic_fee"],
        "pool_state": state,
        "quotes": [],
        "max_working_size": 0,
        "impact_at_max": None,
        "liquidity_score": 0,
        "category": "blacklist",
        "reason": "",
    }

    if not state["exists"] or not state["initialized"]:
        result["reason"] = "Pool doesn't exist or not initialized"
        return result

    # Update dynamic fee from on-chain
    if state["fee"] > 0:
        result["dynamic_fee"] = state["fee"]

    token_in = USDC_E
    token_out = TOKEN_ADDRESSES[out_token]
    baseline_rate = None

    for size_usd in QUOTE_SIZES_USD:
        amount_raw = size_usd * 1_000_000  # USDC 6 decimals
        raw_out = algebra_quote(token_in, token_out, amount_raw)

        passed = raw_out > 0
        human_out = raw_out / (10 ** out_decimals) if raw_out > 0 else 0.0

        # Price impact vs $1 baseline
        impact_pct = None
        if baseline_rate and baseline_rate > 0 and raw_out > 0:
            current_rate = raw_out / amount_raw
            impact_pct = abs(1.0 - current_rate / baseline_rate) * 100

        entry = {
            "size_usd": size_usd,
            "raw_out": raw_out,
            "human_out": human_out,
            "pass": passed,
            "impact_pct": impact_pct,
        }
        result["quotes"].append(entry)

        # Set baseline from $1
        if size_usd == 1 and raw_out > 0:
            baseline_rate = raw_out / amount_raw

        if passed:
            result["max_working_size"] = size_usd
            if impact_pct is not None:
                result["impact_at_max"] = impact_pct

    # Calculate liquidity score (same logic as verify_whitelist_enhanced.py)
    result["liquidity_score"] = calculate_liquidity_score(result)

    # Categorize
    result["category"], result["reason"] = categorize_pool(result)

    return result


def calculate_liquidity_score(r):
    """0-100 liquidity score (same as verify_whitelist_enhanced.py)."""
    max_size = r["max_working_size"]
    impact = r["impact_at_max"]

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
    return 5


def categorize_pool(r):
    """Categorize as whitelist/marginal/blacklist (same thresholds)."""
    max_size = r["max_working_size"]
    impact = r["impact_at_max"]
    pool_state = r["pool_state"]

    if not pool_state["exists"]:
        return ("blacklist", "Pool doesn't exist")
    if not pool_state["initialized"]:
        return ("blacklist", "Pool not initialized")
    if pool_state["liquidity"] < THRESHOLDS["min_liquidity_base"]:
        return ("blacklist", f"Liquidity too low: {format_big_num(pool_state['liquidity'])}")
    if max_size == 0:
        return ("blacklist", "All quotes failed")

    if max_size >= THRESHOLDS["whitelist_min_size"]:
        if impact is None or impact <= THRESHOLDS["impact_whitelist"]:
            return ("whitelist", f"Handles ${max_size} @ {impact:.2f}% impact, score={r['liquidity_score']}")
        elif impact <= THRESHOLDS["impact_marginal"]:
            return ("marginal", f"Works at ${max_size} but high impact ({impact:.2f}%)")
        else:
            return ("blacklist", f"Unacceptable impact: {impact:.2f}% at ${max_size}")

    if max_size >= 10 and max_size <= 100:
        if impact is None or impact <= THRESHOLDS["impact_marginal"]:
            return ("marginal", f"Small trade pool: max ${max_size} @ {impact:.2f}% impact" if impact else f"Small trade pool: max ${max_size}")
        else:
            return ("blacklist", f"High impact at small size: {impact:.2f}% @ ${max_size}")

    if max_size < 10:
        return ("blacklist", f"Insufficient depth: only works up to ${max_size}")

    return ("blacklist", f"Unknown: max_size=${max_size}, impact={impact}")


# --- Display ---

def print_results(results):
    """Print assessment matrix and categorization."""
    print()
    print("=" * 130)
    print(f"  {BOLD}QUICKSWAP V3 (ALGEBRA) ENHANCED POOL ASSESSMENT{RESET}")
    print(f"  {DIM}Same thresholds as verify_whitelist_enhanced.py{RESET}")
    print(f"  {DIM}Quoter: Algebra QuoterV2 ({ALGEBRA_QUOTER[:10]}...) | Selector: {SELECTOR_ALGEBRA_QUOTE}{RESET}")
    print("=" * 130)

    # Column headers
    COL = 10
    size_headers = "".join(f"{'$'+str(s):>{COL}}" for s in QUOTE_SIZES_USD)
    print(f"\n  {'Pool':<14} {'Pair':<13} {'Fee':>6} {'Max$':<6} {size_headers}  {'Impact':>{COL}} {'Score':<6} {'Category':<12}")
    sep = "-" * 126
    print(f"  {sep}")

    for r in results:
        addr = r["address"][:6] + ".." + r["address"][-4:]
        pair = r["pair"]
        fee_str = f"{r['dynamic_fee']/10000:.3f}%"
        max_s = f"${r['max_working_size']}"

        cells = ""
        for q in r["quotes"]:
            if not q["pass"]:
                cells += f"{RED}{'FAIL':>{COL}}{RESET}"
            else:
                impact = q.get("impact_pct")
                if impact is None:
                    cells += f"{GREEN}{'OK':>{COL}}{RESET}"
                elif impact <= 5:
                    cells += f"{GREEN}{impact:.2f}%".rjust(COL) + f"{RESET}"
                elif impact <= 10:
                    cells += f"{YELLOW}{impact:.2f}%".rjust(COL) + f"{RESET}"
                else:
                    cells += f"{RED}{impact:.2f}%".rjust(COL) + f"{RESET}"

        # Impact at max
        if r["impact_at_max"] is not None:
            imp = r["impact_at_max"]
            color = GREEN if imp <= 5 else (YELLOW if imp <= 10 else RED)
            imp_str = f"{color}{imp:.2f}%".rjust(COL) + f"{RESET}"
        else:
            imp_str = "--".rjust(COL)

        score = str(r["liquidity_score"])

        cat = r["category"]
        if cat == "whitelist":
            cat_str = f"{GREEN}{cat.upper():<12}{RESET}"
        elif cat == "marginal":
            cat_str = f"{YELLOW}{cat.upper():<12}{RESET}"
        else:
            cat_str = f"{RED}{cat.upper():<12}{RESET}"

        print(f"  {addr:<14} {pair:<13} {fee_str:>6} {max_s:<6} {cells}  {imp_str} {score:<6} {cat_str}")

    # Summary
    whitelist = [r for r in results if r["category"] == "whitelist"]
    marginal = [r for r in results if r["category"] == "marginal"]
    blacklist = [r for r in results if r["category"] == "blacklist"]

    print()
    print("=" * 130)
    print(f"  {BOLD}CATEGORIZATION SUMMARY{RESET}")
    print("=" * 130)

    print(f"\n  {GREEN}{BOLD}WHITELIST ({len(whitelist)} pools){RESET} — Safe for bot inclusion")
    for r in whitelist:
        fee_str = f"{r['dynamic_fee']/10000:.3f}%"
        print(f"    {r['address']}  {r['pair']:<13} fee={fee_str}")
        print(f"      {r['reason']}")

    print(f"\n  {YELLOW}{BOLD}MARGINAL ({len(marginal)} pools){RESET} — Monitor only, don't activate")
    for r in marginal:
        fee_str = f"{r['dynamic_fee']/10000:.3f}%"
        print(f"    {r['address']}  {r['pair']:<13} fee={fee_str}")
        print(f"      {r['reason']}")

    print(f"\n  {RED}{BOLD}BLACKLIST ({len(blacklist)} pools){RESET} — Exclude from bot")
    for r in blacklist:
        fee_str = f"{r['dynamic_fee']/10000:.3f}%"
        print(f"    {r['address']}  {r['pair']:<13} fee={fee_str}")
        print(f"      {r['reason']}")

    # Cross-DEX arb opportunity analysis
    print()
    print("=" * 130)
    print(f"  {BOLD}CROSS-DEX ARB OPPORTUNITY ANALYSIS{RESET}")
    print(f"  {DIM}QuickSwap V3 (Algebra) vs UniswapV3 / SushiswapV3{RESET}")
    print("=" * 130)

    uni_v3_fees = {
        "WETH/USDC": [500, 3000],      # 0.05%, 0.30%
        "WMATIC/USDC": [500, 3000],
        "WBTC/USDC": [500, 3000],
        "USDT/USDC": [100],
        "DAI/USDC": [100],
        "LINK/USDC": [3000],
    }

    sushi_v3_fees = {
        "WETH/USDC": [3000],            # 0.30%
        "USDT/USDC": [100],
    }

    for r in whitelist + marginal:
        pair = r["pair"]
        qk_fee = r["dynamic_fee"] / 10000  # convert to percent (fee is parts-per-million)
        print(f"\n  {BOLD}{pair}{RESET} — QuickSwap fee: {qk_fee:.2f}%")

        # vs UniswapV3
        for uni_fee in uni_v3_fees.get(pair, []):
            uni_pct = uni_fee / 10000
            roundtrip = qk_fee + uni_pct
            print(f"    vs UniV3 {uni_pct:.2f}% → round-trip: {roundtrip:.2f}%", end="")
            if roundtrip < 0.35:
                print(f"  {GREEN}← BETTER than current best (0.35%){RESET}")
            else:
                print(f"  {DIM}(same or worse than 0.35%){RESET}")

        # vs SushiSwapV3
        for sushi_fee in sushi_v3_fees.get(pair, []):
            sushi_pct = sushi_fee / 10000
            roundtrip = qk_fee + sushi_pct
            print(f"    vs SushiV3 {sushi_pct:.2f}% → round-trip: {roundtrip:.2f}%", end="")
            if roundtrip < 0.35:
                print(f"  {GREEN}← BETTER than current best (0.35%){RESET}")
            else:
                print(f"  {DIM}(same or worse){RESET}")

    print()
    return results


# --- Main ---

def main():
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    print(f"\n  {BOLD}QuickSwap V3 (Algebra) Pool Assessment{RESET} | {now}")
    print(f"  RPC: {RPC_URL[:50]}...")
    print(f"  Pools to assess: {len(POOLS)}")
    print(f"  Quote sizes: {QUOTE_SIZES_USD}")
    print()

    results = []
    for i, pool in enumerate(POOLS):
        addr_short = pool["address"][:8] + ".." + pool["address"][-4:]
        print(f"  [{i+1}/{len(POOLS)}] {pool['pair']} (fee ~{pool['dynamic_fee']/10000:.3f}%) {addr_short} ...", flush=True)
        result = assess_pool(pool)

        # Print inline quote results
        for q in result["quotes"]:
            if q["pass"]:
                impact_str = f" (impact: {q['impact_pct']:.2f}%)" if q["impact_pct"] is not None else ""
                print(f"    ${q['size_usd']}: {q['human_out']:.6g} {pool['out_token']}{impact_str}")
            else:
                print(f"    ${q['size_usd']}: REVERTED")

        results.append(result)
        print()

    print_results(results)


if __name__ == "__main__":
    main()
