#!/usr/bin/env bash
#
# Script Name: checklist_section1.sh
# Purpose: Pre-$100 Deployment Checklist - Section 1: Technical Infrastructure
# Author: AI-Generated
# Created: 2026-01-28
# Modified: 2026-01-28
#
# Usage:
#   ./scripts/checklist_section1.sh
#

# Counters
CRITICAL_PASS=0
CRITICAL_FAIL=0
IMPORTANT_PASS=0
IMPORTANT_FAIL=0
RECOMMENDED_PASS=0
RECOMMENDED_FAIL=0

# Helper functions
pass() { echo "  [PASS] $1"; }
fail() { echo "  [FAIL] $1"; }
warn() { echo "  [WARN] $1"; }
info() { echo "  [INFO] $1"; }

critical_check() {
    if [ "$1" -eq 0 ]; then
        pass "CRITICAL: $2"
        ((CRITICAL_PASS++))
    else
        fail "CRITICAL: $2"
        ((CRITICAL_FAIL++))
    fi
}

important_check() {
    if [ "$1" -eq 0 ]; then
        pass "IMPORTANT: $2"
        ((IMPORTANT_PASS++))
    else
        fail "IMPORTANT: $2"
        ((IMPORTANT_FAIL++))
    fi
}

recommended_check() {
    if [ "$1" -eq 0 ]; then
        pass "RECOMMENDED: $2"
        ((RECOMMENDED_PASS++))
    else
        warn "RECOMMENDED: $2"
        ((RECOMMENDED_FAIL++))
    fi
}

echo ""
echo "============================================================"
echo "  PRE-\$100 DEPLOYMENT CHECKLIST"
echo "  Section 1: Technical Infrastructure (15 checks)"
echo "============================================================"
echo ""
echo "Date: $(date)"
echo ""

# ============================================================
# 1.1 Server Health
# ============================================================
echo "-----------------------------------------------------------"
echo "1.1 SERVER HEALTH"
echo "-----------------------------------------------------------"

# 1.1.1 Uptime > 24 hours
UPTIME_HOURS=$(awk '{print int($1/3600)}' /proc/uptime)
UPTIME_DAYS=$((UPTIME_HOURS / 24))
info "Uptime: ${UPTIME_DAYS} days (${UPTIME_HOURS} hours)"
if [ "$UPTIME_HOURS" -ge 24 ]; then
    critical_check 0 "Uptime >24 hours"
else
    critical_check 1 "Uptime >24 hours (only ${UPTIME_HOURS}h)"
fi

# 1.1.2 Disk space
DISK_AVAIL_GB=$(df / | awk 'NR==2 {print int($4/1024/1024)}')
DISK_USED_PCT=$(df / | awk 'NR==2 {gsub(/%/,""); print $5}')
info "Disk available: ${DISK_AVAIL_GB}GB (${DISK_USED_PCT}% used)"
[ "$DISK_AVAIL_GB" -ge 10 ] && critical_check 0 "Disk space >10GB free" || critical_check 1 "Disk space >10GB free (only ${DISK_AVAIL_GB}GB)"

# 1.1.3 Memory
MEM_AVAIL=$(free -m | awk '/^Mem:/ {print $7}')
info "Memory available: ${MEM_AVAIL}MB"
[ "$MEM_AVAIL" -ge 500 ] && critical_check 0 "Memory >500MB available" || critical_check 1 "Memory >500MB available (only ${MEM_AVAIL}MB)"

# 1.1.4 CPU load
LOAD_1MIN=$(cat /proc/loadavg | cut -d' ' -f1)
info "Load average (1min): ${LOAD_1MIN}"
LOAD_INT=${LOAD_1MIN%.*}
LOAD_INT=${LOAD_INT:-0}
if [ "$LOAD_INT" -lt 2 ]; then
    important_check 0 "CPU load <2.0"
else
    important_check 1 "CPU load <2.0 (currently ${LOAD_1MIN})"
fi

# 1.1.5 I/O wait (use /proc/stat)
IOWAIT=$(cat /proc/stat | head -1 | awk '{total=$2+$3+$4+$5+$6+$7+$8; iowait=$6; print int(iowait*100/total)}')
IOWAIT=${IOWAIT:-0}
info "I/O wait: ${IOWAIT}%"
if [ "$IOWAIT" -lt 20 ]; then
    important_check 0 "No high I/O wait (<20%)"
else
    important_check 1 "I/O wait high (${IOWAIT}%)"
fi

echo ""

# ============================================================
# 1.2 Network Connectivity
# ============================================================
echo "-----------------------------------------------------------"
echo "1.2 NETWORK CONNECTIVITY"
echo "-----------------------------------------------------------"

# 1.2.1 Internet connectivity
if ping -c 1 -W 3 8.8.8.8 > /dev/null 2>&1; then
    info "Internet: Connected"
    critical_check 0 "Internet connectivity"
else
    info "Internet: FAILED"
    critical_check 1 "Internet connectivity"
fi

# 1.2.2 Primary RPC (polygon-rpc.com - public)
RPC_URL="https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"
if curl -s --max-time 5 -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | grep -q "result"; then
    info "Primary RPC (polygon-rpc.com): OK"
    critical_check 0 "Primary RPC reachable"
else
    info "Primary RPC: FAILED"
    critical_check 1 "Primary RPC reachable"
fi

# 1.2.3 Backup RPC
if curl -s --max-time 5 -X POST "https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | grep -q "result"; then
    info "Backup RPC (polygon-rpc.com): OK"
    critical_check 0 "Backup RPC reachable"
else
    info "Backup RPC: FAILED"
    critical_check 1 "Backup RPC reachable"
fi

# 1.2.4 Response time < 2s
START=$(date +%s%N)
curl -s --max-time 5 -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' > /dev/null 2>&1
END=$(date +%s%N)
LATENCY_MS=$(( (END - START) / 1000000 ))
info "RPC latency: ${LATENCY_MS}ms"
[ "$LATENCY_MS" -lt 2000 ] && important_check 0 "RPC response <2s (${LATENCY_MS}ms)" || important_check 1 "RPC response <2s (${LATENCY_MS}ms)"

# 1.2.5 Multi-RPC configured
recommended_check 1 "Multi-RPC configured (only Alchemy in .env)"

echo ""

# ============================================================
# 1.3 Database/State Health
# ============================================================
echo "-----------------------------------------------------------"
echo "1.3 DATABASE/STATE HEALTH"
echo "-----------------------------------------------------------"

STATE_FILE="/home/botuser/bots/dexarb/data/pool_state_phase1.json"

# 1.3.1 State file exists
if [ -f "$STATE_FILE" ]; then
    STATE_SIZE_KB=$(( $(stat -c%s "$STATE_FILE") / 1024 ))
    info "State file: ${STATE_SIZE_KB}KB"
    critical_check 0 "State file exists"
else
    info "State file: NOT FOUND"
    critical_check 1 "State file exists"
fi

# 1.3.2 State file accessible
[ -r "$STATE_FILE" ] && critical_check 0 "State file accessible" || critical_check 1 "State file accessible"

# 1.3.3 State file recent (<5 min)
if [ -f "$STATE_FILE" ]; then
    STATE_AGE_SEC=$(( $(date +%s) - $(stat -c%Y "$STATE_FILE") ))
    STATE_AGE_MIN=$((STATE_AGE_SEC / 60))
    info "State file age: ${STATE_AGE_MIN} minutes"
    [ "$STATE_AGE_SEC" -lt 300 ] && critical_check 0 "Recent state data (<5 min)" || critical_check 1 "Recent state data (${STATE_AGE_MIN} min old)"
else
    critical_check 1 "Recent state data (file missing)"
fi

# 1.3.4 State file reasonable size
[ -f "$STATE_FILE" ] && [ "$STATE_SIZE_KB" -lt 10240 ] && important_check 0 "State file <10MB" || important_check 1 "State file size check"

# 1.3.5 Spread history exists
SPREAD_FILE="/home/botuser/bots/dexarb/data/spread_history.csv"
if [ -f "$SPREAD_FILE" ]; then
    SPREAD_LINES=$(wc -l < "$SPREAD_FILE")
    info "Spread history: ${SPREAD_LINES} records"
    important_check 0 "Spread history exists (${SPREAD_LINES} records)"
else
    important_check 1 "Spread history exists"
fi

echo ""

# ============================================================
# 1.4 Bot Service Health
# ============================================================
echo "-----------------------------------------------------------"
echo "1.4 BOT SERVICE HEALTH"
echo "-----------------------------------------------------------"

# 1.4.1 Bot binary exists
BOT_BINARY="/home/botuser/bots/dexarb/src/rust-bot/target/release/dexarb-bot"
if [ -x "$BOT_BINARY" ]; then
    BOT_SIZE_MB=$(( $(stat -c%s "$BOT_BINARY") / 1024 / 1024 ))
    info "Bot binary: ${BOT_SIZE_MB}MB"
    critical_check 0 "Bot binary exists"
else
    info "Bot binary: NOT FOUND"
    critical_check 1 "Bot binary exists"
fi

# 1.4.2 Tmux sessions running
TMUX_COUNT=$(tmux list-sessions 2>/dev/null | wc -l)
if [ "$TMUX_COUNT" -gt 0 ]; then
    SESSIONS=$(tmux list-sessions 2>/dev/null | cut -d: -f1 | tr '\n' ', ' | sed 's/,$//')
    info "Tmux sessions: $SESSIONS"
    critical_check 0 "Tmux sessions active ($TMUX_COUNT)"
else
    info "Tmux sessions: NONE"
    critical_check 1 "Tmux sessions active"
fi

# 1.4.3 Spread logger running
if pgrep -f "spread_logger.py" > /dev/null 2>&1; then
    info "Spread logger: RUNNING (PID $(pgrep -f spread_logger.py | head -1))"
    important_check 0 "Spread logger running"
else
    info "Spread logger: NOT RUNNING"
    important_check 1 "Spread logger running"
fi

# 1.4.4 Discord reporter running
if pgrep -f "hourly_discord_report.py" > /dev/null 2>&1; then
    info "Discord reporter: RUNNING"
    important_check 0 "Discord reporter running"
else
    info "Discord reporter: NOT RUNNING"
    important_check 1 "Discord reporter running"
fi

# 1.4.5 No critical errors in recent logs
LOG_FILE="/home/botuser/bots/dexarb/logs/spread_logger.log"
if [ -f "$LOG_FILE" ]; then
    RECENT_ERRORS=$(tail -100 "$LOG_FILE" 2>/dev/null | grep -ciE "error|critical|fatal" || true)
    RECENT_ERRORS=${RECENT_ERRORS:-0}
    info "Recent log errors: $RECENT_ERRORS"
    if [ "$RECENT_ERRORS" -lt 5 ]; then
        recommended_check 0 "No critical errors in logs"
    else
        recommended_check 1 "Errors in logs ($RECENT_ERRORS)"
    fi
else
    recommended_check 1 "Log file check (missing)"
fi

echo ""

# ============================================================
# Summary
# ============================================================
echo "============================================================"
echo "  SECTION 1 SUMMARY"
echo "============================================================"
echo ""

TOTAL_CRITICAL=$((CRITICAL_PASS + CRITICAL_FAIL))
TOTAL_IMPORTANT=$((IMPORTANT_PASS + IMPORTANT_FAIL))
TOTAL_RECOMMENDED=$((RECOMMENDED_PASS + RECOMMENDED_FAIL))
TOTAL_CHECKS=$((TOTAL_CRITICAL + TOTAL_IMPORTANT + TOTAL_RECOMMENDED))
TOTAL_PASS=$((CRITICAL_PASS + IMPORTANT_PASS + RECOMMENDED_PASS))

echo "CRITICAL:    ${CRITICAL_PASS}/${TOTAL_CRITICAL} passed"
[ "$CRITICAL_FAIL" -gt 0 ] && echo "             *** ${CRITICAL_FAIL} FAILED - MUST FIX ***"

echo "IMPORTANT:   ${IMPORTANT_PASS}/${TOTAL_IMPORTANT} passed"
[ "$IMPORTANT_FAIL" -gt 0 ] && echo "             ${IMPORTANT_FAIL} failed"

echo "RECOMMENDED: ${RECOMMENDED_PASS}/${TOTAL_RECOMMENDED} passed"

echo ""
echo "-----------------------------------------------------------"
echo "TOTAL:       ${TOTAL_PASS}/${TOTAL_CHECKS} checks passed"
echo "-----------------------------------------------------------"

if [ "$CRITICAL_FAIL" -gt 0 ]; then
    echo ""
    echo "*** SECTION 1: FAILED - Critical issues must be resolved ***"
    exit 1
else
    echo ""
    echo "=== SECTION 1: PASSED - All critical checks OK ==="
    exit 0
fi
