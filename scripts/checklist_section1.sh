#!/usr/bin/env bash
#
# Script Name: checklist_section1.sh
# Purpose: Pre-$100 Deployment Checklist - Section 1: Technical Infrastructure
# Author: AI-Generated
# Created: 2026-01-28
# Modified: 2026-01-30 - V3 shared-data architecture, whitelist, Multicall3
# Modified: 2026-01-30 - Monolithic architecture (state file checks demoted to RECOMMENDED)
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
echo "  Section 1: Technical Infrastructure"
echo "  Architecture: V3 monolithic (direct RPC sync, Multicall3)"
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
[ "$DISK_AVAIL_GB" -ge 5 ] && critical_check 0 "Disk space >5GB free" || critical_check 1 "Disk space >5GB free (only ${DISK_AVAIL_GB}GB)"

# 1.1.3 Memory
MEM_AVAIL=$(free -m | awk '/^Mem:/ {print $7}')
info "Memory available: ${MEM_AVAIL}MB"
[ "$MEM_AVAIL" -ge 200 ] && critical_check 0 "Memory >200MB available" || critical_check 1 "Memory >200MB available (only ${MEM_AVAIL}MB)"

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

# 1.2.2 Primary RPC (Alchemy HTTP — used by checklist scripts)
RPC_URL="https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"
if curl -s --max-time 5 -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | grep -q "result"; then
    info "Primary RPC (Alchemy HTTPS): OK"
    critical_check 0 "Primary RPC reachable"
else
    info "Primary RPC: FAILED"
    critical_check 1 "Primary RPC reachable"
fi

# 1.2.3 Response time < 2s
START=$(date +%s%N)
curl -s --max-time 5 -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' > /dev/null 2>&1
END=$(date +%s%N)
LATENCY_MS=$(( (END - START) / 1000000 ))
info "RPC latency: ${LATENCY_MS}ms"
[ "$LATENCY_MS" -lt 2000 ] && important_check 0 "RPC response <2s (${LATENCY_MS}ms)" || important_check 1 "RPC response <2s (${LATENCY_MS}ms)"

echo ""

# ============================================================
# 1.3 Whitelist & State File Health
# ============================================================
echo "-----------------------------------------------------------"
echo "1.3 WHITELIST & STATE FILE HEALTH"
echo "-----------------------------------------------------------"
echo "  (Monolithic bot syncs directly via RPC — state file is"
echo "   only used by data collector for paper trading/research)"

# 1.3.1 Whitelist file exists and has active pools (CRITICAL — live bot requires it)
WHITELIST_FILE="/home/botuser/bots/dexarb/config/pools_whitelist.json"
if [ -f "$WHITELIST_FILE" ]; then
    WL_ACTIVE_COUNT=$(python3 -c "
import json
with open('$WHITELIST_FILE') as f:
    data = json.load(f)
    pools = data.get('whitelist', {}).get('pools', [])
    active = [p for p in pools if p.get('status') == 'active']
    print(len(active))
" 2>/dev/null || echo "0")
    info "Whitelist active pools: $WL_ACTIVE_COUNT"
    if [ "$WL_ACTIVE_COUNT" -gt 0 ]; then
        critical_check 0 "Whitelist file exists with $WL_ACTIVE_COUNT active pools"
    else
        critical_check 1 "Whitelist file has no active pools"
    fi
else
    critical_check 1 "Whitelist file missing ($WHITELIST_FILE)"
fi

# 1.3.2 Whitelist is valid JSON
if [ -f "$WHITELIST_FILE" ]; then
    if python3 -c "import json; json.load(open('$WHITELIST_FILE'))" 2>/dev/null; then
        critical_check 0 "Whitelist is valid JSON"
    else
        critical_check 1 "Whitelist is INVALID JSON"
    fi
fi

# 1.3.3 Whitelist enforcement mode is strict
if [ -f "$WHITELIST_FILE" ]; then
    ENFORCEMENT=$(python3 -c "
import json
with open('$WHITELIST_FILE') as f:
    data = json.load(f)
print(data.get('config', {}).get('whitelist_enforcement', 'unknown'))
" 2>/dev/null || echo "unknown")
    info "Whitelist enforcement: $ENFORCEMENT"
    if [ "$ENFORCEMENT" = "strict" ]; then
        critical_check 0 "Whitelist enforcement = strict"
    else
        critical_check 1 "Whitelist enforcement not strict ($ENFORCEMENT)"
    fi
fi

# 1.3.4 State file exists (RECOMMENDED — only for paper trading data collector)
STATE_FILE="/home/botuser/bots/dexarb/data/pool_state_phase1.json"
STATE_SIZE_KB=0
if [ -f "$STATE_FILE" ]; then
    STATE_SIZE_KB=$(( $(stat -c%s "$STATE_FILE") / 1024 ))
    info "State file: ${STATE_SIZE_KB}KB (paper trading only)"
    recommended_check 0 "Pool state JSON exists (paper trading)"
else
    info "State file: NOT FOUND (ok — live bot syncs directly)"
    recommended_check 0 "Pool state JSON not required for monolithic bot"
fi

# 1.3.5 State file reasonable size (if it exists)
if [ -f "$STATE_FILE" ] && [ "$STATE_SIZE_KB" -lt 10240 ]; then
    recommended_check 0 "State file <10MB (${STATE_SIZE_KB}KB)"
elif [ -f "$STATE_FILE" ]; then
    recommended_check 1 "State file too large (${STATE_SIZE_KB}KB)"
fi

echo ""

# ============================================================
# 1.4 Bot Service Health
# ============================================================
echo "-----------------------------------------------------------"
echo "1.4 BOT SERVICE HEALTH"
echo "-----------------------------------------------------------"

# 1.4.1 Live bot binary exists
BOT_BINARY="/home/botuser/bots/dexarb/src/rust-bot/target/release/dexarb-bot"
if [ -x "$BOT_BINARY" ]; then
    BOT_SIZE_MB=$(( $(stat -c%s "$BOT_BINARY") / 1024 / 1024 ))
    info "Live bot binary: ${BOT_SIZE_MB}MB"
    critical_check 0 "Live bot binary exists (release)"
else
    info "Live bot binary: NOT FOUND"
    critical_check 1 "Live bot binary exists (release)"
fi

# 1.4.2 Data collector binary exists
DC_BINARY="/home/botuser/bots/dexarb/src/rust-bot/target/release/data-collector"
if [ -x "$DC_BINARY" ]; then
    DC_SIZE_MB=$(( $(stat -c%s "$DC_BINARY") / 1024 / 1024 ))
    info "Data collector binary: ${DC_SIZE_MB}MB"
    critical_check 0 "Data collector binary exists (release)"
else
    info "Data collector binary: NOT FOUND"
    critical_check 1 "Data collector binary exists (release)"
fi

# 1.4.3 .env.live config exists
ENV_LIVE="/home/botuser/bots/dexarb/src/rust-bot/.env.live"
if [ -f "$ENV_LIVE" ]; then
    info ".env.live: exists"
    critical_check 0 "Live config .env.live exists"
else
    info ".env.live: NOT FOUND"
    critical_check 1 "Live config .env.live exists"
fi

# 1.4.4 Unit tests pass (42/42)
TEST_RESULT=$(/home/botuser/.cargo/bin/cargo test --manifest-path /home/botuser/bots/dexarb/src/rust-bot/Cargo.toml 2>&1 | tail -5)
if echo "$TEST_RESULT" | grep -q "0 failed"; then
    PASS_COUNT=$(echo "$TEST_RESULT" | grep -oP '\d+ passed' | head -1)
    info "Tests: $PASS_COUNT"
    important_check 0 "Unit tests pass ($PASS_COUNT)"
else
    info "Tests: FAILURES DETECTED"
    important_check 1 "Unit tests have failures"
fi

# 1.4.5 Cargo build is current (binary newer than source)
if [ -x "$BOT_BINARY" ]; then
    BOT_AGE=$(stat -c%Y "$BOT_BINARY")
    NEWEST_SRC=$(find /home/botuser/bots/dexarb/src/rust-bot/src -name "*.rs" -printf '%T@\n' 2>/dev/null | sort -n | tail -1 | cut -d. -f1)
    NEWEST_SRC=${NEWEST_SRC:-0}
    if [ "$BOT_AGE" -ge "$NEWEST_SRC" ]; then
        recommended_check 0 "Binary is up-to-date with source"
    else
        recommended_check 1 "Binary older than source — rebuild needed"
    fi
else
    recommended_check 1 "Binary freshness check (missing)"
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
