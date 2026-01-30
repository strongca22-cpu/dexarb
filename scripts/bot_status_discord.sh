#!/usr/bin/env bash
#
# Script Name: bot_status_discord.sh
# Purpose: Send live bot status report to Discord every 15 minutes
# Author: AI-Generated
# Created: 2026-01-30
#
# Usage:
#   # One-shot:
#   ./scripts/bot_status_discord.sh
#
#   # Loop (run in tmux):
#   ./scripts/bot_status_discord.sh --loop
#
# Dependencies:
#   - curl (for Discord webhook)
#   - jq or python3 (JSON escaping)
#

set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly BOT_DIR="$(dirname "$SCRIPT_DIR")"
readonly LOG_FILE="$BOT_DIR/data/livebot.log"
readonly WEBHOOK_URL="$(grep 'DISCORD_WEBHOOK=' "$BOT_DIR/src/rust-bot/.env" 2>/dev/null | cut -d'=' -f2-)"
readonly INTERVAL_SEC=1800  # 30 minutes

send_discord() {
    local msg="$1"
    # Escape for JSON
    local escaped
    escaped=$(python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))" <<< "$msg")
    curl -s -o /dev/null -w "%{http_code}" \
        -H "Content-Type: application/json" \
        -d "{\"content\": ${escaped}}" \
        "$WEBHOOK_URL"
}

build_report() {
    local now
    now=$(date -u '+%Y-%m-%d %H:%M:%S UTC')

    # Tmux sessions
    local sessions
    sessions=$(tmux list-sessions 2>/dev/null | cut -d: -f1 | tr '\n' ', ' | sed 's/,$//' || echo "NONE")

    # Bot process
    local pid status
    pid=$(pgrep -x dexarb-bot 2>/dev/null | head -1 || echo "")
    if [ -n "$pid" ]; then
        local ctx mem
        ctx=$(grep "^voluntary_ctxt_switches" /proc/"$pid"/status 2>/dev/null | awk '{print $2}' || echo "?")
        mem=$(grep "VmRSS" /proc/"$pid"/status 2>/dev/null | awk '{print $2, $3}' || echo "?")
        status="PID $pid | ${mem} | ctx=$ctx"
    else
        status="NOT RUNNING"
    fi

    # Latest status line from log
    local latest_status log_lines
    log_lines=$(wc -l < "$LOG_FILE" 2>/dev/null || echo "0")
    latest_status=$(grep "Iteration" "$LOG_FILE" 2>/dev/null | tail -1 | sed 's/\x1b\[[0-9;]*m//g' | sed 's/^.*INFO //' || echo "No iteration log yet")

    # Check for trade activity
    local trades
    trades=$(grep -c "TRY #\|Trade complete\|HALT" "$LOG_FILE" 2>/dev/null || true)
    trades=${trades:-0}

    # Build message
    cat <<EOF
**Bot Status Report** — \`$now\`
\`\`\`
Sessions:  $sessions
Process:   $status
Log lines: $log_lines
Trades:    $trades
Latest:    $latest_status
\`\`\`
EOF
}

# Main
if [ -z "$WEBHOOK_URL" ]; then
    echo "ERROR: No DISCORD_WEBHOOK found in .env"
    exit 1
fi

if [ "${1:-}" = "--loop" ]; then
    echo "Starting status loop (every ${INTERVAL_SEC}s)..."
    while true; do
        report=$(build_report)
        http_code=$(send_discord "$report")
        echo "[$(date -u '+%H:%M:%S')] Sent status (HTTP $http_code)"
        sleep "$INTERVAL_SEC"
    done
else
    report=$(build_report)
    http_code=$(send_discord "$report")
    echo "Sent status (HTTP $http_code)"
    echo "$report"
fi
