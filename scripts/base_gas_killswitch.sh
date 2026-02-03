#!/usr/bin/env bash
#
# Script Name: base_gas_killswitch.sh
# Purpose: Monitor Base wallet ETH balance and kill the bot if gas spend exceeds $1
# Author: AI-Generated
# Created: 2026-02-03
#
# Usage:
#   ./scripts/base_gas_killswitch.sh
#   Runs in a loop, checks ETH balance every 30s.
#   Kills the tmux session "base-bot" if ETH drops below threshold.
#
# Configuration:
#   STARTING_ETH: captured at script start
#   MAX_GAS_USD: $1.00
#   ETH_PRICE: $3300 (conservative)
#
# Dependencies:
#   - cast (foundry)
#   - tmux
#   - bc

set -euo pipefail

readonly SCRIPT_NAME="$(basename "${BASH_SOURCE[0]}")"
readonly WALLET="0x48091E0ee0427A7369c7732f779a09A0988144fa"
readonly RPC="https://mainnet.base.org"
readonly TMUX_SESSION="base-bot"
readonly CHECK_INTERVAL=30  # seconds between checks
readonly MAX_GAS_USD=1.00
readonly ETH_PRICE=3300
readonly LOG_FILE="/home/botuser/bots/dexarb/data/base/logs/killswitch_$(date +%Y%m%d).log"

# Calculate kill threshold
STARTING_ETH=$(cast balance "$WALLET" --rpc-url "$RPC" --ether 2>/dev/null)
MAX_GAS_ETH=$(echo "scale=18; $MAX_GAS_USD / $ETH_PRICE" | bc)
KILL_THRESHOLD=$(echo "scale=18; $STARTING_ETH - $MAX_GAS_ETH" | bc)

mkdir -p "$(dirname "$LOG_FILE")"

log() {
    local msg="[$(date '+%Y-%m-%d %H:%M:%S')] $1"
    echo "$msg"
    echo "$msg" >> "$LOG_FILE"
}

log "========================================="
log "$SCRIPT_NAME started"
log "Wallet:         $WALLET"
log "Starting ETH:   $STARTING_ETH"
log "Max gas spend:  \$$MAX_GAS_USD ($MAX_GAS_ETH ETH at \$$ETH_PRICE)"
log "Kill threshold: $KILL_THRESHOLD ETH"
log "Check interval: ${CHECK_INTERVAL}s"
log "Tmux session:   $TMUX_SESSION"
log "========================================="

check_count=0
while true; do
    sleep "$CHECK_INTERVAL"
    check_count=$((check_count + 1))

    CURRENT_ETH=$(cast balance "$WALLET" --rpc-url "$RPC" --ether 2>/dev/null || echo "error")

    if [ "$CURRENT_ETH" = "error" ]; then
        log "WARNING: RPC call failed (check #$check_count), retrying next cycle"
        continue
    fi

    GAS_SPENT_ETH=$(echo "scale=18; $STARTING_ETH - $CURRENT_ETH" | bc)
    GAS_SPENT_USD=$(echo "scale=4; $GAS_SPENT_ETH * $ETH_PRICE" | bc)

    # Periodic status log (every 10 checks = ~5 min)
    if [ $((check_count % 10)) -eq 0 ]; then
        log "STATUS: ETH=$CURRENT_ETH | gas_spent=\$$GAS_SPENT_USD ($GAS_SPENT_ETH ETH) | check #$check_count"
    fi

    # Kill check
    BELOW=$(echo "$CURRENT_ETH < $KILL_THRESHOLD" | bc -l)
    if [ "$BELOW" -eq 1 ]; then
        log "!!! KILL THRESHOLD REACHED !!!"
        log "Current ETH:  $CURRENT_ETH"
        log "Threshold:    $KILL_THRESHOLD"
        log "Gas spent:    \$$GAS_SPENT_USD ($GAS_SPENT_ETH ETH)"
        log "Killing tmux session: $TMUX_SESSION"

        tmux kill-session -t "$TMUX_SESSION" 2>/dev/null && \
            log "Session $TMUX_SESSION killed successfully" || \
            log "WARNING: Failed to kill session (may already be stopped)"

        log "Killswitch exiting."
        exit 0
    fi
done
