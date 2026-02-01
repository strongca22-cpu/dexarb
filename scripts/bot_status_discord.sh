#!/usr/bin/env bash
#
# Script Name: bot_status_discord.sh
# Purpose: Send live bot status report to Discord every 30 minutes (aligned to :00/:30)
# Author: AI-Generated
# Created: 2026-01-30
# Modified: 2026-01-30 - Fix log path, trade counting, clock-aligned 30min schedule
# Modified: 2026-01-31 - Multi-chain naming (livebot.polygon), prominent status indicator
# Modified: 2026-02-01 - Multi-chain report: separate Polygon (LIVE) + Base (DRY-RUN) sections
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
#   - python3 (JSON escaping)
#   - cast (wallet balance queries)
#

set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly BOT_DIR="$(dirname "$SCRIPT_DIR")"
readonly WEBHOOK_URL="$(grep 'DISCORD_WEBHOOK=' "$BOT_DIR/src/rust-bot/.env" 2>/dev/null | cut -d'=' -f2-)"

# Polygon config
readonly POL_WALLET="0xa532eb528ae17efc881fce6894a08b5b70ff21e2"
readonly POL_USDC_ADDR="0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"
readonly POL_RPC_URL="https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"
readonly POL_LOG_DIR="$BOT_DIR/data/polygon/logs"

# Base config
readonly BASE_WALLET="0x48091E0ee0427A7369c7732f779a09A0988144fa"
readonly BASE_USDC_ADDR="0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"
readonly BASE_RPC_URL="https://base-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"
readonly BASE_LOG_DIR="$BOT_DIR/data/base/logs"

# Globals set by collect_log_stats()
_LOG_LINES=0
_LATEST_STATUS="No log file"
_ATTEMPTS=0
_COMPLETED=0
_HALTS=0

send_discord() {
    local msg="$1"
    local escaped
    escaped=$(python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))" <<< "$msg")
    curl -s -o /dev/null -w "%{http_code}" \
        -H "Content-Type: application/json" \
        -d "{\"content\": ${escaped}}" \
        "$WEBHOOK_URL"
}

# Find PID for a specific chain's dexarb-bot process
find_chain_pid() {
    local chain="$1"
    pgrep -a dexarb-bot 2>/dev/null | grep -- "--chain $chain" | awk '{print $1}' | head -1 || echo ""
}

# Build process detail string for a PID
build_proc_detail() {
    local pid="$1"
    if [ -n "$pid" ]; then
        local ctx mem
        ctx=$(grep "^voluntary_ctxt_switches" /proc/"$pid"/status 2>/dev/null | awk '{print $2}' || echo "?")
        mem=$(grep "VmRSS" /proc/"$pid"/status 2>/dev/null | awk '{print $2, $3}' || echo "?")
        echo "PID $pid | ${mem} | ctx=$ctx"
    else
        echo "no process"
    fi
}

# Collect log stats into global variables (avoids delimiter issues with | in log lines)
collect_log_stats() {
    local log_dir="$1"
    local LOG_FILE
    LOG_FILE="$(ls -t "$log_dir"/livebot*.log 2>/dev/null | head -1 || echo "")"

    if [ -z "$LOG_FILE" ] || [ ! -f "$LOG_FILE" ]; then
        _LOG_LINES=0
        _LATEST_STATUS="No log file"
        _ATTEMPTS=0
        _COMPLETED=0
        _HALTS=0
        return
    fi

    _LOG_LINES=$(wc -l < "$LOG_FILE" 2>/dev/null || echo "0")
    _LATEST_STATUS=$(grep "Iteration" "$LOG_FILE" 2>/dev/null | tail -1 | sed 's/\x1b\[[0-9;]*m//g' | sed 's/^.*INFO //' || echo "No iteration log yet")
    _ATTEMPTS=$(grep -c "TRY #" "$LOG_FILE" 2>/dev/null || true)
    _ATTEMPTS=${_ATTEMPTS:-0}
    _COMPLETED=$(grep -c "Trade complete" "$LOG_FILE" 2>/dev/null || true)
    _COMPLETED=${_COMPLETED:-0}
    _HALTS=$(grep -c "HALT" "$LOG_FILE" 2>/dev/null || true)
    _HALTS=${_HALTS:-0}
}

build_report() {
    local now
    now=$(date -u '+%Y-%m-%d %H:%M:%S UTC')

    # Tmux sessions
    local sessions
    sessions=$(tmux list-sessions 2>/dev/null | cut -d: -f1 | tr '\n' ', ' | sed 's/,$//' || echo "NONE")

    # --- POLYGON (LIVE) ---
    local pol_pid pol_proc pol_status
    pol_pid=$(find_chain_pid "polygon")
    pol_proc=$(build_proc_detail "$pol_pid")
    [ -n "$pol_pid" ] && pol_status="LIVE" || pol_status="DOWN"

    collect_log_stats "$POL_LOG_DIR"
    local pol_log_lines="$_LOG_LINES"
    local pol_latest="$_LATEST_STATUS"
    local pol_attempts="$_ATTEMPTS"
    local pol_completed="$_COMPLETED"
    local pol_halts="$_HALTS"

    # Polygon wallet
    local pol_usdc pol_matic pol_usdc_raw
    pol_usdc_raw=$(timeout 10 cast call "$POL_USDC_ADDR" "balanceOf(address)(uint256)" "$POL_WALLET" --rpc-url "$POL_RPC_URL" 2>/dev/null | head -1 | awk '{print $1}' || echo "0")
    pol_usdc=$(python3 -c "print(f'{int(\"${pol_usdc_raw}\") / 1e6:.2f}')" 2>/dev/null || echo "?")
    pol_matic=$(timeout 10 cast balance "$POL_WALLET" --rpc-url "$POL_RPC_URL" --ether 2>/dev/null | head -1 || echo "?")
    pol_matic=$(python3 -c "print(f'{float(\"${pol_matic}\"):.2f}')" 2>/dev/null || echo "$pol_matic")

    # --- BASE (DRY-RUN) ---
    local base_pid base_proc base_status
    base_pid=$(find_chain_pid "base")
    base_proc=$(build_proc_detail "$base_pid")
    [ -n "$base_pid" ] && base_status="DRY-RUN" || base_status="DOWN"

    collect_log_stats "$BASE_LOG_DIR"
    local base_log_lines="$_LOG_LINES"
    local base_latest="$_LATEST_STATUS"
    local base_attempts="$_ATTEMPTS"
    local base_completed="$_COMPLETED"
    local base_halts="$_HALTS"

    # Base wallet (ETH + USDC)
    local base_usdc base_eth base_usdc_raw
    base_usdc_raw=$(timeout 10 cast call "$BASE_USDC_ADDR" "balanceOf(address)(uint256)" "$BASE_WALLET" --rpc-url "$BASE_RPC_URL" 2>/dev/null | head -1 | awk '{print $1}' || echo "0")
    base_usdc=$(python3 -c "print(f'{int(\"${base_usdc_raw}\") / 1e6:.2f}')" 2>/dev/null || echo "?")
    base_eth=$(timeout 10 cast balance "$BASE_WALLET" --rpc-url "$BASE_RPC_URL" --ether 2>/dev/null | head -1 || echo "?")
    base_eth=$(python3 -c "print(f'{float(\"${base_eth}\"):.6f}')" 2>/dev/null || echo "$base_eth")

    # Build combined message
    cat <<EOF
**livebot.polygon** [$pol_status] — \`$now\`
\`\`\`
Process:   $pol_proc
Wallet:    $pol_usdc USDC | $pol_matic MATIC
Log lines: $pol_log_lines
Attempts:  $pol_attempts  Completed: $pol_completed  HALTs: $pol_halts
Latest:    $pol_latest
\`\`\`
**dryrun.base** [$base_status] — paper trading, unfunded
\`\`\`
Process:   $base_proc
Wallet:    $base_usdc USDC | $base_eth ETH (gas only)
Log lines: $base_log_lines
Attempts:  $base_attempts  Completed: $base_completed  HALTs: $base_halts
Latest:    $base_latest
\`\`\`
\`Sessions: $sessions\`
EOF
}

# Main
if [ -z "$WEBHOOK_URL" ]; then
    echo "ERROR: No DISCORD_WEBHOOK found in .env"
    exit 1
fi

if [ "${1:-}" = "--loop" ]; then
    echo "Starting status loop (every 30 min, aligned to :00/:30)..."
    echo "Reporting: Polygon (LIVE) + Base (DRY-RUN)"
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
