#!/usr/bin/env bash
#
# Script Name: monitor_trade.sh
# Purpose: Monitor the arbitrage bot for completed trades and stop after first trade
# Author: AI-Generated
# Created: 2026-01-28
#
# Usage:
#   ./scripts/monitor_trade.sh
#
# Notes:
#   - Checks tax log directory for new trade records every 15 seconds
#   - Stops the bot (kills dexarb-bot) once a trade is detected
#   - Also monitors bot log output for "Trade complete" messages
#

set -euo pipefail

BOT_DIR="/home/botuser/bots/dexarb"
TAX_DIR="$BOT_DIR/data/tax"
LOG_FILE="$BOT_DIR/data/bot_live.log"
CHECK_INTERVAL=15

echo "========================================================"
echo "  LIVE TRADE MONITOR"
echo "  Started: $(date)"
echo "========================================================"
echo ""
echo "Monitoring for trades every ${CHECK_INTERVAL}s..."
echo "Tax directory: $TAX_DIR"
echo "Bot log: $LOG_FILE"
echo ""

# Get initial state
INITIAL_CSV_LINES=0
INITIAL_JSON_LINES=0

if [ -f "$TAX_DIR/trades_2026.csv" ]; then
    INITIAL_CSV_LINES=$(wc -l < "$TAX_DIR/trades_2026.csv" 2>/dev/null || echo "0")
fi

if [ -f "$TAX_DIR/trades_2026.jsonl" ]; then
    INITIAL_JSON_LINES=$(wc -l < "$TAX_DIR/trades_2026.jsonl" 2>/dev/null || echo "0")
fi

echo "Initial state:"
echo "  CSV lines: $INITIAL_CSV_LINES"
echo "  JSON lines: $INITIAL_JSON_LINES"
echo ""

ITERATION=0

while true; do
    ITERATION=$((ITERATION + 1))
    echo "[$(date '+%H:%M:%S')] Check #$ITERATION..."

    # Method 1: Check tax log files for new entries
    CURRENT_CSV_LINES=0
    CURRENT_JSON_LINES=0

    if [ -f "$TAX_DIR/trades_2026.csv" ]; then
        CURRENT_CSV_LINES=$(wc -l < "$TAX_DIR/trades_2026.csv" 2>/dev/null || echo "0")
    fi

    if [ -f "$TAX_DIR/trades_2026.jsonl" ]; then
        CURRENT_JSON_LINES=$(wc -l < "$TAX_DIR/trades_2026.jsonl" 2>/dev/null || echo "0")
    fi

    if [ "$CURRENT_CSV_LINES" -gt "$INITIAL_CSV_LINES" ] || [ "$CURRENT_JSON_LINES" -gt "$INITIAL_JSON_LINES" ]; then
        echo ""
        echo "========================================================"
        echo "  TRADE DETECTED!"
        echo "  Time: $(date)"
        echo "========================================================"
        echo ""
        echo "Tax records increased:"
        echo "  CSV: $INITIAL_CSV_LINES -> $CURRENT_CSV_LINES"
        echo "  JSON: $INITIAL_JSON_LINES -> $CURRENT_JSON_LINES"
        echo ""

        # Show the latest trade
        if [ "$CURRENT_CSV_LINES" -gt "$INITIAL_CSV_LINES" ] && [ -f "$TAX_DIR/trades_2026.csv" ]; then
            echo "Latest trade from CSV:"
            tail -1 "$TAX_DIR/trades_2026.csv" | head -c 500
            echo ""
        fi

        # Stop the bot
        echo ""
        echo "Stopping bot..."
        pkill -f "dexarb-bot" 2>/dev/null && echo "Bot stopped." || echo "Bot may have already stopped."

        # Send notification
        echo ""
        echo "========================================================"
        echo "  ONE TRADE COMPLETE - BOT STOPPED"
        echo "========================================================"

        exit 0
    fi

    # Method 2: Check log file for "Trade complete" message
    if [ -f "$LOG_FILE" ]; then
        if grep -q "Trade complete" "$LOG_FILE" 2>/dev/null; then
            TRADE_LINE=$(grep "Trade complete" "$LOG_FILE" | tail -1)
            echo ""
            echo "========================================================"
            echo "  TRADE DETECTED IN LOG!"
            echo "  Time: $(date)"
            echo "========================================================"
            echo ""
            echo "Trade line: $TRADE_LINE"
            echo ""

            # Stop the bot
            echo "Stopping bot..."
            pkill -f "dexarb-bot" 2>/dev/null && echo "Bot stopped." || echo "Bot may have already stopped."

            exit 0
        fi
    fi

    # Show bot status
    if pgrep -f "dexarb-bot" > /dev/null 2>&1; then
        echo "  Bot running: YES"
    else
        echo "  Bot running: NO - may have crashed or stopped"
        echo ""
        echo "Bot is no longer running. Exiting monitor."
        exit 1
    fi

    # Show last log line if available
    if [ -f "$LOG_FILE" ]; then
        LAST_LOG=$(tail -1 "$LOG_FILE" 2>/dev/null | head -c 100)
        echo "  Last log: $LAST_LOG..."
    fi

    echo ""
    sleep $CHECK_INTERVAL
done
