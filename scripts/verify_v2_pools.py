#!/usr/bin/env python3
"""
V2 Pool Assessment for V2↔V3 Cross-Protocol Arbitrage

Purpose:
    Assesses V2 pool quality (QuickSwapV2, SushiSwapV2) for the V2↔V3 integration.
    Uses getReserves() + constant-product math to compute:
      - TVL (USD value of reserves)
      - Price impact at $1, $10, $100, $140, $500, $1000, $5000
      - Liquidity score and automated categorization (whitelist/marginal/blacklist)

    V2 pools use constant-product AMM (x * y = k) with 0.3% swap fee.
    amountOut = (amountIn * 997 * reserveOut) / (reserveIn * 1000 + amountIn * 997)

Author: AI-Generated
Created: 2026-01-30

Dependencies:
    - python3 (standard library only)
    - curl (for JSON-RPC calls)

Usage:
    python3 scripts/verify_v2_pools.py                    # Full assessment
    python3 scripts/verify_v2_pools.py --rpc https://...   # Custom RPC
    python3 scripts/verify_v2_pools.py --verbose            # Detailed output
"""

import json
import subprocess
import sys
import argparse
from datetime import datetime, timezone

# --- Constants ---

DEFAULT_RPC = "https://polygon-bor.publicnode.com"

# V2 Factory addresses (Polygon)
QUICKSWAP_V2_FACTORY = "0x5757371414417b8C6CAad45bAeF941aBc7d3Ab32"
SUSHISWAP_V2_FACTORY = "0xc35DADB65012eC5796536bD9864eD8773aBc74C4"

# Function selectors
SELECTOR_GET_PAIR = "0xe6a43905"      # getPair(address,address) → address
SELECTOR_GET_RESERVES = "0x0902f1ac"  # getReserves() → (uint112, uint112, uint32)
SELECTOR_TOKEN0 = "0x0dfe1681"        # token0() → address
SELECTOR_TOKEN1 = "0xd21220a7"        # token1() → address

# Token addresses (Polygon)
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

# Approximate token prices for TVL calculation (updated 2026-01-30)
TOKEN_PRICES_USD = {
    "USDC": 1.0, "USDT": 1.0, "DAI": 1.0,
    "WETH": 3300.0, "WMATIC": 0.50, "WBTC": 100000.0,
    "LINK": 25.0,
}

# V2 fee: 0.3% for both QuickSwap and SushiSwap
V2_FEE_BPS = 30  # 0.30%

# Quote sizes for depth analysis (in USD)
QUOTE_SIZES_USD = [1, 10, 100, 140, 500, 1000, 5000]

# Categorization thresholds (aligned with verify_whitelist_enhanced.py)
THRESHOLDS = {
    "whitelist_min_size": 500,       # V2 pools: whitelist if handles $500+
    "impact_whitelist": 5.0,         # Max impact % for whitelist
    "impact_marginal": 10.0,         # Max impact % for marginal at small sizes
    "min_tvl_usd": 50000,            # Minimum TVL in USD
}

# Pairs to assess (all V2 combinations)
PAIRS_TO_ASSESS = [
    ("WETH",   "USDC"),
    ("WMATIC", "USDC"),
    ("WBTC",   "USDC"),
    ("USDT",   "USDC"),
    ("DAI",    "USDC"),
    ("LINK",   "USDC"),
]

DEXES = [
    ("QuickSwapV2", QUICKSWAP_V2_FACTORY),
    ("SushiSwapV2", SUSHISWAP_V2_FACTORY),
]

# ANSI colors
GREEN = "\033[92m"
RED = "\033[91m"
YELLOW = "\033[93m"
BOLD = "\033[1m"
DIM = "\033[2m"
CYAN = "\033[96m"
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
    """Check if contract exists."""
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
            capture_output=True, text=True, timeout=15,
        )
        resp = json.loads(result.stdout)
        return int(resp["result"], 16)
    except Exception:
        return 0


def pad_address(addr: str) -> str:
    return addr.lower().replace("0x", "").zfill(64)


def pad_uint(val: int) -> str:
    return hex(val)[2:].zfill(64)


def hex_to_int(hex_str: str) -> int:
    if not hex_str or hex_str == "0x":
        return 0
    return int(hex_str, 16)


def format_usd(val: float) -> str:
    if val >= 1_000_000:
        return f"${val / 1_000_000:.2f}M"
    if val >= 1_000:
        return f"${val / 1_000:.1f}K"
    if val >= 1:
        return f"${val:.2f}"
    return f"${val:.4f}"


# --- V2 Pool Discovery ---

def get_pair_address(rpc_url: str, factory: str, token_a: str, token_b: str) -> str:
    """Query V2 factory getPair(tokenA, tokenB) → pool address."""
    calldata = SELECTOR_GET_PAIR + pad_address(token_a) + pad_address(token_b)
    resp = eth_call(rpc_url, factory, calldata)
    if not resp or len(resp) < 66:
        return ""
    addr_hex = resp[2:66]
    # Check for zero address
    if int(addr_hex, 16) == 0:
        return ""
    return "0x" + addr_hex[24:]  # last 20 bytes = address


def get_reserves(rpc_url: str, pool_address: str) -> tuple:
    """Call getReserves() → (reserve0, reserve1, blockTimestampLast)."""
    resp = eth_call(rpc_url, pool_address, SELECTOR_GET_RESERVES)
    if not resp or len(resp) < 194:
        return (0, 0, 0)
    reserve0 = hex_to_int("0x" + resp[2:66])
    reserve1 = hex_to_int("0x" + resp[66:130])
    timestamp = hex_to_int("0x" + resp[130:194])
    return (reserve0, reserve1, timestamp)


def get_token0(rpc_url: str, pool_address: str) -> str:
    """Get token0 address from pool."""
    resp = eth_call(rpc_url, pool_address, SELECTOR_TOKEN0)
    if not resp or len(resp) < 66:
        return ""
    return "0x" + resp[26:66]


def get_token1(rpc_url: str, pool_address: str) -> str:
    """Get token1 address from pool."""
    resp = eth_call(rpc_url, pool_address, SELECTOR_TOKEN1)
    if not resp or len(resp) < 66:
        return ""
    return "0x" + resp[26:66]


# --- V2 Constant-Product Math ---

def v2_get_amount_out(amount_in: int, reserve_in: int, reserve_out: int) -> int:
    """
    Compute V2 swap output using constant-product formula.
    amountOut = (amountIn * 997 * reserveOut) / (reserveIn * 1000 + amountIn * 997)

    Fee: 0.3% (997/1000 factor on input)
    """
    if reserve_in == 0 or reserve_out == 0 or amount_in == 0:
        return 0
    amount_in_with_fee = amount_in * 997
    numerator = amount_in_with_fee * reserve_out
    denominator = reserve_in * 1000 + amount_in_with_fee
    if denominator == 0:
        return 0
    return numerator // denominator


# --- Pool Assessment ---

def assess_v2_pool(rpc_url: str, pool_address: str, token_a_sym: str,
                    dex_name: str, verbose: bool = False) -> dict:
    """
    Full V2 pool assessment:
    1. Verify pool exists
    2. Get reserves and determine token ordering
    3. Calculate TVL
    4. Run quote depth matrix using constant-product math
    5. Calculate price impact at each size
    6. Categorize
    """
    pair = f"{token_a_sym}/USDC"
    token_a_addr = TOKEN_ADDRESSES[token_a_sym].lower()
    usdc_addr = TOKEN_ADDRESSES["USDC"].lower()
    token_a_decimals = TOKEN_DECIMALS[token_a_sym]
    usdc_decimals = TOKEN_DECIMALS["USDC"]
    token_a_price = TOKEN_PRICES_USD.get(token_a_sym, 0)

    result = {
        "address": pool_address,
        "pair": pair,
        "dex": dex_name,
        "fee_bps": V2_FEE_BPS,
        "exists": False,
        "token0_is_usdc": False,
        "reserve_usdc": 0,
        "reserve_other": 0,
        "tvl_usd": 0.0,
        "quotes": [],
        "max_working_size": 0,
        "impact_at_max": None,
        "liquidity_score": 0,
        "category": "blacklist",
        "reason": "",
    }

    if not pool_address:
        result["reason"] = "Pool not found in factory"
        return result

    # Check exists
    code = eth_get_code(rpc_url, pool_address)
    if not code or code in ("0x", "0x0"):
        result["reason"] = "No bytecode at address"
        return result
    result["exists"] = True

    # Get reserves
    reserve0, reserve1, ts = get_reserves(rpc_url, pool_address)
    if reserve0 == 0 and reserve1 == 0:
        result["reason"] = "Zero reserves"
        return result

    # Determine token ordering
    token0_addr = get_token0(rpc_url, pool_address).lower()
    token0_is_usdc = (token0_addr == usdc_addr)
    result["token0_is_usdc"] = token0_is_usdc

    if token0_is_usdc:
        reserve_usdc = reserve0
        reserve_other = reserve1
    else:
        reserve_usdc = reserve1
        reserve_other = reserve0

    result["reserve_usdc"] = reserve_usdc
    result["reserve_other"] = reserve_other

    # Calculate TVL
    usdc_value = reserve_usdc / (10 ** usdc_decimals)
    other_value = (reserve_other / (10 ** token_a_decimals)) * token_a_price
    tvl_usd = usdc_value + other_value
    result["tvl_usd"] = tvl_usd

    if verbose:
        print(f"    Reserves: {reserve_usdc / 10**usdc_decimals:.2f} USDC + "
              f"{reserve_other / 10**token_a_decimals:.6g} {token_a_sym}")
        print(f"    TVL: {format_usd(tvl_usd)} | token0_is_usdc: {token0_is_usdc}")

    # Run quote depth matrix
    # We simulate: USDC → token_a (buying the other token with USDC)
    baseline_rate = None

    for size_usd in QUOTE_SIZES_USD:
        amount_in = size_usd * (10 ** usdc_decimals)  # USDC raw

        # Compute swap output
        amount_out = v2_get_amount_out(amount_in, reserve_usdc, reserve_other)

        passed = amount_out > 0
        human_out = amount_out / (10 ** token_a_decimals) if amount_out > 0 else 0.0

        # Price impact vs $1 baseline
        impact_pct = None
        if baseline_rate and baseline_rate > 0 and amount_out > 0:
            current_rate = amount_out / amount_in
            impact_pct = abs(1.0 - current_rate / baseline_rate) * 100

        # USD value of output
        value_usd = human_out * token_a_price if amount_out > 0 else 0.0

        entry = {
            "size_usd": size_usd,
            "amount_out": amount_out,
            "human_out": human_out,
            "pass": passed,
            "impact_pct": impact_pct,
            "value_usd": value_usd,
        }
        result["quotes"].append(entry)

        if size_usd == 1 and amount_out > 0:
            baseline_rate = amount_out / amount_in

        if passed:
            result["max_working_size"] = size_usd
            if impact_pct is not None:
                result["impact_at_max"] = impact_pct

        if verbose and passed:
            impact_str = f" (impact: {impact_pct:.2f}%)" if impact_pct else ""
            print(f"    ${size_usd}: {human_out:.6g} {token_a_sym} ≈ {format_usd(value_usd)}{impact_str}")

    # Liquidity score
    result["liquidity_score"] = calculate_liquidity_score(result)

    # Categorize
    result["category"], result["reason"] = categorize_pool(result)

    return result


def calculate_liquidity_score(r: dict) -> int:
    """0-100 liquidity score (aligned with verify_whitelist_enhanced.py)."""
    max_size = r["max_working_size"]
    impact = r["impact_at_max"]
    tvl = r["tvl_usd"]

    if max_size == 0 or tvl < 100:
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

    if max_size >= 500:
        if impact is None or impact < 5:
            return 60
        elif impact < 10:
            return 40
        else:
            return 25

    if max_size >= 100:
        if impact is None or impact < 10:
            return 35
        else:
            return 20

    if max_size >= 10:
        return 10

    return 5


def categorize_pool(r: dict) -> tuple:
    """Categorize V2 pool as whitelist/marginal/blacklist."""
    max_size = r["max_working_size"]
    impact = r["impact_at_max"]
    tvl = r["tvl_usd"]

    if not r["exists"]:
        return ("blacklist", "Pool doesn't exist")
    if r["reserve_usdc"] == 0:
        return ("blacklist", "Zero USDC reserves")
    if tvl < THRESHOLDS["min_tvl_usd"]:
        return ("blacklist", f"TVL too low: {format_usd(tvl)}")
    if max_size == 0:
        return ("blacklist", "All swaps return zero")

    # WHITELIST: handles $500+ with acceptable impact
    if max_size >= THRESHOLDS["whitelist_min_size"]:
        if impact is None or impact <= THRESHOLDS["impact_whitelist"]:
            return ("whitelist", f"Handles ${max_size} @ {impact:.2f}% impact, TVL={format_usd(tvl)}")
        elif impact <= THRESHOLDS["impact_marginal"]:
            return ("marginal", f"Works at ${max_size} but impact {impact:.2f}%, TVL={format_usd(tvl)}")
        else:
            return ("blacklist", f"High impact: {impact:.2f}% at ${max_size}")

    # MARGINAL: works at small sizes
    if max_size >= 10:
        if impact is None or impact <= THRESHOLDS["impact_marginal"]:
            return ("marginal", f"Small pool: max ${max_size} @ {impact:.2f}% impact, TVL={format_usd(tvl)}")
        else:
            return ("blacklist", f"High impact at ${max_size}: {impact:.2f}%")

    return ("blacklist", f"Insufficient depth: max ${max_size}")


# --- Display ---

def print_results(results: list, block: int):
    """Print assessment matrix and categorization."""
    print()
    print("=" * 140)
    print(f"  {BOLD}V2 POOL ASSESSMENT FOR V2↔V3 CROSS-PROTOCOL ARBITRAGE{RESET}")
    print(f"  {DIM}Constant-product AMM | 0.3% fee | getReserves() math{RESET}")
    print(f"  {DIM}Block: {block}{RESET}")
    print("=" * 140)

    # Headers
    COL = 10
    size_headers = "".join(f"{'$'+str(s):>{COL}}" for s in QUOTE_SIZES_USD)
    print(f"\n  {'Pool':<14} {'Pair':<13} {'DEX':<12} {'TVL':>10} {'Max$':<7}"
          f"{size_headers}  {'Impact':>{COL}} {'Score':<6} {'Category':<12}")
    sep = "-" * 136
    print(f"  {sep}")

    for r in results:
        if not r["address"]:
            addr = "NOT FOUND"
        else:
            addr = r["address"][:6] + ".." + r["address"][-4:]
        pair = r["pair"]
        dex = r["dex"]
        tvl = format_usd(r["tvl_usd"]) if r["tvl_usd"] > 0 else "N/A"
        max_s = f"${r['max_working_size']}" if r["max_working_size"] > 0 else "FAIL"

        cells = ""
        for q in r["quotes"]:
            if not q["pass"]:
                cells += f"{RED}{'FAIL':>{COL}}{RESET}"
            else:
                impact = q.get("impact_pct")
                if impact is None:
                    cells += f"{GREEN}{'OK':>{COL}}{RESET}"
                elif impact <= 2:
                    cells += f"{GREEN}{impact:.2f}%".rjust(COL) + f"{RESET}"
                elif impact <= 5:
                    cells += f"{GREEN}{impact:.2f}%".rjust(COL) + f"{RESET}"
                elif impact <= 10:
                    cells += f"{YELLOW}{impact:.2f}%".rjust(COL) + f"{RESET}"
                else:
                    cells += f"{RED}{impact:.2f}%".rjust(COL) + f"{RESET}"

        # Impact
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

        print(f"  {addr:<14} {pair:<13} {dex:<12} {tvl:>10} {max_s:<7}"
              f"{cells}  {imp_str} {score:<6} {cat_str}")

    # Categorization summary
    whitelist = [r for r in results if r["category"] == "whitelist"]
    marginal = [r for r in results if r["category"] == "marginal"]
    blacklist = [r for r in results if r["category"] == "blacklist"]

    print()
    print("=" * 140)
    print(f"  {BOLD}CATEGORIZATION SUMMARY{RESET}")
    print("=" * 140)

    print(f"\n  {GREEN}{BOLD}WHITELIST ({len(whitelist)} pools){RESET} — Safe for V2↔V3 cross-protocol arb")
    for r in whitelist:
        print(f"    {r['address']}  {r['pair']:<13} {r['dex']:<12} TVL={format_usd(r['tvl_usd'])}")
        print(f"      {r['reason']}")

    print(f"\n  {YELLOW}{BOLD}MARGINAL ({len(marginal)} pools){RESET} — Monitor only, small trades possible")
    for r in marginal:
        addr = r['address'] if r['address'] else 'N/A'
        print(f"    {addr}  {r['pair']:<13} {r['dex']:<12} TVL={format_usd(r['tvl_usd'])}")
        print(f"      {r['reason']}")

    print(f"\n  {RED}{BOLD}BLACKLIST ({len(blacklist)} pools){RESET} — Exclude from bot")
    for r in blacklist:
        addr = r['address'] if r['address'] else 'N/A'
        print(f"    {addr}  {r['pair']:<13} {r['dex']:<12} TVL={format_usd(r['tvl_usd'])}")
        print(f"      {r['reason']}")

    # Cross-protocol arb analysis
    print()
    print("=" * 140)
    print(f"  {BOLD}V2↔V3 CROSS-PROTOCOL ARB OPPORTUNITY ANALYSIS{RESET}")
    print(f"  {DIM}V2 fee: 0.30% | V3 UniswapV3 fee tiers: 0.01%, 0.05%, 0.30%{RESET}")
    print(f"  {DIM}V3 SushiswapV3: 0.01%, 0.30% | V3 QuickSwap: ~0.09% (dynamic){RESET}")
    print("=" * 140)

    v3_fees = {
        "WETH/USDC": [("UniV3 0.05%", 0.05), ("UniV3 0.30%", 0.30), ("SushiV3 0.30%", 0.30), ("QS V3", 0.09)],
        "WMATIC/USDC": [("UniV3 0.05%", 0.05), ("QS V3", 0.09)],
        "WBTC/USDC": [("UniV3 0.05%", 0.05), ("QS V3", 0.09)],
        "USDT/USDC": [("UniV3 0.01%", 0.01), ("UniV3 0.05%", 0.05), ("SushiV3 0.01%", 0.01), ("QS V3", 0.001)],
        "DAI/USDC": [("UniV3 0.01%", 0.01), ("UniV3 0.05%", 0.05), ("QS V3", 0.001)],
        "LINK/USDC": [],
    }

    for r in whitelist + marginal:
        pair = r["pair"]
        v2_fee = 0.30
        tvl = r["tvl_usd"]
        print(f"\n  {BOLD}{pair}{RESET} — {r['dex']} (TVL={format_usd(tvl)}, fee=0.30%)")

        for v3_name, v3_fee in v3_fees.get(pair, []):
            rt = v2_fee + v3_fee
            print(f"    ↔ {v3_name} → round-trip: {rt:.2f}%", end="")
            if rt <= 0.35:
                print(f"  {GREEN}LOW FEE ← good arb potential{RESET}")
            elif rt <= 0.60:
                print(f"  {YELLOW}moderate fee{RESET}")
            else:
                print(f"  {DIM}high fee{RESET}")

    print()


# --- Main ---

def main():
    parser = argparse.ArgumentParser(description="V2 Pool Assessment for V2↔V3 Arbitrage")
    parser.add_argument("--rpc", "-r", default=None, help="RPC URL")
    parser.add_argument("--verbose", "-v", action="store_true", help="Show detailed output")
    args = parser.parse_args()

    # Resolve RPC
    import os
    rpc_url = args.rpc
    if not rpc_url:
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
                            rpc_url = url
                            break
            except Exception:
                pass
    if not rpc_url:
        rpc_url = DEFAULT_RPC

    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    block = get_block_number(rpc_url)
    if block == 0:
        print(f"{RED}ERROR: Cannot connect to RPC at {rpc_url}{RESET}")
        sys.exit(1)

    print(f"\n  {BOLD}V2 Pool Assessment{RESET} | {now} | Block {block}")
    print(f"  RPC: {rpc_url}")
    print(f"  Pairs: {len(PAIRS_TO_ASSESS)} | DEXes: {len(DEXES)}")
    print(f"  Quote sizes: {QUOTE_SIZES_USD}")
    print()

    # Discover and assess all V2 pools
    results = []
    total = len(PAIRS_TO_ASSESS) * len(DEXES)
    idx = 0

    for token_sym, quote_sym in PAIRS_TO_ASSESS:
        for dex_name, factory_addr in DEXES:
            idx += 1
            pair = f"{token_sym}/{quote_sym}"

            token_addr = TOKEN_ADDRESSES[token_sym]
            quote_addr = TOKEN_ADDRESSES[quote_sym]

            print(f"  [{idx}/{total}] {pair} on {dex_name}...", end="", flush=True)

            # Discover pool address from factory
            pool_addr = get_pair_address(rpc_url, factory_addr, token_addr, quote_addr)

            if not pool_addr:
                print(f" {RED}NOT FOUND{RESET}")
                results.append({
                    "address": "",
                    "pair": pair,
                    "dex": dex_name,
                    "fee_bps": V2_FEE_BPS,
                    "exists": False,
                    "token0_is_usdc": False,
                    "reserve_usdc": 0,
                    "reserve_other": 0,
                    "tvl_usd": 0.0,
                    "quotes": [{"size_usd": s, "amount_out": 0, "human_out": 0, "pass": False,
                                "impact_pct": None, "value_usd": 0} for s in QUOTE_SIZES_USD],
                    "max_working_size": 0,
                    "impact_at_max": None,
                    "liquidity_score": 0,
                    "category": "blacklist",
                    "reason": "Pool not found in factory",
                })
                continue

            # Assess
            r = assess_v2_pool(rpc_url, pool_addr, token_sym, dex_name, verbose=args.verbose)
            results.append(r)

            cat = r["category"]
            tvl = format_usd(r["tvl_usd"])
            if cat == "whitelist":
                print(f" {GREEN}WHITELIST{RESET} | TVL={tvl} | addr={pool_addr}")
            elif cat == "marginal":
                print(f" {YELLOW}MARGINAL{RESET} | TVL={tvl} | addr={pool_addr}")
            else:
                print(f" {RED}BLACKLIST{RESET} | TVL={tvl} | addr={pool_addr}")

    # Print full results
    print_results(results, block)


if __name__ == "__main__":
    main()
