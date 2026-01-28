#!/usr/bin/env python3
"""
Spread History Logger - Comprehensive Version

Purpose:
    Logs DEX spreads to CSV for historical analysis.
    Tracks ALL V2 and V3 pools from pool_state_phase1.json.
    Calculates cross-DEX and V2-V3 spreads for arbitrage detection.

Author: AI-Generated
Created: 2026-01-28
Modified: 2026-01-28

Usage:
    python3 spread_logger.py
    # Or run in background:
    nohup python3 spread_logger.py > /dev/null 2>&1 &

Output:
    data/spread_history_v2.csv - Timestamped spread data (all pairs)
    data/spread_opportunities.csv - Actionable spreads only
"""

import json
import csv
import time
import os
import math
from datetime import datetime
from pathlib import Path
from collections import defaultdict

# Configuration
POOL_STATE_FILE = "/home/botuser/bots/dexarb/data/pool_state_phase1.json"
SPREAD_HISTORY_CSV = "/home/botuser/bots/dexarb/data/spread_history_v2.csv"
OPPORTUNITIES_CSV = "/home/botuser/bots/dexarb/data/spread_opportunities.csv"
LOG_INTERVAL_SECONDS = 10
MIN_OPPORTUNITY_SPREAD_PCT = 0.10  # Log opportunities above 0.10%

# Token decimals for price normalization
TOKEN_DECIMALS = {
    "USDC": 6,
    "USDT": 6,
    "DAI": 18,
    "WETH": 18,
    "WMATIC": 18,
    "WBTC": 8,
    "LINK": 18,
    "UNI": 18,
    "AAVE": 18,
    "CRV": 18,
}


def calculate_spread(price1: float, price2: float) -> float:
    """Calculate spread as percentage."""
    if price1 == 0 or price2 == 0:
        return 0.0
    return abs(price2 - price1) / min(price1, price2) * 100


def v3_sqrt_price_to_price(sqrt_price_x96) -> float:
    """Convert V3 sqrtPriceX96 to raw price ratio (token1/token0 in native units).

    NOTE: We don't adjust for decimals here because V2 pools also store
    raw reserve ratios. Both V2 and V3 prices are in the same units
    (token1_native_units / token0_native_units), making them directly comparable.
    """
    # Handle string or int input
    if isinstance(sqrt_price_x96, str):
        sqrt_price_x96 = int(sqrt_price_x96) if sqrt_price_x96 else 0
    if sqrt_price_x96 == 0:
        return 0.0
    # price = (sqrtPriceX96 / 2^96)^2
    return (sqrt_price_x96 / (2**96)) ** 2


def get_token_from_pair(pair_symbol: str) -> tuple:
    """Extract token symbols from pair like 'WETH/USDC'."""
    parts = pair_symbol.split('/')
    return (parts[0], parts[1]) if len(parts) == 2 else (None, None)


def read_pool_state() -> dict:
    """Read current pool state from JSON file."""
    try:
        with open(POOL_STATE_FILE, 'r') as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"Error reading pool state: {e}")
        return None


def ensure_csv_headers():
    """Create CSV files with headers if they don't exist."""
    # Spread history - general log
    if not os.path.exists(SPREAD_HISTORY_CSV):
        with open(SPREAD_HISTORY_CSV, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerow([
                'timestamp',
                'block_number',
                'pair',
                'dex1',
                'dex2',
                'price1',
                'price2',
                'spread_pct',
                'type'  # V2-V2, V3-V3, V2-V3
            ])
        print(f"Created: {SPREAD_HISTORY_CSV}")

    # Opportunities - actionable only
    if not os.path.exists(OPPORTUNITIES_CSV):
        with open(OPPORTUNITIES_CSV, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerow([
                'timestamp',
                'block_number',
                'pair',
                'dex1',
                'dex2',
                'price1',
                'price2',
                'spread_pct',
                'type',
                'direction'  # buy_at_dex1 or buy_at_dex2
            ])
        print(f"Created: {OPPORTUNITIES_CSV}")


def extract_prices(state: dict) -> dict:
    """Extract normalized prices from all pools."""
    prices = defaultdict(dict)

    # V2 pools
    for pool_key, pool_data in state.get('pools', {}).items():
        dex = pool_data.get('dex', 'Unknown')
        pair = pool_data.get('pair_symbol', 'Unknown')
        price = pool_data.get('price', 0)

        if price > 0:
            prices[pair][f"V2:{dex}"] = {
                'price': price,
                'type': 'V2',
                'dex': dex
            }

    # V3 pools
    for pool_key, pool_data in state.get('v3_pools', {}).items():
        # Parse key like "UniswapV3_0.05%:WETH/USDC"
        parts = pool_key.split(':')
        if len(parts) != 2:
            continue
        dex_fee = parts[0]  # e.g., "UniswapV3_0.05%"
        pair = parts[1]      # e.g., "WETH/USDC"

        sqrt_price = pool_data.get('sqrt_price_x96', 0)
        if sqrt_price == 0:
            continue

        # Convert V3 sqrt price to raw price ratio (same units as V2)
        price = v3_sqrt_price_to_price(sqrt_price)

        if price > 0:
            prices[pair][f"V3:{dex_fee}"] = {
                'price': price,
                'type': 'V3',
                'dex': dex_fee
            }

    return prices


def find_spreads(prices: dict) -> list:
    """Find all spreads between pools for the same pair."""
    spreads = []

    for pair, pool_prices in prices.items():
        pools = list(pool_prices.items())

        # Compare all pool pairs for this token pair
        for i in range(len(pools)):
            for j in range(i + 1, len(pools)):
                dex1_key, data1 = pools[i]
                dex2_key, data2 = pools[j]

                price1 = data1['price']
                price2 = data2['price']

                spread = calculate_spread(price1, price2)

                # Determine spread type
                if data1['type'] == 'V2' and data2['type'] == 'V2':
                    spread_type = 'V2-V2'
                elif data1['type'] == 'V3' and data2['type'] == 'V3':
                    spread_type = 'V3-V3'
                else:
                    spread_type = 'V2-V3'

                # Determine direction (buy low, sell high)
                direction = 'buy_at_' + (dex1_key if price1 < price2 else dex2_key)

                spreads.append({
                    'pair': pair,
                    'dex1': dex1_key,
                    'dex2': dex2_key,
                    'price1': price1,
                    'price2': price2,
                    'spread_pct': spread,
                    'type': spread_type,
                    'direction': direction
                })

    return spreads


def log_spreads():
    """Log current spreads to CSV files."""
    state = read_pool_state()
    if not state:
        return None

    timestamp = datetime.utcnow().isoformat()
    block_number = state.get('block_number', 0)

    # Extract prices and find spreads
    prices = extract_prices(state)
    spreads = find_spreads(prices)

    if not spreads:
        return None

    # Sort by spread for reporting
    spreads.sort(key=lambda x: x['spread_pct'], reverse=True)

    # Log top spreads to history (limit to top 20 per interval to avoid huge files)
    with open(SPREAD_HISTORY_CSV, 'a', newline='') as f:
        writer = csv.writer(f)
        for s in spreads[:20]:
            writer.writerow([
                timestamp,
                block_number,
                s['pair'],
                s['dex1'],
                s['dex2'],
                f"{s['price1']:.8e}",
                f"{s['price2']:.8e}",
                f"{s['spread_pct']:.4f}",
                s['type']
            ])

    # Log opportunities (spreads above threshold)
    opportunities = [s for s in spreads if s['spread_pct'] >= MIN_OPPORTUNITY_SPREAD_PCT]
    if opportunities:
        with open(OPPORTUNITIES_CSV, 'a', newline='') as f:
            writer = csv.writer(f)
            for s in opportunities:
                writer.writerow([
                    timestamp,
                    block_number,
                    s['pair'],
                    s['dex1'],
                    s['dex2'],
                    f"{s['price1']:.8e}",
                    f"{s['price2']:.8e}",
                    f"{s['spread_pct']:.4f}",
                    s['type'],
                    s['direction']
                ])

    return {
        'total_spreads': len(spreads),
        'opportunities': len(opportunities),
        'top_spread': spreads[0] if spreads else None,
        'by_type': {
            'V2-V2': len([s for s in spreads if s['type'] == 'V2-V2']),
            'V3-V3': len([s for s in spreads if s['type'] == 'V3-V3']),
            'V2-V3': len([s for s in spreads if s['type'] == 'V2-V3']),
        }
    }


def main():
    """Main loop - log spreads at regular intervals."""
    print("=" * 60)
    print("Comprehensive Spread Logger v2")
    print("=" * 60)
    print(f"Pool state file: {POOL_STATE_FILE}")
    print(f"Spread history:  {SPREAD_HISTORY_CSV}")
    print(f"Opportunities:   {OPPORTUNITIES_CSV}")
    print(f"Log interval:    {LOG_INTERVAL_SECONDS} seconds")
    print(f"Min opportunity: {MIN_OPPORTUNITY_SPREAD_PCT}%")
    print("=" * 60)

    ensure_csv_headers()

    iteration = 0
    while True:
        try:
            result = log_spreads()
            iteration += 1

            # Print status every 6 iterations (60 seconds)
            if iteration % 6 == 0 and result:
                top = result['top_spread']
                print(f"[{datetime.utcnow().strftime('%H:%M:%S')}] "
                      f"Spreads: {result['total_spreads']} | "
                      f"Opps: {result['opportunities']} | "
                      f"V2-V2: {result['by_type']['V2-V2']} | "
                      f"V3-V3: {result['by_type']['V3-V3']} | "
                      f"V2-V3: {result['by_type']['V2-V3']}")
                if top:
                    print(f"         Top: {top['pair']} {top['spread_pct']:.2f}% ({top['type']})")

            time.sleep(LOG_INTERVAL_SECONDS)

        except KeyboardInterrupt:
            print("\nStopping spread logger...")
            break
        except Exception as e:
            print(f"Error: {e}")
            time.sleep(LOG_INTERVAL_SECONDS)


if __name__ == "__main__":
    main()
