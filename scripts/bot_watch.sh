#!/usr/bin/env bash
#
# Script Name: bot_watch.sh
# Purpose: Monitor live bot log and kill it after first completed trade
# Author: AI-Generated
# Created: 2026-01-30
# Modified: 2026-01-31 - Renamed session target to livebot.polygon (multi-chain)
#
# Usage:
#   ./scripts/bot_watch.sh
#
# Watches livebot_ws.log for trade completion patterns.
# Kills the livebot tmux session only when a trade actually executes on-chain.
# Trade *attempts* (TRY #, gas rejections, quoter rejections) are allowed
# to proceed without interruption.
#
# Trigger: "Trade complete" only (main.rs:520)
#   Logged AFTER: receipt confirmed, profit computed, tax record written.
#   Only fires on result.success == true (profitable trade).
#
# NOT triggered by:
#   - "ATOMIC PROFIT"  = fires before tax logging (race risk), now ignored
#   - "ATOMIC LOSS"    = gas-negative trade, bot keeps running to recover
#   - "HALT"           = receipt timeout or legacy mode failure
#   - Quoter rejections, estimateGas reverts, atomic on-chain reverts
#   - Atomic reverts are safe: contract revert protects capital, only gas burned
#

set -euo pipefail

LOG_FILE="/home/botuser/bots/dexarb/data/polygon/logs/livebot_ws.log"
TMUX_SESSION="livebot_polygon"

echo "=== Bot Watch ==="
echo "Monitoring: $LOG_FILE"
echo "Kill target: tmux session '$TMUX_SESSION'"
echo "Trigger: first profitable trade (\"Trade complete\" — after tax log)"
echo "Started: $(date)"
echo ""
echo "Waiting for on-chain trade activity (attempts are allowed)..."

# Follow log file, grep for completion patterns, kill on first match
tail -n 0 -f "$LOG_FILE" | while IFS= read -r line; do
    if echo "$line" | grep -qF "Trade complete"; then
        echo ""
        echo "!!! PROFITABLE TRADE at $(date) !!!"
        echo "Line: $line"
        echo ""
        echo "Killing tmux session '$TMUX_SESSION'..."
        tmux kill-session -t "$TMUX_SESSION" 2>/dev/null && echo "Session killed." || echo "Session already dead."
        echo ""
        echo "Bot watch complete. Review the log:"
        echo "  tail -50 $LOG_FILE"
        exit 0
    fi
done
