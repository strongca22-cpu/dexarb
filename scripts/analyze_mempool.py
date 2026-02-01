#!/usr/bin/env python3
"""
A4 Mempool Observation Analyzer

Purpose:
    Analyze A4 Phase 1 mempool observation data: pending swap CSVs and
    confirmation/lead-time data from livebot logs. Answers the A4 decision
    gate questions:
      - What % of V3 swaps does Alchemy show us pending before confirmation?
      - What is the lead time distribution (pending seen → block confirmed)?
      - Which routers / functions / token pairs have the most volume?
      - How does visibility vary by hour of day?

    Companion to the existing analysis scripts in /scripts:
      - analyze_bot_session.py — execution funnel, latency, spread analysis
      - analyze_price_logs.py  — cross-DEX price spreads, volatility, frequency
    This script focuses on A4 mempool observation data specifically.

Author: AI-Generated
Created: 2026-02-01
Modified: 2026-02-01

Dependencies:
    - Python 3.8+ (stdlib only — no pandas/numpy required)

Data Sources:
    - data/{chain}/mempool/pending_swaps_YYYYMMDD.csv (written by mempool monitor)
    - data/{chain}/logs/livebot_*.log (CONFIRMED and MEMPOOL STATS lines)

Usage:
    # Polygon (default)
    python3 scripts/analyze_mempool.py

    # Base
    python3 scripts/analyze_mempool.py --chain base

    # Specific date
    python3 scripts/analyze_mempool.py --date 20260201

    # Specific log file
    python3 scripts/analyze_mempool.py --log data/polygon/logs/livebot_ws.log

Notes:
    - CSV and log data are cross-referenced by tx_hash
    - Lead time comes from log CONFIRMED lines (not CSV)
    - CSV provides decoded calldata details (function, tokens, amounts, gas)
    - Decision gate thresholds: >30% visibility, >500ms median lead time
"""

import argparse
import csv
import glob
import math
import os
import re
import sys
from collections import defaultdict
from datetime import datetime, timezone

# ── Constants ────────────────────────────────────────────────────────────────

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
BOT_DIR = os.path.dirname(SCRIPT_DIR)

# Known token address → symbol mapping (Polygon + Base)
# Add new tokens as they appear in observation data.
TOKEN_SYMBOLS = {
    # Polygon — core trading pairs
    "0x2791bca1f2de4661ed88a30c99a7a9449aa84174": "USDC.e",
    "0x3c499c542cef5e3811e1192ce70d8cc03d5c3359": "USDC",
    "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619": "WETH",
    "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270": "WMATIC",
    "0x1bfd67037b42cf73acf2047067bd4f2c47d9bfd6": "WBTC",
    "0xc2132d05d31c914a87c6611c10748aeb04b58e8f": "USDT",
    "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063": "DAI",
    "0x53e0bca35ec356bd5dddfebbd1fc0fd03fabad39": "LINK",
    "0xb33eaad8d922b1083446dc23f610c2567fb5180f": "UNI",
    # Polygon — commonly observed in mempool swaps (addresses from live CSV data)
    "0xd6df932a45c0f255f85145f286ea0b292b21c90b": "AAVE",
    "0x172370d5cd63279efa6d502dab29171933a610af": "CRV",
    "0x440017a1b021006d556d7fc06a54c32e42eb745b": "IMX",
    "0x1cca311b786dd7906c07414095fa719eabfd070f": "1INCH",
    "0x13646e0e2d768d31b75d1a1e375e3e17f18567f2": "INST",
    "0xbbba073c31bf03b8acf7c28ef0738decf3695683": "SAND",
    # Observed but not yet identified — update as they appear
    "0xeb51d9a39ad5eef215dc0bf39a8821ff804a0f01": "0xeb51",
    "0x49ddee75d588b79a3eb1225dd386644eeeeeaf08": "0x49dd",
    "0x311434160d7537be358930def317afb606c0d737": "0x3114",
    "0x658cda444ac43b0a7da13d638700931319b64014": "0x658c",
    "0x9c6605eeb66bd05858e0cb5204432aa6c7d0fa24": "0x9c66",
    "0xd2e57e7019a8faea8b3e4a3738ee5b269975008a": "0xd2e5",
    "0xac0f66379a6d7801d7726d5a943356a172549adb": "0xac0f",
    "0xcc44674022a792794d219847362bb95c661937a9": "0xcc44",
    # Base tokens
    "0x4200000000000000000000000000000000000006": "WETH",
    "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913": "USDC",
}

# ANSI escape stripping
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")

# Log line patterns
TS_RE = re.compile(r"^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z)")
CONFIRMED_RE = re.compile(
    r"CONFIRMED: (0x[0-9a-fA-F]+) \| (\S+) \| lead_time=(\d+)ms \| block=(\d+)"
)
PENDING_RE = re.compile(
    r"PENDING: (0x[0-9a-fA-F]+) \| (\S+) \| (\S+) \|"
)
PENDING_UNDECODED_RE = re.compile(
    r"PENDING \(undecoded\): (0x[0-9a-fA-F]+) \| (\S+) \| selector=(\S+)"
)
STATS_RE = re.compile(
    r"MEMPOOL STATS \| decoded=(\d+) undecoded=(\d+) \| "
    r"confirmed=(\d+)/(\d+) \(([\d.]+)%\) \| "
    r"median_lead=(\d+)ms mean_lead=(\d+)ms \| "
    r"tracking=(\d+) \| blocks_checked=(\d+)"
)


# ── Helpers ──────────────────────────────────────────────────────────────────

def strip_ansi(line):
    return ANSI_RE.sub("", line)


def parse_ts(ts_str):
    """Parse ISO timestamp string to datetime (UTC)."""
    ts_str = ts_str.rstrip("Z").replace("Z", "")
    for fmt in ("%Y-%m-%dT%H:%M:%S.%f", "%Y-%m-%dT%H:%M:%S"):
        try:
            return datetime.strptime(ts_str, fmt).replace(tzinfo=timezone.utc)
        except ValueError:
            continue
    return None


def token_symbol(addr):
    """Look up token symbol from address, or return truncated address."""
    if not addr:
        return "?"
    addr_lower = addr.lower().strip()
    return TOKEN_SYMBOLS.get(addr_lower, addr_lower[:10])


def mean(vals):
    return sum(vals) / len(vals) if vals else 0.0


def median(vals):
    if not vals:
        return 0.0
    s = sorted(vals)
    n = len(s)
    if n % 2 == 1:
        return s[n // 2]
    return (s[n // 2 - 1] + s[n // 2]) / 2


def percentile(vals, p):
    if not vals:
        return 0.0
    s = sorted(vals)
    k = int(len(s) * p / 100)
    k = min(k, len(s) - 1)
    return s[k]


def stdev(vals):
    if len(vals) < 2:
        return 0.0
    m = mean(vals)
    return math.sqrt(sum((v - m) ** 2 for v in vals) / (len(vals) - 1))


# ── Data Loading ─────────────────────────────────────────────────────────────

def load_csv_data(mempool_dir, date_filter=None):
    """Load pending swap CSV data from the mempool directory."""
    rows = []
    pattern = os.path.join(mempool_dir, "pending_swaps_*.csv")
    csv_files = sorted(glob.glob(pattern))

    if not csv_files:
        return rows

    for fpath in csv_files:
        fname = os.path.basename(fpath)
        file_date = fname.replace("pending_swaps_", "").replace(".csv", "")
        if date_filter and file_date != date_filter:
            continue

        with open(fpath, "r") as f:
            reader = csv.DictReader(f)
            for row in reader:
                ts = parse_ts(row.get("timestamp_utc", ""))
                rows.append({
                    "ts": ts,
                    "tx_hash": row.get("tx_hash", ""),
                    "router": row.get("router", ""),
                    "router_name": row.get("router_name", ""),
                    "function": row.get("function", ""),
                    "token_in": row.get("token_in", ""),
                    "token_out": row.get("token_out", ""),
                    "amount_in": row.get("amount_in", ""),
                    "amount_out_min": row.get("amount_out_min", ""),
                    "fee_tier": row.get("fee_tier", ""),
                    "gas_price_gwei": float(row.get("gas_price_gwei", 0) or 0),
                    "max_priority_fee_gwei": float(row.get("max_priority_fee_gwei", 0) or 0),
                })
    return rows


def parse_log_confirmations(log_path):
    """Parse CONFIRMED lines from the livebot log."""
    confirmations = []
    undecoded_count = 0
    decoded_count = 0
    stats_lines = []

    if not os.path.exists(log_path):
        return confirmations, undecoded_count, decoded_count, stats_lines

    with open(log_path, "r") as f:
        for raw_line in f:
            line = strip_ansi(raw_line.strip())

            # CONFIRMED
            m = CONFIRMED_RE.search(line)
            if m:
                ts_m = TS_RE.match(line)
                ts = parse_ts(ts_m.group(1)) if ts_m else None
                confirmations.append({
                    "ts": ts,
                    "tx_hash": m.group(1),
                    "router_name": m.group(2),
                    "lead_time_ms": int(m.group(3)),
                    "block": int(m.group(4)),
                })

            # PENDING counts (decoded)
            if PENDING_RE.search(line) and "undecoded" not in line:
                decoded_count += 1

            # PENDING undecoded
            if PENDING_UNDECODED_RE.search(line):
                undecoded_count += 1

            # MEMPOOL STATS
            m = STATS_RE.search(line)
            if m:
                ts_m = TS_RE.match(line)
                ts = parse_ts(ts_m.group(1)) if ts_m else None
                stats_lines.append({
                    "ts": ts,
                    "decoded": int(m.group(1)),
                    "undecoded": int(m.group(2)),
                    "confirmed": int(m.group(3)),
                    "total_seen": int(m.group(4)),
                    "conf_rate_pct": float(m.group(5)),
                    "median_lead_ms": int(m.group(6)),
                    "mean_lead_ms": int(m.group(7)),
                    "tracking": int(m.group(8)),
                    "blocks_checked": int(m.group(9)),
                })

    return confirmations, undecoded_count, decoded_count, stats_lines


# ── Analysis Functions ───────────────────────────────────────────────────────

def analyze_visibility(csv_rows, confirmations):
    """Cross-reference CSV pending swaps with log confirmations."""
    csv_hashes = set(r["tx_hash"] for r in csv_rows)
    conf_hashes = set(c["tx_hash"] for c in confirmations)

    seen_and_confirmed = csv_hashes & conf_hashes
    seen_not_confirmed = csv_hashes - conf_hashes
    confirmed_not_csv = conf_hashes - csv_hashes  # shouldn't happen

    return {
        "total_pending_seen": len(csv_hashes),
        "total_confirmed": len(conf_hashes),
        "seen_and_confirmed": len(seen_and_confirmed),
        "seen_not_confirmed": len(seen_not_confirmed),
        "confirmation_rate": len(seen_and_confirmed) / max(len(csv_hashes), 1) * 100,
    }


def analyze_lead_times(confirmations):
    """Analyze lead time distribution from confirmations."""
    times = [c["lead_time_ms"] for c in confirmations]
    if not times:
        return {}

    return {
        "count": len(times),
        "min_ms": min(times),
        "max_ms": max(times),
        "mean_ms": mean(times),
        "median_ms": median(times),
        "stdev_ms": stdev(times),
        "p10_ms": percentile(times, 10),
        "p25_ms": percentile(times, 25),
        "p75_ms": percentile(times, 75),
        "p90_ms": percentile(times, 90),
        "p99_ms": percentile(times, 99),
        "lt_500ms": sum(1 for t in times if t < 500),
        "lt_1000ms": sum(1 for t in times if t < 1000),
        "lt_2000ms": sum(1 for t in times if t < 2000),
        "gt_5000ms": sum(1 for t in times if t > 5000),
        "gt_10000ms": sum(1 for t in times if t > 10000),
    }


def analyze_by_router(csv_rows, confirmations):
    """Break down stats by router."""
    conf_by_hash = {c["tx_hash"]: c for c in confirmations}

    router_stats = defaultdict(lambda: {
        "pending": 0, "confirmed": 0, "lead_times": [],
        "functions": defaultdict(int),
    })

    for row in csv_rows:
        name = row["router_name"]
        router_stats[name]["pending"] += 1
        router_stats[name]["functions"][row["function"]] += 1

        conf = conf_by_hash.get(row["tx_hash"])
        if conf:
            router_stats[name]["confirmed"] += 1
            router_stats[name]["lead_times"].append(conf["lead_time_ms"])

    return dict(router_stats)


def analyze_token_pairs(csv_rows):
    """Analyze which token pairs are most frequently swapped."""
    pair_counts = defaultdict(int)
    pair_amounts = defaultdict(list)

    for row in csv_rows:
        sym_in = token_symbol(row["token_in"])
        sym_out = token_symbol(row["token_out"])
        pair = f"{sym_in} → {sym_out}"
        pair_counts[pair] += 1

        # Try to parse amount_in for volume estimation
        try:
            amt = int(row["amount_in"])
            pair_amounts[pair].append(amt)
        except (ValueError, TypeError):
            pass

    return pair_counts, pair_amounts


def analyze_gas_prices(csv_rows):
    """Analyze gas price distribution of pending swaps."""
    gas_prices = [r["gas_price_gwei"] for r in csv_rows if r["gas_price_gwei"] > 0]
    priority_fees = [r["max_priority_fee_gwei"] for r in csv_rows if r["max_priority_fee_gwei"] > 0]

    result = {}
    if gas_prices:
        result["gas_price"] = {
            "count": len(gas_prices),
            "mean": mean(gas_prices),
            "median": median(gas_prices),
            "min": min(gas_prices),
            "max": max(gas_prices),
            "p25": percentile(gas_prices, 25),
            "p75": percentile(gas_prices, 75),
        }
    if priority_fees:
        result["priority_fee"] = {
            "count": len(priority_fees),
            "mean": mean(priority_fees),
            "median": median(priority_fees),
            "min": min(priority_fees),
            "max": max(priority_fees),
        }
    return result


def analyze_hourly(csv_rows, confirmations):
    """Analyze pending swap volume and confirmation rate by hour."""
    conf_hashes = set(c["tx_hash"] for c in confirmations)
    conf_lead = {c["tx_hash"]: c["lead_time_ms"] for c in confirmations}

    hourly = defaultdict(lambda: {"pending": 0, "confirmed": 0, "lead_times": []})

    for row in csv_rows:
        if row["ts"] is None:
            continue
        h = row["ts"].hour
        hourly[h]["pending"] += 1
        if row["tx_hash"] in conf_hashes:
            hourly[h]["confirmed"] += 1
        lt = conf_lead.get(row["tx_hash"])
        if lt is not None:
            hourly[h]["lead_times"].append(lt)

    return dict(hourly)


def analyze_function_selectors(csv_rows, undecoded_log_count):
    """Analyze decoded function distribution + undecoded rate."""
    func_counts = defaultdict(int)
    for row in csv_rows:
        func_counts[row["function"]] += 1

    total_decoded = len(csv_rows)
    total = total_decoded + undecoded_log_count
    decode_rate = total_decoded / max(total, 1) * 100

    return func_counts, total_decoded, undecoded_log_count, decode_rate


# ── Report Output ────────────────────────────────────────────────────────────

def print_header(title):
    width = 76
    print()
    print("=" * width)
    print(f"  {title}")
    print("=" * width)


def print_section(title):
    print(f"\n--- {title} {'─' * max(70 - len(title), 4)}")


def print_report(chain, csv_rows, confirmations, undecoded_count,
                 decoded_count, stats_lines):
    """Generate the full A4 mempool observation report."""

    print_header(f"A4 MEMPOOL OBSERVATION REPORT — {chain.upper()}")

    # ── Overview ──
    if csv_rows:
        ts_min = min(r["ts"] for r in csv_rows if r["ts"])
        ts_max = max(r["ts"] for r in csv_rows if r["ts"])
        duration_hrs = (ts_max - ts_min).total_seconds() / 3600
        print(f"  Data range:       {ts_min.strftime('%Y-%m-%d %H:%M:%S')} → "
              f"{ts_max.strftime('%H:%M:%S UTC')}  ({duration_hrs:.1f} hours)")
    else:
        duration_hrs = 0
        print("  No CSV data found.")

    print(f"  Pending decoded:  {len(csv_rows):,}")
    print(f"  Pending undecod:  {undecoded_count:,}")
    total_pending = len(csv_rows) + undecoded_count
    print(f"  Total pending:    {total_pending:,}")
    print(f"  Confirmed:        {len(confirmations):,}")
    if duration_hrs > 0:
        print(f"  Rate:             {len(csv_rows) / duration_hrs:.1f} decoded/hr, "
              f"{len(confirmations) / duration_hrs:.1f} confirmed/hr")

    # ── 1. Decision Gate: Visibility + Lead Time ──
    print_header("1. A4 DECISION GATE")

    vis = analyze_visibility(csv_rows, confirmations)
    lt = analyze_lead_times(confirmations)

    print(f"\n  Pending seen:              {vis['total_pending_seen']:,}")
    print(f"  Of which confirmed:        {vis['seen_and_confirmed']:,}")
    print(f"  CONFIRMATION RATE:         {vis['confirmation_rate']:.1f}%", end="")
    if vis['confirmation_rate'] >= 30:
        print("  ✓ PASSES (threshold: >30%)")
    else:
        print("  ✗ BELOW threshold (need >30%)")

    if lt:
        print(f"\n  MEDIAN LEAD TIME:          {lt['median_ms']:,}ms", end="")
        if lt['median_ms'] >= 500:
            print("  ✓ PASSES (threshold: >500ms)")
        else:
            print("  ✗ BELOW threshold (need >500ms)")
        print(f"  Mean lead time:            {lt['mean_ms']:,.0f}ms")
        print(f"  Stdev:                     {lt['stdev_ms']:,.0f}ms")
        print(f"  P10 / P25 / P75 / P90:    {lt['p10_ms']:,} / {lt['p25_ms']:,} / "
              f"{lt['p75_ms']:,} / {lt['p90_ms']:,}ms")
        print(f"  Min / Max:                 {lt['min_ms']:,} / {lt['max_ms']:,}ms")

        # Lead time buckets
        print(f"\n  Lead time distribution:")
        print(f"    <500ms:     {lt['lt_500ms']:>5,}  ({lt['lt_500ms']/lt['count']*100:>5.1f}%)")
        print(f"    500-1000ms: {lt['lt_1000ms'] - lt['lt_500ms']:>5,}  "
              f"({(lt['lt_1000ms'] - lt['lt_500ms'])/lt['count']*100:>5.1f}%)")
        print(f"    1-2s:       {lt['lt_2000ms'] - lt['lt_1000ms']:>5,}  "
              f"({(lt['lt_2000ms'] - lt['lt_1000ms'])/lt['count']*100:>5.1f}%)")
        bucket_2_5 = lt['count'] - lt['lt_2000ms'] - lt['gt_5000ms']
        print(f"    2-5s:       {bucket_2_5:>5,}  ({bucket_2_5/lt['count']*100:>5.1f}%)")
        print(f"    5-10s:      {lt['gt_5000ms'] - lt['gt_10000ms']:>5,}  "
              f"({(lt['gt_5000ms'] - lt['gt_10000ms'])/lt['count']*100:>5.1f}%)")
        print(f"    >10s:       {lt['gt_10000ms']:>5,}  ({lt['gt_10000ms']/lt['count']*100:>5.1f}%)")

    else:
        print("\n  No confirmations recorded yet.")

    # ── 2. By Router ──
    router_stats = analyze_by_router(csv_rows, confirmations)
    if router_stats:
        print_header("2. BREAKDOWN BY ROUTER")
        print(f"\n  {'Router':<16} {'Pending':>8} {'Confirmed':>10} {'Conf%':>7} "
              f"{'Med Lead':>10} {'Mean Lead':>10}")
        print(f"  {'─'*16} {'─'*8} {'─'*10} {'─'*7} {'─'*10} {'─'*10}")
        for name, st in sorted(router_stats.items(), key=lambda x: -x[1]["pending"]):
            conf_rate = st["confirmed"] / max(st["pending"], 1) * 100
            med_lt = f"{median(st['lead_times']):,.0f}ms" if st["lead_times"] else "—"
            mean_lt = f"{mean(st['lead_times']):,.0f}ms" if st["lead_times"] else "—"
            print(f"  {name:<16} {st['pending']:>8,} {st['confirmed']:>10,} "
                  f"{conf_rate:>6.1f}% {med_lt:>10} {mean_lt:>10}")

        # Functions per router
        for name, st in sorted(router_stats.items(), key=lambda x: -x[1]["pending"]):
            print(f"\n  {name} functions:")
            for func, cnt in sorted(st["functions"].items(), key=lambda x: -x[1]):
                print(f"    {func:<35} {cnt:>6,}  "
                      f"({cnt/st['pending']*100:>5.1f}%)")

    # ── 3. Decoder Coverage ──
    func_counts, n_decoded, n_undecoded, decode_rate = analyze_function_selectors(
        csv_rows, undecoded_count
    )
    print_header("3. DECODER COVERAGE")
    print(f"\n  Decoded:           {n_decoded:,}")
    print(f"  Undecoded:         {n_undecoded:,}")
    print(f"  Decode rate:       {decode_rate:.1f}%")
    if func_counts:
        print(f"\n  {'Function':<38} {'Count':>7} {'%':>7}")
        print(f"  {'─'*38} {'─'*7} {'─'*7}")
        total_funcs = sum(func_counts.values())
        for func, cnt in sorted(func_counts.items(), key=lambda x: -x[1]):
            print(f"  {func:<38} {cnt:>7,} {cnt/total_funcs*100:>6.1f}%")

    # ── 4. Token Pairs ──
    pair_counts, pair_amounts = analyze_token_pairs(csv_rows)
    if pair_counts:
        print_header("4. TOKEN PAIRS (by swap count)")
        print(f"\n  {'Pair':<30} {'Count':>7} {'%':>7}")
        print(f"  {'─'*30} {'─'*7} {'─'*7}")
        total_swaps = sum(pair_counts.values())
        for pair, cnt in sorted(pair_counts.items(), key=lambda x: -x[1])[:20]:
            print(f"  {pair:<30} {cnt:>7,} {cnt/total_swaps*100:>6.1f}%")

    # ── 5. Gas Price Analysis ──
    gas = analyze_gas_prices(csv_rows)
    if gas:
        print_header("5. GAS PRICE ANALYSIS")
        if "gas_price" in gas:
            gp = gas["gas_price"]
            print(f"\n  Gas price (gwei):")
            print(f"    Mean:   {gp['mean']:>10.1f}")
            print(f"    Median: {gp['median']:>10.1f}")
            print(f"    P25:    {gp['p25']:>10.1f}")
            print(f"    P75:    {gp['p75']:>10.1f}")
            print(f"    Min:    {gp['min']:>10.1f}")
            print(f"    Max:    {gp['max']:>10.1f}")
        if "priority_fee" in gas:
            pf = gas["priority_fee"]
            print(f"\n  Priority fee (gwei):")
            print(f"    Mean:   {pf['mean']:>10.1f}")
            print(f"    Median: {pf['median']:>10.1f}")
            print(f"    Min:    {pf['min']:>10.1f}")
            print(f"    Max:    {pf['max']:>10.1f}")

    # ── 6. Hourly Pattern ──
    hourly = analyze_hourly(csv_rows, confirmations)
    if hourly:
        print_header("6. HOURLY PATTERN (UTC)")
        hours = sorted(hourly.keys())
        max_pending = max(h["pending"] for h in hourly.values()) if hourly else 1
        print(f"\n  {'Hour':>6} {'Pend':>7} {'Conf':>7} {'Rate':>7} "
              f"{'MedLead':>9} {'':>3} Distribution")
        print(f"  {'─'*6} {'─'*7} {'─'*7} {'─'*7} {'─'*9} {'─'*3} {'─'*30}")
        for h in hours:
            st = hourly[h]
            rate = st["confirmed"] / max(st["pending"], 1) * 100
            med_lt = f"{median(st['lead_times']):,.0f}ms" if st["lead_times"] else "—"
            bar_len = int(st["pending"] / max(max_pending, 1) * 25)
            bar = "█" * bar_len
            print(f"  {h:02d}:00 {st['pending']:>7,} {st['confirmed']:>7,} "
                  f"{rate:>6.1f}% {med_lt:>9} {'':>3} {bar}")

    # ── 7. In-Log Cumulative Stats ──
    if stats_lines:
        print_header("7. IN-LOG CUMULATIVE STATS (from MEMPOOL STATS lines)")
        latest = stats_lines[-1]
        print(f"\n  Latest stats snapshot ({latest['ts'].strftime('%H:%M:%S UTC') if latest['ts'] else '?'}):")
        print(f"    Decoded total:       {latest['decoded']:,}")
        print(f"    Undecoded total:     {latest['undecoded']:,}")
        print(f"    Confirmed:           {latest['confirmed']:,} / {latest['total_seen']:,}")
        print(f"    Confirmation rate:   {latest['conf_rate_pct']:.1f}%")
        print(f"    Median lead time:    {latest['median_lead_ms']:,}ms")
        print(f"    Mean lead time:      {latest['mean_lead_ms']:,}ms")
        print(f"    Currently tracking:  {latest['tracking']:,} pending tx hashes")
        print(f"    Blocks checked:      {latest['blocks_checked']:,}")

        if len(stats_lines) > 1:
            print(f"\n  Stats over time ({len(stats_lines)} snapshots):")
            rates = [s["conf_rate_pct"] for s in stats_lines]
            leads = [s["median_lead_ms"] for s in stats_lines if s["median_lead_ms"] > 0]
            print(f"    Conf rate range:     {min(rates):.1f}% — {max(rates):.1f}%")
            if leads:
                print(f"    Med lead range:      {min(leads):,}ms — {max(leads):,}ms")

    # ── Summary & Decision ──
    print_header("SUMMARY & A4 DECISION")

    conf_rate = vis['confirmation_rate']
    med_lead = lt.get('median_ms', 0) if lt else 0

    print(f"\n  Confirmation rate:   {conf_rate:.1f}%", end="")
    if conf_rate >= 30:
        print("  ✓ PASS")
    elif conf_rate >= 20:
        print("  ~ BORDERLINE (20-30%)")
    else:
        print("  ✗ FAIL (<20%)")

    print(f"  Median lead time:    {med_lead:,}ms", end="")
    if med_lead >= 500:
        print("  ✓ PASS")
    elif med_lead >= 200:
        print("  ~ BORDERLINE (200-500ms)")
    else:
        print("  ✗ FAIL (<200ms)")

    if conf_rate >= 30 and med_lead >= 500:
        print("\n  >>> DECISION: PROCEED to A4 Phase 2 (AMM simulation)")
        print("      Alchemy provides sufficient visibility + lead time.")
    elif conf_rate >= 20 and med_lead >= 200:
        print("\n  >>> DECISION: MARGINAL — continue observation, consider own Bor node")
    elif conf_rate < 5 and len(csv_rows) < 10:
        print("\n  >>> DECISION: INSUFFICIENT DATA — extend observation period")
    else:
        print("\n  >>> DECISION: CONSIDER own Bor node ($80-100/mo) or different chain")
        print("      Alchemy mempool visibility too partial for reliable backrunning.")

    print(f"\n  Observation data:    {len(csv_rows):,} decoded swaps, "
          f"{len(confirmations):,} confirmations")
    if duration_hrs > 0:
        print(f"  Collection period:   {duration_hrs:.1f} hours")
    if duration_hrs < 12:
        print(f"  ⚠ Consider extending to 24h+ for robust statistics")

    print()


# ── Main ─────────────────────────────────────────────────────────────────────

def find_newest_log(log_dir):
    """Find the newest livebot log file in the given directory."""
    patterns = [
        os.path.join(log_dir, "livebot_ws.log"),
        os.path.join(log_dir, "livebot_*.log"),
    ]
    candidates = []
    for p in patterns:
        candidates.extend(glob.glob(p))
    if not candidates:
        return None
    return max(candidates, key=os.path.getmtime)


def main():
    parser = argparse.ArgumentParser(
        description="Analyze A4 mempool observation data (pending swaps CSV + log confirmations)"
    )
    parser.add_argument(
        "--chain", default="polygon",
        help="Chain name: polygon, base (default: polygon)"
    )
    parser.add_argument(
        "--date", default=None,
        help="Date filter for CSV files (YYYYMMDD format, e.g. 20260201)"
    )
    parser.add_argument(
        "--log", default=None,
        help="Path to specific livebot log file (auto-detected if not set)"
    )
    args = parser.parse_args()

    chain = args.chain.lower()
    mempool_dir = os.path.join(BOT_DIR, "data", chain, "mempool")
    log_dir = os.path.join(BOT_DIR, "data", chain, "logs")

    # Find log file
    if args.log:
        log_path = args.log
    else:
        log_path = find_newest_log(log_dir)
        if log_path is None:
            print(f"No log files found in {log_dir}")
            log_path = ""

    print(f"Chain:       {chain}")
    print(f"Mempool dir: {mempool_dir}")
    print(f"Log file:    {log_path or '(none)'}")
    if args.date:
        print(f"Date filter: {args.date}")
    print()

    # Load data
    print("Loading CSV data...", flush=True)
    csv_rows = load_csv_data(mempool_dir, args.date)
    print(f"  {len(csv_rows):,} pending swap records loaded")

    print("Parsing log confirmations...", flush=True)
    confirmations, undecoded_count, decoded_count, stats_lines = \
        parse_log_confirmations(log_path) if log_path else ([], 0, 0, [])
    print(f"  {len(confirmations):,} confirmations, "
          f"{undecoded_count:,} undecoded log lines, "
          f"{len(stats_lines)} stats snapshots")

    # Report
    print_report(chain, csv_rows, confirmations, undecoded_count,
                 decoded_count, stats_lines)


if __name__ == "__main__":
    main()
