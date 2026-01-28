#!/usr/bin/env python3
"""
Spread History Logger

Purpose:
    Logs DEX spreads to CSV for historical analysis.
    Runs alongside data collector to capture spread history.

Author: AI-Generated
Created: 2026-01-28

Usage:
    python3 spread_logger.py
    # Or run in background:
    nohup python3 spread_logger.py > /dev/null 2>&1 &

Output:
    data/spread_history.csv - Timestamped spread data
"""

import json
import csv
import time
import os
from datetime import datetime
from pathlib import Path

# Configuration
POOL_STATE_FILE = "/home/botuser/bots/dexarb/data/pool_state.json"
OUTPUT_CSV = "/home/botuser/bots/dexarb/data/spread_history.csv"
LOG_INTERVAL_SECONDS = 10  # Log every 10 seconds

def calculate_spread(price1: float, price2: float) -> float:
    """Calculate spread as percentage."""
    if price1 == 0 or price2 == 0:
        return 0.0
    return abs(price2 - price1) / min(price1, price2) * 100

def read_pool_state() -> dict:
    """Read current pool state from JSON file."""
    try:
        with open(POOL_STATE_FILE, 'r') as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"Error reading pool state: {e}")
        return None

def ensure_csv_header():
    """Create CSV file with header if it doesn't exist."""
    if not os.path.exists(OUTPUT_CSV):
        with open(OUTPUT_CSV, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerow([
                'timestamp',
                'block_number',
                'weth_usdc_spread_pct',
                'weth_uni_price',
                'weth_sushi_price',
                'wmatic_usdc_spread_pct',
                'wmatic_uni_price',
                'wmatic_sushi_price'
            ])
        print(f"Created CSV file: {OUTPUT_CSV}")

def log_spreads():
    """Log current spreads to CSV."""
    state = read_pool_state()
    if not state:
        return

    pools = state.get('pools', {})

    # Get prices
    weth_uni = pools.get('Uniswap:WETH/USDC', {}).get('price', 0)
    weth_sushi = pools.get('Sushiswap:WETH/USDC', {}).get('price', 0)
    wmatic_uni = pools.get('Uniswap:WMATIC/USDC', {}).get('price', 0)
    wmatic_sushi = pools.get('Sushiswap:WMATIC/USDC', {}).get('price', 0)

    # Calculate spreads
    weth_spread = calculate_spread(weth_uni, weth_sushi)
    wmatic_spread = calculate_spread(wmatic_uni, wmatic_sushi)

    # Get metadata
    timestamp = datetime.utcnow().isoformat()
    block_number = state.get('block_number', 0)

    # Write to CSV
    with open(OUTPUT_CSV, 'a', newline='') as f:
        writer = csv.writer(f)
        writer.writerow([
            timestamp,
            block_number,
            f"{weth_spread:.6f}",
            f"{weth_uni:.2f}",
            f"{weth_sushi:.2f}",
            f"{wmatic_spread:.6f}",
            f"{wmatic_uni:.6e}",
            f"{wmatic_sushi:.6e}"
        ])

    # Print status every 60 seconds
    return weth_spread, wmatic_spread

def main():
    """Main loop - log spreads at regular intervals."""
    print("=" * 60)
    print("Spread History Logger")
    print("=" * 60)
    print(f"Pool state file: {POOL_STATE_FILE}")
    print(f"Output CSV: {OUTPUT_CSV}")
    print(f"Log interval: {LOG_INTERVAL_SECONDS} seconds")
    print("=" * 60)

    ensure_csv_header()

    iteration = 0
    while True:
        try:
            result = log_spreads()
            iteration += 1

            # Print status every 6 iterations (60 seconds)
            if iteration % 6 == 0 and result:
                weth_spread, wmatic_spread = result
                print(f"[{datetime.utcnow().strftime('%H:%M:%S')}] "
                      f"WETH: {weth_spread:.4f}% | WMATIC: {wmatic_spread:.4f}%")

            time.sleep(LOG_INTERVAL_SECONDS)

        except KeyboardInterrupt:
            print("\nStopping spread logger...")
            break
        except Exception as e:
            print(f"Error: {e}")
            time.sleep(LOG_INTERVAL_SECONDS)

if __name__ == "__main__":
    main()
