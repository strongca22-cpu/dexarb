#!/usr/bin/env python3
"""
De-duplicated Profitability Estimator (Corrected Gas)

Purpose:
    Runs cross-DEX spread analysis with de-duplication (best single route
    per pair per block) and corrected gas cost. Extends trade sizes to
    $15K for dynamic pricing analysis.

Author: AI-Generated
Created: 2026-02-01
Modified: 2026-02-01

Usage:
    python3 scripts/analyze_dedup_corrected.py <csv_path> [hours]
    python3 scripts/analyze_dedup_corrected.py data/polygon/price_history/prices_20260131.csv 24.0

Notes:
    - Gas cost based on 500K gas * 5030 gwei * $0.35 POL / 1e9 = ~$0.88
    - De-duplication: only best net profit route per (pair, block) is counted
    - Extended trade sizes: $500 through $15,000

Dependencies:
    - Python 3.8+ (stdlib only)
"""

import csv
import sys
from collections import defaultdict

# ── Configuration ────────────────────────────────────────────────────────────

TRADE_SIZES = [500, 1000, 2000, 5000, 8000, 10000, 15000]

# Gas cost scenarios
GAS_LOW = 0.10    # Minimal priority (30 gwei), low competition
GAS_MED = 0.50    # Moderate priority (~2500 gwei)
GAS_HIGH = 0.88   # Aggressive priority (5000 gwei)

# We'll report at GAS_HIGH as default (competitive scenario)
GAS_COST_USD = GAS_HIGH


def load_data(path):
    """Load CSV price data into list of dicts."""
    rows = []
    with open(path, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            row['price'] = float(row['price'])
            row['fee'] = int(row['fee'])
            row['block'] = int(row['block'])
            rows.append(row)
    return rows


def analyze_dedup(rows, hours):
    """
    De-duplicated analysis: for each (pair, block), find the single best
    profitable route and count only that one.
    """
    # Group by (pair, block) -> {(dex, fee): price}
    block_prices = defaultdict(dict)
    for r in rows:
        bk = (r['pair'], r['block'])
        dk = (r['dex'], r['fee'])
        block_prices[bk][dk] = r['price']

    # For each trade size, compute de-duplicated profitability
    print("=" * 110)
    print("DE-DUPLICATED PROFITABILITY ANALYSIS (best route per pair per block)")
    print(f"Gas cost: ${GAS_COST_USD:.2f}/trade | Observation: {hours:.1f} hours | Rows: {len(rows):,}")
    print("=" * 110)

    size_results = {}

    for size in TRADE_SIZES:
        # For each (pair, block): find best net profit across all DEX combos
        pair_block_best = {}  # (pair, block) -> (net_profit, buy_label, sell_label)

        for (pair, block), dex_prices in block_prices.items():
            if len(dex_prices) < 2:
                continue
            items = list(dex_prices.items())
            best_net = None
            best_route = None

            for i in range(len(items)):
                for j in range(i + 1, len(items)):
                    (dex_a, fee_a), price_a = items[i]
                    (dex_b, fee_b), price_b = items[j]
                    if price_a <= 0 or price_b <= 0:
                        continue

                    spread = abs(price_a - price_b) / min(price_a, price_b)
                    rt_fee = (fee_a + fee_b) / 1_000_000
                    net = (spread - rt_fee) * size - GAS_COST_USD

                    if net > 0:
                        if best_net is None or net > best_net:
                            if price_a > price_b:
                                buy_l = f"{dex_a}({fee_a})"
                                sell_l = f"{dex_b}({fee_b})"
                            else:
                                buy_l = f"{dex_b}({fee_b})"
                                sell_l = f"{dex_a}({fee_a})"
                            best_net = net
                            best_route = (buy_l, sell_l, pair)

            if best_net is not None:
                pair_block_best[(pair, block)] = (best_net, best_route)

        # Aggregate by pair
        pair_totals = defaultdict(lambda: {'count': 0, 'total_net': 0.0, 'max_net': 0.0, 'nets': []})
        for (pair, block), (net, route) in pair_block_best.items():
            pair_totals[pair]['count'] += 1
            pair_totals[pair]['total_net'] += net
            pair_totals[pair]['max_net'] = max(pair_totals[pair]['max_net'], net)
            pair_totals[pair]['nets'].append(net)

        total_count = sum(d['count'] for d in pair_totals.values())
        total_net = sum(d['total_net'] for d in pair_totals.values())

        size_results[size] = {
            'total_count': total_count,
            'total_net': total_net,
            'pair_totals': dict(pair_totals),
        }

        print(f"\n{'─' * 110}")
        print(f"  TRADE SIZE: ${size:,}  (gas: ${GAS_COST_USD:.2f})")
        print(f"{'─' * 110}")
        print(f"  {'Pair':<16} {'Blocks':>8} {'Prof Blocks':>12} {'%':>6} {'AvgNet$':>9} {'MaxNet$':>9} {'Total$':>10} {'$/hr':>8}")
        print(f"  {'-'*82}")

        for pair in sorted(pair_totals.keys()):
            d = pair_totals[pair]
            # Count total blocks for this pair
            total_blocks = sum(1 for (p, b) in block_prices if p == pair and len(block_prices[(p, b)]) >= 2)
            pct = d['count'] / total_blocks * 100 if total_blocks > 0 else 0
            avg = d['total_net'] / d['count'] if d['count'] > 0 else 0
            per_hr = d['total_net'] / hours
            print(f"  {pair:<16} {total_blocks:>8,} {d['count']:>12,} {pct:>5.1f}% ${avg:>8.2f} ${d['max_net']:>8.2f} ${d['total_net']:>9.2f} ${per_hr:>7.2f}")

        per_hr = total_net / hours
        per_day = per_hr * 24
        per_month = per_day * 30
        print(f"  {'TOTAL':<16} {'':>8} {total_count:>12,} {'':>6} {'':>9} {'':>9} ${total_net:>9.2f} ${per_hr:>7.2f}")

    # Summary table
    print(f"\n{'=' * 110}")
    print("SUMMARY: DE-DUPLICATED THEORETICAL MAX (best route per pair per block)")
    print(f"Gas: ${GAS_COST_USD:.2f}/trade | Observation: {hours:.1f}h")
    print(f"{'=' * 110}")
    print(f"  {'Size':>8} {'Prof Blocks':>12} {'Total Net$':>12} {'$/hour':>10} {'$/day':>10} {'$/month':>12}")
    print(f"  {'-'*70}")

    for size in TRADE_SIZES:
        r = size_results[size]
        per_hr = r['total_net'] / hours
        per_day = per_hr * 24
        per_month = per_day * 30
        print(f"  ${size:>7,} {r['total_count']:>12,} ${r['total_net']:>11.2f} ${per_hr:>9.2f} ${per_day:>9.2f} ${per_month:>11.2f}")

    # Gas sensitivity
    print(f"\n{'=' * 110}")
    print("GAS SENSITIVITY (at $5,000 trade size)")
    print(f"{'=' * 110}")
    for gas_label, gas_val in [("Low ($0.10)", 0.10), ("Med ($0.50)", 0.50), ("High ($0.88)", 0.88)]:
        total_net = 0
        total_count = 0
        for (pair, block), dex_prices in block_prices.items():
            if len(dex_prices) < 2:
                continue
            items = list(dex_prices.items())
            best_net = None
            for i in range(len(items)):
                for j in range(i + 1, len(items)):
                    (_, fee_a), price_a = items[i]
                    (_, fee_b), price_b = items[j]
                    if price_a <= 0 or price_b <= 0:
                        continue
                    spread = abs(price_a - price_b) / min(price_a, price_b)
                    rt_fee = (fee_a + fee_b) / 1_000_000
                    net = (spread - rt_fee) * 5000 - gas_val
                    if net > 0 and (best_net is None or net > best_net):
                        best_net = net
            if best_net is not None:
                total_net += best_net
                total_count += 1

        per_hr = total_net / hours
        print(f"  Gas {gas_label:<14}: {total_count:>6,} blocks | ${total_net:>10.2f} total | ${per_hr:>8.2f}/hr | ${per_hr*24:>9.2f}/day")

    print(f"\nNOTE: These are THEORETICAL MAXIMUMS (100% capture). Apply capture rate:")
    print(f"  15% conservative | 25% moderate | 40% optimistic")
    print(f"  Also apply ~25% slippage haircut for trades > $2K")
    print()


if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: python3 scripts/analyze_dedup_corrected.py <csv_path> [hours]")
        sys.exit(1)

    path = sys.argv[1]
    hours = float(sys.argv[2]) if len(sys.argv) > 2 else 9.25

    rows = load_data(path)
    analyze_dedup(rows, hours)
