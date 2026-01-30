#!/usr/bin/env python3
"""
Trade Size Profitability Estimator

Purpose:
    Re-runs cross-DEX spread analysis at multiple trade sizes ($100-$5000)
    using logged price data. Estimates are midmarket (no slippage adjustment).

Author: AI-Generated
Created: 2026-01-30
Modified: 2026-01-30

Usage:
    python3 scripts/analyze_trade_sizes.py
"""

import csv
import sys
from collections import defaultdict

TRADE_SIZES = [100, 500, 1000, 2000, 5000]
GAS_COST_USD = 0.01

def load_data(path):
    rows = []
    with open(path, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            row['price'] = float(row['price'])
            row['fee'] = int(row['fee'])
            row['block'] = int(row['block'])
            rows.append(row)
    return rows

def analyze(rows):
    # Group by (pair, block) → {(dex, fee): price}
    block_prices = defaultdict(dict)
    for r in rows:
        bk = (r['pair'], r['block'])
        dk = (r['dex'], r['fee'])
        block_prices[bk][dk] = r['price']

    # Collect raw spreads per combo: combo → [spread_pct, ...]
    combo_data = defaultdict(lambda: {'spreads': [], 'rt_fee': 0.0})

    for (pair, block), dex_prices in block_prices.items():
        if len(dex_prices) < 2:
            continue
        items = list(dex_prices.items())
        for i in range(len(items)):
            for j in range(i + 1, len(items)):
                (dex_a, fee_a), price_a = items[i]
                (dex_b, fee_b), price_b = items[j]
                if price_a <= 0 or price_b <= 0:
                    continue

                if price_a > price_b:
                    buy_dex, buy_fee, buy_p = dex_a, fee_a, price_a
                    sell_dex, sell_fee, sell_p = dex_b, fee_b, price_b
                else:
                    buy_dex, buy_fee, buy_p = dex_b, fee_b, price_b
                    sell_dex, sell_fee, sell_p = dex_a, fee_a, price_a

                midmarket_spread = (buy_p - sell_p) / sell_p
                rt_fee = (buy_fee + sell_fee) / 1_000_000

                combo_key = (pair, f"{buy_dex}({buy_fee})", f"{sell_dex}({sell_fee})")
                combo_data[combo_key]['spreads'].append(midmarket_spread)
                combo_data[combo_key]['rt_fee'] = rt_fee

    # For each trade size, compute profitability across all combos
    print("=" * 100)
    print("TRADE SIZE PROFITABILITY ESTIMATES (midmarket, no slippage)")
    print(f"Gas: ${GAS_COST_USD:.2f} per trade")
    print("=" * 100)

    # Aggregate: for each trade size, show top combos
    for size in TRADE_SIZES:
        print(f"\n{'─' * 100}")
        print(f"  TRADE SIZE: ${size:,}")
        print(f"{'─' * 100}")
        print(f"  {'Pair':<14} {'Buy':<24} {'Sell':<24} {'RT%':>6} {'Blocks':>7} {'Prof#':>6} {'%':>6} {'AvgNet$':>8} {'MaxNet$':>8} {'Σ Net$':>9}")
        print(f"  {'-'*94}")

        results = []
        for combo_key, data in combo_data.items():
            pair, buy_label, sell_label = combo_key
            rt_fee = data['rt_fee']
            spreads = data['spreads']

            nets = [(s - rt_fee) * size - GAS_COST_USD for s in spreads]
            profitable = [n for n in nets if n > 0]
            prof_count = len(profitable)
            total = len(spreads)
            sum_net = sum(profitable)
            avg_net = sum_net / prof_count if prof_count > 0 else 0
            max_net = max(nets) if nets else 0

            if prof_count > 0:
                results.append((
                    pair, buy_label, sell_label,
                    rt_fee * 100, total, prof_count,
                    prof_count / total * 100,
                    avg_net, max_net, sum_net
                ))

        # Sort by sum_net desc (total extractable value)
        results.sort(key=lambda x: -x[9])

        for r in results[:15]:
            pair, buy, sell, rt, total, prof, pct, avg, mx, sm = r
            print(f"  {pair:<14} {buy:<24} {sell:<24} {rt:>5.3f}% {total:>7} {prof:>6} {pct:>5.1f}% ${avg:>7.2f} ${mx:>7.2f} ${sm:>8.2f}")

        if not results:
            print("  No profitable combinations at this trade size.")

    # Summary table: total extractable value per trade size
    print(f"\n{'=' * 100}")
    print("SUMMARY: TOTAL ESTIMATED VALUE OVER 9h 15m OBSERVATION PERIOD")
    print(f"{'=' * 100}")
    print(f"  {'Size':>8} {'Total Prof Blocks':>18} {'Total Net $':>12} {'$/hour':>10} {'$/day':>10} {'$/month':>12} {'Note'}")
    print(f"  {'-'*90}")

    hours = 9.25  # approx observation duration

    for size in TRADE_SIZES:
        total_prof = 0
        total_net = 0.0
        for combo_key, data in combo_data.items():
            rt_fee = data['rt_fee']
            for s in data['spreads']:
                net = (s - rt_fee) * size - GAS_COST_USD
                if net > 0:
                    total_prof += 1
                    total_net += net

        per_hour = total_net / hours if hours > 0 else 0
        per_day = per_hour * 24
        per_month = per_day * 30

        # Slippage warning
        if size >= 2000:
            note = "⚠ slippage likely significant"
        elif size >= 1000:
            note = "⚠ moderate slippage expected"
        else:
            note = ""

        print(f"  ${size:>7,} {total_prof:>18,} ${total_net:>11.2f} ${per_hour:>9.2f} ${per_day:>9.2f} ${per_month:>11.2f}  {note}")

    # Best combo at each size
    print(f"\n{'=' * 100}")
    print("BEST SINGLE COMBO AT EACH TRADE SIZE (by total extractable $)")
    print(f"{'=' * 100}")
    for size in TRADE_SIZES:
        best_key = None
        best_sum = 0
        for combo_key, data in combo_data.items():
            rt_fee = data['rt_fee']
            s_net = sum(max(0, (s - rt_fee) * size - GAS_COST_USD) for s in data['spreads'])
            if s_net > best_sum:
                best_sum = s_net
                best_key = combo_key

        if best_key:
            pair, buy, sell = best_key
            per_h = best_sum / hours
            print(f"  ${size:>5,}: {pair} {buy} → {sell} | ${best_sum:.2f} total (${per_h:.2f}/hr)")
        else:
            print(f"  ${size:>5,}: No profitable combo")

    print(f"\n⚠ IMPORTANT: These are MIDMARKET estimates. Real execution will have:")
    print(f"  - Slippage (increases with trade size, especially > $1000)")
    print(f"  - MEV/frontrunning risk (increases with trade size)")
    print(f"  - Quoter rejection (the bot's Multicall3 pre-screen already filters these)")
    print(f"  - Gas spikes (1558 gwei spike was observed today, blocking execution)")

if __name__ == '__main__':
    path = sys.argv[1] if len(sys.argv) > 1 else '/home/botuser/bots/dexarb/data/price_history/prices_20260130.csv'
    rows = load_data(path)
    analyze(rows)
