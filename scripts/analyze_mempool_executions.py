#!/usr/bin/env python3
"""
A4 Phase 3 — Mempool Execution Analyzer

Purpose:
    Analyze mempool_executions_*.csv data from Phase 3 execution pipeline.
    Reports success rates, gas spend, route performance, timing distributions,
    and hourly patterns. Complements analyze_mempool.py (Phase 1/2 observation).

Author: AI-Generated
Created: 2026-02-01
Modified: 2026-02-01

Dependencies:
    - Python 3.8+ (stdlib only — no pandas/numpy required)

Data Sources:
    - data/{chain}/mempool/mempool_executions_YYYYMMDD.csv

Usage:
    # Polygon (default)
    python3 scripts/analyze_mempool_executions.py

    # Specific date
    python3 scripts/analyze_mempool_executions.py --date 20260201

    # All dates
    python3 scripts/analyze_mempool_executions.py --all

    # Base chain
    python3 scripts/analyze_mempool_executions.py --chain base

Notes:
    - CSV columns: timestamp_utc,tx_hash,pair,buy_dex,sell_dex,spread_pct,
      est_profit_usd,result,profit_usd,gas_cost_usd,net_profit_usd,
      exec_time_ms,lead_time_ms,source
    - "FAIL" with empty tx_hash = pre-send revert (estimateGas or sign failure)
    - "FAIL" with tx_hash = on-chain revert (gas burned)
    - "SUCCESS" = profitable trade confirmed
"""

import argparse
import csv
import glob
import os
import sys
from collections import defaultdict
from datetime import datetime

# ── Constants ────────────────────────────────────────────────────────────────

BOLD = "\033[1m"
GREEN = "\033[92m"
RED = "\033[91m"
YELLOW = "\033[93m"
CYAN = "\033[96m"
DIM = "\033[2m"
RESET = "\033[0m"

# ── Helpers ──────────────────────────────────────────────────────────────────

def parse_float(s, default=0.0):
    try:
        return float(s)
    except (ValueError, TypeError):
        return default

def parse_int(s, default=0):
    try:
        return int(s)
    except (ValueError, TypeError):
        return default

def percentile(sorted_list, p):
    """Simple percentile (nearest rank)."""
    if not sorted_list:
        return 0.0
    k = max(0, min(len(sorted_list) - 1, int(len(sorted_list) * p / 100)))
    return sorted_list[k]

def fmt_pct(num, den):
    """Format percentage with fallback."""
    if den == 0:
        return "—"
    return f"{100 * num / den:.1f}%"

# ── Main Analysis ────────────────────────────────────────────────────────────

def load_csv(filepath):
    """Load execution CSV, return list of dicts."""
    rows = []
    with open(filepath, "r") as f:
        reader = csv.DictReader(f)
        for row in reader:
            rows.append(row)
    return rows

def analyze(rows, label=""):
    """Run full analysis on a list of execution rows."""
    if not rows:
        print(f"\n{YELLOW}No execution data found.{RESET}")
        return

    # ── Summary ──
    total = len(rows)
    success = [r for r in rows if r.get("result") == "SUCCESS"]
    fail = [r for r in rows if r.get("result") == "FAIL"]
    presend_fail = [r for r in fail if not r.get("tx_hash")]
    onchain_fail = [r for r in fail if r.get("tx_hash")]

    n_success = len(success)
    n_presend = len(presend_fail)
    n_onchain = len(onchain_fail)

    print(f"\n{BOLD}═══ Mempool Execution Analysis{f' — {label}' if label else ''} ═══{RESET}")
    print(f"\n{BOLD}Summary{RESET}")
    print(f"  Total signals:      {total}")
    print(f"  {GREEN}SUCCESS:            {n_success} ({fmt_pct(n_success, total)}){RESET}")
    print(f"  {RED}FAIL (pre-send):    {n_presend} ({fmt_pct(n_presend, total)}){RESET}")
    print(f"  {YELLOW}FAIL (on-chain):    {n_onchain} ({fmt_pct(n_onchain, total)}){RESET}")

    # ── Profitability ──
    total_gross = sum(parse_float(r.get("profit_usd")) for r in rows)
    total_gas = sum(parse_float(r.get("gas_cost_usd")) for r in rows)
    total_net = sum(parse_float(r.get("net_profit_usd")) for r in rows)
    total_est = sum(parse_float(r.get("est_profit_usd")) for r in rows)

    print(f"\n{BOLD}Profitability{RESET}")
    print(f"  Total estimated:    ${total_est:.4f}")
    print(f"  Total gross profit: ${total_gross:.4f}")
    print(f"  Total gas spent:    ${total_gas:.4f}")
    color = GREEN if total_net > 0 else RED
    print(f"  {color}Net P&L:            ${total_net:.4f}{RESET}")
    if n_success > 0:
        avg_profit = total_net / n_success
        print(f"  Avg profit/success: ${avg_profit:.4f}")

    # ── Pre-send revert analysis ──
    if presend_fail:
        print(f"\n{BOLD}Pre-Send Reverts (no gas burned){RESET}")
        print(f"  Count: {n_presend}")
        est_profits = sorted(parse_float(r.get("est_profit_usd")) for r in presend_fail)
        print(f"  Est. profit range:  ${est_profits[0]:.2f} — ${est_profits[-1]:.2f}")
        print(f"  Est. profit median: ${percentile(est_profits, 50):.2f}")
        # Common error messages
        errors = defaultdict(int)
        for r in presend_fail:
            err = r.get("error", "unknown") or "sign/send failure"
            # Truncate long errors
            if len(err) > 60:
                err = err[:60] + "..."
            errors[err] += 1
        if errors:
            print(f"  Error breakdown:")
            for err, cnt in sorted(errors.items(), key=lambda x: -x[1])[:5]:
                print(f"    {cnt:3d}× {err}")

    # ── On-chain revert analysis ──
    if onchain_fail:
        print(f"\n{BOLD}On-Chain Reverts (gas burned){RESET}")
        print(f"  Count: {n_onchain}")
        gas_costs = sorted(parse_float(r.get("gas_cost_usd")) for r in onchain_fail)
        print(f"  Gas spent: ${sum(gas_costs):.4f} total")
        print(f"  Gas per revert: ${percentile(gas_costs, 50):.4f} median")

    # ── Successful trades ──
    if success:
        print(f"\n{BOLD}{GREEN}Successful Trades{RESET}")
        for r in success:
            print(f"  {r.get('timestamp_utc','')} | {r.get('pair','')} | "
                  f"{r.get('buy_dex','')}→{r.get('sell_dex','')} | "
                  f"net=${parse_float(r.get('net_profit_usd')):.4f} | "
                  f"tx={r.get('tx_hash','')[:18]}...")

    # ── Route breakdown ──
    print(f"\n{BOLD}Route Breakdown{RESET}")
    route_stats = defaultdict(lambda: {"total": 0, "success": 0, "fail": 0,
                                        "est_sum": 0.0, "net_sum": 0.0, "gas_sum": 0.0})
    for r in rows:
        key = f"{r.get('pair','')} | {r.get('buy_dex','')} → {r.get('sell_dex','')}"
        s = route_stats[key]
        s["total"] += 1
        if r.get("result") == "SUCCESS":
            s["success"] += 1
        else:
            s["fail"] += 1
        s["est_sum"] += parse_float(r.get("est_profit_usd"))
        s["net_sum"] += parse_float(r.get("net_profit_usd"))
        s["gas_sum"] += parse_float(r.get("gas_cost_usd"))

    # Sort by total descending
    for route, s in sorted(route_stats.items(), key=lambda x: -x[1]["total"]):
        rate = fmt_pct(s["success"], s["total"])
        print(f"  {route}")
        print(f"    {s['total']} signals | {s['success']} ok | {s['fail']} fail ({rate}) | "
              f"est=${s['est_sum']:.2f} | net=${s['net_sum']:.4f} | gas=${s['gas_sum']:.4f}")

    # ── Timing analysis ──
    exec_times = sorted(parse_int(r.get("exec_time_ms")) for r in rows if parse_int(r.get("exec_time_ms")) > 0)
    lead_times = sorted(parse_int(r.get("lead_time_ms")) for r in rows if parse_int(r.get("lead_time_ms")) > 0)

    print(f"\n{BOLD}Timing{RESET}")
    if exec_times:
        print(f"  Execution time (signal → result):")
        print(f"    median={percentile(exec_times, 50)}ms  p95={percentile(exec_times, 95)}ms  "
              f"min={exec_times[0]}ms  max={exec_times[-1]}ms")
    else:
        print(f"  Execution time: no data (all 0ms = pre-send failures)")
    if lead_times:
        print(f"  Lead time (pending seen → signal processed):")
        print(f"    median={percentile(lead_times, 50)}ms  p95={percentile(lead_times, 95)}ms  "
              f"min={lead_times[0]}ms  max={lead_times[-1]}ms")

    # ── Spread distribution ──
    spreads = sorted(parse_float(r.get("spread_pct")) for r in rows)
    if spreads:
        print(f"\n{BOLD}Spread Distribution{RESET}")
        print(f"  Median: {percentile(spreads, 50):.4f}%")
        print(f"  P25:    {percentile(spreads, 25):.4f}%")
        print(f"  P75:    {percentile(spreads, 75):.4f}%")
        print(f"  Max:    {spreads[-1]:.4f}%")
        # Spread buckets
        buckets = {"<0.03%": 0, "0.03-0.05%": 0, "0.05-0.10%": 0, "0.10-0.20%": 0, ">0.20%": 0}
        for s in spreads:
            if s < 0.03: buckets["<0.03%"] += 1
            elif s < 0.05: buckets["0.03-0.05%"] += 1
            elif s < 0.10: buckets["0.05-0.10%"] += 1
            elif s < 0.20: buckets["0.10-0.20%"] += 1
            else: buckets[">0.20%"] += 1
        for bucket, cnt in buckets.items():
            bar = "█" * min(40, cnt)
            print(f"    {bucket:>10s}: {cnt:3d} {bar}")

    # ── Hourly pattern ──
    hourly = defaultdict(lambda: {"total": 0, "success": 0})
    for r in rows:
        ts = r.get("timestamp_utc", "")
        try:
            dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
            h = dt.hour
        except (ValueError, AttributeError):
            continue
        hourly[h]["total"] += 1
        if r.get("result") == "SUCCESS":
            hourly[h]["success"] += 1

    if hourly:
        print(f"\n{BOLD}Hourly Pattern (UTC){RESET}")
        for h in sorted(hourly.keys()):
            s = hourly[h]
            rate = fmt_pct(s["success"], s["total"])
            bar = "█" * min(40, s["total"])
            print(f"  {h:02d}:00  {s['total']:3d} signals  {s['success']:2d} ok ({rate:>5s})  {bar}")

    # ── Verdict ──
    print(f"\n{BOLD}Verdict{RESET}")
    if n_success == 0 and total > 0:
        if n_onchain == 0:
            print(f"  {YELLOW}All {total} signals failed pre-send (no gas burned).{RESET}")
            print(f"  Likely: pool conditions changed between signal and execution.")
            print(f"  The spread detected in pending tx is already closed by the time we build+sign.")
            print(f"  Check: Are we losing to faster backrunners, or are signals inherently stale?")
        else:
            print(f"  {RED}{n_onchain} on-chain reverts (${total_gas:.4f} gas burned).{RESET}")
            print(f"  On-chain reverts mean we're submitting but the opportunity is gone by inclusion.")
        if total < 50:
            print(f"  {DIM}Sample too small ({total} signals). Collect more data before tuning.{RESET}")
    elif n_success > 0:
        rate = 100 * n_success / total
        if rate >= 5:
            print(f"  {GREEN}Success rate {rate:.1f}% — PROFITABLE. {RESET}")
        elif rate >= 1:
            print(f"  {YELLOW}Success rate {rate:.1f}% — MARGINAL. Tune gas/thresholds.{RESET}")
        else:
            print(f"  {RED}Success rate {rate:.1f}% — investigate timing/gas.{RESET}")
        print(f"  Net P&L: ${total_net:.4f}")

    print()

# ── CLI ──────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Analyze mempool execution CSV data (A4 Phase 3)")
    parser.add_argument("--chain", default="polygon", help="Chain name (default: polygon)")
    parser.add_argument("--date", default=None, help="Specific date YYYYMMDD (default: today)")
    parser.add_argument("--all", action="store_true", help="Analyze all available dates")
    args = parser.parse_args()

    data_dir = f"/home/botuser/bots/dexarb/data/{args.chain}/mempool"

    if args.all:
        pattern = os.path.join(data_dir, "mempool_executions_*.csv")
        files = sorted(glob.glob(pattern))
        if not files:
            print(f"No execution CSVs found in {data_dir}")
            sys.exit(1)
        all_rows = []
        for f in files:
            date_str = os.path.basename(f).replace("mempool_executions_", "").replace(".csv", "")
            rows = load_csv(f)
            if rows:
                analyze(rows, label=date_str)
                all_rows.extend(rows)
        if len(files) > 1 and all_rows:
            analyze(all_rows, label="ALL DATES COMBINED")
    else:
        if args.date:
            date_str = args.date
        else:
            date_str = datetime.utcnow().strftime("%Y%m%d")
        filepath = os.path.join(data_dir, f"mempool_executions_{date_str}.csv")
        if not os.path.exists(filepath):
            print(f"No data: {filepath}")
            # Try to find any file
            pattern = os.path.join(data_dir, "mempool_executions_*.csv")
            available = sorted(glob.glob(pattern))
            if available:
                print(f"Available files:")
                for f in available:
                    print(f"  {os.path.basename(f)}")
            sys.exit(1)
        rows = load_csv(filepath)
        analyze(rows, label=date_str)

if __name__ == "__main__":
    main()
