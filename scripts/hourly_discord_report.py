#!/usr/bin/env python3
"""
Hourly Discord Report Publisher

Purpose:
    Publishes standardized paper trading reports to Discord every hour.
    Captures opportunities from the paper trading tmux session and
    generates comprehensive statistics.

Author: AI-Generated
Created: 2026-01-28
Modified: 2026-01-28

Usage:
    python3 hourly_discord_report.py
    # Or run in tmux:
    # tmux new-session -d -s discord-reports 'python3 hourly_discord_report.py'

Configuration:
    - DISCORD_WEBHOOK: Set in environment or use default from .env
    - TMUX_SESSION: Paper trading tmux session name
    - REPORT_INTERVAL_HOURS: How often to publish (default: 1)
"""

import os
import re
import json
import subprocess
import requests
from datetime import datetime, timedelta
from collections import defaultdict
import time
import pytz

# Configuration
DISCORD_WEBHOOK = os.environ.get(
    'DISCORD_WEBHOOK',
    'https://discord.com/api/webhooks/1444394184852111621/jWmTkmmr7yXKQ65S1eF3WBm1ffnIbu0I-Vva8vnInnZ5mF-iVbRD98BxEErVjG5zjNjD'
)
TMUX_SESSION = 'dexarb-phase1'
TMUX_WINDOW = '1'  # Paper trading window
REPORT_INTERVAL_HOURS = 1
POOL_STATE_FILE = '/home/botuser/bots/dexarb/data/pool_state_phase1.json'

# Timezone
PACIFIC = pytz.timezone('America/Los_Angeles')
UTC = pytz.UTC


def capture_tmux_output(lines=3000):
    """Capture recent output from paper trading tmux window."""
    try:
        result = subprocess.run(
            ['tmux', 'capture-pane', '-t', f'{TMUX_SESSION}:{TMUX_WINDOW}',
             '-p', '-S', f'-{lines}', '-J'],
            capture_output=True, text=True, timeout=10
        )
        return result.stdout
    except Exception as e:
        print(f"Error capturing tmux output: {e}")
        return ""


def parse_opportunities(raw_output, period_start):
    """Parse FOUND opportunities from raw output within the time period."""
    opportunities = []

    # Pre-process: join wrapped lines (lines not starting with timestamp are continuations)
    # Wrapped lines occur when tmux pane width causes line breaks mid-log
    lines = raw_output.split('\n')
    joined_lines = []
    current_line = ""

    for line in lines:
        # Lines starting with timestamp pattern are new entries
        if re.match(r'^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}', line):
            if current_line:
                joined_lines.append(current_line)
            current_line = line
        else:
            # Continuation of previous line - join with space
            current_line += " " + line.strip()

    if current_line:
        joined_lines.append(current_line)

    pattern = r'(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}).*?\[(\w+[^]]*)\] FOUND Opportunity: (\w+/\w+).*?Midmarket: ([\d.]+)%.*?Executable: ([\d.]+)%.*?Est\. Profit: \$([\d.]+).*?\| (.+?)(?:\s*$)'
    v3_pattern = r'\[V3 fee=([\d.]+)%\]'

    for line in joined_lines:
        if 'FOUND' not in line:
            continue

        match = re.search(pattern, line)
        if match:
            timestamp_str = match.group(1)
            try:
                timestamp = datetime.fromisoformat(timestamp_str).replace(tzinfo=UTC)
                # Filter to current period
                if timestamp < period_start:
                    continue
            except:
                continue

            strategy = match.group(2)
            pair = match.group(3)
            midmarket = float(match.group(4))
            executable = float(match.group(5))
            profit = float(match.group(6))
            route = match.group(7).strip()

            # Detect V3
            v3_match = re.search(v3_pattern, line)
            is_v3 = v3_match is not None
            v3_fee = float(v3_match.group(1)) if v3_match else None

            opportunities.append({
                'timestamp': timestamp_str,
                'strategy': strategy,
                'pair': pair,
                'midmarket': midmarket,
                'executable': executable,
                'profit': profit,
                'route': route,
                'is_v3': is_v3,
                'v3_fee': v3_fee
            })

    return opportunities


def calculate_stats(opportunities):
    """Calculate statistics from parsed opportunities."""
    if not opportunities:
        return None

    total_opps = len(opportunities)
    unique_pairs = set(o['pair'] for o in opportunities)
    v3_opps = sum(1 for o in opportunities if o['is_v3'])
    v2_opps = total_opps - v3_opps

    # Profit stats
    total_profit = sum(o['profit'] for o in opportunities)
    profitable_opps = [o for o in opportunities if o['profit'] > 0]
    best_trade = max((o['profit'] for o in opportunities), default=0)
    avg_profit = total_profit / total_opps if total_opps > 0 else 0

    # Estimated profit with competition/slippage (60% success rate, 10% slippage)
    estimated_profit = total_profit * 0.60 * 0.90

    # Strategy stats
    strategy_counts = defaultdict(int)
    strategy_profits = defaultdict(float)
    for o in opportunities:
        strategy_counts[o['strategy']] += 1
        strategy_profits[o['strategy']] += o['profit']

    most_opps_strategy = max(strategy_counts, key=strategy_counts.get) if strategy_counts else "N/A"
    most_profit_strategy = max(strategy_profits, key=strategy_profits.get) if strategy_profits else "N/A"

    # Deduplicate opportunities by (pair, route) and aggregate
    # Group by unique (pair, route) combination
    # Normalize route: strip whitespace, collapse multiple spaces
    unique_opps = defaultdict(lambda: {'count': 0, 'total_profit': 0.0, 'example': None})
    for o in opportunities:
        # Normalize the route for consistent grouping
        normalized_route = ' '.join(o['route'].split())  # Collapse whitespace
        key = (o['pair'], normalized_route)
        unique_opps[key]['count'] += 1
        unique_opps[key]['total_profit'] += o['profit']
        if unique_opps[key]['example'] is None:
            # Store normalized route in example
            example_copy = o.copy()
            example_copy['route'] = normalized_route
            unique_opps[key]['example'] = example_copy

    # Build list of unique opportunities with aggregated stats
    aggregated = []
    for (pair, route), data in unique_opps.items():
        example = data['example']
        aggregated.append({
            'pair': pair,
            'route': route,
            'single_profit': example['profit'],
            'count': data['count'],
            'total_profit': data['total_profit'],
            'midmarket': example['midmarket'],
            'is_v3': example['is_v3']
        })

    # Sort by total_profit (accounts for duplicates) - highest total potential first
    top_3 = sorted(aggregated, key=lambda x: x['total_profit'], reverse=True)[:3]

    return {
        'total_opps': total_opps,
        'unique_pairs': list(unique_pairs),
        'unique_routes': len(unique_opps),  # New: count of unique (pair, route) combos
        'v2_opps': v2_opps,
        'v3_opps': v3_opps,
        'total_profit': total_profit,
        'best_trade': best_trade,
        'avg_profit': avg_profit,
        'estimated_profit': estimated_profit,
        'profitable_count': len(profitable_opps),
        'strategy_counts': dict(strategy_counts),
        'strategy_profits': dict(strategy_profits),
        'most_opps_strategy': most_opps_strategy,
        'most_profit_strategy': most_profit_strategy,
        'top_3': top_3
    }


def get_pool_counts():
    """Get V2 and V3 pool counts from state file."""
    try:
        with open(POOL_STATE_FILE, 'r') as f:
            state = json.load(f)
        return len(state.get('pools', {})), len(state.get('v3_pools', {}))
    except:
        return 0, 0


def send_discord_report(stats, period_start, period_end):
    """Send standardized report to Discord."""
    now = datetime.now(UTC)
    pacific_now = now.astimezone(PACIFIC)

    v2_pools, v3_pools = get_pool_counts()

    # Handle empty stats
    if stats is None:
        embed = {
            "title": "📊 DEX Arbitrage Paper Trading Report",
            "description": f"**No opportunities detected this period**",
            "color": 0x95A5A6,  # Gray
            "fields": [
                {
                    "name": "⚙️ General",
                    "value": f"**Timestamp:** {pacific_now.strftime('%Y-%m-%d %H:%M')} PT\n"
                             f"**Version:** V2+V3 Multi-Strategy\n"
                             f"**Period:** {period_start.strftime('%H:%M')}-{period_end.strftime('%H:%M')} UTC\n"
                             f"**Pools:** {v2_pools} V2 + {v3_pools} V3",
                    "inline": False
                }
            ],
            "footer": {"text": "DEX Arbitrage Bot • Polygon Mainnet • Paper Trading"},
            "timestamp": now.isoformat()
        }
        payload = {"embeds": [embed]}
        response = requests.post(DISCORD_WEBHOOK, json=payload)
        return response.status_code == 204

    # Calculate strategy averages
    num_strategies = len(stats['strategy_counts'])
    avg_opps_per_strategy = stats['total_opps'] / num_strategies if num_strategies > 0 else 0
    avg_profit_per_strategy = stats['total_profit'] / num_strategies if num_strategies > 0 else 0

    # Format top opportunities (now deduplicated with counts)
    top_3_text = ""
    for i, opp in enumerate(stats['top_3'][:3], 1):
        if opp['count'] > 1:
            # Show: pair | $single × count = $total | spread
            top_3_text += f"**#{i}** {opp['pair']} | ${opp['single_profit']:.2f} × {opp['count']} = **${opp['total_profit']:.2f}** | {opp['midmarket']:.2f}%\n"
        else:
            # Single occurrence - simpler display
            top_3_text += f"**#{i}** {opp['pair']} | ${opp['single_profit']:.2f} | {opp['midmarket']:.2f}% spread\n"
        top_3_text += f"    └ {opp['route']}\n"

    # Strategy leaderboard
    strat_counts = stats['strategy_counts']
    strat_profits = stats['strategy_profits']

    strategy_text = ""
    for strat in sorted(strat_profits.keys(), key=lambda x: strat_profits[x], reverse=True):
        strategy_text += f"• **{strat}**: {strat_counts[strat]} opps, ${strat_profits[strat]:.2f}\n"

    # Build Discord embeds
    embeds = [
        {
            "title": "📊 DEX Arbitrage Paper Trading Report",
            "description": f"**Hourly Summary** • Auto-generated",
            "color": 0x5865F2,  # Discord blurple
            "fields": [
                {
                    "name": "⚙️ General",
                    "value": f"**Timestamp:** {pacific_now.strftime('%Y-%m-%d %H:%M')} PT\n"
                             f"**Version:** V2+V3 Multi-Strategy\n"
                             f"**Period:** {period_start.strftime('%H:%M')}-{period_end.strftime('%H:%M')} UTC\n"
                             f"**Pools:** {v2_pools} V2 + {v3_pools} V3",
                    "inline": False
                }
            ],
            "footer": {"text": "DEX Arbitrage Bot • Polygon Mainnet • Paper Trading"}
        },
        {
            "title": "🎯 Opportunity Overview",
            "color": 0x57F287,  # Green
            "fields": [
                {
                    "name": "Total Opportunities",
                    "value": f"**{stats['total_opps']}**",
                    "inline": True
                },
                {
                    "name": "Unique Pairs/Routes",
                    "value": f"**{len(stats['unique_pairs'])}** pairs, **{stats['unique_routes']}** routes\n({', '.join(stats['unique_pairs'][:5])})",
                    "inline": True
                },
                {
                    "name": "V2 vs V3 Split",
                    "value": f"V2: **{stats['v2_opps']}** ({stats['v2_opps']*100//max(stats['total_opps'],1)}%)\n"
                             f"V3: **{stats['v3_opps']}** ({stats['v3_opps']*100//max(stats['total_opps'],1)}%)",
                    "inline": True
                }
            ]
        },
        {
            "title": "💰 Profit Summary",
            "color": 0xFEE75C,  # Yellow
            "fields": [
                {
                    "name": "Total Potential",
                    "value": f"**${stats['total_profit']:.2f}**",
                    "inline": True
                },
                {
                    "name": "Best Single Trade",
                    "value": f"**${stats['best_trade']:.2f}**",
                    "inline": True
                },
                {
                    "name": "Average per Opp",
                    "value": f"**${stats['avg_profit']:.2f}**",
                    "inline": True
                },
                {
                    "name": "📈 Estimated Realized",
                    "value": f"**${stats['estimated_profit']:.2f}**\n"
                             f"_(60% win × 90% slippage)_\n"
                             f"Profitable: {stats['profitable_count']}/{stats['total_opps']}",
                    "inline": False
                }
            ]
        },
        {
            "title": "🏆 Top 3 Opportunities",
            "color": 0xEB459E,  # Fuchsia
            "fields": [
                {
                    "name": "Best Trades",
                    "value": top_3_text if top_3_text else "No profitable trades",
                    "inline": False
                }
            ]
        },
        {
            "title": "📈 Strategy Performance",
            "color": 0x5865F2,  # Blurple
            "fields": [
                {
                    "name": "Most Opportunities",
                    "value": f"**{stats['most_opps_strategy']}** ({strat_counts.get(stats['most_opps_strategy'], 0)})",
                    "inline": True
                },
                {
                    "name": "Most Profit",
                    "value": f"**{stats['most_profit_strategy']}** (${strat_profits.get(stats['most_profit_strategy'], 0):.2f})",
                    "inline": True
                },
                {
                    "name": "Averages",
                    "value": f"Opps: **{avg_opps_per_strategy:.1f}**/strat\nProfit: **${avg_profit_per_strategy:.2f}**/strat",
                    "inline": True
                },
                {
                    "name": "Strategy Breakdown",
                    "value": strategy_text[:1000] if strategy_text else "N/A",
                    "inline": False
                }
            ],
            "timestamp": now.isoformat()
        }
    ]

    payload = {"embeds": embeds}

    try:
        response = requests.post(DISCORD_WEBHOOK, json=payload)
        return response.status_code == 204
    except Exception as e:
        print(f"Error sending to Discord: {e}")
        return False


def get_next_hour_start():
    """Get the start of the next hour."""
    now = datetime.now(UTC)
    next_hour = now.replace(minute=0, second=0, microsecond=0) + timedelta(hours=1)
    return next_hour


def get_midnight_pacific():
    """Get the next midnight Pacific time."""
    now = datetime.now(PACIFIC)
    midnight = now.replace(hour=0, minute=0, second=0, microsecond=0)
    if midnight <= now:
        midnight += timedelta(days=1)
    return midnight.astimezone(UTC)


def main():
    """Main loop - publish reports every hour."""
    print("=" * 60)
    print("Hourly Discord Report Publisher")
    print("=" * 60)
    print(f"TMUX Session: {TMUX_SESSION}:{TMUX_WINDOW}")
    print(f"Report Interval: {REPORT_INTERVAL_HOURS} hour(s)")
    print(f"Webhook: ...{DISCORD_WEBHOOK[-20:]}")
    print("=" * 60)

    # Calculate time until next report (midnight Pacific for first run)
    now = datetime.now(UTC)
    pacific_now = now.astimezone(PACIFIC)

    # If it's before midnight Pacific, wait until midnight
    # Otherwise, start at the next hour
    next_midnight = get_midnight_pacific()
    next_hour = get_next_hour_start()

    if next_midnight < next_hour + timedelta(hours=1):
        next_report = next_midnight
        print(f"First report scheduled for: {next_report.astimezone(PACIFIC).strftime('%Y-%m-%d %H:%M')} PT (midnight)")
    else:
        next_report = next_hour
        print(f"First report scheduled for: {next_report.astimezone(PACIFIC).strftime('%Y-%m-%d %H:%M')} PT")

    wait_seconds = (next_report - now).total_seconds()
    print(f"Waiting {wait_seconds/60:.1f} minutes until first report...")
    print("-" * 60)

    while True:
        # Wait until next report time
        now = datetime.now(UTC)
        wait_seconds = (next_report - now).total_seconds()

        if wait_seconds > 0:
            time.sleep(min(wait_seconds, 60))  # Check every minute
            continue

        # Time to generate report
        period_end = next_report
        period_start = period_end - timedelta(hours=REPORT_INTERVAL_HOURS)

        print(f"\n[{datetime.now(PACIFIC).strftime('%Y-%m-%d %H:%M:%S')} PT] Generating report...")
        print(f"  Period: {period_start.strftime('%H:%M')} - {period_end.strftime('%H:%M')} UTC")

        # Capture and parse
        raw_output = capture_tmux_output(lines=5000)
        opportunities = parse_opportunities(raw_output, period_start)
        print(f"  Opportunities found: {len(opportunities)}")

        # Calculate stats
        stats = calculate_stats(opportunities) if opportunities else None

        # Send report
        success = send_discord_report(stats, period_start, period_end)

        if success:
            print(f"  ✅ Report sent to Discord")
        else:
            print(f"  ❌ Failed to send report")

        # Schedule next report
        next_report = period_end + timedelta(hours=REPORT_INTERVAL_HOURS)
        print(f"  Next report: {next_report.astimezone(PACIFIC).strftime('%Y-%m-%d %H:%M')} PT")
        print("-" * 60)


if __name__ == "__main__":
    main()
