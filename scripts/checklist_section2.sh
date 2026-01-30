#!/usr/bin/env bash
#
# Script Name: checklist_section2.sh
# Purpose: Pre-$100 Deployment Checklist - Section 2: Smart Contract Verification
# Author: AI-Generated
# Created: 2026-01-28
# Modified: 2026-01-30 - V3 architecture, Multicall3, whitelist pool verification
#
# Usage:
#   ./scripts/checklist_section2.sh
#
# Dependencies:
#   - curl
#   - python3
#

# Counters
CRITICAL_PASS=0
CRITICAL_FAIL=0
IMPORTANT_PASS=0
IMPORTANT_FAIL=0
RECOMMENDED_PASS=0
RECOMMENDED_FAIL=0

# RPC URL (Alchemy HTTPS for checklist scripts)
RPC_URL="https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"

# Wallet addresses (two-wallet architecture)
WALLET_LIVE="0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2"
WALLET_BACKUP="0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb"

# Contract addresses
V3_ROUTER="0xE592427A0AEce92De3Edee1F18E0157C05861564"
V3_QUOTER="0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6"
V3_FACTORY="0x1F98431c8aD98523631AE4a59f267346ea31F984"
MULTICALL3="0xcA11bde05977b3631167028862bE2a173976CA11"

# Token addresses
USDC_BRIDGED="0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"
WMATIC="0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270"
WETH="0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619"
UNI="0xb33EaAd8d922B1083446DC23f610c2567fB5180f"

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

# Function to call contract and get code size
get_code_size() {
    local addr=$1
    local result=$(curl -s --max-time 10 -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getCode\",\"params\":[\"$addr\",\"latest\"],\"id\":1}" \
        | python3 -c "import sys,json; r=json.load(sys.stdin).get('result','0x'); print(len(r)//2 - 1)" 2>/dev/null)
    echo "${result:-0}"
}

# Function to call contract method
call_contract() {
    local addr=$1
    local data=$2
    local result=$(curl -s --max-time 10 -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_call\",\"params\":[{\"to\":\"$addr\",\"data\":\"$data\"},\"latest\"],\"id\":1}" \
        | python3 -c "import sys,json; print(json.load(sys.stdin).get('result','0x'))" 2>/dev/null)
    echo "$result"
}

# Function to decode uint256 from hex
decode_uint() {
    local hex=$1
    python3 -c "print(int('$hex', 16))" 2>/dev/null || echo "0"
}

echo ""
echo "============================================================"
echo "  PRE-\$100 DEPLOYMENT CHECKLIST"
echo "  Section 2: Smart Contract Verification"
echo "  V3 + Multicall3 + Whitelist Pool Verification"
echo "============================================================"
echo ""
echo "Date: $(date)"
echo "RPC: Alchemy Polygon Mainnet"
echo ""

# ============================================================
# 2.1 Core Contract Addresses
# ============================================================
echo "-----------------------------------------------------------"
echo "2.1 CORE CONTRACT ADDRESSES"
echo "-----------------------------------------------------------"

# 2.1.1 V3 Router has code
CODE_SIZE=$(get_code_size "$V3_ROUTER")
info "V3 Router code size: ${CODE_SIZE} bytes"
if [ "$CODE_SIZE" -gt 1000 ]; then
    critical_check 0 "V3 Router has code (SwapRouter)"
else
    critical_check 1 "V3 Router has code - only ${CODE_SIZE} bytes"
fi

# 2.1.2 V3 Quoter has code (QuoterV1 — used for pre-checks)
CODE_SIZE=$(get_code_size "$V3_QUOTER")
info "V3 Quoter code size: ${CODE_SIZE} bytes"
if [ "$CODE_SIZE" -gt 1000 ]; then
    critical_check 0 "V3 QuoterV1 has code"
else
    critical_check 1 "V3 QuoterV1 has code - only ${CODE_SIZE} bytes"
fi

# 2.1.3 Multicall3 has code (Phase 2.1 batch pre-screening)
CODE_SIZE=$(get_code_size "$MULTICALL3")
info "Multicall3 code size: ${CODE_SIZE} bytes"
if [ "$CODE_SIZE" -gt 100 ]; then
    critical_check 0 "Multicall3 has code (batch Quoter)"
else
    critical_check 1 "Multicall3 has code - only ${CODE_SIZE} bytes"
fi

# 2.1.4 V3 Factory has code
CODE_SIZE=$(get_code_size "$V3_FACTORY")
info "V3 Factory code size: ${CODE_SIZE} bytes"
if [ "$CODE_SIZE" -gt 1000 ]; then
    important_check 0 "V3 Factory has code"
else
    important_check 1 "V3 Factory has code - only ${CODE_SIZE} bytes"
fi

# 2.1.5 Addresses match .env.live config
ENV_LIVE="/home/botuser/bots/dexarb/src/rust-bot/.env.live"
if [ -f "$ENV_LIVE" ]; then
    ENV_QUOTER=$(grep "UNISWAP_V3_QUOTER=" "$ENV_LIVE" | cut -d'=' -f2)
    ENV_ROUTER=$(grep "UNISWAP_V3_ROUTER=" "$ENV_LIVE" | cut -d'=' -f2)
    if [ "$ENV_QUOTER" = "$V3_QUOTER" ] && [ "$ENV_ROUTER" = "$V3_ROUTER" ]; then
        critical_check 0 "Contract addresses match .env.live"
    else
        critical_check 1 "Contract addresses MISMATCH in .env.live"
    fi
else
    critical_check 1 ".env.live missing for address verification"
fi

echo ""

# ============================================================
# 2.2 Token Addresses
# ============================================================
echo "-----------------------------------------------------------"
echo "2.2 TOKEN ADDRESSES"
echo "-----------------------------------------------------------"

# Helper to get decimals - calls decimals() = 0x313ce567
get_decimals() {
    local addr=$1
    local result=$(call_contract "$addr" "0x313ce567")
    decode_uint "$result"
}

# 2.2.1 USDC.e verification
USDC_DECIMALS=$(get_decimals "$USDC_BRIDGED")
info "USDC.e: decimals=$USDC_DECIMALS"
if [ "$USDC_DECIMALS" = "6" ]; then
    critical_check 0 "USDC.e address correct, 6 decimals"
else
    critical_check 1 "USDC.e verification failed (decimals=$USDC_DECIMALS)"
fi

# 2.2.2 WETH verification
WETH_DECIMALS=$(get_decimals "$WETH")
info "WETH: decimals=$WETH_DECIMALS"
if [ "$WETH_DECIMALS" = "18" ]; then
    important_check 0 "WETH address correct, 18 decimals"
else
    important_check 1 "WETH verification failed (decimals=$WETH_DECIMALS)"
fi

# 2.2.3 WMATIC verification
WMATIC_DECIMALS=$(get_decimals "$WMATIC")
info "WMATIC: decimals=$WMATIC_DECIMALS"
if [ "$WMATIC_DECIMALS" = "18" ]; then
    important_check 0 "WMATIC address correct, 18 decimals"
else
    important_check 1 "WMATIC verification failed (decimals=$WMATIC_DECIMALS)"
fi

echo ""

# ============================================================
# 2.3 Whitelist Pool Verification
# ============================================================
echo "-----------------------------------------------------------"
echo "2.3 WHITELIST POOL VERIFICATION"
echo "-----------------------------------------------------------"

WHITELIST_FILE="/home/botuser/bots/dexarb/config/pools_whitelist.json"

if [ -f "$WHITELIST_FILE" ]; then
    # Verify each whitelisted pool exists on-chain (has code)
    POOL_RESULTS=$(python3 -c "
import json, sys
with open('$WHITELIST_FILE') as f:
    data = json.load(f)
pools = data.get('whitelist', {}).get('pools', [])
for p in pools:
    addr = p.get('address', '')
    pair = p.get('pair', '?')
    fee = p.get('fee_tier', 0)
    status = p.get('status', 'unknown')
    print(f'{addr}|{pair}|{fee}|{status}')
" 2>/dev/null)

    TOTAL_WL=0
    VERIFIED_WL=0
    while IFS='|' read -r addr pair fee status; do
        [ -z "$addr" ] && continue
        TOTAL_WL=$((TOTAL_WL + 1))
        CODE_SIZE=$(get_code_size "$addr")
        if [ "$CODE_SIZE" -gt 100 ]; then
            info "  $pair (fee=$fee): verified on-chain ($CODE_SIZE bytes)"
            VERIFIED_WL=$((VERIFIED_WL + 1))
        else
            info "  $pair (fee=$fee): NO CODE at $addr"
        fi
    done <<< "$POOL_RESULTS"

    if [ "$TOTAL_WL" -gt 0 ] && [ "$VERIFIED_WL" -eq "$TOTAL_WL" ]; then
        critical_check 0 "All $TOTAL_WL whitelisted pools verified on-chain"
    elif [ "$VERIFIED_WL" -gt 0 ]; then
        critical_check 1 "Only $VERIFIED_WL/$TOTAL_WL whitelisted pools verified"
    else
        critical_check 1 "No whitelisted pools verified"
    fi
else
    critical_check 1 "Whitelist file missing for pool verification"
fi

# 2.3.2 Blacklist entries exist
if [ -f "$WHITELIST_FILE" ]; then
    BL_COUNT=$(python3 -c "
import json
with open('$WHITELIST_FILE') as f:
    data = json.load(f)
bl = data.get('blacklist', {})
# Count entries across all types
total = 0
if isinstance(bl, dict):
    for k,v in bl.items():
        if isinstance(v, list): total += len(v)
        elif isinstance(v, dict): total += len(v)
elif isinstance(bl, list):
    total = len(bl)
print(total)
" 2>/dev/null || echo "0")
    info "Blacklisted entries: $BL_COUNT"
    if [ "$BL_COUNT" -gt 0 ]; then
        important_check 0 "Blacklist configured ($BL_COUNT entries)"
    else
        important_check 1 "Blacklist empty (thin/phantom pools not filtered)"
    fi
else
    important_check 1 "Blacklist check (whitelist file missing)"
fi

echo ""

# ============================================================
# 2.4 Approval Status
# ============================================================
echo "-----------------------------------------------------------"
echo "2.4 APPROVAL STATUS"
echo "-----------------------------------------------------------"

# Check allowance: allowance(address,address) = 0xdd62ed3e
check_allowance() {
    local token=$1
    local owner=$2
    local spender=$3

    local owner_padded=$(echo "$owner" | sed 's/0x//' | tr '[:upper:]' '[:lower:]')
    owner_padded=$(printf "%064s" "$owner_padded" | tr ' ' '0')
    local spender_padded=$(echo "$spender" | sed 's/0x//' | tr '[:upper:]' '[:lower:]')
    spender_padded=$(printf "%064s" "$spender_padded" | tr ' ' '0')

    local data="0xdd62ed3e${owner_padded}${spender_padded}"
    local result=$(call_contract "$token" "$data")
    decode_uint "$result"
}

# 2.4.1 USDC.e approved for V3 router (live wallet)
USDC_ALLOWANCE=$(check_allowance "$USDC_BRIDGED" "$WALLET_LIVE" "$V3_ROUTER")
USDC_ALLOWANCE_FORMATTED=$(python3 -c "print(f'{$USDC_ALLOWANCE/1e6:.2f}')" 2>/dev/null || echo "0")
info "USDC.e allowance (live wallet -> V3 Router): $USDC_ALLOWANCE_FORMATTED USDC"
if [ "$USDC_ALLOWANCE" -gt 0 ]; then
    critical_check 0 "USDC.e approved for V3 router ($USDC_ALLOWANCE_FORMATTED)"
else
    critical_check 1 "USDC.e NOT approved for V3 router"
fi

# 2.4.2 Check if approval is unlimited (security recommendation)
MAX_UINT256="115792089237316195423570985008687907853269984665640564039457584007913129639935"
if [ "$USDC_ALLOWANCE" = "$MAX_UINT256" ]; then
    recommended_check 1 "Approval is unlimited (consider using exact amounts)"
else
    recommended_check 0 "Approval not unlimited (good security practice)"
fi

echo ""

# ============================================================
# Summary
# ============================================================
echo "============================================================"
echo "  SECTION 2 SUMMARY"
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
    echo "*** SECTION 2: FAILED - Critical issues must be resolved ***"
    exit 1
else
    echo ""
    echo "=== SECTION 2: PASSED - All critical checks OK ==="
    exit 0
fi
