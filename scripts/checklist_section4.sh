#!/usr/bin/env bash
#
# Script Name: checklist_section4.sh
# Purpose: Pre-$100 Deployment Checklist - Section 4: Data Integrity
# Author: AI-Generated
# Created: 2026-01-28
# Modified: 2026-01-30 - V3 shared-data architecture, whitelist verification, no PostgreSQL
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
STATE_FILE="$DATA_DIR/pool_state_phase1.json"
WHITELIST_FILE="/home/botuser/bots/dexarb/config/pools_whitelist.json"

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
echo "  Section 4: Data Integrity"
echo "  Shared-data architecture: JSON pool state + whitelist"
echo "============================================================"
echo ""
echo "Date: $(date)"
echo ""

# ============================================================
# 4.1 Pool State JSON Integrity
# ============================================================
echo "-----------------------------------------------------------"
echo "4.1 POOL STATE JSON INTEGRITY"
echo "-----------------------------------------------------------"

# 4.1.1 State file exists and is valid JSON
if [ -f "$STATE_FILE" ]; then
    STATE_SIZE=$(stat -c%s "$STATE_FILE")
    info "Pool state file: $(($STATE_SIZE/1024))KB"
    if python3 -c "import json; json.load(open('$STATE_FILE'))" 2>/dev/null; then
        critical_check 0 "Pool state is valid JSON"
    else
        critical_check 1 "Pool state is INVALID JSON"
    fi
else
    info "Pool state file: NOT FOUND"
    critical_check 1 "Pool state file exists"
fi

# 4.1.2 State file recently updated (data collector writing)
if [ -f "$STATE_FILE" ]; then
    STATE_AGE=$(( $(date +%s) - $(stat -c%Y "$STATE_FILE") ))
    info "Pool state age: $((STATE_AGE/60)) minutes"
    if [ "$STATE_AGE" -lt 120 ]; then
        critical_check 0 "Pool state fresh (<2 min, data collector active)"
    else
        critical_check 1 "Pool state stale ($((STATE_AGE/60)) min — data collector down?)"
    fi
else
    critical_check 1 "Pool state freshness (file missing)"
fi

# 4.1.3 State has V3 pools with prices
if [ -f "$STATE_FILE" ]; then
    V3_STATS=$(python3 -c "
import json
with open('$STATE_FILE') as f:
    data = json.load(f)
v3 = data.get('v3_pools', {})
block = data.get('block_number', 0)
prices_ok = 0
for k, p in v3.items():
    price = p.get('price', 0)
    if price and price > 0 and price < 1e15:
        prices_ok += 1
print(f'{len(v3)}|{prices_ok}|{block}')
" 2>/dev/null || echo "0|0|0")

    V3_TOTAL=$(echo "$V3_STATS" | cut -d'|' -f1)
    V3_PRICED=$(echo "$V3_STATS" | cut -d'|' -f2)
    BLOCK_NUM=$(echo "$V3_STATS" | cut -d'|' -f3)
    info "V3 pools: $V3_TOTAL total, $V3_PRICED with valid prices"
    info "Block number: $BLOCK_NUM"

    if [ "$V3_TOTAL" -gt 0 ] && [ "$V3_PRICED" -gt 0 ]; then
        critical_check 0 "V3 pools have price data ($V3_PRICED/$V3_TOTAL priced)"
    else
        critical_check 1 "V3 pools missing price data"
    fi

    if [ "$BLOCK_NUM" -gt 0 ]; then
        important_check 0 "Block number tracked ($BLOCK_NUM)"
    else
        important_check 1 "Block number missing from state"
    fi
else
    critical_check 1 "V3 pool data (file missing)"
fi

echo ""

# ============================================================
# 4.2 Whitelist Data Consistency
# ============================================================
echo "-----------------------------------------------------------"
echo "4.2 WHITELIST DATA CONSISTENCY"
echo "-----------------------------------------------------------"

# 4.2.1 Whitelist file is valid JSON
if [ -f "$WHITELIST_FILE" ]; then
    if python3 -c "import json; json.load(open('$WHITELIST_FILE'))" 2>/dev/null; then
        critical_check 0 "Whitelist file is valid JSON"
    else
        critical_check 1 "Whitelist file is INVALID JSON"
    fi
else
    critical_check 1 "Whitelist file missing"
fi

# 4.2.2 Whitelist has enforcement mode set to strict
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

# 4.2.3 Whitelisted pools appear in state file
if [ -f "$STATE_FILE" ] && [ -f "$WHITELIST_FILE" ]; then
    WL_IN_STATE=$(python3 -c "
import json
with open('$STATE_FILE') as f:
    state = json.load(f)
with open('$WHITELIST_FILE') as f:
    wl = json.load(f)
v3_addrs = set()
for k, p in state.get('v3_pools', {}).items():
    addr = p.get('address', '').lower()
    if addr:
        v3_addrs.add(addr)
wl_pools = wl.get('whitelist', {}).get('pools', [])
found = 0
total = 0
for p in wl_pools:
    addr = p.get('address', '').lower()
    if addr:
        total += 1
        if addr in v3_addrs:
            found += 1
print(f'{found}|{total}')
" 2>/dev/null || echo "0|0")

    WL_FOUND=$(echo "$WL_IN_STATE" | cut -d'|' -f1)
    WL_TOTAL=$(echo "$WL_IN_STATE" | cut -d'|' -f2)
    info "Whitelisted pools in state: $WL_FOUND/$WL_TOTAL"
    if [ "$WL_TOTAL" -gt 0 ] && [ "$WL_FOUND" -gt 0 ]; then
        important_check 0 "$WL_FOUND/$WL_TOTAL whitelisted pools present in state"
    else
        important_check 1 "No whitelisted pools found in state data"
    fi
else
    important_check 1 "Whitelist-state cross-check (files missing)"
fi

# 4.2.4 Liquidity thresholds configured
if [ -f "$WHITELIST_FILE" ]; then
    THRESHOLDS=$(python3 -c "
import json
with open('$WHITELIST_FILE') as f:
    data = json.load(f)
t = data.get('config', {}).get('liquidity_thresholds', {})
if t:
    parts = [f'{k}={v}' for k,v in t.items()]
    print('|'.join(parts))
else:
    print('NONE')
" 2>/dev/null || echo "NONE")
    if [ "$THRESHOLDS" != "NONE" ]; then
        info "Liquidity thresholds: $THRESHOLDS"
        important_check 0 "Liquidity thresholds configured"
    else
        important_check 1 "Liquidity thresholds not configured"
    fi
fi

echo ""

# ============================================================
# 4.3 Data Directory Health
# ============================================================
echo "-----------------------------------------------------------"
echo "4.3 DATA DIRECTORY HEALTH"
echo "-----------------------------------------------------------"

# 4.3.1 Data directory writable
if [ -w "$DATA_DIR" ]; then
    important_check 0 "Data directory writable"
else
    important_check 1 "Data directory not writable"
fi

# 4.3.2 Tax directory exists and writable
TAX_DIR="$DATA_DIR/tax"
if [ -d "$TAX_DIR" ] && [ -w "$TAX_DIR" ]; then
    important_check 0 "Tax directory exists and writable"
else
    if mkdir -p "$TAX_DIR" 2>/dev/null; then
        important_check 0 "Tax directory created"
    else
        important_check 1 "Tax directory missing or not writable"
    fi
fi

# 4.3.3 No corrupted JSON files
CORRUPT_COUNT=0
for f in "$DATA_DIR"/*.json; do
    if [ -f "$f" ]; then
        python3 -c "import json; json.load(open('$f'))" 2>/dev/null || ((CORRUPT_COUNT++))
    fi
done
info "Corrupted JSON files: $CORRUPT_COUNT"
if [ "$CORRUPT_COUNT" -eq 0 ]; then
    recommended_check 0 "No corrupted data files"
else
    recommended_check 1 "Corrupted data files found ($CORRUPT_COUNT)"
fi

echo ""

# ============================================================
# 4.4 Trading Pair Coverage
# ============================================================
echo "-----------------------------------------------------------"
echo "4.4 TRADING PAIR COVERAGE IN STATE"
echo "-----------------------------------------------------------"

# Check which of the 7 configured pairs have V3 pool data
if [ -f "$STATE_FILE" ]; then
    PAIR_COVERAGE=$(python3 -c "
import json
with open('$STATE_FILE') as f:
    state = json.load(f)
v3 = state.get('v3_pools', {})
# Extract pair symbols from pool keys
pairs_seen = set()
for k in v3.keys():
    # Keys are like 'WETH/USDC_UniV3_0.05%'
    parts = k.split('_')
    if parts:
        pairs_seen.add(parts[0])
expected = ['WETH/USDC', 'WMATIC/USDC', 'WBTC/USDC', 'USDT/USDC', 'DAI/USDC', 'LINK/USDC', 'UNI/USDC']
found = 0
for p in expected:
    if p in pairs_seen:
        found += 1
        print(f'FOUND: {p}')
    else:
        print(f'MISSING: {p}')
print(f'TOTAL|{found}|{len(expected)}')
" 2>/dev/null)

    FOUND_PAIRS=$(echo "$PAIR_COVERAGE" | grep "^TOTAL|" | cut -d'|' -f2)
    EXPECTED_PAIRS=$(echo "$PAIR_COVERAGE" | grep "^TOTAL|" | cut -d'|' -f3)
    echo "$PAIR_COVERAGE" | grep -v "^TOTAL" | while read line; do
        info "  $line"
    done

    if [ "$FOUND_PAIRS" -ge 5 ]; then
        important_check 0 "Trading pairs in state ($FOUND_PAIRS/$EXPECTED_PAIRS)"
    else
        important_check 1 "Only $FOUND_PAIRS/$EXPECTED_PAIRS pairs in state"
    fi
else
    important_check 1 "Pair coverage check (state file missing)"
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
