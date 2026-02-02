#!/usr/bin/env python3
"""
Pool Scanner — DEX Pool Discovery on Polygon

Purpose:
    Discovers DEX pool addresses on Polygon by querying factory contracts
    (Uniswap V3, SushiSwap V3, QuickSwap V3/V2, SushiSwap V2).
    For each discovered pool, queries liquidity/reserves and token ordering.
    Outputs a formatted table and CSV file.

Author: AI-Generated (Claude Code)
Created: 2026-02-02
Modified: 2026-02-02

Dependencies:
    - requests (HTTP JSON-RPC calls to Alchemy)

Usage:
    python3 pool_scanner.py

Notes:
    - Uses eth_call JSON-RPC to query factory contracts directly
    - Rate-limited to 100ms between RPC calls to respect Alchemy limits
    - V3 factories are queried across 4 fee tiers (100, 500, 3000, 10000)
    - Algebra (QuickSwap V3) uses dynamic fees, no fee tier parameter
    - V2 factories use getPair with no fee tier

Data Sources:
    - Input: On-chain Polygon factory contracts via Alchemy RPC
    - Output: /home/botuser/bots/dexarb/data/polygon/pool_scan_results.csv

Configuration:
    - RPC_URL: Alchemy HTTPS endpoint for Polygon mainnet
"""

# Standard library imports
import csv
import json
import os
import sys
import time

# Third-party imports
import requests

# =============================================================================
# Constants
# =============================================================================

RPC_URL = "https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"

OUTPUT_DIR = "/home/botuser/bots/dexarb/data/polygon"
OUTPUT_CSV = os.path.join(OUTPUT_DIR, "pool_scan_results.csv")

RATE_LIMIT_SEC = 0.10  # 100ms between RPC calls

# Zero address constant
ZERO_ADDR = "0x" + "0" * 40

# ---------------------------------------------------------------------------
# Factory definitions
# ---------------------------------------------------------------------------
FACTORIES = {
    "Uniswap V3": {
        "address": "0x1F98431c8aD98523631AE4a59f267346ea31F984",
        "type": "v3",
        "selector": "0x1698ee82",  # getPool(address,address,uint24)
    },
    "SushiSwap V3": {
        "address": "0x917933899c6a5F8E37F31E19f92CdBFF7e8FF0e2",
        "type": "v3",
        "selector": "0x1698ee82",  # getPool(address,address,uint24)
    },
    "QuickSwap V3 (Algebra)": {
        "address": "0x411b0fAcC3489691f28ad58c47006AF5E3Ab3A28",
        "type": "algebra",
        "selector": "0xd9a641e1",  # poolByPair(address,address)
    },
    "QuickSwap V2": {
        "address": "0x5757371414417b8C6CAad45bAeF941aBc7d3Ab32",
        "type": "v2",
        "selector": "0xe6a43905",  # getPair(address,address)
    },
    "SushiSwap V2": {
        "address": "0xc35DADB65012eC5796536bD9864eD8773aBc74C4",
        "type": "v2",
        "selector": "0xe6a43905",  # getPair(address,address)
    },
}

# ---------------------------------------------------------------------------
# Token definitions
# ---------------------------------------------------------------------------
BASE_TOKENS = {
    "WETH":   "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619",
    "WMATIC": "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270",
    "WBTC":   "0x1BFD67037B42Cf73acF2047067bd4F2C47D9BfD6",
    "UNI":    "0xb33EaAd8d922B1083446DC23f610c2567fB5180f",
    "LINK":   "0x53E0bca35eC356BD5ddDFebbD1Fc0fD03FaBad39",
    "AAVE":   "0xD6DF932A45C0f255f85145f286eA0b292B21C90B",
    "DAI":    "0x8f3Cf7ad23Cd3CaDbD9735AFf958023239c6A063",
    "SAND":   "0xBbba073C31bF03b8ACf7c28EF0738DeCF3695683",
    "SOL":    "0xd93f7e271cb87c23aaa73edc008a79646d1f9912",
    "CRV":    "0x172370d5Cd63279eFa6d502DAB29171933a610AF",
    "GRT":    "0x5fe2B58c013d7601147DcDd68C143A77499f5531",
    "SUSHI":  "0x0b3F868E0BE5597D5DB7fEB59E1CADbb0fdDa50a",
}

QUOTE_TOKENS = {
    "USDC.e":      "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174",
    "USDC native": "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359",
    "USDT":        "0xc2132D05D31c914a87C6611C10748AEb04B58e8F",
    "WETH":        "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619",
}

FEE_TIERS = [100, 500, 3000, 10000]

# Selectors for pool queries
SEL_LIQUIDITY    = "0x1a686502"  # liquidity()
SEL_GET_RESERVES = "0x0902f1ac"  # getReserves()
SEL_TOKEN0       = "0x0dfe1681"  # token0()


# =============================================================================
# RPC helpers
# =============================================================================

_rpc_id = 0


def rpc_call(to, data):
    """
    Execute an eth_call against the Polygon RPC endpoint.

    Args:
        to: Contract address (hex string with 0x prefix)
        data: Calldata (hex string with 0x prefix)

    Returns:
        Hex-encoded return data from the contract, or "0x" on error.
    """
    global _rpc_id
    _rpc_id += 1

    payload = {
        "jsonrpc": "2.0",
        "id": _rpc_id,
        "method": "eth_call",
        "params": [
            {"to": to, "data": data},
            "latest",
        ],
    }

    time.sleep(RATE_LIMIT_SEC)

    try:
        resp = requests.post(
            RPC_URL,
            json=payload,
            headers={"Content-Type": "application/json"},
            timeout=15,
        )
        result = resp.json()
        if "error" in result:
            return "0x"
        return result.get("result", "0x")
    except Exception as exc:
        print(f"  [RPC error] {exc}")
        return "0x"


def encode_address(addr):
    """Encode an address as a 32-byte ABI word (left-padded with zeros)."""
    return addr.lower().replace("0x", "").zfill(64)


def encode_uint24(val):
    """Encode a uint24 as a 32-byte ABI word."""
    return hex(val)[2:].zfill(64)


def decode_address(hex_data):
    """Decode an address from a 32-byte ABI word."""
    if not hex_data or hex_data == "0x" or len(hex_data) < 42:
        return ZERO_ADDR
    raw = hex_data.replace("0x", "")
    if len(raw) < 64:
        raw = raw.zfill(64)
    return "0x" + raw[24:64]


def decode_uint128(hex_data):
    """Decode a uint128 from hex return data (first 32-byte word)."""
    if not hex_data or hex_data == "0x":
        return 0
    raw = hex_data.replace("0x", "")
    if len(raw) < 64:
        raw = raw.zfill(64)
    return int(raw[:64], 16)


def decode_reserves(hex_data):
    """
    Decode getReserves() return: (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast).

    Returns:
        Tuple of (reserve0, reserve1) as ints, or (0, 0) on failure.
    """
    if not hex_data or hex_data == "0x":
        return (0, 0)
    raw = hex_data.replace("0x", "")
    if len(raw) < 192:
        raw = raw.zfill(192)
    reserve0 = int(raw[0:64], 16)
    reserve1 = int(raw[64:128], 16)
    return (reserve0, reserve1)


# =============================================================================
# Factory query functions
# =============================================================================

def query_v3_pool(factory_addr, selector, token_a, token_b, fee):
    """Query a V3-style factory: getPool(tokenA, tokenB, fee)."""
    data = selector + encode_address(token_a) + encode_address(token_b) + encode_uint24(fee)
    result = rpc_call(factory_addr, data)
    addr = decode_address(result)
    return addr


def query_algebra_pool(factory_addr, selector, token_a, token_b):
    """Query an Algebra factory: poolByPair(tokenA, tokenB)."""
    data = selector + encode_address(token_a) + encode_address(token_b)
    result = rpc_call(factory_addr, data)
    addr = decode_address(result)
    return addr


def query_v2_pair(factory_addr, selector, token_a, token_b):
    """Query a V2-style factory: getPair(tokenA, tokenB)."""
    data = selector + encode_address(token_a) + encode_address(token_b)
    result = rpc_call(factory_addr, data)
    addr = decode_address(result)
    return addr


# =============================================================================
# Pool data queries
# =============================================================================

def get_v3_liquidity(pool_addr):
    """Query liquidity() on a V3 pool."""
    result = rpc_call(pool_addr, SEL_LIQUIDITY)
    return decode_uint128(result)


def get_v2_reserves(pool_addr):
    """Query getReserves() on a V2 pool. Returns (reserve0, reserve1)."""
    result = rpc_call(pool_addr, SEL_GET_RESERVES)
    return decode_reserves(result)


def get_token0(pool_addr):
    """Query token0() on a pool."""
    result = rpc_call(pool_addr, SEL_TOKEN0)
    return decode_address(result)


def format_fee_tier(fee):
    """Convert a V3 fee tier integer to a human-readable percentage string."""
    return f"{fee / 10000:.2f}%"


def format_liquidity(value):
    """Format a large liquidity number with thousand separators."""
    if value == 0:
        return "0"
    return f"{value:,}"


# =============================================================================
# Main scanning logic
# =============================================================================

def scan_all_pools():
    """
    Scan all factory x token-pair combinations and return a list of result dicts.

    Returns:
        List of dicts with keys: base, quote, dex, fee_display, pool_address,
        liquidity_raw, liquidity_display, status, token0
    """
    results = []
    total_pairs = len(BASE_TOKENS) * len(QUOTE_TOKENS)
    total_factories = len(FACTORIES)
    print(f"Scanning {total_pairs} token pairs across {total_factories} factories...")
    print(f"V3 factories will be checked at {len(FEE_TIERS)} fee tiers each.\n")

    pair_count = 0
    found_count = 0

    for base_name, base_addr in BASE_TOKENS.items():
        for quote_name, quote_addr in QUOTE_TOKENS.items():
            pair_count += 1
            print(f"[{pair_count}/{total_pairs}] {base_name}/{quote_name}")

            for dex_name, factory in FACTORIES.items():
                f_type = factory["type"]
                f_addr = factory["address"]
                f_sel  = factory["selector"]

                if f_type == "v3":
                    # Try each fee tier
                    for fee in FEE_TIERS:
                        pool_addr = query_v3_pool(f_addr, f_sel, base_addr, quote_addr, fee)
                        if pool_addr.lower() == ZERO_ADDR:
                            continue

                        # Pool exists - query liquidity and token0
                        liquidity = get_v3_liquidity(pool_addr)
                        token0 = get_token0(pool_addr)
                        status = "found" if liquidity > 0 else "empty"
                        found_count += 1

                        results.append({
                            "base": base_name,
                            "quote": quote_name,
                            "dex": dex_name,
                            "fee_display": format_fee_tier(fee),
                            "fee_raw": fee,
                            "pool_address": pool_addr,
                            "liquidity_raw": liquidity,
                            "liquidity_display": format_liquidity(liquidity),
                            "status": status,
                            "token0": token0,
                        })
                        print(f"  + {dex_name} fee={format_fee_tier(fee)} -> {pool_addr}  liq={format_liquidity(liquidity)}  [{status}]")

                elif f_type == "algebra":
                    pool_addr = query_algebra_pool(f_addr, f_sel, base_addr, quote_addr)
                    if pool_addr.lower() == ZERO_ADDR:
                        continue

                    liquidity = get_v3_liquidity(pool_addr)
                    token0 = get_token0(pool_addr)
                    status = "found" if liquidity > 0 else "empty"
                    found_count += 1

                    results.append({
                        "base": base_name,
                        "quote": quote_name,
                        "dex": dex_name,
                        "fee_display": "dynamic",
                        "fee_raw": -1,
                        "pool_address": pool_addr,
                        "liquidity_raw": liquidity,
                        "liquidity_display": format_liquidity(liquidity),
                        "status": status,
                        "token0": token0,
                    })
                    print(f"  + {dex_name} fee=dynamic -> {pool_addr}  liq={format_liquidity(liquidity)}  [{status}]")

                elif f_type == "v2":
                    pool_addr = query_v2_pair(f_addr, f_sel, base_addr, quote_addr)
                    if pool_addr.lower() == ZERO_ADDR:
                        continue

                    reserve0, reserve1 = get_v2_reserves(pool_addr)
                    token0 = get_token0(pool_addr)

                    # Determine which reserve is the quote token
                    if token0.lower() == quote_addr.lower():
                        quote_reserve = reserve0
                    else:
                        quote_reserve = reserve1

                    status = "found" if (reserve0 > 0 and reserve1 > 0) else "empty"
                    found_count += 1

                    results.append({
                        "base": base_name,
                        "quote": quote_name,
                        "dex": dex_name,
                        "fee_display": "0.30%",
                        "fee_raw": 3000,
                        "pool_address": pool_addr,
                        "liquidity_raw": quote_reserve,
                        "liquidity_display": format_liquidity(quote_reserve),
                        "status": status,
                        "token0": token0,
                    })
                    print(f"  + {dex_name} fee=0.30% -> {pool_addr}  reserve_quote={format_liquidity(quote_reserve)}  [{status}]")

    print(f"\nScan complete: {found_count} pools found across {pair_count} pairs.\n")
    return results


def print_table(results):
    """Print results as a formatted ASCII table."""
    if not results:
        print("No pools found.")
        return

    # Column headers and widths
    headers = ["Base", "Quote", "DEX", "Fee", "Pool Address", "Liquidity", "Status"]
    col_widths = [
        max(len(headers[0]), max(len(r["base"]) for r in results)),
        max(len(headers[1]), max(len(r["quote"]) for r in results)),
        max(len(headers[2]), max(len(r["dex"]) for r in results)),
        max(len(headers[3]), max(len(r["fee_display"]) for r in results)),
        42,  # Pool address is always 42 chars
        max(len(headers[5]), max(len(r["liquidity_display"]) for r in results)),
        max(len(headers[6]), max(len(r["status"]) for r in results)),
    ]

    # Build separator
    sep = "+-" + "-+-".join("-" * w for w in col_widths) + "-+"

    # Build header row
    hdr = "| " + " | ".join(h.ljust(w) for h, w in zip(headers, col_widths)) + " |"

    print(sep)
    print(hdr)
    print(sep)

    for r in results:
        row_data = [
            r["base"],
            r["quote"],
            r["dex"],
            r["fee_display"],
            r["pool_address"],
            r["liquidity_display"],
            r["status"],
        ]
        row = "| " + " | ".join(val.ljust(w) for val, w in zip(row_data, col_widths)) + " |"
        print(row)

    print(sep)
    print(f"Total: {len(results)} pools")


def write_csv(results, filepath):
    """Write scan results to a CSV file."""
    os.makedirs(os.path.dirname(filepath), exist_ok=True)

    fieldnames = [
        "base", "quote", "dex", "fee_display", "fee_raw",
        "pool_address", "liquidity_raw", "liquidity_display",
        "status", "token0",
    ]

    with open(filepath, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(results)

    print(f"CSV written to: {filepath}")


def print_summary(results):
    """Print a summary of found vs empty pools by DEX."""
    print("\n=== Summary by DEX ===")
    dex_counts = {}
    for r in results:
        dex = r["dex"]
        if dex not in dex_counts:
            dex_counts[dex] = {"found": 0, "empty": 0, "total": 0}
        dex_counts[dex]["total"] += 1
        dex_counts[dex][r["status"]] += 1

    for dex, counts in sorted(dex_counts.items()):
        print(f"  {dex:30s}  total={counts['total']:3d}  found={counts['found']:3d}  empty={counts['empty']:3d}")

    total_found = sum(1 for r in results if r["status"] == "found")
    total_empty = sum(1 for r in results if r["status"] == "empty")
    print(f"\n  {'TOTAL':30s}  total={len(results):3d}  found={total_found:3d}  empty={total_empty:3d}")


# =============================================================================
# Main
# =============================================================================

def main():
    """Main entry point: scan pools, print table, write CSV, print summary."""
    print("=" * 80)
    print("  Polygon DEX Pool Scanner")
    print("=" * 80)
    print(f"  RPC: {RPC_URL[:50]}...")
    print(f"  Factories: {len(FACTORIES)}")
    print(f"  Base tokens: {len(BASE_TOKENS)}")
    print(f"  Quote tokens: {len(QUOTE_TOKENS)}")
    print(f"  Fee tiers (V3): {FEE_TIERS}")
    print(f"  Rate limit: {RATE_LIMIT_SEC * 1000:.0f}ms between calls")
    print("=" * 80)
    print()

    start_time = time.time()

    results = scan_all_pools()

    elapsed = time.time() - start_time
    print(f"Elapsed time: {elapsed:.1f}s  ({_rpc_id} RPC calls made)\n")

    # Print formatted table
    print("=" * 80)
    print("  RESULTS TABLE")
    print("=" * 80)
    print_table(results)

    # Print summary
    print_summary(results)

    # Write CSV
    print()
    write_csv(results, OUTPUT_CSV)

    # Also print counts of active (found with liquidity) pools per pair
    print("\n=== Active Pools per Token Pair ===")
    pair_map = {}
    for r in results:
        key = f"{r['base']}/{r['quote']}"
        if key not in pair_map:
            pair_map[key] = 0
        if r["status"] == "found":
            pair_map[key] += 1

    for pair, count in sorted(pair_map.items(), key=lambda x: -x[1]):
        bar = "#" * count
        print(f"  {pair:20s}  {count:2d}  {bar}")

    print(f"\nDone. CSV saved to: {OUTPUT_CSV}")


if __name__ == "__main__":
    main()
