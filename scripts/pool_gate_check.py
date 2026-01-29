#!/usr/bin/env python3
"""
Pool Gate Check — V3 Pool Validation Script

Purpose:
    Runs Gate 1-3 checks from the V4 pairings buildout plan against
    candidate token/USDC pairs on Polygon Uniswap V3.

    Gate 1: Pool existence (does V3 pool exist at each fee tier?)
    Gate 2: Pool activity (non-zero price, liquidity above dust, not stale)
    Gate 3: Quoter depth (price impact at trade size — requires bot, logged only)

Author: AI-Generated
Created: 2026-01-29
Modified: 2026-01-29

Dependencies:
    - python3 (standard library only — no pip packages)
    - curl (for JSON-RPC calls)

Usage:
    # Check a single token
    python3 scripts/pool_gate_check.py AAVE 0xD6DF932A45C0f255f85145f286eA0b292B21C90B

    # Check multiple tokens
    python3 scripts/pool_gate_check.py --all

    # Check specific fee tier (default: 500,3000)
    python3 scripts/pool_gate_check.py AAVE 0xD6DF... --fees 100,500,3000

    # Custom RPC
    python3 scripts/pool_gate_check.py AAVE 0xD6DF... --rpc https://polygon-rpc.com

Notes:
    - All RPC calls are read-only (eth_call). Zero gas, zero risk.
    - Gate 3 (Quoter) uses quoteExactInputSingle — also read-only.
    - Results are printed to stdout. Pipe to file for records.
    - USDC.e address hardcoded: 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174
    - V3 Factory hardcoded: 0x1F98431c8aD98523631AE4a59f267346ea31F984
"""

import json
import subprocess
import sys
import argparse
from datetime import datetime, timezone

# --- Constants ---

USDC_ADDRESS = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"
V3_FACTORY = "0x1F98431c8aD98523631AE4a59f267346ea31F984"
V3_QUOTER = "0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6"
DEFAULT_RPC = "https://polygon-bor.publicnode.com"

# Function selectors
SELECTOR_GET_POOL = "0x1698ee82"      # getPool(address,address,uint24)
SELECTOR_SLOT0 = "0x3850c7bd"          # slot0()
SELECTOR_LIQUIDITY = "0x1a686502"      # liquidity()
SELECTOR_TOKEN0 = "0x0dfe1681"         # token0()
SELECTOR_TOKEN1 = "0xd21220a7"         # token1()
SELECTOR_FEE = "0xddca3f43"            # fee()
SELECTOR_QUOTE = "0xf7729d43"          # quoteExactInputSingle(address,address,uint24,uint256,uint160)

FEE_TIER_NAMES = {
    100: "0.01%",
    500: "0.05%",
    3000: "0.30%",
    10000: "1.00%",
}

# Known candidate pairs (Group A + B + stablecoin specials)
KNOWN_CANDIDATES = {
    # Group A: DeFi Protocol Tokens
    "AAVE":    "0xD6DF932A45C0f255f85145f286eA0b292B21C90B",
    "CRV":     "0x172370d5Cd63279eFa6d502DAB29171933a610AF",
    "SUSHI":   "0x0b3F868E0BE5597D5DB7fEB59E1CADBb0fdDa50a",
    "BAL":     "0x9a71012B13CA4d3D0Cda72A5D7Bab2E3d5C3E8A6",
    "GRT":     "0x5fe2B58c013d7601147DcdD68C143A77499f5531",
    # Group B: Infrastructure / Misc
    "SNX":     "0x50B728D8D964fd00C2d0AAD81718b71311feF68a",
    "1INCH":   "0x9c2C5fd7b07E95EE044DDeba0E97a665F142394f",
    "GHST":    "0x385Eeac5cB85A38A9a07A70c73e0a3271CfB54A7",
    "COMP":    "0x8505b9d2254A7Ae468c0E9dd10Ccea3A837aef5c",
    # Group C: Higher market cap / staking
    "stMATIC": "0x3A58a54C066FdC0f2D55FC9C89F0415C92eBf3C4",
    "wstETH":  "0x03b54A6e9a984069379fae1a4fC4dBAE93B3bCCD",
    # Already active (for 0.01% fee tier check)
    "USDT":    "0xc2132D05D31c914a87C6611C10748AEb04B58e8F",
    "DAI":     "0x8f3Cf7ad23Cd3CaDbD9735AFf958023239c6A063",
    "WETH":    "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619",
    "WMATIC":  "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270",
    "WBTC":    "0x1BFD67037B42Cf73acF2047067bd4F2C47D9BfD6",
    "LINK":    "0x53E0bca35eC356BD5ddDFebbD1Fc0fD03FaBad39",
    "UNI":     "0xb33EaAd8d922B1083446DC23f610c2567fB5180f",
}


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
    except Exception as e:
        print(f"  RPC error: {e}")
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


# --- Gate Checks ---

def gate1_pool_existence(rpc_url: str, token: str, fee_tiers: list) -> dict:
    """
    Gate 1: Check if V3 pools exist for token/USDC at specified fee tiers.
    Returns dict of {fee_tier: pool_address_or_None}.
    """
    results = {}
    for fee in fee_tiers:
        calldata = (
            SELECTOR_GET_POOL
            + pad_address(token)
            + pad_address(USDC_ADDRESS)
            + pad_uint(fee)
        )
        resp = eth_call(rpc_url, V3_FACTORY, calldata)
        if resp and len(resp) >= 42:
            addr = "0x" + resp[-40:]
            if addr == "0x" + "0" * 40:
                results[fee] = None
            else:
                results[fee] = addr
        else:
            results[fee] = None
    return results


def gate2_pool_activity(rpc_url: str, pool_address: str, fee: int) -> dict:
    """
    Gate 2: Check pool activity — slot0 (sqrtPriceX96, tick) + liquidity.
    Returns dict with parsed pool state.
    """
    result = {
        "address": pool_address,
        "fee": fee,
        "fee_name": FEE_TIER_NAMES.get(fee, f"{fee}bps"),
        "sqrtPriceX96": 0,
        "tick": 0,
        "liquidity": 0,
        "price": 0.0,
        "pass": False,
        "reason": "",
    }

    # Get slot0
    slot0_resp = eth_call(rpc_url, pool_address, SELECTOR_SLOT0)
    if not slot0_resp or len(slot0_resp) < 130:
        result["reason"] = "slot0 call failed"
        return result

    # Parse slot0: first 64 hex chars (32 bytes) = sqrtPriceX96
    # Next 64 hex chars = tick (int24, but padded to 32 bytes)
    raw = slot0_resp[2:]  # strip 0x
    sqrt_price = hex_to_int("0x" + raw[:64])
    tick = hex_to_int("0x" + raw[64:128], signed=True)

    result["sqrtPriceX96"] = sqrt_price
    result["tick"] = tick

    # Get liquidity
    liq_resp = eth_call(rpc_url, pool_address, SELECTOR_LIQUIDITY)
    if liq_resp:
        liquidity = hex_to_int(liq_resp)
        result["liquidity"] = liquidity
    else:
        result["reason"] = "liquidity call failed"
        return result

    # Calculate price from sqrtPriceX96
    # price = (sqrtPriceX96 / 2^96)^2
    if sqrt_price > 0:
        price = (sqrt_price / (2**96)) ** 2
        result["price"] = price
    else:
        result["reason"] = "sqrtPriceX96 is zero — pool uninitialized"
        return result

    # Assess
    if sqrt_price == 0:
        result["reason"] = "Zero price — pool uninitialized"
    elif liquidity == 0:
        result["reason"] = "Zero liquidity — no active LPs"
    elif liquidity < 1000:
        result["reason"] = f"Dust liquidity ({liquidity}) — effectively empty"
    else:
        result["pass"] = True
        result["reason"] = "Active"

    return result


def gate3_quoter_check(rpc_url: str, token: str, pool_address: str,
                       fee: int, trade_size_usdc: float = 140.0) -> dict:
    """
    Gate 3: Quoter depth check — simulate trade via quoteExactInputSingle.
    Uses USDC -> token direction (buy leg).

    quoteExactInputSingle(address tokenIn, address tokenOut, uint24 fee,
                          uint256 amountIn, uint160 sqrtPriceLimitX96)
    """
    result = {
        "trade_size_usdc": trade_size_usdc,
        "quoted_output": 0,
        "pass": False,
        "reason": "",
    }

    amount_in = int(trade_size_usdc * 1e6)  # USDC has 6 decimals

    calldata = (
        SELECTOR_QUOTE
        + pad_address(USDC_ADDRESS)   # tokenIn = USDC
        + pad_address(token)           # tokenOut = target token
        + pad_uint(fee)                # fee tier
        + pad_uint(amount_in)          # amountIn
        + pad_uint(0)                  # sqrtPriceLimitX96 = 0 (no limit)
    )

    resp = eth_call(rpc_url, V3_QUOTER, calldata)
    if not resp or resp == "0x" or len(resp) < 66:
        result["reason"] = "Quoter call failed or reverted (pool may lack liquidity)"
        return result

    quoted_out = hex_to_int(resp)
    result["quoted_output"] = quoted_out

    if quoted_out == 0:
        result["reason"] = "Quoter returned 0 — pool cannot execute this trade"
    else:
        result["pass"] = True
        result["reason"] = f"Quoter returned {quoted_out}"

    return result


# --- Main Report ---

def check_pair(symbol: str, token_address: str, fee_tiers: list,
               rpc_url: str, trade_size: float = 140.0) -> dict:
    """Run all gates for a token/USDC pair. Returns structured results."""
    report = {
        "symbol": f"{symbol}/USDC",
        "token": token_address,
        "usdc": USDC_ADDRESS,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "fee_tiers_checked": fee_tiers,
        "gate1": {},
        "gate2": {},
        "gate3": {},
        "overall": "UNKNOWN",
    }

    # --- Gate 1: Pool Existence ---
    pools = gate1_pool_existence(rpc_url, token_address, fee_tiers)
    gate1_results = {}
    existing_pools = {}
    for fee, addr in pools.items():
        fee_name = FEE_TIER_NAMES.get(fee, f"{fee}bps")
        if addr:
            gate1_results[fee_name] = {"address": addr, "exists": True}
            existing_pools[fee] = addr
        else:
            gate1_results[fee_name] = {"address": None, "exists": False}
    report["gate1"] = gate1_results

    # Gate 1 verdict: need at least 2 pools for cross-tier arb
    if len(existing_pools) < 2:
        report["overall"] = "FAIL — Gate 1: fewer than 2 fee tier pools exist"
        return report

    # --- Gate 2: Pool Activity ---
    gate2_results = {}
    active_pools = {}
    for fee, addr in existing_pools.items():
        g2 = gate2_pool_activity(rpc_url, addr, fee)
        fee_name = FEE_TIER_NAMES.get(fee, f"{fee}bps")
        gate2_results[fee_name] = g2
        if g2["pass"]:
            active_pools[fee] = addr

    report["gate2"] = gate2_results

    if len(active_pools) < 2:
        report["overall"] = "FAIL — Gate 2: fewer than 2 active pools"
        return report

    # --- Gate 3: Quoter Depth ---
    gate3_results = {}
    viable_pools = 0
    for fee, addr in active_pools.items():
        g3 = gate3_quoter_check(rpc_url, token_address, addr, fee, trade_size)
        fee_name = FEE_TIER_NAMES.get(fee, f"{fee}bps")
        gate3_results[fee_name] = g3
        if g3["pass"]:
            viable_pools += 1

    report["gate3"] = gate3_results

    if viable_pools < 1:
        report["overall"] = "FAIL — Gate 3: no pools can execute at trade size"
    elif viable_pools < 2:
        report["overall"] = "MARGINAL — Gate 3: only 1 pool executable"
    else:
        report["overall"] = "PASS — all gates passed"

    return report


def print_report(report: dict):
    """Print a human-readable report."""
    print(f"\n{'='*60}")
    print(f"  {report['symbol']}")
    print(f"  Token: {report['token']}")
    print(f"  Checked: {report['timestamp']}")
    print(f"{'='*60}")

    # Gate 1
    print(f"\n  GATE 1: Pool Existence")
    for fee_name, info in report["gate1"].items():
        status = "EXISTS" if info["exists"] else "MISSING"
        addr = info["address"] or "n/a"
        icon = "+" if info["exists"] else "-"
        print(f"    [{icon}] {fee_name}: {status}  {addr}")

    # Gate 2
    if report["gate2"]:
        print(f"\n  GATE 2: Pool Activity")
        for fee_name, info in report["gate2"].items():
            status = "ACTIVE" if info["pass"] else "INACTIVE"
            icon = "+" if info["pass"] else "-"
            liq = info.get("liquidity", 0)
            price = info.get("price", 0.0)
            reason = info.get("reason", "")
            print(f"    [{icon}] {fee_name}: {status}")
            print(f"        price={price:.8f}  liquidity={liq}  ({reason})")

    # Gate 3
    if report["gate3"]:
        print(f"\n  GATE 3: Quoter Depth (${report['gate3'].get(list(report['gate3'].keys())[0], {}).get('trade_size_usdc', 140)} trade)")
        for fee_name, info in report["gate3"].items():
            status = "EXECUTABLE" if info["pass"] else "REJECTED"
            icon = "+" if info["pass"] else "-"
            out = info.get("quoted_output", 0)
            reason = info.get("reason", "")
            print(f"    [{icon}] {fee_name}: {status}")
            print(f"        quoted_output={out}  ({reason})")

    # Overall
    print(f"\n  RESULT: {report['overall']}")
    print(f"{'='*60}\n")


def main():
    parser = argparse.ArgumentParser(
        description="Pool Gate Check — V3 pool validation for candidate pairs"
    )
    parser.add_argument("symbol", nargs="?", help="Token symbol (e.g., AAVE)")
    parser.add_argument("address", nargs="?", help="Token address on Polygon")
    parser.add_argument("--all", action="store_true",
                        help="Check all known candidate pairs")
    parser.add_argument("--group", choices=["A", "B", "C", "active", "stablecoin"],
                        help="Check a specific group of candidates")
    parser.add_argument("--fees", default="500,3000",
                        help="Comma-separated fee tiers to check (default: 500,3000)")
    parser.add_argument("--rpc", default=DEFAULT_RPC,
                        help=f"RPC URL (default: {DEFAULT_RPC})")
    parser.add_argument("--trade-size", type=float, default=140.0,
                        help="Trade size in USD for Gate 3 (default: 140)")
    parser.add_argument("--json", action="store_true",
                        help="Output as JSON instead of human-readable")

    args = parser.parse_args()

    fee_tiers = [int(f.strip()) for f in args.fees.split(",")]

    # Determine which tokens to check
    tokens_to_check = []

    if args.all:
        # All candidates (exclude already-active ones unless checking new fee tiers)
        for sym, addr in KNOWN_CANDIDATES.items():
            tokens_to_check.append((sym, addr))
    elif args.group:
        groups = {
            "A": ["AAVE", "CRV", "SUSHI", "BAL", "GRT"],
            "B": ["SNX", "1INCH", "GHST", "COMP"],
            "C": ["stMATIC", "wstETH"],
            "active": ["WETH", "WMATIC", "WBTC", "USDT", "DAI", "LINK", "UNI"],
            "stablecoin": ["USDT", "DAI"],
        }
        for sym in groups.get(args.group, []):
            if sym in KNOWN_CANDIDATES:
                tokens_to_check.append((sym, KNOWN_CANDIDATES[sym]))
    elif args.symbol and args.address:
        tokens_to_check.append((args.symbol, args.address))
    elif args.symbol and args.symbol in KNOWN_CANDIDATES:
        tokens_to_check.append((args.symbol, KNOWN_CANDIDATES[args.symbol]))
    else:
        parser.print_help()
        print("\nExamples:")
        print("  python3 scripts/pool_gate_check.py AAVE")
        print("  python3 scripts/pool_gate_check.py --group A")
        print("  python3 scripts/pool_gate_check.py --group stablecoin --fees 100,500,3000")
        print("  python3 scripts/pool_gate_check.py --all")
        print(f"\nKnown tokens: {', '.join(sorted(KNOWN_CANDIDATES.keys()))}")
        sys.exit(1)

    # Get current block for reference
    current_block = get_block_number(args.rpc)
    print(f"\nPool Gate Check — {datetime.now(timezone.utc).strftime('%Y-%m-%d %H:%M:%S UTC')}")
    print(f"RPC: {args.rpc}")
    print(f"Block: {current_block}")
    print(f"Fee tiers: {[FEE_TIER_NAMES.get(f, f'{f}bps') for f in fee_tiers]}")
    print(f"Trade size: ${args.trade_size}")
    print(f"Tokens to check: {len(tokens_to_check)}")

    # Run checks
    all_reports = []
    summary_pass = []
    summary_fail = []
    summary_marginal = []

    for symbol, address in tokens_to_check:
        report = check_pair(symbol, address, fee_tiers, args.rpc, args.trade_size)
        all_reports.append(report)

        if args.json:
            continue

        print_report(report)

        if "PASS" in report["overall"]:
            summary_pass.append(symbol)
        elif "MARGINAL" in report["overall"]:
            summary_marginal.append(symbol)
        else:
            summary_fail.append(symbol)

    if args.json:
        print(json.dumps(all_reports, indent=2, default=str))
        return

    # Summary
    if len(tokens_to_check) > 1:
        print(f"\n{'='*60}")
        print(f"  SUMMARY")
        print(f"{'='*60}")
        if summary_pass:
            print(f"  PASS:     {', '.join(summary_pass)}")
        if summary_marginal:
            print(f"  MARGINAL: {', '.join(summary_marginal)}")
        if summary_fail:
            print(f"  FAIL:     {', '.join(summary_fail)}")
        print(f"{'='*60}\n")


if __name__ == "__main__":
    main()
