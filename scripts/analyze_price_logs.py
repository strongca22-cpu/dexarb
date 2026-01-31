#!/usr/bin/env python3
"""
Price Log Analyzer — Live Bot Run

Purpose:
    Analyze price history CSVs from the current live bot run.
    Computes per-pair cross-DEX spread statistics, volatility,
    opportunity frequency, and time-of-day patterns.

Author: AI-Generated
Created: 2026-01-31
Modified: 2026-01-31

Usage:
    python3 scripts/analyze_price_logs.py              # auto-detect current run
    python3 scripts/analyze_price_logs.py --since 2026-01-31T00:23:00

Dependencies:
    - Python 3.8+ (stdlib only — no pandas/numpy required)

Data Sources:
    - data/price_history/prices_YYYYMMDD.csv
    - data/logs/livebot_*.log  (to detect current run start time)
"""

import csv
import os
import sys
import glob
import math
import argparse
from datetime import datetime, timezone
from collections import defaultdict

# ── Constants ────────────────────────────────────────────────────────────────

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
BOT_DIR = os.path.dirname(SCRIPT_DIR)
PRICE_DIR = os.path.join(BOT_DIR, "data", "price_history")
LOG_DIR = os.path.join(BOT_DIR, "data", "logs")

# Pairs where price is inverted in CSV (token0/token1 → show as USD per token)
# WETH/USDC ~0.000375, WMATIC/USDC ~0.112 — invert for readability
# WBTC/USDC is already stored as ~83000 (USD per BTC) — do NOT invert
INVERT_PAIRS = {"WETH/USDC", "WMATIC/USDC"}

# ── Helpers ──────────────────────────────────────────────────────────────────

def parse_ts(ts_str):
    """Parse ISO timestamp string to datetime (UTC)."""
    ts_str = ts_str.rstrip("Z").replace("Z", "")
    for fmt in ("%Y-%m-%dT%H:%M:%S.%f", "%Y-%m-%dT%H:%M:%S"):
        try:
            return datetime.strptime(ts_str, fmt).replace(tzinfo=timezone.utc)
        except ValueError:
            continue
    return None


def detect_run_start():
    """Find the newest livebot log and extract its start timestamp from filename."""
    logs = sorted(glob.glob(os.path.join(LOG_DIR, "livebot_*.log")))
    if not logs:
        return None
    newest = os.path.basename(logs[-1])  # e.g. livebot_20260131_002323.log
    parts = newest.replace("livebot_", "").replace(".log", "").split("_")
    if len(parts) >= 2:
        try:
            dt = datetime.strptime(f"{parts[0]}_{parts[1]}", "%Y%m%d_%H%M%S")
            return dt.replace(tzinfo=timezone.utc)
        except ValueError:
            pass
    return None


def mean(vals):
    return sum(vals) / len(vals) if vals else 0.0

def stdev(vals):
    if len(vals) < 2:
        return 0.0
    m = mean(vals)
    return math.sqrt(sum((v - m) ** 2 for v in vals) / (len(vals) - 1))

def percentile(vals, p):
    """Simple percentile (nearest rank)."""
    if not vals:
        return 0.0
    s = sorted(vals)
    k = int(len(s) * p / 100)
    k = min(k, len(s) - 1)
    return s[k]

def fmt_price(pair, price):
    """Format price for display — invert small prices for readability."""
    if pair in INVERT_PAIRS and price > 0:
        inv = 1.0 / price
        if inv > 1000:
            return f"${inv:,.2f}"
        return f"${inv:,.4f}"
    if abs(price - 1.0) < 0.1:  # stablecoin pair
        return f"${price:.6f}"
    return f"{price:.10f}"

def fmt_usd(val):
    if abs(val) >= 1:
        return f"${val:,.2f}"
    return f"${val:.4f}"


# ── Data Loading ─────────────────────────────────────────────────────────────

def load_prices(since_dt):
    """Load all price CSV rows since the given datetime."""
    rows = []
    csv_files = sorted(glob.glob(os.path.join(PRICE_DIR, "prices_*.csv")))
    if not csv_files:
        print("ERROR: No price CSV files found in", PRICE_DIR)
        sys.exit(1)

    # Filter to files that might contain data after since_dt
    for fpath in csv_files:
        fname = os.path.basename(fpath)
        # Extract date from filename: prices_20260131.csv
        date_str = fname.replace("prices_", "").replace(".csv", "")
        try:
            file_date = datetime.strptime(date_str, "%Y%m%d").replace(tzinfo=timezone.utc)
        except ValueError:
            continue
        # Skip files from before the run date (could span midnight)
        if file_date.date() < (since_dt - __import__('datetime').timedelta(days=1)).date():
            continue

        with open(fpath, "r") as f:
            reader = csv.DictReader(f)
            for row in reader:
                ts = parse_ts(row["timestamp"])
                if ts is None or ts < since_dt:
                    continue
                try:
                    price = float(row["price"])
                except (ValueError, KeyError):
                    continue
                rows.append({
                    "ts": ts,
                    "block": int(row.get("block", 0)),
                    "pair": row["pair"],
                    "dex": row["dex"],
                    "fee": int(row.get("fee", 0)),
                    "price": price,
                    "liquidity": row.get("liquidity", "0"),
                    "address": row.get("address", ""),
                })
    return rows


# ── Analysis Functions ───────────────────────────────────────────────────────

def normalize_dex(dex, fee):
    """Normalize DEX name for grouping (QuickswapV3 has dynamic fees)."""
    return dex  # Keep as-is; we group by dex name


def compute_pair_stats(rows):
    """Per-pair price statistics."""
    pair_data = defaultdict(lambda: defaultdict(list))  # pair -> dex -> [prices]
    for r in rows:
        pair_data[r["pair"]][r["dex"]].append(r["price"])

    stats = {}
    for pair, dex_prices in sorted(pair_data.items()):
        pair_stats = {}
        for dex, prices in sorted(dex_prices.items()):
            pair_stats[dex] = {
                "count": len(prices),
                "mean": mean(prices),
                "std": stdev(prices),
                "min": min(prices),
                "max": max(prices),
                "p5": percentile(prices, 5),
                "p95": percentile(prices, 95),
            }
        stats[pair] = pair_stats
    return stats


def compute_spreads(rows):
    """
    Compute cross-DEX spreads per block per pair.
    For each block, find min and max price across DEXes → spread.
    """
    # Group: (pair, block) -> [(dex, price)]
    block_prices = defaultdict(list)
    for r in rows:
        block_prices[(r["pair"], r["block"])].append((r["dex"], r["price"]))

    # Per-pair spread stats
    pair_spreads = defaultdict(list)  # pair -> [spread_pct]
    pair_spread_details = defaultdict(list)  # pair -> [(spread_pct, buy_dex, sell_dex, block)]

    for (pair, block), dex_prices in block_prices.items():
        if len(dex_prices) < 2:
            continue
        # Find best buy (lowest price) and best sell (highest price)
        dex_prices_sorted = sorted(dex_prices, key=lambda x: x[1])
        buy_dex, buy_price = dex_prices_sorted[0]
        sell_dex, sell_price = dex_prices_sorted[-1]

        if buy_price <= 0:
            continue
        spread_pct = (sell_price - buy_price) / buy_price * 100
        pair_spreads[pair].append(spread_pct)
        pair_spread_details[pair].append((spread_pct, buy_dex, sell_dex, block))

    return pair_spreads, pair_spread_details


def compute_time_patterns(rows):
    """Analyze spread patterns by hour of day."""
    # Group by (pair, block, hour)
    block_prices = defaultdict(list)  # (pair, block) -> [(dex, price, hour)]
    block_hours = {}  # (pair, block) -> hour

    for r in rows:
        key = (r["pair"], r["block"])
        block_prices[key].append((r["dex"], r["price"]))
        block_hours[key] = r["ts"].hour

    # Compute spreads by hour
    hour_spreads = defaultdict(lambda: defaultdict(list))  # pair -> hour -> [spread]
    for (pair, block), dex_prices in block_prices.items():
        if len(dex_prices) < 2:
            continue
        prices = [p for _, p in dex_prices]
        spread_pct = (max(prices) - min(prices)) / min(prices) * 100 if min(prices) > 0 else 0
        hour = block_hours[(pair, block)]
        hour_spreads[pair][hour].append(spread_pct)

    return hour_spreads


def compute_volatility(rows, window_blocks=50):
    """Compute rolling volatility (stdev of returns) per pair per DEX."""
    # Group by (pair, dex) ordered by block
    pair_dex_series = defaultdict(list)  # (pair, dex) -> [(block, price)]
    for r in rows:
        pair_dex_series[(r["pair"], r["dex"])].append((r["block"], r["price"]))

    volatility = {}
    for (pair, dex), series in pair_dex_series.items():
        series.sort(key=lambda x: x[0])
        if len(series) < 10:
            continue
        # Compute block-to-block returns
        returns = []
        for i in range(1, len(series)):
            if series[i - 1][1] > 0:
                ret = (series[i][1] - series[i - 1][1]) / series[i - 1][1]
                returns.append(ret)
        if returns:
            vol = stdev(returns) * 100  # as percentage
            volatility[(pair, dex)] = {
                "vol_pct": vol,
                "n_samples": len(series),
                "n_returns": len(returns),
                "max_return": max(returns) * 100,
                "min_return": min(returns) * 100,
            }
    return volatility


def find_top_opportunities(pair_spread_details, top_n=15):
    """Find the widest spreads across all pairs."""
    all_opps = []
    for pair, details in pair_spread_details.items():
        for spread_pct, buy_dex, sell_dex, block in details:
            all_opps.append((spread_pct, pair, buy_dex, sell_dex, block))
    all_opps.sort(reverse=True)
    return all_opps[:top_n]


def compute_opportunity_frequency(pair_spreads, thresholds=[0.05, 0.10, 0.15, 0.20, 0.30]):
    """Count how often spreads exceed given thresholds."""
    freq = {}
    for pair, spreads in sorted(pair_spreads.items()):
        total = len(spreads)
        freq[pair] = {"total_blocks": total}
        for t in thresholds:
            count = sum(1 for s in spreads if s >= t)
            freq[pair][f">={t:.2f}%"] = count
            freq[pair][f">={t:.2f}%_pct"] = (count / total * 100) if total > 0 else 0
    return freq


# ── Output Formatting ────────────────────────────────────────────────────────

def print_header(title):
    width = 80
    print()
    print("=" * width)
    print(f"  {title}")
    print("=" * width)


def print_section(title):
    print(f"\n--- {title} {'─' * (74 - len(title))}")


def report(rows, since_dt):
    """Generate the full report."""
    if not rows:
        print("No data found for the current run.")
        return

    # Timespan
    ts_min = min(r["ts"] for r in rows)
    ts_max = max(r["ts"] for r in rows)
    block_min = min(r["block"] for r in rows)
    block_max = max(r["block"] for r in rows)
    unique_pairs = sorted(set(r["pair"] for r in rows))
    unique_dexes = sorted(set(r["dex"] for r in rows))
    unique_blocks = len(set(r["block"] for r in rows))

    print_header("PRICE LOG ANALYSIS — Current Live Bot Run")
    print(f"  Run start:    {since_dt.strftime('%Y-%m-%d %H:%M:%S UTC')}")
    print(f"  Data range:   {ts_min.strftime('%H:%M:%S')} — {ts_max.strftime('%H:%M:%S UTC')} ({(ts_max - ts_min).total_seconds()/3600:.1f} hours)")
    print(f"  Blocks:       {block_min:,} — {block_max:,} ({block_max - block_min:,} blocks)")
    print(f"  Unique blocks: {unique_blocks:,}")
    print(f"  Total rows:   {len(rows):,}")
    print(f"  Pairs:        {', '.join(unique_pairs)}")
    print(f"  DEXes:        {', '.join(unique_dexes)}")

    # ── 1. Per-Pair Price Stats ──
    pair_stats = compute_pair_stats(rows)
    print_header("1. PRICE STATISTICS BY PAIR & DEX")

    for pair in unique_pairs:
        if pair not in pair_stats:
            continue
        print_section(pair)
        dex_stats = pair_stats[pair]

        # Determine display format
        is_inverted = pair in INVERT_PAIRS
        label = "USD price" if is_inverted else "Price"

        print(f"  {'DEX':<28} {'Samples':>8}  {'Mean':>14}  {'Std':>14}  {'Min':>14}  {'Max':>14}")
        for dex, st in sorted(dex_stats.items(), key=lambda x: -x[1]["count"]):
            if is_inverted:
                m = 1/st["mean"] if st["mean"] > 0 else 0
                s = st["std"] / (st["mean"]**2) if st["mean"] > 0 else 0  # delta method approx
                mn = 1/st["max"] if st["max"] > 0 else 0  # inverted: max raw → min USD
                mx = 1/st["min"] if st["min"] > 0 else 0
                if m > 1000:
                    print(f"  {dex:<28} {st['count']:>8,}  ${m:>13,.2f}  ${s:>13,.2f}  ${mn:>13,.2f}  ${mx:>13,.2f}")
                else:
                    print(f"  {dex:<28} {st['count']:>8,}  ${m:>13,.4f}  ${s:>13,.4f}  ${mn:>13,.4f}  ${mx:>13,.4f}")
            elif st["mean"] > 1000:
                # Already in high-value USD terms (e.g. WBTC/USDC)
                print(f"  {dex:<28} {st['count']:>8,}  ${st['mean']:>13,.2f}  ${st['std']:>13,.2f}  ${st['min']:>13,.2f}  ${st['max']:>13,.2f}")
            elif abs(st["mean"] - 1.0) < 0.5:
                # Stablecoin pair
                print(f"  {dex:<28} {st['count']:>8,}  {st['mean']:>14.8f}  {st['std']:>14.8f}  {st['min']:>14.8f}  {st['max']:>14.8f}")
            else:
                print(f"  {dex:<28} {st['count']:>8,}  {st['mean']:>14.8f}  {st['std']:>14.8f}  {st['min']:>14.8f}  {st['max']:>14.8f}")

    # ── 2. Cross-DEX Spread Analysis ──
    pair_spreads, pair_spread_details = compute_spreads(rows)
    print_header("2. CROSS-DEX SPREAD ANALYSIS")
    print("  (Spread = best sell - best buy across all DEXes per block)")

    for pair in unique_pairs:
        if pair not in pair_spreads:
            continue
        spreads = pair_spreads[pair]
        if not spreads:
            continue

        print_section(f"{pair}  ({len(spreads):,} block samples)")
        print(f"    Mean spread:    {mean(spreads):.4f}%")
        print(f"    Median spread:  {percentile(spreads, 50):.4f}%")
        print(f"    Std dev:        {stdev(spreads):.4f}%")
        print(f"    Min spread:     {min(spreads):.4f}%")
        print(f"    Max spread:     {max(spreads):.4f}%")
        print(f"    P5 / P95:       {percentile(spreads, 5):.4f}% / {percentile(spreads, 95):.4f}%")
        print(f"    P99:            {percentile(spreads, 99):.4f}%")

        # Show most common buy/sell DEX pairs
        route_counts = defaultdict(int)
        for spread_pct, buy_dex, sell_dex, _ in pair_spread_details[pair]:
            if spread_pct > 0.05:  # Only count meaningful spreads
                route_counts[(buy_dex, sell_dex)] += 1
        if route_counts:
            print(f"\n    Top routes (spread > 0.05%):")
            for (bd, sd), cnt in sorted(route_counts.items(), key=lambda x: -x[1])[:5]:
                print(f"      Buy {bd:<24} → Sell {sd:<24}  ({cnt:,}x)")

    # ── 3. Opportunity Frequency ──
    print_header("3. OPPORTUNITY FREQUENCY")
    thresholds = [0.05, 0.10, 0.15, 0.20, 0.30, 0.50]
    freq = compute_opportunity_frequency(pair_spreads, thresholds)

    # Header row
    hdr = f"  {'Pair':<16} {'Blocks':>8}"
    for t in thresholds:
        hdr += f"  {'>=' + f'{t:.2f}%':>10}"
    print(hdr)
    print("  " + "-" * (len(hdr) - 2))

    for pair, data in sorted(freq.items()):
        row = f"  {pair:<16} {data['total_blocks']:>8,}"
        for t in thresholds:
            key = f">={t:.2f}%"
            cnt = data[key]
            pct = data[f"{key}_pct"]
            row += f"  {cnt:>5,} ({pct:>4.1f}%)"  # fixed alignment
        print(row)

    # ── 4. Volatility ──
    volatility = compute_volatility(rows)
    print_header("4. BLOCK-TO-BLOCK VOLATILITY")
    print("  (Stdev of block-to-block returns, as %)")
    print(f"\n  {'Pair':<16} {'DEX':<28} {'Vol%':>8} {'MaxRet%':>9} {'MinRet%':>9} {'Samples':>8}")
    print("  " + "-" * 80)

    # Sort by volatility descending
    for (pair, dex), vdata in sorted(volatility.items(), key=lambda x: -x[1]["vol_pct"]):
        print(f"  {pair:<16} {dex:<28} {vdata['vol_pct']:>8.4f} {vdata['max_return']:>+9.4f} {vdata['min_return']:>+9.4f} {vdata['n_samples']:>8,}")

    # ── 5. Hourly Spread Patterns ──
    hour_spreads = compute_time_patterns(rows)
    hours_present = sorted(set(h for pd in hour_spreads.values() for h in pd))
    if len(hours_present) > 1:
        print_header("5. SPREAD BY HOUR (UTC)")
        print(f"\n  {'Pair':<16}", end="")
        for h in hours_present:
            print(f"  {h:02d}:00", end="")
        print("   (mean spread %)")
        print("  " + "-" * (16 + 8 * len(hours_present)))

        for pair in unique_pairs:
            if pair not in hour_spreads:
                continue
            print(f"  {pair:<16}", end="")
            for h in hours_present:
                spreads = hour_spreads[pair].get(h, [])
                if spreads:
                    print(f"  {mean(spreads):>5.3f}", end="")
                else:
                    print(f"  {'—':>5}", end="")
            print()

    # ── 6. Top Observed Spreads ──
    top_opps = find_top_opportunities(pair_spread_details, top_n=20)
    print_header("6. TOP 20 WIDEST SPREADS OBSERVED")
    print(f"\n  {'#':>3}  {'Spread':>8}  {'Pair':<16} {'Buy DEX':<26} {'Sell DEX':<26} {'Block':>10}")
    print("  " + "-" * 94)
    for i, (spread_pct, pair, buy_dex, sell_dex, block) in enumerate(top_opps, 1):
        print(f"  {i:>3}  {spread_pct:>7.4f}%  {pair:<16} {buy_dex:<26} {sell_dex:<26} {block:>10,}")

    # ── 7. Summary / Key Takeaways ──
    print_header("7. KEY TAKEAWAYS")

    # Most active pair
    if pair_spreads:
        best_pair = max(pair_spreads.keys(), key=lambda p: mean(pair_spreads[p]))
        best_mean = mean(pair_spreads[best_pair])
        best_p95 = percentile(pair_spreads[best_pair], 95)
        print(f"\n  Widest avg spread:    {best_pair} at {best_mean:.4f}% mean, {best_p95:.4f}% P95")

        # Pair with most >0.10% occurrences
        most_opps_pair = max(pair_spreads.keys(),
                            key=lambda p: sum(1 for s in pair_spreads[p] if s >= 0.10))
        n_opps = sum(1 for s in pair_spreads[most_opps_pair] if s >= 0.10)
        total = len(pair_spreads[most_opps_pair])
        print(f"  Most >0.10% spreads:  {most_opps_pair} ({n_opps:,} / {total:,} blocks = {n_opps/total*100:.1f}%)")

    total_blocks = unique_blocks
    blocks_per_hour = total_blocks / max((ts_max - ts_min).total_seconds() / 3600, 0.01)
    print(f"  Scanning rate:        ~{blocks_per_hour:,.0f} blocks/hour ({total_blocks:,} blocks in {(ts_max - ts_min).total_seconds()/3600:.1f}h)")

    # Estimated gas-only cost of failed attempts at current rate
    print(f"\n  Trade attempts seen in logs: Check livebot log for TRY # count")
    print(f"  All reverts caught by eth_estimateGas (free) — $0 cost per failed attempt")
    print()


# ── Main ─────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Analyze price logs from live bot run")
    parser.add_argument("--since", type=str, default=None,
                        help="Start time (ISO format, e.g. 2026-01-31T00:23:00)")
    parser.add_argument("--all", action="store_true",
                        help="Use all available data (ignore run start detection)")
    args = parser.parse_args()

    if args.since:
        since_dt = parse_ts(args.since)
        if since_dt is None:
            print(f"ERROR: Could not parse --since timestamp: {args.since}")
            sys.exit(1)
    elif args.all:
        since_dt = datetime(2020, 1, 1, tzinfo=timezone.utc)
    else:
        since_dt = detect_run_start()
        if since_dt is None:
            print("WARNING: Could not detect run start from log filenames.")
            print("         Using all data from today. Use --since to override.")
            today = datetime.now(timezone.utc).replace(hour=0, minute=0, second=0, microsecond=0)
            since_dt = today

    print(f"Loading price data since {since_dt.strftime('%Y-%m-%d %H:%M:%S UTC')}...")
    rows = load_prices(since_dt)
    print(f"Loaded {len(rows):,} price records.")

    report(rows, since_dt)


if __name__ == "__main__":
    main()
