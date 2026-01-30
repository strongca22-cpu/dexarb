#!/usr/bin/env bash
#
# Script Name: bot_watch.sh
# Purpose: Monitor live bot log and kill it after first trade execution
# Author: AI-Generated
# Created: 2026-01-30
#
# Usage:
#   ./scripts/bot_watch.sh
#
# Watches /home/botuser/bots/dexarb/data/livebot.log for trade execution
# patterns. Kills the livebot tmux session when a trade is detected.
#
# Detected patterns:
#   - "TRY #"       = trade attempt (Quoter verified, executing)
#   - "Trade complete" = successful trade
#   - "HALT"         = on-chain tx submitted (capital committed)
#

set -euo pipefail

LOG_FILE="/home/botuser/bots/dexarb/data/logs/livebot_ws.log"
TMUX_SESSION="livebot"

echo "=== Bot Watch ==="
echo "Monitoring: $LOG_FILE"
echo "Kill target: tmux session '$TMUX_SESSION'"
echo "Trigger: first trade attempt (TRY #), completion, or HALT"
echo "Started: $(date)"
echo ""
echo "Waiting for trade activity..."

# Follow log file, grep for trade patterns, kill on first match
tail -n 0 -f "$LOG_FILE" | while IFS= read -r line; do
    if echo "$line" | grep -qE "TRY #|Trade complete|HALT"; then
        echo ""
        echo "!!! TRADE DETECTED at $(date) !!!"
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
