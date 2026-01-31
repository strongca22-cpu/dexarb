#!/usr/bin/env bash
#
# Script Name: bot_status_discord.sh
# Purpose: Send live bot status report to Discord every 30 minutes (aligned to :00/:30)
# Author: AI-Generated
# Created: 2026-01-30
# Modified: 2026-01-30 - Fix log path, trade counting, clock-aligned 30min schedule
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
readonly LOG_FILE="$BOT_DIR/data/logs/livebot_ws.log"
readonly WEBHOOK_URL="$(grep 'DISCORD_WEBHOOK=' "$BOT_DIR/src/rust-bot/.env" 2>/dev/null | cut -d'=' -f2-)"
readonly WALLET="0xa532eb528ae17efc881fce6894a08b5b70ff21e2"
readonly USDC_ADDR="0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"
readonly RPC_URL="https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"

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

    # Check for trade activity (separate attempts from completions)
    local attempts completed halts
    attempts=$(grep -c "TRY #" "$LOG_FILE" 2>/dev/null || true)
    attempts=${attempts:-0}
    completed=$(grep -c "Trade complete" "$LOG_FILE" 2>/dev/null || true)
    completed=${completed:-0}
    halts=$(grep -c "HALT" "$LOG_FILE" 2>/dev/null || true)
    halts=${halts:-0}

    # Wallet balances (via cast, with timeout to avoid hanging)
    local usdc_bal matic_bal
    local usdc_raw
    usdc_raw=$(timeout 10 cast call "$USDC_ADDR" "balanceOf(address)(uint256)" "$WALLET" --rpc-url "$RPC_URL" 2>/dev/null | head -1 | awk '{print $1}' || echo "0")
    usdc_bal=$(python3 -c "print(f'{int(\"${usdc_raw}\") / 1e6:.2f}')" 2>/dev/null || echo "?")
    matic_bal=$(timeout 10 cast balance "$WALLET" --rpc-url "$RPC_URL" --ether 2>/dev/null | head -1 || echo "?")
    # Trim MATIC to 2 decimal places
    matic_bal=$(python3 -c "print(f'{float(\"${matic_bal}\"):.2f}')" 2>/dev/null || echo "$matic_bal")

    # Build message
    cat <<EOF
**Bot Status Report** — \`$now\`
\`\`\`
Sessions:  $sessions
Process:   $status
Wallet:    $usdc_bal USDC | $matic_bal MATIC
Log lines: $log_lines
Attempts:  $attempts  Completed: $completed  HALTs: $halts
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
    echo "Starting status loop (every 30 min, aligned to :00/:30)..."
    while true; do
        report=$(build_report)
        http_code=$(send_discord "$report")
        echo "[$(date -u '+%H:%M:%S')] Sent status (HTTP $http_code)"
        # Sleep until next :00 or :30 mark
        min=$(date '+%M' | sed 's/^0//')
        if [ "$min" -lt 30 ]; then
            wait_sec=$(( (30 - min) * 60 - $(date '+%S' | sed 's/^0//') ))
        else
            wait_sec=$(( (60 - min) * 60 - $(date '+%S' | sed 's/^0//') ))
        fi
        # Minimum 60s to avoid rapid-fire on edge cases
        [ "$wait_sec" -lt 60 ] && wait_sec=60
        echo "  Next report in ${wait_sec}s (at next :00/:30)"
        sleep "$wait_sec"
    done
else
    report=$(build_report)
    http_code=$(send_discord "$report")
    echo "Sent status (HTTP $http_code)"
    echo "$report"
fi
