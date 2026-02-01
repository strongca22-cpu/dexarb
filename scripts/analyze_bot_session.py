#!/usr/bin/env python3
"""
Bot Session Analyzer

Purpose:
    Parse livebot_ws.log and price_history CSVs to produce a comprehensive
    statistical report of bot performance.  Designed to be run from the
    command line; Claude Code reads the output rather than doing calculations
    inline.

Author: AI-Generated
Created: 2026-01-31
Modified: 2026-01-31

Usage:
    python3 scripts/analyze_bot_session.py [--log FILE] [--prices DIR]

Dependencies:
    - pandas, numpy (standard data-science stack)
"""

import argparse
import re
import sys
from collections import defaultdict
from datetime import datetime, timedelta
from pathlib import Path

import numpy as np
import pandas as pd

# ── ANSI-stripping regex ────────────────────────────────────────────────
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")

# ── Log line patterns ───────────────────────────────────────────────────
# Timestamp at start of every log line (after stripping ANSI)
TS_RE = re.compile(r"^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z)")

# Opportunity detected
OPP_RE = re.compile(
    r"(?:V3|V2) OPPORTUNITY: (\S+) \| Buy (\S+) \(([^)]+)\) @ ([\d.]+) "
    r"\| Sell (\S+) \(([^)]+)\) @ ([\d.]+) \| Spread ([\d.]+)% \| Net \$([\d.]+)"
)

# Execution attempt
TRY_RE = re.compile(
    r"TRY #(\d+): (\S+) - Buy (\S+) Sell (\S+) - \$([\d.]+)"
)

# Trade failure
FAIL_RE = re.compile(
    r"Trade failed: (\S+) \| Error: (.+)"
)

# Atomic execution type
ATOMIC_RE = re.compile(
    r"ATOMIC (V3↔V3|V2↔V3|V2↔V2) execution: (\S+) \| Buy (\S+) → Sell (\S+)"
)

# Route cooldown
COOLDOWN_RE = re.compile(
    r"(\d+) routes suppressed \(cooldown\), (\d+) remaining"
)

# Private mempool send
PRIVATE_SEND_RE = re.compile(r"Sending via private mempool")

# Tx submitted (on-chain)
TX_SUBMIT_RE = re.compile(r"Tx submitted: (0x[0-9a-fA-F]+)")

# Atomic profit/loss
PROFIT_RE = re.compile(r"ATOMIC PROFIT")
LOSS_RE = re.compile(r"ATOMIC LOSS")
TRADE_COMPLETE_RE = re.compile(r"Trade complete")
HALT_RE = re.compile(r"HALT")


def strip_ansi(line: str) -> str:
    return ANSI_RE.sub("", line)


def parse_ts(line: str):
    """Extract datetime from a log line.  Returns None on failure."""
    m = TS_RE.match(line)
    if m:
        ts_str = m.group(1)
        # Handle variable-length fractional seconds
        try:
            return datetime.fromisoformat(ts_str.replace("Z", "+00:00"))
        except Exception:
            return None
    return None


def parse_log(log_path: str) -> dict:
    """Parse the entire log file into structured data."""

    opportunities = []
    attempts = []
    failures = []
    atomic_types = []
    cooldowns = []
    private_sends = []
    tx_submissions = []
    profits = []
    losses = []
    trade_completes = []
    halts = []

    with open(log_path, "r") as f:
        for raw_line in f:
            line = strip_ansi(raw_line.strip())
            ts = parse_ts(line)

            # Opportunity
            m = OPP_RE.search(line)
            if m:
                opportunities.append({
                    "ts": ts,
                    "pair": m.group(1),
                    "buy_dex": m.group(2),
                    "buy_fee": m.group(3),
                    "buy_price": float(m.group(4)),
                    "sell_dex": m.group(5),
                    "sell_fee": m.group(6),
                    "sell_price": float(m.group(7)),
                    "spread_pct": float(m.group(8)),
                    "net_usd": float(m.group(9)),
                })

            # TRY attempt
            m = TRY_RE.search(line)
            if m:
                attempts.append({
                    "ts": ts,
                    "try_num": int(m.group(1)),
                    "pair": m.group(2),
                    "buy_dex": m.group(3),
                    "sell_dex": m.group(4),
                    "est_usd": float(m.group(5)),
                })

            # Trade failure
            m = FAIL_RE.search(line)
            if m:
                error_msg = m.group(2)
                # Classify error
                if "Too little received" in error_msg:
                    error_type = "Too little received (V3)"
                elif "INSUFFICIENT_OUTPUT_AMOUNT" in error_msg:
                    error_type = "Insufficient output (V2)"
                elif "fill failed" in error_msg:
                    error_type = "Fill failed (other)"
                elif "send failed" in error_msg:
                    error_type = "Send failed"
                else:
                    error_type = "Other"
                failures.append({
                    "ts": ts,
                    "pair": m.group(1),
                    "error_type": error_type,
                    "error_msg": error_msg[:120],
                })

            # Atomic execution type
            m = ATOMIC_RE.search(line)
            if m:
                atomic_types.append({
                    "ts": ts,
                    "exec_type": m.group(1),
                    "pair": m.group(2),
                    "buy_dex": m.group(3),
                    "sell_dex": m.group(4),
                })

            # Cooldown
            m = COOLDOWN_RE.search(line)
            if m:
                cooldowns.append({
                    "ts": ts,
                    "suppressed": int(m.group(1)),
                    "remaining": int(m.group(2)),
                })

            # Private mempool send
            if PRIVATE_SEND_RE.search(line):
                private_sends.append({"ts": ts})

            # Tx submitted
            m = TX_SUBMIT_RE.search(line)
            if m:
                tx_submissions.append({"ts": ts, "tx_hash": m.group(1)})

            # Outcomes
            if PROFIT_RE.search(line):
                profits.append({"ts": ts})
            if LOSS_RE.search(line):
                losses.append({"ts": ts})
            if TRADE_COMPLETE_RE.search(line):
                trade_completes.append({"ts": ts})
            if HALT_RE.search(line):
                halts.append({"ts": ts})

    return {
        "opportunities": opportunities,
        "attempts": attempts,
        "failures": failures,
        "atomic_types": atomic_types,
        "cooldowns": cooldowns,
        "private_sends": private_sends,
        "tx_submissions": tx_submissions,
        "profits": profits,
        "losses": losses,
        "trade_completes": trade_completes,
        "halts": halts,
    }


def analyze_timing(attempts, failures):
    """Compute fill/estimateGas latency by matching TRY → Trade failed pairs."""
    latencies = []
    fail_by_pair = defaultdict(list)
    for f in failures:
        if f["ts"]:
            fail_by_pair[f["pair"]].append(f)

    for a in attempts:
        if not a["ts"]:
            continue
        pair_fails = fail_by_pair.get(a["pair"], [])
        # Find the first failure AFTER this attempt timestamp
        for f in pair_fails:
            if f["ts"] and f["ts"] > a["ts"]:
                delta_ms = (f["ts"] - a["ts"]).total_seconds() * 1000
                if delta_ms < 5000:  # Sanity: within 5s
                    latencies.append({
                        "pair": a["pair"],
                        "est_usd": a["est_usd"],
                        "latency_ms": delta_ms,
                        "error_type": f["error_type"],
                    })
                    pair_fails.remove(f)
                    break
    return latencies


def analyze_opportunity_clustering(opportunities):
    """Analyze how opportunities cluster in time (burst vs steady)."""
    if not opportunities:
        return {}

    df = pd.DataFrame(opportunities)
    df["ts"] = pd.to_datetime(df["ts"], utc=True)
    df = df.sort_values("ts")

    # Time gaps between consecutive opportunities
    gaps = df["ts"].diff().dt.total_seconds().dropna()

    # Bucket into 5-minute windows
    df["window"] = df["ts"].dt.floor("5min")
    window_counts = df.groupby("window").size()

    return {
        "gap_median_s": float(gaps.median()),
        "gap_mean_s": float(gaps.mean()),
        "gap_p10_s": float(gaps.quantile(0.10)),
        "gap_p90_s": float(gaps.quantile(0.90)),
        "busiest_5min_count": int(window_counts.max()),
        "quietest_5min_count": int(window_counts.min()),
        "windows_with_0_opps": 0,  # placeholder - computed below
        "total_5min_windows": len(window_counts),
    }


def analyze_prices(prices_dir: str) -> dict:
    """Analyze price history CSVs for cross-DEX spread patterns."""
    prices_path = Path(prices_dir)
    csv_files = sorted(prices_path.glob("prices_*.csv"))
    if not csv_files:
        return {"error": "No price CSV files found"}

    # Only read the most recent file (today's)
    latest = csv_files[-1]
    print(f"  Reading {latest.name} ...", flush=True)

    df = pd.read_csv(latest, parse_dates=["timestamp"])
    df = df.sort_values(["pair", "timestamp", "dex"])

    results = {}
    for pair, pdf in df.groupby("pair"):
        # Pivot: one column per DEX, rows are (timestamp, block)
        pivot = pdf.pivot_table(
            index=["timestamp", "block"],
            columns="dex",
            values="price",
            aggfunc="first",
        )

        if pivot.shape[1] < 2:
            continue

        # Compute all pairwise spreads
        dexes = pivot.columns.tolist()
        spreads = []
        for i, d1 in enumerate(dexes):
            for d2 in dexes[i + 1 :]:
                col1 = pivot[d1]
                col2 = pivot[d2]
                valid = col1.notna() & col2.notna() & (col1 > 0) & (col2 > 0)
                if valid.sum() < 10:
                    continue
                # Spread = abs(p1 - p2) / min(p1, p2) * 100
                s = (
                    (col1[valid] - col2[valid]).abs()
                    / col1[valid].combine(col2[valid], min)
                    * 100
                )
                spreads.append({
                    "pair": pair,
                    "dex_a": d1,
                    "dex_b": d2,
                    "spread_mean_pct": float(s.mean()),
                    "spread_median_pct": float(s.median()),
                    "spread_p95_pct": float(s.quantile(0.95)),
                    "spread_max_pct": float(s.max()),
                    "spread_gt_0_05_pct": float((s > 0.05).mean() * 100),
                    "spread_gt_0_10_pct": float((s > 0.10).mean() * 100),
                    "spread_gt_0_20_pct": float((s > 0.20).mean() * 100),
                    "n_observations": int(valid.sum()),
                })
        if spreads:
            results[pair] = spreads

    return results


def print_report(data: dict, latencies: list, price_analysis: dict):
    """Print a comprehensive text report."""
    opps = data["opportunities"]
    atts = data["attempts"]
    fails = data["failures"]
    atomic = data["atomic_types"]
    cools = data["cooldowns"]
    psends = data["private_sends"]
    txsubs = data["tx_submissions"]
    profits = data["profits"]
    losses = data["losses"]
    completes = data["trade_completes"]
    halts_list = data["halts"]

    print("\n" + "=" * 72)
    print("  DEX ARBITRAGE BOT — SESSION ANALYSIS REPORT")
    print("=" * 72)

    # ── Session Duration ────────────────────────────────────────────────
    ts_list = [o["ts"] for o in opps if o["ts"]] + [a["ts"] for a in atts if a["ts"]]
    if ts_list:
        t_start = min(ts_list)
        t_end = max(ts_list)
        duration = t_end - t_start
        dur_str = str(duration).split(".")[0]
        print(f"\n  Session: {t_start.strftime('%Y-%m-%d %H:%M:%S UTC')} → "
              f"{t_end.strftime('%H:%M:%S UTC')}  ({dur_str})")

    # ── High-Level Funnel ───────────────────────────────────────────────
    print("\n─── EXECUTION FUNNEL ───────────────────────────────────────")
    print(f"  Opportunities detected:        {len(opps):>6}")
    print(f"  Cooldown suppressions:         {len(cools):>6}")
    print(f"  Execution attempts (TRY #):    {len(atts):>6}")
    print(f"  Private mempool sends:         {len(psends):>6}")
    print(f"  Reverted at estimateGas/fill:  {len(fails):>6}")
    print(f"  Submitted on-chain (tx hash):  {len(txsubs):>6}")
    print(f"  ATOMIC PROFIT:                 {len(profits):>6}")
    print(f"  ATOMIC LOSS:                   {len(losses):>6}")
    print(f"  Trade complete (profitable):   {len(completes):>6}")
    print(f"  HALT:                          {len(halts_list):>6}")

    if len(atts) > 0:
        fill_fail_rate = len(fails) / len(atts) * 100
        onchain_rate = len(txsubs) / len(atts) * 100
        print(f"\n  Fill/estimateGas revert rate:   {fill_fail_rate:.1f}%")
        print(f"  On-chain submission rate:       {onchain_rate:.1f}%")
        if len(txsubs) > 0:
            success_rate = len(completes) / len(txsubs) * 100
            print(f"  On-chain success rate:          {success_rate:.1f}%")

    # ── Opportunity Rate ────────────────────────────────────────────────
    if ts_list and duration.total_seconds() > 0:
        hrs = duration.total_seconds() / 3600
        print(f"\n  Opportunities per hour:         {len(opps)/hrs:.1f}")
        print(f"  Attempts per hour:              {len(atts)/hrs:.1f}")
        print(f"  Cooldown suppressions per hour: {len(cools)/hrs:.1f}")

    # ── Failure Breakdown ───────────────────────────────────────────────
    print("\n─── FAILURE BREAKDOWN ─────────────────────────────────────")
    fail_types = defaultdict(int)
    for f in fails:
        fail_types[f["error_type"]] += 1
    for err_type, count in sorted(fail_types.items(), key=lambda x: -x[1]):
        print(f"  {err_type:<40s} {count:>5}")

    # ── By Pair ─────────────────────────────────────────────────────────
    print("\n─── ACTIVITY BY PAIR ──────────────────────────────────────")
    pair_opps = defaultdict(int)
    pair_atts = defaultdict(int)
    pair_fails = defaultdict(int)
    pair_est_usd = defaultdict(list)

    for o in opps:
        pair_opps[o["pair"]] += 1
    for a in atts:
        pair_atts[a["pair"]] += 1
        pair_est_usd[a["pair"]].append(a["est_usd"])
    for f in fails:
        pair_fails[f["pair"]] += 1

    all_pairs = sorted(set(list(pair_opps.keys()) + list(pair_atts.keys())))
    print(f"  {'Pair':<14s} {'Opps':>6} {'Tries':>6} {'Fails':>6} "
          f"{'Avg$':>7} {'Max$':>7} {'Pass':>6}")
    print(f"  {'─'*14} {'─'*6} {'─'*6} {'─'*6} {'─'*7} {'─'*7} {'─'*6}")
    for pair in all_pairs:
        o_count = pair_opps[pair]
        a_count = pair_atts[pair]
        f_count = pair_fails[pair]
        passed = a_count - f_count
        ests = pair_est_usd.get(pair, [])
        avg_est = np.mean(ests) if ests else 0
        max_est = max(ests) if ests else 0
        print(f"  {pair:<14s} {o_count:>6} {a_count:>6} {f_count:>6} "
              f"{avg_est:>7.2f} {max_est:>7.2f} {passed:>6}")

    # ── By Route (buy_dex → sell_dex) ───────────────────────────────────
    print("\n─── ACTIVITY BY ROUTE (Buy → Sell) ────────────────────────")
    route_atts = defaultdict(int)
    route_fails = defaultdict(int)
    route_est = defaultdict(list)
    for a in atts:
        route = f"{a['buy_dex']} → {a['sell_dex']}"
        route_atts[route] += 1
        route_est[route].append(a["est_usd"])
    for a_type in atomic:
        route = f"{a_type['buy_dex']} → {a_type['sell_dex']}"
    # Match failures to routes via atomic_types (same timestamp)
    fail_ts_map = {f["ts"]: f for f in fails if f["ts"]}
    for a_type in atomic:
        route = f"{a_type['buy_dex']} → {a_type['sell_dex']}"
        # Check if there's a failure within 1s of this atomic execution
        for f in fails:
            if f["ts"] and a_type["ts"] and f["pair"] == a_type["pair"]:
                delta = abs((f["ts"] - a_type["ts"]).total_seconds())
                if delta < 1.0:
                    route_fails[route] += 1
                    break

    print(f"  {'Route':<40s} {'Tries':>6} {'Fails':>6} {'Avg$':>7}")
    print(f"  {'─'*40} {'─'*6} {'─'*6} {'─'*7}")
    for route in sorted(route_atts.keys(), key=lambda r: -route_atts[r]):
        a_count = route_atts[route]
        f_count = route_fails.get(route, 0)
        avg_est = np.mean(route_est[route])
        print(f"  {route:<40s} {a_count:>6} {f_count:>6} {avg_est:>7.2f}")

    # ── Execution Type Breakdown ────────────────────────────────────────
    print("\n─── ATOMIC EXECUTION TYPES ────────────────────────────────")
    type_counts = defaultdict(int)
    for a_type in atomic:
        type_counts[a_type["exec_type"]] += 1
    for exec_type, count in sorted(type_counts.items(), key=lambda x: -x[1]):
        print(f"  {exec_type:<20s} {count:>5}")

    # ── Estimated Profit Distribution ───────────────────────────────────
    print("\n─── ESTIMATED PROFIT DISTRIBUTION (at detection) ─────────")
    if opps:
        nets = [o["net_usd"] for o in opps]
        spreads = [o["spread_pct"] for o in opps]
        print(f"  Net USD  — min: ${min(nets):.2f}  median: ${np.median(nets):.2f}  "
              f"mean: ${np.mean(nets):.2f}  max: ${max(nets):.2f}")
        print(f"  Spread % — min: {min(spreads):.3f}%  median: {np.median(spreads):.3f}%  "
              f"mean: {np.mean(spreads):.3f}%  max: {max(spreads):.3f}%")

        # Histogram buckets
        print("\n  Estimated profit buckets:")
        buckets = [0.10, 0.15, 0.20, 0.30, 0.50, 1.00, 2.00, 5.00]
        prev = 0
        for b in buckets:
            count = sum(1 for n in nets if prev <= n < b)
            pct = count / len(nets) * 100
            bar = "█" * int(pct / 2)
            print(f"    ${prev:.2f}-${b:.2f}:  {count:>5}  ({pct:>5.1f}%)  {bar}")
            prev = b
        count = sum(1 for n in nets if n >= buckets[-1])
        pct = count / len(nets) * 100
        bar = "█" * int(pct / 2)
        print(f"    >=${buckets[-1]:.2f}:     {count:>5}  ({pct:>5.1f}%)  {bar}")

    # ── Timing / Latency ────────────────────────────────────────────────
    print("\n─── FILL/ESTIMATEGAS LATENCY (TRY → Trade failed) ───────")
    if latencies:
        lats = [l["latency_ms"] for l in latencies]
        print(f"  Matched attempt→failure pairs:  {len(latencies)}")
        print(f"  Latency — min: {min(lats):.0f}ms  median: {np.median(lats):.0f}ms  "
              f"mean: {np.mean(lats):.0f}ms  max: {max(lats):.0f}ms  p95: {np.percentile(lats, 95):.0f}ms")

        # By pair
        lat_df = pd.DataFrame(latencies)
        pair_lats = lat_df.groupby("pair")["latency_ms"].agg(["median", "mean", "count"])
        print("\n  By pair:")
        for pair, row in pair_lats.iterrows():
            print(f"    {pair:<14s}  median: {row['median']:>6.0f}ms  "
                  f"mean: {row['mean']:>6.0f}ms  (n={int(row['count'])})")
    else:
        print("  No matched attempt→failure pairs found.")

    # ── Opportunity Time-of-Day Heatmap ─────────────────────────────────
    print("\n─── OPPORTUNITY FREQUENCY BY HOUR (UTC) ──────────────────")
    if opps:
        hourly = defaultdict(int)
        for o in opps:
            if o["ts"]:
                hourly[o["ts"].hour] += 1
        hours_present = sorted(hourly.keys())
        max_count = max(hourly.values()) if hourly else 1
        for h in hours_present:
            count = hourly[h]
            bar_len = int(count / max_count * 40)
            bar = "█" * bar_len
            print(f"    {h:02d}:00  {count:>5}  {bar}")

    # ── Cooldown Effectiveness ──────────────────────────────────────────
    print("\n─── COOLDOWN ANALYSIS ─────────────────────────────────────")
    if cools:
        total_suppressed = sum(c["suppressed"] for c in cools)
        print(f"  Total cooldown events:    {len(cools)}")
        print(f"  Total routes suppressed:  {total_suppressed}")
        remaining_vals = [c["remaining"] for c in cools]
        print(f"  Routes remaining (when suppression occurs):")
        print(f"    min: {min(remaining_vals)}  max: {max(remaining_vals)}  "
              f"mean: {np.mean(remaining_vals):.1f}")
        # How many RPC calls did cooldown save?
        # Each suppressed route would have been 1 fill_transaction (estimateGas) call
        print(f"  Estimated RPC calls saved: {total_suppressed} "
              f"(1 fill_transaction per suppressed route)")

    # ── Opportunity Clustering ──────────────────────────────────────────
    print("\n─── OPPORTUNITY CLUSTERING ────────────────────────────────")
    if opps:
        opp_df = pd.DataFrame(opps)
        opp_df["ts"] = pd.to_datetime(opp_df["ts"], utc=True)
        opp_df = opp_df.sort_values("ts")
        gaps = opp_df["ts"].diff().dt.total_seconds().dropna()

        print(f"  Inter-opportunity gap:")
        print(f"    median: {gaps.median():.1f}s  mean: {gaps.mean():.1f}s")
        print(f"    p10: {gaps.quantile(0.10):.1f}s  p90: {gaps.quantile(0.90):.1f}s")

        # Burst detection: opportunities within 5s of each other
        bursts = (gaps <= 5).sum()
        isolated = (gaps > 30).sum()
        print(f"    Burst pairs (gap ≤ 5s): {bursts}")
        print(f"    Isolated (gap > 30s):   {isolated}")

    # ── Cross-DEX Spread Analysis (from price CSVs) ─────────────────────
    print("\n─── CROSS-DEX SPREAD ANALYSIS (price history) ─────────────")
    if isinstance(price_analysis, dict) and "error" not in price_analysis:
        for pair, spreads in sorted(price_analysis.items()):
            print(f"\n  {pair}:")
            # Sort by mean spread descending
            spreads_sorted = sorted(spreads, key=lambda x: -x["spread_mean_pct"])
            for s in spreads_sorted[:8]:  # top 8 routes per pair
                gt005 = s["spread_gt_0_05_pct"]
                gt010 = s["spread_gt_0_10_pct"]
                gt020 = s["spread_gt_0_20_pct"]
                print(f"    {s['dex_a']:<22s} vs {s['dex_b']:<22s}  "
                      f"mean={s['spread_mean_pct']:.4f}%  "
                      f"p95={s['spread_p95_pct']:.4f}%  "
                      f"max={s['spread_max_pct']:.4f}%  "
                      f"(>0.05%: {gt005:.1f}%  >0.10%: {gt010:.1f}%  >0.20%: {gt020:.1f}%)  "
                      f"n={s['n_observations']}")
    elif isinstance(price_analysis, dict) and "error" in price_analysis:
        print(f"  {price_analysis['error']}")
    else:
        print("  No price data available.")

    # ── Repeated Opportunity Analysis ───────────────────────────────────
    print("\n─── REPEATED / PHANTOM SPREAD ANALYSIS ────────────────────")
    if opps:
        opp_df = pd.DataFrame(opps)
        opp_df["route"] = opp_df["buy_dex"] + " → " + opp_df["sell_dex"]
        opp_df["route_pair"] = opp_df["pair"] + " | " + opp_df["route"]

        # Count how many times the same route fires at the same estimated price
        route_counts = opp_df.groupby("route_pair").agg(
            count=("net_usd", "size"),
            mean_net=("net_usd", "mean"),
            std_net=("net_usd", "std"),
            min_net=("net_usd", "min"),
            max_net=("net_usd", "max"),
        ).sort_values("count", ascending=False)

        print(f"  {'Route':<50s} {'Count':>6} {'Avg$':>7} {'Std$':>7} {'Range$':>12}")
        print(f"  {'─'*50} {'─'*6} {'─'*7} {'─'*7} {'─'*12}")
        for route, row in route_counts.head(15).iterrows():
            std = row["std_net"] if not np.isnan(row["std_net"]) else 0
            print(f"  {route:<50s} {int(row['count']):>6} "
                  f"{row['mean_net']:>7.2f} {std:>7.3f} "
                  f"${row['min_net']:.2f}-${row['max_net']:.2f}")

        # Flag potential phantom spreads: same route fires 10+ times with low std
        phantoms = route_counts[
            (route_counts["count"] >= 10) & (route_counts["std_net"] < 0.02)
        ]
        if len(phantoms) > 0:
            print(f"\n  ⚠ POTENTIAL PHANTOM SPREADS (≥10 repeats, std < $0.02):")
            for route, row in phantoms.iterrows():
                print(f"    {route}  (n={int(row['count'])}, "
                      f"mean=${row['mean_net']:.2f}, std=${row['std_net']:.3f})")
        else:
            print(f"\n  No phantom spread patterns detected (good).")

    # ── Summary & Key Metrics ───────────────────────────────────────────
    print("\n" + "=" * 72)
    print("  KEY METRICS SUMMARY")
    print("=" * 72)
    if ts_list:
        hrs = duration.total_seconds() / 3600
        print(f"  Runtime:                    {dur_str}")
        print(f"  Opportunities:              {len(opps)} ({len(opps)/hrs:.1f}/hr)")
        print(f"  Attempts:                   {len(atts)} ({len(atts)/hrs:.1f}/hr)")
        print(f"  Fill revert rate:           {len(fails)/max(len(atts),1)*100:.1f}%")
        print(f"  On-chain submissions:       {len(txsubs)}")
        print(f"  Successful trades:          {len(completes)}")
        print(f"  Cooldown suppressions:      {len(cools)}")
        if opps:
            print(f"  Median estimated profit:    ${np.median([o['net_usd'] for o in opps]):.2f}")
        if latencies:
            print(f"  Median fill latency:        {np.median([l['latency_ms'] for l in latencies]):.0f}ms")
        print(f"  Net P&L:                    $0.00 (no successful trades)")

    print("\n" + "=" * 72)
    print("  END OF REPORT")
    print("=" * 72 + "\n")


def main():
    parser = argparse.ArgumentParser(description="Analyze DEX arb bot session data")
    parser.add_argument(
        "--log",
        default="/home/botuser/bots/dexarb/data/logs/livebot_ws.log",
        help="Path to livebot log file",
    )
    parser.add_argument(
        "--prices",
        default="/home/botuser/bots/dexarb/data/polygon/price_history",
        help="Path to price history directory",
    )
    parser.add_argument(
        "--skip-prices",
        action="store_true",
        help="Skip price CSV analysis (faster)",
    )
    args = parser.parse_args()

    print("Parsing log file...", flush=True)
    data = parse_log(args.log)

    print("Computing timing/latency...", flush=True)
    latencies = analyze_timing(data["attempts"], data["failures"])

    price_analysis = {}
    if not args.skip_prices:
        print("Analyzing price history...", flush=True)
        price_analysis = analyze_prices(args.prices)
    else:
        price_analysis = {"error": "Skipped (--skip-prices)"}

    print_report(data, latencies, price_analysis)


if __name__ == "__main__":
    main()
