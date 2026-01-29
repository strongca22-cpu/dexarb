#!/usr/bin/env bash
#
# Script Name: stop_after_trade.sh
# Purpose: Monitor live bot log and kill it after one successful trade or on-chain failure
# Author: AI-Generated
# Created: 2026-01-29
# Modified: 2026-01-29 - Allow Quoter rejections (keep scanning)
#
# Usage:
#   ./scripts/stop_after_trade.sh
#
# Watches data/bot_live.log for events that involve on-chain capital.
#
# STOP triggers (capital at risk or deployed):
#   - "Trade complete"  — both legs succeeded (verify results)
#   - "Buy swap failed" — buy tx sent on-chain (assess damage)
#   - "Sell swap failed" — sell tx sent after buy succeeded (assess damage)
#   - "Execution error" — unexpected error during execution
#
# CONTINUE triggers (logged but bot keeps scanning):
#   - "Trade failed"    — includes V3 Quoter rejections (zero capital, safe)
#
# On STOP trigger: kills the live-bot tmux session and sets LIVE_MODE=false
#

set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
readonly LOG_FILE="${PROJECT_DIR}/data/bot_live.log"
readonly ENV_FILE="${PROJECT_DIR}/src/rust-bot/.env"
readonly TMUX_SESSION="live-bot"

echo "========================================"
echo "  DEX Arb Bot — One-Trade Watcher"
echo "========================================"
echo "Log file:     ${LOG_FILE}"
echo "Tmux session: ${TMUX_SESSION}"
echo "STOP on:      Trade complete | Buy/Sell swap failed | Execution error"
echo "CONTINUE on:  Trade failed (Quoter rejections — zero capital)"
echo "Action:       kill bot + set LIVE_MODE=false"
echo "========================================"
echo ""

# Wait for log file to exist
echo "[$(date '+%H:%M:%S')] Waiting for log file to appear..."
while [ ! -f "$LOG_FILE" ]; do
    sleep 1
done
echo "[$(date '+%H:%M:%S')] Log file found. Monitoring..."

# Monitor log for trade execution events
#
# STOP patterns (on-chain capital involved):
#   "Trade complete"  — both legs succeeded
#   "Buy swap failed" — buy tx was sent on-chain
#   "Sell swap failed" — sell tx was sent (buy already succeeded)
#   "Execution error" — unexpected error
#
# CONTINUE patterns (zero capital risk):
#   "Trade failed"    — includes Quoter rejections; no tx was sent
#
readonly STOP_PATTERN="(Trade complete|Buy swap failed|Sell swap failed|Execution error)"
readonly INFO_PATTERN="Trade failed"

quoter_rejections=0

tail -f "$LOG_FILE" | while IFS= read -r line; do
    # Check for STOP events first (on-chain capital at risk)
    if echo "$line" | grep -qE "$STOP_PATTERN"; then
        echo ""
        echo "========================================"
        echo "  TRADE EVENT DETECTED — STOPPING BOT"
        echo "========================================"
        echo "[$(date '+%H:%M:%S')] Trigger line:"
        echo "  $line"
        echo ""
        echo "[$(date '+%H:%M:%S')] Quoter rejections before this event: ${quoter_rejections}"

        # Give 5 seconds for any final logging (tax records, etc.)
        echo "[$(date '+%H:%M:%S')] Waiting 5s for logging to flush..."
        sleep 5

        # Kill the bot
        echo "[$(date '+%H:%M:%S')] Killing tmux session: ${TMUX_SESSION}"
        tmux kill-session -t "$TMUX_SESSION" 2>/dev/null || true

        # Set LIVE_MODE back to false for safety
        echo "[$(date '+%H:%M:%S')] Setting LIVE_MODE=false in .env"
        sed -i 's/^LIVE_MODE=true/LIVE_MODE=false/' "$ENV_FILE"

        echo ""
        echo "========================================"
        echo "  BOT STOPPED"
        echo "========================================"
        echo "[$(date '+%H:%M:%S')] Check the full log: tail -100 ${LOG_FILE}"
        echo "[$(date '+%H:%M:%S')] Check tax records:  ls -la ${PROJECT_DIR}/data/tax/"
        echo ""

        # Exit the watcher
        exit 0

    # Check for INFO events (Quoter rejections — safe, keep scanning)
    elif echo "$line" | grep -q "$INFO_PATTERN"; then
        quoter_rejections=$((quoter_rejections + 1))
        echo "[$(date '+%H:%M:%S')] Quoter rejection #${quoter_rejections} (safe — zero capital). Bot continues scanning."
        echo "  $line"
    fi
done
