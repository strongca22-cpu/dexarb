#!/usr/bin/env python3
"""
Pool Scanner — DEX Pool Discovery on Base (L2)

Purpose:
    Discovers DEX pool addresses on Base by querying factory contracts
    (Uniswap V3, SushiSwap V3, Aerodrome Slipstream CL).
    For each discovered pool, queries liquidity and token ordering.
    Outputs a formatted table and CSV file.

    Base-specific adaptation of pool_scanner.py (Polygon).
    Kept in a separate file per project compartmentalization rules.

Author: AI-Generated (Claude Code)
Created: 2026-02-03
Modified: 2026-02-03

Dependencies:
    - requests (HTTP JSON-RPC calls to Alchemy)

Usage:
    python3 scripts/base_pool_scanner.py
    python3 scripts/base_pool_scanner.py --quick   # WETH/USDC only (fast check)

Notes:
    - Uses eth_call JSON-RPC to query factory contracts directly
    - Rate-limited to 100ms between RPC calls to respect Alchemy limits
    - V3 factories checked at fee tiers: 100, 500, 3000, 10000
    - Aerodrome Slipstream uses tick spacings instead of fee tiers
    - No Algebra (QuickSwap) on Base
    - No major V2 DEXes on Base (Aerodrome V2 is Solidly-style, different interface)

Data Sources:
    - Input: On-chain Base factory contracts via Alchemy RPC
    - Output: /home/botuser/bots/dexarb/data/base/pool_scan_results.csv

Configuration:
    - RPC_URL: Alchemy HTTPS endpoint for Base mainnet
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

RPC_URL = "https://base-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"

OUTPUT_DIR = "/home/botuser/bots/dexarb/data/base"
OUTPUT_CSV = os.path.join(OUTPUT_DIR, "pool_scan_results.csv")

RATE_LIMIT_SEC = 0.10  # 100ms between RPC calls

# Zero address constant
ZERO_ADDR = "0x" + "0" * 40

# ---------------------------------------------------------------------------
# Factory definitions — Base Mainnet
# ---------------------------------------------------------------------------
FACTORIES = {
    "Uniswap V3": {
        "address": "0x33128a8fC17869897dcE68Ed026d694621f6FDfD",
        "type": "v3",
        "selector": "0x1698ee82",  # getPool(address,address,uint24)
    },
    "SushiSwap V3": {
        "address": "0xc35DADB65012eC5796536bD9864eD8773aBc74C4",
        "type": "v3",
        "selector": "0x1698ee82",  # getPool(address,address,uint24)
    },
    # Aerodrome Slipstream (concentrated liquidity, Velodrome v3 fork).
    # Uses getPool(tokenA, tokenB, tickSpacing) — same selector as V3 getPool
    # but the uint24 parameter is tick spacing, not fee tier.
    # Tick spacings: 1, 50, 100, 200 (map to ~0.01%, 1%, 2%, 4% fee equivalents).
    # Router: 0xBE6D8f0d05cC4be24d5167a3eF062215bE6D18a5
    "Aerodrome CL": {
        "address": "0x5e7BB104d84c7CB9B682AaC2F3d509f5F406809A",
        "type": "aerodrome_cl",
        "selector": "0x1698ee82",  # getPool(address,address,int24) — same ABI encoding
    },
}

# Aerodrome tick spacings (equivalent to fee tiers in V3 pools)
AERODROME_TICK_SPACINGS = [1, 50, 100, 200]

# ---------------------------------------------------------------------------
# Token definitions — Base Mainnet
# ---------------------------------------------------------------------------

# Base tokens: traded assets we want to find pools for
BASE_TOKENS = {
    "WETH":   "0x4200000000000000000000000000000000000006",
    "cbETH":  "0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22",
    "wstETH": "0xc1CBa3fCea344f92D9239c08C0568f6F2F0ee452",
    "DEGEN":  "0x4ed4E862860beD51a9570b96d89aF5E1B0Efefed",
    "AERO":   "0x940181a94A35A4569E4529A3CDfB74e38FD98631",
    "BRETT":  "0x532f27101965dd16442E59d40670FaF5eBB142E4",
    "TOSHI":  "0xAC1Bd2486aAf3B5C0fc3Fd868558b082a531B2B4",
    "DAI":    "0x50c5725949A6F0c72E6C4a641F24049A917DB0Cb",
    "COMP":   "0x9e1028F5F1D5eDE59748FFceE5532509976840E0",
    "SNX":    "0x22e6966B799c4D5B13BE962E1D117b56327FDa66",
    "WELL":   "0xA88594D404727625A9437C3f886C7643872296AE",
    "rETH":   "0xB6fe221Fe9EeF5aBa221c348bA20A1Bf5e73624c",
}

# Quote tokens: what we arb against (USDC primary, WETH secondary, USDbC for stablecoin arb)
QUOTE_TOKENS = {
    "USDC":  "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
    "WETH":  "0x4200000000000000000000000000000000000006",
    "USDbC": "0xd9aAEc86B65D86f6A7B5B1b0c42FFA531710b6CA",
}

# V3 fee tiers (standard across Uniswap V3 and SushiSwap V3)
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
    Execute an eth_call against the Base RPC endpoint.

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


# =============================================================================
# Factory query functions
# =============================================================================

def query_v3_pool(factory_addr, selector, token_a, token_b, fee):
    """Query a V3-style factory: getPool(tokenA, tokenB, fee)."""
    data = selector + encode_address(token_a) + encode_address(token_b) + encode_uint24(fee)
    result = rpc_call(factory_addr, data)
    addr = decode_address(result)
    return addr


# =============================================================================
# Pool data queries
# =============================================================================

def get_v3_liquidity(pool_addr):
    """Query liquidity() on a V3/CL pool."""
    result = rpc_call(pool_addr, SEL_LIQUIDITY)
    return decode_uint128(result)


def get_token0(pool_addr):
    """Query token0() on a pool."""
    result = rpc_call(pool_addr, SEL_TOKEN0)
    return decode_address(result)


def format_fee_tier(fee):
    """Convert a V3 fee tier integer to a human-readable percentage string."""
    return f"{fee / 10000:.2f}%"


def format_tick_spacing(ts):
    """Convert Aerodrome tick spacing to a display string."""
    # Approximate fee mapping for Aerodrome Slipstream
    fee_approx = {1: "~0.01%", 50: "~1%", 100: "~2%", 200: "~4%"}
    return fee_approx.get(ts, f"ts={ts}")


def format_liquidity(value):
    """Format a large liquidity number with thousand separators."""
    if value == 0:
        return "0"
    return f"{value:,}"


# =============================================================================
# Main scanning logic
# =============================================================================

def scan_all_pools(quick_mode=False):
    """
    Scan all factory x token-pair combinations and return a list of result dicts.

    Args:
        quick_mode: If True, only scan WETH/USDC pair (fast diagnostic).

    Returns:
        List of dicts with keys: base, quote, dex, fee_display, fee_raw,
        pool_address, liquidity_raw, liquidity_display, status, token0
    """
    results = []

    # In quick mode, only scan WETH against USDC
    if quick_mode:
        base_tokens = {"WETH": BASE_TOKENS["WETH"]}
        quote_tokens = {"USDC": QUOTE_TOKENS["USDC"]}
        print("QUICK MODE: scanning WETH/USDC only\n")
    else:
        base_tokens = BASE_TOKENS
        quote_tokens = QUOTE_TOKENS

    # Remove self-pairs (WETH is both base and quote)
    skip_pairs = set()
    for bname, baddr in base_tokens.items():
        for qname, qaddr in quote_tokens.items():
            if baddr.lower() == qaddr.lower():
                skip_pairs.add((bname, qname))

    total_pairs = len(base_tokens) * len(quote_tokens) - len(skip_pairs)
    total_factories = len(FACTORIES)
    print(f"Scanning {total_pairs} token pairs across {total_factories} factories...")
    print(f"V3 factories: {len(FEE_TIERS)} fee tiers each")
    print(f"Aerodrome CL: {len(AERODROME_TICK_SPACINGS)} tick spacings each\n")

    pair_count = 0
    found_count = 0

    for base_name, base_addr in base_tokens.items():
        for quote_name, quote_addr in quote_tokens.items():
            if (base_name, quote_name) in skip_pairs:
                continue

            pair_count += 1
            print(f"[{pair_count}/{total_pairs}] {base_name}/{quote_name}")

            for dex_name, factory in FACTORIES.items():
                f_type = factory["type"]
                f_addr = factory["address"]
                f_sel  = factory["selector"]

                if f_type == "v3":
                    # Standard V3: try each fee tier
                    for fee in FEE_TIERS:
                        pool_addr = query_v3_pool(f_addr, f_sel, base_addr, quote_addr, fee)
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
                            "fee_display": format_fee_tier(fee),
                            "fee_raw": fee,
                            "pool_address": pool_addr,
                            "liquidity_raw": liquidity,
                            "liquidity_display": format_liquidity(liquidity),
                            "status": status,
                            "token0": token0,
                        })
                        print(f"  + {dex_name} fee={format_fee_tier(fee)} -> {pool_addr}  liq={format_liquidity(liquidity)}  [{status}]")

                elif f_type == "aerodrome_cl":
                    # Aerodrome Slipstream: try each tick spacing
                    # Same getPool ABI but parameter is tick spacing, not fee
                    for ts in AERODROME_TICK_SPACINGS:
                        pool_addr = query_v3_pool(f_addr, f_sel, base_addr, quote_addr, ts)
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
                            "fee_display": format_tick_spacing(ts),
                            "fee_raw": ts,
                            "pool_address": pool_addr,
                            "liquidity_raw": liquidity,
                            "liquidity_display": format_liquidity(liquidity),
                            "status": status,
                            "token0": token0,
                        })
                        print(f"  + {dex_name} ts={ts} ({format_tick_spacing(ts)}) -> {pool_addr}  liq={format_liquidity(liquidity)}  [{status}]")

    print(f"\nScan complete: {found_count} pools found across {pair_count} pairs.\n")
    return results


def print_table(results):
    """Print results as a formatted ASCII table."""
    if not results:
        print("No pools found.")
        return

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

    sep = "+-" + "-+-".join("-" * w for w in col_widths) + "-+"
    hdr = "| " + " | ".join(h.ljust(w) for h, w in zip(headers, col_widths)) + " |"

    print(sep)
    print(hdr)
    print(sep)

    for r in results:
        row_data = [
            r["base"], r["quote"], r["dex"], r["fee_display"],
            r["pool_address"], r["liquidity_display"], r["status"],
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


def print_arb_candidates(results):
    """
    Identify arbitrage candidates: pairs with pools on 2+ DEXes.
    These are the pairs where cross-DEX price differences can be exploited.
    """
    print("\n=== Arbitrage Candidates (2+ DEXes per pair) ===")

    # Group by base/quote pair
    pair_pools = {}
    for r in results:
        if r["status"] != "found":
            continue
        key = f"{r['base']}/{r['quote']}"
        if key not in pair_pools:
            pair_pools[key] = []
        pair_pools[key].append(r)

    candidates = []
    for pair, pools in sorted(pair_pools.items()):
        dexes = set(p["dex"] for p in pools)
        if len(dexes) >= 2:
            candidates.append((pair, pools, dexes))

    if not candidates:
        print("  No cross-DEX arbitrage candidates found.")
        return

    for pair, pools, dexes in sorted(candidates, key=lambda x: -len(x[1])):
        print(f"\n  {pair} — {len(pools)} pools across {len(dexes)} DEXes")
        for p in sorted(pools, key=lambda x: (x["dex"], x["fee_raw"])):
            print(f"    {p['dex']:20s} {p['fee_display']:8s}  liq={p['liquidity_display']:>25s}  {p['pool_address']}")

    print(f"\n  Total: {len(candidates)} pairs with cross-DEX arb potential")
    print(f"  Total pools involved: {sum(len(p) for _, p, _ in candidates)}")


# =============================================================================
# Main
# =============================================================================

def main():
    """Main entry point: scan pools, print table, write CSV, print summary."""
    quick_mode = "--quick" in sys.argv

    print("=" * 80)
    print("  Base (L2) DEX Pool Scanner")
    print("=" * 80)
    print(f"  RPC: {RPC_URL[:50]}...")
    print(f"  Factories: {', '.join(FACTORIES.keys())}")
    print(f"  Base tokens: {len(BASE_TOKENS)}")
    print(f"  Quote tokens: {len(QUOTE_TOKENS)}")
    print(f"  V3 fee tiers: {FEE_TIERS}")
    print(f"  Aerodrome tick spacings: {AERODROME_TICK_SPACINGS}")
    print(f"  Rate limit: {RATE_LIMIT_SEC * 1000:.0f}ms between calls")
    print("=" * 80)
    print()

    start_time = time.time()

    results = scan_all_pools(quick_mode=quick_mode)

    elapsed = time.time() - start_time
    print(f"Elapsed time: {elapsed:.1f}s  ({_rpc_id} RPC calls made)\n")

    # Print formatted table
    print("=" * 80)
    print("  RESULTS TABLE")
    print("=" * 80)
    print_table(results)

    # Print summary
    print_summary(results)

    # Print arb candidates (pairs with pools on multiple DEXes)
    print_arb_candidates(results)

    # Write CSV
    print()
    write_csv(results, OUTPUT_CSV)

    # Active pools per token pair
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
