#!/usr/bin/env python3
"""
Depth Assessment Script for Polygon DEX Pools

Purpose:
    Queries on-chain quoter contracts to assess price impact at multiple trade
    sizes ($100, $500, $5000) for all non-whitelisted pools that have liquidity.
    Produces a ranked table showing depth, price impact, and recommended trade
    sizing for potential whitelist additions.

Author: AI-Generated
Created: 2026-02-02
Modified: 2026-02-02

Dependencies:
    - requests (for JSON-RPC calls)
    - csv, json (standard library)

Usage:
    python3 /home/botuser/bots/dexarb/scripts/depth_assessment.py

Data Sources:
    - Input: /home/botuser/bots/dexarb/data/polygon/pool_scan_results.csv
    - Input: /home/botuser/bots/dexarb/config/polygon/pools_whitelist.json
    - Output: /home/botuser/bots/dexarb/data/polygon/depth_assessment.csv

Notes:
    - Uses 150ms rate limiting between RPC calls
    - Supports Uniswap V3, SushiSwap V3, QuickSwap V3 (Algebra), and V2 pools
    - V2 pools use getReserves() + constant product formula (no quoter needed)
"""

# Standard library imports
import csv
import json
import sys
import time
from collections import defaultdict

# Third-party imports
import requests

# =============================================================================
# Constants
# =============================================================================

RPC_URL = "https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"

# Quoter contract addresses
UNISWAP_V3_QUOTER = "0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6"
SUSHISWAP_V3_QUOTER = "0xb1E835Dc2785b52265711e17fCCb0fd018226a6e"
QUICKSWAP_V3_QUOTER = "0xa15F0D7377B2A0C0c10db057f641beD21028FC89"

# Token addresses on Polygon
TOKEN_ADDRESSES = {
    "USDC.e":       "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174",
    "USDC native":  "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359",
    "USDT":         "0xc2132D05D31c914a87C6611C10748AEb04B58e8F",
    "WETH":         "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619",
    "WMATIC":       "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270",
    "WBTC":         "0x1BFD67037B42Cf73acF2047067bd4F2C47D9BfD6",
    "LINK":         "0x53E0bca35eC356BD5ddDFebbD1Fc0fD03FaBad39",
    "AAVE":         "0xD6DF932A45C0f255f85145f286eA0b292B21C90B",
    "UNI":          "0xb33EaAd8d922B1083446DC23f610c2567fB5180f",
    "GRT":          "0x5fe2B58c013d7601147DcdD68C143A77499f5531",
    "CRV":          "0x172370d5Cd63279eFa6d502DAB29171933a610AF",
    "SOL":          "0x7DfF46370e9eA5f0Bad3C4E29711aD50062EA7A4",
    "SAND":         "0xBbba073C31bF03b8ACf7c28EF0738DeCF3695683",
    "SUSHI":        "0x0b3F868E0BE5597D5DB7fEB59E1CADBb0fdDa50a",
    "DAI":          "0x8f3Cf7ad23Cd3CaDbD9735AFf958023239c6A063",
}

# Quote tokens all have 6 decimals
QUOTE_DECIMALS = 6

# Trade sizes in USD
TRADE_SIZES_USD = [100, 500, 5000]

# Trade sizes in raw units (6 decimals)
TRADE_SIZES_RAW = {
    100:  100_000_000,
    500:  500_000_000,
    5000: 5_000_000_000,
}

# Rate limit between RPC calls (seconds)
RATE_LIMIT_SEC = 0.15

# Price impact threshold for recommended max trade size
IMPACT_THRESHOLD = 0.02  # 2%

# DEX to quoter mapping
DEX_QUOTER_MAP = {
    "Uniswap V3":           UNISWAP_V3_QUOTER,
    "SushiSwap V3":         SUSHISWAP_V3_QUOTER,
    "QuickSwap V3 (Algebra)": QUICKSWAP_V3_QUOTER,
}

# =============================================================================
# RPC Helpers
# =============================================================================

_rpc_id = 0


def eth_call(to, data):
    """Execute an eth_call and return the hex result, or None on revert."""
    global _rpc_id
    _rpc_id += 1
    payload = {
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [{"to": to, "data": data}, "latest"],
        "id": _rpc_id,
    }
    try:
        resp = requests.post(RPC_URL, json=payload, timeout=15)
        result = resp.json()
        if "error" in result:
            return None
        hex_result = result.get("result", "0x")
        if hex_result == "0x" or len(hex_result) < 10:
            return None
        return hex_result
    except Exception as e:
        print(f"  [RPC error] {e}", file=sys.stderr)
        return None


def decode_uint256(hex_str, offset=0):
    """Decode a uint256 from a hex string at a given 32-byte word offset."""
    start = 2 + offset * 64  # skip '0x' prefix
    end = start + 64
    if len(hex_str) < end:
        return 0
    return int(hex_str[start:end], 16)


def encode_address(addr):
    """Encode an address as a 32-byte ABI word (zero-padded)."""
    return addr.lower().replace("0x", "").zfill(64)


def encode_uint256(val):
    """Encode a uint256 as a 32-byte ABI word."""
    return hex(val)[2:].zfill(64)


def encode_uint24(val):
    """Encode a uint24 as a 32-byte ABI word."""
    return hex(val)[2:].zfill(64)


# =============================================================================
# Quoter Call Functions
# =============================================================================

def quote_uniswap_v3(quoter_addr, token_in, token_out, fee, amount_in):
    """
    Call quoteExactInputSingle on Uniswap V3 / SushiSwap V3 QuoterV1.
    Selector: 0xf7729d43
    Args: (address tokenIn, address tokenOut, uint24 fee, uint256 amountIn, uint160 sqrtPriceLimitX96)
    Returns: uint256 amountOut
    """
    calldata = (
        "0xf7729d43"
        + encode_address(token_in)
        + encode_address(token_out)
        + encode_uint24(fee)
        + encode_uint256(amount_in)
        + encode_uint256(0)  # sqrtPriceLimitX96 = 0
    )
    time.sleep(RATE_LIMIT_SEC)
    result = eth_call(quoter_addr, calldata)
    if result is None:
        return None
    amount_out = decode_uint256(result, 0)
    return amount_out if amount_out > 0 else None


def quote_quickswap_v3(token_in, token_out, amount_in):
    """
    Call quoteExactInputSingle on QuickSwap V3 (Algebra) Quoter.
    Selector: 0xcdca1753
    Args: (address tokenIn, address tokenOut, uint256 amountIn, uint160 sqrtPriceLimitX96)
    Returns: (uint256 amountOut, uint16 fee)
    """
    calldata = (
        "0xcdca1753"
        + encode_address(token_in)
        + encode_address(token_out)
        + encode_uint256(amount_in)
        + encode_uint256(0)  # sqrtPriceLimitX96 = 0
    )
    time.sleep(RATE_LIMIT_SEC)
    result = eth_call(QUICKSWAP_V3_QUOTER, calldata)
    if result is None:
        return None
    amount_out = decode_uint256(result, 0)
    return amount_out if amount_out > 0 else None


def query_v2_reserves(pool_address):
    """
    Call getReserves() on a V2 pool.
    Selector: 0x0902f1ac
    Returns: (reserve0, reserve1) or None on failure.
    """
    calldata = "0x0902f1ac"
    time.sleep(RATE_LIMIT_SEC)
    result = eth_call(pool_address, calldata)
    if result is None:
        return None
    reserve0 = decode_uint256(result, 0)
    reserve1 = decode_uint256(result, 1)
    if reserve0 == 0 or reserve1 == 0:
        return None
    return (reserve0, reserve1)


def calc_v2_amount_out(amount_in, reserve_in, reserve_out):
    """Calculate V2 swap output using constant product formula with 0.3% fee."""
    amount_in_with_fee = amount_in * 997
    numerator = amount_in_with_fee * reserve_out
    denominator = reserve_in * 1000 + amount_in_with_fee
    return numerator // denominator


# =============================================================================
# Token Address Resolution
# =============================================================================

def get_token_address(symbol):
    """Get the address for a token symbol."""
    addr = TOKEN_ADDRESSES.get(symbol)
    if addr is None:
        print(f"  [WARN] Unknown token symbol: {symbol}", file=sys.stderr)
    return addr


def get_base_decimals(symbol):
    """Return the number of decimals for a base token."""
    decimals_map = {
        "WETH": 18,
        "WMATIC": 18,
        "WBTC": 8,
        "LINK": 18,
        "AAVE": 18,
        "UNI": 18,
        "GRT": 18,
        "CRV": 18,
        "SOL": 18,
        "SAND": 18,
        "SUSHI": 18,
        "DAI": 18,
    }
    return decimals_map.get(symbol, 18)


# =============================================================================
# Pool Assessment
# =============================================================================

def assess_v3_pool(pool):
    """Assess a V3 pool by querying the quoter at multiple trade sizes."""
    dex = pool["dex"]
    base_symbol = pool["base"]
    quote_symbol = pool["quote"]
    fee_raw = int(pool["fee_raw"])

    quote_addr = get_token_address(quote_symbol)
    base_addr = get_token_address(base_symbol)
    if quote_addr is None or base_addr is None:
        return make_fail_result(pool, "unknown_token")

    is_algebra = "Algebra" in dex
    quoter_addr = DEX_QUOTER_MAP.get(dex)
    if quoter_addr is None:
        return make_fail_result(pool, "no_quoter")

    results = {}
    for usd_size in TRADE_SIZES_USD:
        amount_in = TRADE_SIZES_RAW[usd_size]
        if is_algebra:
            amount_out = quote_quickswap_v3(quote_addr, base_addr, amount_in)
        else:
            amount_out = quote_uniswap_v3(quoter_addr, quote_addr, base_addr,
                                           fee_raw, amount_in)
        results[usd_size] = amount_out

    return build_assessment(pool, results)


def assess_v2_pool(pool):
    """Assess a V2 pool by querying getReserves() and computing outputs."""
    base_symbol = pool["base"]
    quote_symbol = pool["quote"]
    token0_addr = pool["token0"].lower()

    quote_addr = get_token_address(quote_symbol)
    base_addr = get_token_address(base_symbol)
    if quote_addr is None or base_addr is None:
        return make_fail_result(pool, "unknown_token")

    reserves = query_v2_reserves(pool["pool_address"])
    if reserves is None:
        return make_fail_result(pool, "no_reserves")

    reserve0, reserve1 = reserves

    # Determine which reserve is quote and which is base
    if token0_addr == quote_addr.lower():
        reserve_quote = reserve0
        reserve_base = reserve1
    elif token0_addr == base_addr.lower():
        reserve_quote = reserve1
        reserve_base = reserve0
    else:
        print(f"  [WARN] token0 mismatch for {pool['pool_address']}: "
              f"token0={token0_addr}, quote={quote_addr}, base={base_addr}",
              file=sys.stderr)
        return make_fail_result(pool, "token0_mismatch")

    results = {}
    for usd_size in TRADE_SIZES_USD:
        amount_in = TRADE_SIZES_RAW[usd_size]
        amount_out = calc_v2_amount_out(amount_in, reserve_quote, reserve_base)
        results[usd_size] = amount_out if amount_out > 0 else None

    return build_assessment(pool, results)


def build_assessment(pool, results):
    """Build the assessment dict from raw quoter/calculation results."""
    pair = f"{pool['base']}/{pool['quote']}"
    assessment = {
        "pair": pair,
        "base": pool["base"],
        "quote": pool["quote"],
        "dex": pool["dex"],
        "fee_display": pool["fee_display"],
        "fee_raw": pool["fee_raw"],
        "pool_address": pool["pool_address"],
        "liquidity_raw": pool.get("liquidity_raw", "0"),
    }

    amt_100 = results.get(100)
    amt_500 = results.get(500)
    amt_5000 = results.get(5000)

    assessment["out_100"] = amt_100
    assessment["out_500"] = amt_500
    assessment["out_5000"] = amt_5000

    if amt_100 is None or amt_100 == 0:
        assessment["impact_500"] = None
        assessment["impact_5000"] = None
        assessment["max_trade_usd"] = 0
        assessment["verdict"] = "DEAD"
        return assessment

    # Fair price = amount of base tokens per dollar at $100 size
    fair_price_per_dollar = amt_100 / 100.0

    # Price impact at each size
    for size, key in [(500, "impact_500"), (5000, "impact_5000")]:
        amt = results.get(size)
        if amt is None or amt == 0:
            assessment[key] = None
        else:
            expected = size * fair_price_per_dollar
            impact = 1.0 - (amt / expected)
            assessment[key] = impact

    # Recommended max trade size: largest size with <2% impact
    assessment["max_trade_usd"] = 100  # default
    if amt_5000 is not None and amt_5000 > 0:
        expected_5000 = 5000 * fair_price_per_dollar
        impact_5000 = 1.0 - (amt_5000 / expected_5000)
        if impact_5000 < IMPACT_THRESHOLD:
            assessment["max_trade_usd"] = 5000
        elif amt_500 is not None and amt_500 > 0:
            expected_500 = 500 * fair_price_per_dollar
            impact_500 = 1.0 - (amt_500 / expected_500)
            if impact_500 < IMPACT_THRESHOLD:
                assessment["max_trade_usd"] = 500
    elif amt_500 is not None and amt_500 > 0:
        expected_500 = 500 * fair_price_per_dollar
        impact_500 = 1.0 - (amt_500 / expected_500)
        if impact_500 < IMPACT_THRESHOLD:
            assessment["max_trade_usd"] = 500

    # Verdict logic
    if assessment["max_trade_usd"] >= 500:
        assessment["verdict"] = "ADD"
    elif amt_100 is not None and amt_100 > 0:
        assessment["verdict"] = "THIN"
    else:
        assessment["verdict"] = "DEAD"

    return assessment


def make_fail_result(pool, reason):
    """Create a failure result for a pool that couldn't be assessed."""
    pair = f"{pool['base']}/{pool['quote']}"
    return {
        "pair": pair,
        "base": pool["base"],
        "quote": pool["quote"],
        "dex": pool["dex"],
        "fee_display": pool["fee_display"],
        "fee_raw": pool["fee_raw"],
        "pool_address": pool["pool_address"],
        "liquidity_raw": pool.get("liquidity_raw", "0"),
        "out_100": None,
        "out_500": None,
        "out_5000": None,
        "impact_500": None,
        "impact_5000": None,
        "max_trade_usd": 0,
        "verdict": "DEAD",
    }


# =============================================================================
# Output Formatting
# =============================================================================

def format_impact(impact):
    """Format a price impact percentage."""
    if impact is None:
        return "FAIL"
    return f"{impact * 100:.2f}%"


def format_base_amount(amount, base_symbol):
    """Format base token amount with proper decimal places."""
    if amount is None:
        return "FAIL"
    decimals = get_base_decimals(base_symbol)
    value = amount / (10 ** decimals)
    if value >= 1000:
        return f"{value:,.2f}"
    elif value >= 1:
        return f"{value:.4f}"
    elif value >= 0.0001:
        return f"{value:.6f}"
    else:
        return f"{value:.10f}"


# =============================================================================
# Main
# =============================================================================

def main():
    """Main entry point."""
    print("=" * 100)
    print("POLYGON DEX POOL DEPTH ASSESSMENT")
    print("=" * 100)
    print()

    # --- Load data ---
    csv_path = "/home/botuser/bots/dexarb/data/polygon/pool_scan_results.csv"
    whitelist_path = "/home/botuser/bots/dexarb/config/polygon/pools_whitelist.json"
    output_path = "/home/botuser/bots/dexarb/data/polygon/depth_assessment.csv"

    with open(csv_path) as f:
        reader = csv.DictReader(f)
        all_pools = list(reader)

    with open(whitelist_path) as f:
        whitelist_data = json.load(f)

    # Extract whitelisted addresses (case-insensitive)
    wl_addresses = set(
        p["address"].lower() for p in whitelist_data["whitelist"]["pools"]
    )

    # Filter: status=found and not in whitelist
    found_pools = [p for p in all_pools if p["status"] == "found"]
    pools_to_assess = [
        p for p in found_pools if p["pool_address"].lower() not in wl_addresses
    ]

    print(f"Total pools in CSV:        {len(all_pools)}")
    print(f"Pools with liquidity:      {len(found_pools)}")
    print(f"Already whitelisted:       {len(found_pools) - len(pools_to_assess)}")
    print(f"Pools to assess:           {len(pools_to_assess)}")
    print()

    # --- Assess each pool ---
    assessments = []
    total = len(pools_to_assess)
    for i, pool in enumerate(pools_to_assess):
        pair = f"{pool['base']}/{pool['quote']}"
        dex = pool["dex"]
        progress = f"[{i+1}/{total}]"
        print(f"  {progress} {pair:20s} {dex:25s} {pool['fee_display']:6s} {pool['pool_address']}", end="")
        sys.stdout.flush()

        is_v2 = "V2" in dex
        if is_v2:
            result = assess_v2_pool(pool)
        else:
            result = assess_v3_pool(pool)

        verdict = result["verdict"]
        print(f"  -> {verdict}", end="")
        if result["out_100"] is not None:
            print(f"  ($100->{format_base_amount(result['out_100'], pool['base'])} {pool['base']})", end="")
        print()
        assessments.append(result)

    print()
    print(f"Assessment complete. {len(assessments)} pools assessed.")
    print()

    # --- Sort results: by pair, then by max_trade_usd descending ---
    assessments.sort(key=lambda x: (x["pair"], -x["max_trade_usd"], x["dex"]))

    # --- Print results table ---
    print("=" * 150)
    print(f"{'Pair':<18} {'DEX':<25} {'Fee':>6} {'Pool Address':<44} "
          f"{'$100 Out':>18} {'$500 Imp':>10} {'$5K Imp':>10} "
          f"{'MaxTrade':>10} {'Verdict':>8}")
    print("-" * 150)

    for a in assessments:
        out_100_str = format_base_amount(a["out_100"], a["base"])
        impact_500_str = format_impact(a["impact_500"])
        impact_5000_str = format_impact(a["impact_5000"])
        max_trade_str = f"${a['max_trade_usd']}" if a["max_trade_usd"] > 0 else "N/A"

        print(f"{a['pair']:<18} {a['dex']:<25} {a['fee_display']:>6} {a['pool_address']:<44} "
              f"{out_100_str:>18} {impact_500_str:>10} {impact_5000_str:>10} "
              f"{max_trade_str:>10} {a['verdict']:>8}")

    print("=" * 150)
    print()

    # --- Save CSV ---
    csv_fields = [
        "pair", "base", "quote", "dex", "fee_display", "fee_raw",
        "pool_address", "liquidity_raw",
        "out_100", "out_500", "out_5000",
        "impact_500", "impact_5000",
        "max_trade_usd", "verdict",
    ]
    with open(output_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=csv_fields)
        writer.writeheader()
        for a in assessments:
            row = {}
            for field in csv_fields:
                val = a.get(field)
                if field.startswith("impact_") and val is not None:
                    row[field] = f"{val * 100:.4f}"
                elif val is None:
                    row[field] = "FAIL"
                else:
                    row[field] = val
            writer.writerow(row)
    print(f"Results saved to: {output_path}")
    print()

    # --- SUMMARY: Cross-DEX Arbitrage Candidates ---
    print("=" * 100)
    print("SUMMARY: CROSS-DEX ARBITRAGE CANDIDATES")
    print("=" * 100)
    print()

    # Group active (non-DEAD) pools by pair
    active_by_pair = defaultdict(list)
    for a in assessments:
        if a["verdict"] != "DEAD":
            active_by_pair[a["pair"]].append(a)

    # Also include whitelisted pools for cross-DEX counting
    wl_by_pair = defaultdict(list)
    for p in whitelist_data["whitelist"]["pools"]:
        if p.get("status") in ("active", "v2_ready", "monitoring"):
            wl_by_pair[p["pair"]].append(p)

    # Combine all pair names
    all_pairs = set(list(active_by_pair.keys()))
    for p_name in wl_by_pair:
        all_pairs.add(p_name)

    print("Pairs with >= 2 active pools on different DEXes (arb-viable):")
    print("-" * 100)

    arb_candidates = []
    for pair in sorted(all_pairs):
        # Collect all active DEXes for this pair
        dexes = set()
        pool_details = []

        # From whitelist
        for wp in wl_by_pair.get(pair, []):
            dexes.add(wp["dex"])
            pool_details.append({
                "dex": wp["dex"],
                "address": wp["address"],
                "source": "WHITELIST",
                "max_trade": wp.get("max_trade_size_usd", "default"),
            })

        # From our new assessment
        for ap in active_by_pair.get(pair, []):
            dexes.add(ap["dex"])
            pool_details.append({
                "dex": ap["dex"],
                "address": ap["pool_address"],
                "source": ap["verdict"],
                "max_trade": ap["max_trade_usd"],
            })

        if len(dexes) >= 2:
            arb_candidates.append((pair, dexes, pool_details))
            print(f"\n  {pair} ({len(dexes)} DEXes: {', '.join(sorted(dexes))})")
            for pd in pool_details:
                print(f"    [{pd['source']:<10}] {pd['dex']:<25} {pd['address']}  max=${pd['max_trade']}")

    if not arb_candidates:
        print("  (No cross-DEX arb candidates found among non-whitelisted pools)")

    print()
    print("=" * 100)
    print("RECOMMENDED WHITELIST ADDITIONS")
    print("=" * 100)
    print()

    # Recommend ADD/THIN pools that are part of a cross-DEX pair
    recommendations = []
    arb_pair_names = set(pair for pair, _, _ in arb_candidates)
    for a in assessments:
        if a["verdict"] in ("ADD", "THIN") and a["pair"] in arb_pair_names:
            recommendations.append(a)

    if recommendations:
        print(f"{'Pair':<18} {'DEX':<25} {'Fee':>6} {'Pool Address':<44} "
              f"{'MaxTrade':>10} {'Verdict':>8}")
        print("-" * 115)
        for r in recommendations:
            max_trade_str = f"${r['max_trade_usd']}"
            print(f"{r['pair']:<18} {r['dex']:<25} {r['fee_display']:>6} "
                  f"{r['pool_address']:<44} {max_trade_str:>10} {r['verdict']:>8}")
    else:
        print("  No new recommendations (all viable pools may already be whitelisted).")

    print()

    # --- Final Stats ---
    verdicts = defaultdict(int)
    for a in assessments:
        verdicts[a["verdict"]] += 1
    print("Verdict breakdown:")
    for v in ["ADD", "THIN", "DEAD"]:
        print(f"  {v}: {verdicts.get(v, 0)}")
    print()
    print("Done.")


if __name__ == "__main__":
    main()
