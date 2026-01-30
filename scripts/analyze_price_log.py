#!/usr/bin/env python3
"""
Price Log Analyzer

Purpose:
    Analyze logged price data for cross-DEX spread patterns,
    opportunity frequency, and profitability estimates.

Author: AI-Generated
Created: 2026-01-30
Modified: 2026-01-30

Usage:
    python3 scripts/analyze_price_log.py [csv_path]
"""

import csv
import sys
from collections import defaultdict
from datetime import datetime, timedelta

def load_data(path):
    """Load price CSV into list of dicts."""
    rows = []
    with open(path, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            row['price'] = float(row['price'])
            row['fee'] = int(row['fee'])
            row['tick'] = int(row['tick'])
            row['block'] = int(row['block'])
            rows.append(row)
    return rows

def analyze(rows):
    # ── Basic stats ──
    pairs = sorted(set(r['pair'] for r in rows))
    dexes = sorted(set(r['dex'] for r in rows))
    blocks = sorted(set(r['block'] for r in rows))
    t0 = rows[0]['timestamp']
    t1 = rows[-1]['timestamp']
    dt0 = datetime.fromisoformat(t0.replace('Z', '+00:00'))
    dt1 = datetime.fromisoformat(t1.replace('Z', '+00:00'))
    duration = dt1 - dt0

    print("=" * 72)
    print("PRICE LOG ANALYSIS")
    print("=" * 72)
    print(f"File rows:      {len(rows):,}")
    print(f"Time range:     {t0} → {t1}")
    print(f"Duration:       {duration}")
    print(f"Blocks:         {blocks[0]:,} → {blocks[-1]:,} ({blocks[-1]-blocks[0]:,} blocks)")
    print(f"Unique pairs:   {len(pairs)}: {', '.join(pairs)}")
    print(f"Unique DEXes:   {len(dexes)}: {', '.join(dexes)}")
    print()

    # ── Per-pool summary ──
    pool_stats = defaultdict(lambda: {'prices': [], 'fees': set(), 'count': 0})
    for r in rows:
        key = (r['pair'], r['dex'], r['fee'])
        pool_stats[key]['prices'].append(r['price'])
        pool_stats[key]['fees'].add(r['fee'])
        pool_stats[key]['count'] += 1

    print("-" * 72)
    print("PER-POOL STATS")
    print("-" * 72)
    print(f"{'Pair':<14} {'DEX':<20} {'Fee':>6} {'Count':>7} {'Avg Price':>14} {'Min':>14} {'Max':>14} {'Spread%':>9}")
    print("-" * 72)
    for key in sorted(pool_stats.keys()):
        pair, dex, fee = key
        ps = pool_stats[key]['prices']
        avg_p = sum(ps) / len(ps)
        min_p = min(ps)
        max_p = max(ps)
        spread = (max_p - min_p) / avg_p * 100 if avg_p > 0 else 0
        print(f"{pair:<14} {dex:<20} {fee:>6} {len(ps):>7} {avg_p:>14.8f} {min_p:>14.8f} {max_p:>14.8f} {spread:>8.4f}%")
    print()

    # ── Cross-DEX spread analysis (the key analysis) ──
    # Group by (pair, block) to compare same-block prices across DEXes
    block_prices = defaultdict(dict)  # (pair, block) -> {(dex, fee): price}
    for r in rows:
        bk = (r['pair'], r['block'])
        dk = (r['dex'], r['fee'])
        block_prices[bk][dk] = r['price']

    print("=" * 72)
    print("CROSS-DEX SPREAD ANALYSIS (same-block price comparisons)")
    print("=" * 72)

    # For each pair, find all pool-pair combinations and their spreads
    pair_combos = defaultdict(lambda: {
        'spreads': [], 'profitable_count': 0, 'total_count': 0,
        'max_spread': 0, 'max_net': 0
    })

    ESTIMATED_GAS_USD = 0.01
    TRADE_SIZE_USD = 140.0

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

                # Higher price = buy pool, lower = sell pool
                if price_a > price_b:
                    buy_dex, buy_fee, buy_p = dex_a, fee_a, price_a
                    sell_dex, sell_fee, sell_p = dex_b, fee_b, price_b
                else:
                    buy_dex, buy_fee, buy_p = dex_b, fee_b, price_b
                    sell_dex, sell_fee, sell_p = dex_a, fee_a, price_a

                midmarket_spread = (buy_p - sell_p) / sell_p
                round_trip_fee = (buy_fee + sell_fee) / 1_000_000  # fee is in ppm
                net_spread = midmarket_spread - round_trip_fee
                net_profit = net_spread * TRADE_SIZE_USD - ESTIMATED_GAS_USD

                combo_key = (pair, f"{buy_dex}({buy_fee})", f"{sell_dex}({sell_fee})")
                stats = pair_combos[combo_key]
                stats['spreads'].append(midmarket_spread * 100)
                stats['total_count'] += 1
                stats['round_trip_fee'] = round_trip_fee * 100
                if net_profit > 0:
                    stats['profitable_count'] += 1
                if midmarket_spread > stats['max_spread']:
                    stats['max_spread'] = midmarket_spread * 100
                if net_profit > stats['max_net']:
                    stats['max_net'] = net_profit

    # Sort by profitable_count desc, then max_net desc
    sorted_combos = sorted(
        pair_combos.items(),
        key=lambda x: (-x[1]['profitable_count'], -x[1]['max_net'])
    )

    print(f"\nTrade size: ${TRADE_SIZE_USD:.0f} | Gas est: ${ESTIMATED_GAS_USD:.2f}")
    print(f"{'Pair':<14} {'Buy Pool':<24} {'Sell Pool':<24} {'RT Fee%':>7} {'Blocks':>7} {'Prof#':>6} {'Prof%':>6} {'AvgSprd':>8} {'MaxSprd':>8} {'MaxNet$':>8}")
    print("-" * 130)

    for combo_key, stats in sorted_combos:
        pair, buy_label, sell_label = combo_key
        avg_spread = sum(stats['spreads']) / len(stats['spreads'])
        prof_pct = stats['profitable_count'] / stats['total_count'] * 100 if stats['total_count'] > 0 else 0
        print(f"{pair:<14} {buy_label:<24} {sell_label:<24} {stats.get('round_trip_fee', 0):>6.3f}% {stats['total_count']:>7} {stats['profitable_count']:>6} {prof_pct:>5.1f}% {avg_spread:>7.4f}% {stats['max_spread']:>7.4f}% ${stats['max_net']:>7.2f}")

    # ── Top profitable combinations summary ──
    profitable = [(k, v) for k, v in sorted_combos if v['profitable_count'] > 0]
    print(f"\n{'=' * 72}")
    print(f"PROFITABLE COMBINATIONS: {len(profitable)} / {len(sorted_combos)} total")
    print(f"{'=' * 72}")

    if profitable:
        for combo_key, stats in profitable[:20]:
            pair, buy_label, sell_label = combo_key
            avg_spread = sum(stats['spreads']) / len(stats['spreads'])
            prof_pct = stats['profitable_count'] / stats['total_count'] * 100
            print(f"  {pair} | {buy_label} → {sell_label}")
            print(f"    RT fee: {stats.get('round_trip_fee', 0):.3f}% | Profitable: {stats['profitable_count']}/{stats['total_count']} ({prof_pct:.1f}%) | Avg spread: {avg_spread:.4f}% | Max net: ${stats['max_net']:.2f}")
    else:
        print("  No profitable combinations found at current fee levels.")

    # ── Time-series: opportunities per hour ──
    print(f"\n{'=' * 72}")
    print("OPPORTUNITIES PER HOUR (net_profit > $0)")
    print(f"{'=' * 72}")

    hour_opps = defaultdict(int)
    hour_total = defaultdict(int)
    for (pair, block), dex_prices in block_prices.items():
        if len(dex_prices) < 2:
            continue
        # Find the timestamp for this block from the raw data
        ts = None
        for r in rows:
            if r['block'] == block and r['pair'] == pair:
                ts = r['timestamp']
                break
        if not ts:
            continue
        hour_key = ts[:13]  # YYYY-MM-DDTHH

        items = list(dex_prices.items())
        for i in range(len(items)):
            for j in range(i + 1, len(items)):
                (_, fee_a), price_a = items[i]
                (_, fee_b), price_b = items[j]
                if price_a <= 0 or price_b <= 0:
                    continue
                spread = abs(price_a - price_b) / min(price_a, price_b)
                rt_fee = (fee_a + fee_b) / 1_000_000
                net = (spread - rt_fee) * TRADE_SIZE_USD - ESTIMATED_GAS_USD
                hour_total[hour_key] += 1
                if net > 0:
                    hour_opps[hour_key] += 1

    for hour in sorted(hour_total.keys()):
        total = hour_total[hour]
        opps = hour_opps.get(hour, 0)
        bar = '█' * min(opps, 60)
        print(f"  {hour} | {opps:>4} / {total:>6} comparisons | {bar}")

    # ── QuickSwap V3 vs UniV3 specific analysis ──
    print(f"\n{'=' * 72}")
    print("QUICKSWAP V3 vs UNISWAP V3 — CROSS-DEX DETAIL")
    print(f"{'=' * 72}")

    qs_combos = [(k, v) for k, v in sorted_combos
                 if 'QuickswapV3' in k[1] or 'QuickswapV3' in k[2]]

    if qs_combos:
        for combo_key, stats in qs_combos:
            pair, buy_label, sell_label = combo_key
            avg_spread = sum(stats['spreads']) / len(stats['spreads'])
            p5 = sorted(stats['spreads'])[int(len(stats['spreads']) * 0.05)] if len(stats['spreads']) > 20 else min(stats['spreads'])
            p50 = sorted(stats['spreads'])[len(stats['spreads']) // 2]
            p95 = sorted(stats['spreads'])[int(len(stats['spreads']) * 0.95)] if len(stats['spreads']) > 20 else max(stats['spreads'])
            prof_pct = stats['profitable_count'] / stats['total_count'] * 100 if stats['total_count'] > 0 else 0
            print(f"\n  {pair}: {buy_label} → {sell_label}")
            print(f"    RT Fee: {stats.get('round_trip_fee', 0):.3f}% | Blocks: {stats['total_count']}")
            print(f"    Spread pctiles: P5={p5:.4f}% | P50={p50:.4f}% | P95={p95:.4f}% | Max={stats['max_spread']:.4f}%")
            print(f"    Profitable: {stats['profitable_count']} ({prof_pct:.1f}%) | Max net: ${stats['max_net']:.2f}")
    else:
        print("  No QuickSwap V3 cross-DEX data found.")

    print(f"\n{'=' * 72}")
    print("ANALYSIS COMPLETE")
    print(f"{'=' * 72}")


if __name__ == '__main__':
    path = sys.argv[1] if len(sys.argv) > 1 else '/home/botuser/bots/dexarb/data/price_history/prices_20260130.csv'
    rows = load_data(path)
    analyze(rows)
