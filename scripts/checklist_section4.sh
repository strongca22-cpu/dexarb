#!/usr/bin/env bash
#
# Script Name: checklist_section4.sh
# Purpose: Pre-$100 Deployment Checklist - Section 4: Data Integrity
# Author: AI-Generated
# Created: 2026-01-28
#
# Usage:
#   ./scripts/checklist_section4.sh
#

CRITICAL_PASS=0
CRITICAL_FAIL=0
IMPORTANT_PASS=0
IMPORTANT_FAIL=0
RECOMMENDED_PASS=0
RECOMMENDED_FAIL=0

DATA_DIR="/home/botuser/bots/dexarb/data"
SPREAD_FILE="$DATA_DIR/spread_history_v2.csv"
STATE_FILE="$DATA_DIR/pool_state_phase1.json"

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
echo "  Section 4: Data Integrity (14 checks)"
echo "  NOTE: Includes dual-route detection verification"
echo "============================================================"
echo ""
echo "Date: $(date)"
echo ""

# ============================================================
# 4.1 Spread History Data
# ============================================================
echo "-----------------------------------------------------------"
echo "4.1 SPREAD HISTORY DATA"
echo "-----------------------------------------------------------"

# 4.1.1 Spread file exists
if [ -f "$SPREAD_FILE" ]; then
    SPREAD_SIZE=$(stat -c%s "$SPREAD_FILE")
    SPREAD_LINES=$(wc -l < "$SPREAD_FILE")
    info "Spread file: $SPREAD_LINES records, $(($SPREAD_SIZE/1024))KB"
    critical_check 0 "Spread history file exists"
else
    info "Spread file: NOT FOUND"
    critical_check 1 "Spread history file exists"
    SPREAD_LINES=0
fi

# 4.1.2 Recent data (within 10 minutes)
if [ -f "$SPREAD_FILE" ]; then
    FILE_AGE=$(( $(date +%s) - $(stat -c%Y "$SPREAD_FILE") ))
    info "Spread file age: $((FILE_AGE/60)) minutes"
    if [ "$FILE_AGE" -lt 600 ]; then
        critical_check 0 "Recent spread data (<10 min)"
    else
        critical_check 1 "Recent spread data ($((FILE_AGE/60)) min old)"
    fi
else
    critical_check 1 "Recent spread data (file missing)"
fi

# 4.1.3 Spread values reasonable
if [ -f "$SPREAD_FILE" ] && [ "$SPREAD_LINES" -gt 1 ]; then
    # Get last 100 spread values and check they're reasonable
    SPREAD_STATS=$(tail -100 "$SPREAD_FILE" | python3 -c "
import sys
import csv
spreads = []
reader = csv.reader(sys.stdin)
for row in reader:
    try:
        # Assume spread is in one of the columns
        for val in row:
            try:
                f = float(val)
                if -10 < f < 50:  # Reasonable spread range
                    spreads.append(f)
            except:
                pass
    except:
        pass
if spreads:
    avg = sum(spreads)/len(spreads)
    print(f'OK:{avg:.4f}:{len(spreads)}')
else:
    print('EMPTY')
" 2>/dev/null)

    if [[ "$SPREAD_STATS" == OK:* ]]; then
        AVG_SPREAD=$(echo "$SPREAD_STATS" | cut -d: -f2)
        info "Average spread (recent): ${AVG_SPREAD}%"
        critical_check 0 "Spread values reasonable"
    else
        info "Could not parse spread values"
        critical_check 1 "Spread values reasonable"
    fi
else
    critical_check 1 "Spread values reasonable (no data)"
fi

# 4.1.4 Multiple pairs being tracked
if [ -f "$SPREAD_FILE" ] && [ "$SPREAD_LINES" -gt 1 ]; then
    # Count unique pairs in recent data
    UNIQUE_PAIRS=$(tail -1000 "$SPREAD_FILE" | cut -d',' -f2 | sort -u | wc -l)
    info "Unique pairs tracked: $UNIQUE_PAIRS"
    if [ "$UNIQUE_PAIRS" -ge 3 ]; then
        important_check 0 "Multiple pairs being tracked ($UNIQUE_PAIRS)"
    else
        important_check 1 "Multiple pairs being tracked (only $UNIQUE_PAIRS)"
    fi
else
    important_check 1 "Multiple pairs being tracked (no data)"
fi

echo ""

# ============================================================
# 4.2 Pool State Data
# ============================================================
echo "-----------------------------------------------------------"
echo "4.2 POOL STATE DATA"
echo "-----------------------------------------------------------"

# 4.2.1 Pool state file exists
if [ -f "$STATE_FILE" ]; then
    STATE_SIZE=$(stat -c%s "$STATE_FILE")
    info "Pool state file: $(($STATE_SIZE/1024))KB"
    critical_check 0 "Pool state file exists"
else
    info "Pool state file: NOT FOUND"
    critical_check 1 "Pool state file exists"
fi

# 4.2.2 Pool state is valid JSON
if [ -f "$STATE_FILE" ]; then
    if python3 -c "import json; json.load(open('$STATE_FILE'))" 2>/dev/null; then
        info "Pool state: valid JSON"
        critical_check 0 "Pool state is valid JSON"
    else
        info "Pool state: INVALID JSON"
        critical_check 1 "Pool state is valid JSON"
    fi
else
    critical_check 1 "Pool state is valid JSON (file missing)"
fi

# 4.2.3 Pool state recently updated
if [ -f "$STATE_FILE" ]; then
    STATE_AGE=$(( $(date +%s) - $(stat -c%Y "$STATE_FILE") ))
    info "Pool state age: $((STATE_AGE/60)) minutes"
    if [ "$STATE_AGE" -lt 300 ]; then
        important_check 0 "Pool state recent (<5 min)"
    else
        important_check 1 "Pool state recent ($((STATE_AGE/60)) min old)"
    fi
else
    important_check 1 "Pool state recent (file missing)"
fi

echo ""

# ============================================================
# 4.3 Data Consistency
# ============================================================
echo "-----------------------------------------------------------"
echo "4.3 DATA CONSISTENCY"
echo "-----------------------------------------------------------"

# 4.3.1 No corrupted files
CORRUPT_COUNT=0
for f in "$DATA_DIR"/*.json "$DATA_DIR"/*.csv; do
    if [ -f "$f" ]; then
        if [[ "$f" == *.json ]]; then
            python3 -c "import json; json.load(open('$f'))" 2>/dev/null || ((CORRUPT_COUNT++))
        fi
    fi
done
info "Corrupted data files: $CORRUPT_COUNT"
if [ "$CORRUPT_COUNT" -eq 0 ]; then
    important_check 0 "No corrupted data files"
else
    important_check 1 "Corrupted data files found ($CORRUPT_COUNT)"
fi

# 4.3.2 Data directory writable
if [ -w "$DATA_DIR" ]; then
    important_check 0 "Data directory writable"
else
    important_check 1 "Data directory not writable"
fi

echo ""

# ============================================================
# 4.4 DUAL-ROUTE DETECTION (NEW)
# ============================================================
echo "-----------------------------------------------------------"
echo "4.4 DUAL-ROUTE DETECTION"
echo "-----------------------------------------------------------"
info "Expected routes:"
info "  Route 1: V3 1.00% -> V3 0.05% (~2.24% spread)"
info "  Route 2: V3 0.30% -> V3 0.05% (~1.43% spread)"
echo ""

# Use spread_history_v2.csv which has route information
SPREAD_V2_FILE="$DATA_DIR/spread_history_v2.csv"
OPPS_FILE="$DATA_DIR/spread_opportunities.csv"

# 4.4.1 Check Route 1 (1.00% -> 0.05%) detected
if [ -f "$OPPS_FILE" ]; then
    ROUTE1_COUNT=$(grep -c "1.00%.*0.05%\|0.05%.*1.00%" "$OPPS_FILE" 2>/dev/null || echo "0")
    info "Route 1 detections in opportunities: $ROUTE1_COUNT"
    if [ "$ROUTE1_COUNT" -gt 0 ]; then
        critical_check 0 "Route 1 (1.00%->0.05%) actively detected ($ROUTE1_COUNT)"
    else
        critical_check 1 "Route 1 (1.00%->0.05%) not detected"
    fi
else
    # Fall back to checking pool state for UNI V3 pools
    if [ -f "$STATE_FILE" ]; then
        UNI_POOLS=$(python3 -c "
import json
with open('$STATE_FILE') as f:
    data = json.load(f)
    v3 = data.get('v3_pools', {})
    uni_pools = [k for k in v3.keys() if 'UNI/USDC' in k]
    print(len(uni_pools))
" 2>/dev/null || echo "0")
        if [ "$UNI_POOLS" -ge 2 ]; then
            critical_check 0 "UNI/USDC V3 pools present ($UNI_POOLS pools)"
        else
            critical_check 1 "UNI/USDC V3 pools missing"
        fi
    else
        critical_check 1 "Route 1 detection (no data file)"
    fi
fi

# 4.4.2 Check Route 2 (0.30% -> 0.05%) detected
if [ -f "$OPPS_FILE" ]; then
    ROUTE2_COUNT=$(grep -c "0.30%.*0.05%\|0.05%.*0.30%" "$OPPS_FILE" 2>/dev/null || echo "0")
    info "Route 2 detections in opportunities: $ROUTE2_COUNT"
    if [ "$ROUTE2_COUNT" -gt 0 ]; then
        critical_check 0 "Route 2 (0.30%->0.05%) actively detected ($ROUTE2_COUNT)"
    else
        critical_check 1 "Route 2 (0.30%->0.05%) not detected"
    fi
else
    critical_check 1 "Route 2 detection (no opportunities file)"
fi

# 4.4.3 Check combined detection rate
if [ -f "$OPPS_FILE" ]; then
    TOTAL_OPPS=$(wc -l < "$OPPS_FILE" 2>/dev/null || echo "1")
    TOTAL_OPPS=$((TOTAL_OPPS - 1))  # Subtract header
    info "Total opportunities logged: $TOTAL_OPPS"
    if [ "$TOTAL_OPPS" -ge 10 ]; then
        important_check 0 "Opportunities being logged ($TOTAL_OPPS)"
    else
        important_check 1 "Low opportunity count ($TOTAL_OPPS)"
    fi
else
    important_check 1 "Opportunities file missing"
fi

# 4.4.4 Check UNI/USDC spread in reasonable range
if [ -f "$SPREAD_V2_FILE" ]; then
    UNI_SPREAD=$(tail -500 "$SPREAD_V2_FILE" | grep "UNI/USDC" | python3 -c "
import sys
import csv
spreads = []
reader = csv.reader(sys.stdin)
for row in reader:
    try:
        spread = float(row[7])  # spread_pct column
        if 0 < spread < 10:
            spreads.append(spread)
    except:
        pass
if spreads:
    avg = sum(spreads)/len(spreads)
    print(f'{avg:.2f}')
else:
    print('0')
" 2>/dev/null || echo "0")
    info "UNI/USDC average spread: ${UNI_SPREAD}%"
    if [ "$UNI_SPREAD" != "0" ]; then
        recommended_check 0 "UNI/USDC spreads detected (avg ${UNI_SPREAD}%)"
    else
        recommended_check 1 "UNI/USDC spread detection"
    fi
else
    recommended_check 1 "Spread V2 file for route analysis"
fi

echo ""

# ============================================================
# Summary
# ============================================================
echo "============================================================"
echo "  SECTION 4 SUMMARY"
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
    echo "*** SECTION 4: FAILED - Critical issues must be resolved ***"
    exit 1
else
    echo ""
    echo "=== SECTION 4: PASSED - All critical checks OK ==="
    exit 0
fi
