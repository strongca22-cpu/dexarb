#!/usr/bin/env bash
#
# Script Name: checklist_section3.sh
# Purpose: Pre-$100 Deployment Checklist - Section 3: Bot Configuration
# Author: AI-Generated
# Created: 2026-01-28
# Modified: 2026-01-30 - Switch to .env.live, add whitelist/Multicall3/shared-data config
#
# Usage:
#   ./scripts/checklist_section3.sh
#

# Counters
CRITICAL_PASS=0
CRITICAL_FAIL=0
IMPORTANT_PASS=0
IMPORTANT_FAIL=0
RECOMMENDED_PASS=0
RECOMMENDED_FAIL=0

# Paths — live bot uses .env.live (separate from data collector .env)
BOT_DIR="/home/botuser/bots/dexarb/src/rust-bot"
ENV_FILE="$BOT_DIR/.env.live"

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

# Helper to get .env.live value
get_env() {
    local key=$1
    grep "^${key}=" "$ENV_FILE" 2>/dev/null | cut -d'=' -f2- | tr -d '"' | tr -d "'"
}

echo ""
echo "============================================================"
echo "  PRE-\$100 DEPLOYMENT CHECKLIST"
echo "  Section 3: Bot Configuration (.env.live)"
echo "  Architecture: V3 shared-data, Multicall3, whitelist"
echo "============================================================"
echo ""
echo "Date: $(date)"
echo "Config: $ENV_FILE"
echo ""

# Check .env.live exists
if [ ! -f "$ENV_FILE" ]; then
    echo "ERROR: .env.live file not found at $ENV_FILE"
    echo "Live bot uses .env.live (separate from data collector .env)"
    exit 1
fi

# ============================================================
# 3.1 Core Configuration
# ============================================================
echo "-----------------------------------------------------------"
echo "3.1 CORE CONFIGURATION"
echo "-----------------------------------------------------------"

# 3.1.1 RPC URL configured (WebSocket for live bot)
RPC_URL=$(get_env "RPC_URL")
if [ -n "$RPC_URL" ]; then
    RPC_MASKED=$(echo "$RPC_URL" | sed 's/\(v2\/\)[^/]*/\1***/')
    info "RPC URL: $RPC_MASKED"
    critical_check 0 "RPC URL configured"
    # Check it's WebSocket (live bot requires WSS)
    if [[ "$RPC_URL" == wss://* ]]; then
        important_check 0 "RPC is WebSocket (wss://)"
    else
        important_check 1 "RPC should be WebSocket (wss://) for live bot, got: ${RPC_URL:0:10}..."
    fi
else
    info "RPC URL: NOT SET"
    critical_check 1 "RPC URL configured"
fi

# 3.1.2 Chain ID = 137 (Polygon)
CHAIN_ID=$(get_env "CHAIN_ID")
info "Chain ID: $CHAIN_ID"
if [ "$CHAIN_ID" = "137" ]; then
    critical_check 0 "Chain ID = 137 (Polygon mainnet)"
else
    critical_check 1 "Chain ID = 137 (currently $CHAIN_ID)"
fi

# 3.1.3 Private key configured (check exists, don't expose)
PRIVATE_KEY=$(get_env "PRIVATE_KEY")
if [ -n "$PRIVATE_KEY" ] && [ ${#PRIVATE_KEY} -ge 64 ]; then
    info "Private key: configured (${#PRIVATE_KEY} chars)"
    critical_check 0 "Private key configured"
else
    info "Private key: MISSING or invalid"
    critical_check 1 "Private key configured"
fi

# 3.1.4 Poll interval configured
POLL_INTERVAL=$(get_env "POLL_INTERVAL_MS")
info "Poll interval: ${POLL_INTERVAL}ms"
if [ -n "$POLL_INTERVAL" ] && [ "$POLL_INTERVAL" -ge 1000 ]; then
    important_check 0 "Poll interval configured (${POLL_INTERVAL}ms)"
else
    important_check 1 "Poll interval configured"
fi

# 3.1.5 LIVE_MODE set
LIVE_MODE=$(get_env "LIVE_MODE")
info "LIVE_MODE: $LIVE_MODE"
if [ "$LIVE_MODE" = "true" ]; then
    critical_check 0 "LIVE_MODE=true (real trading enabled)"
else
    critical_check 1 "LIVE_MODE not set to true (currently: $LIVE_MODE)"
fi

echo ""

# ============================================================
# 3.2 Shared Data Architecture
# ============================================================
echo "-----------------------------------------------------------"
echo "3.2 SHARED DATA ARCHITECTURE"
echo "-----------------------------------------------------------"

# 3.2.1 POOL_STATE_FILE configured
POOL_STATE_FILE=$(get_env "POOL_STATE_FILE")
info "Pool state file: $POOL_STATE_FILE"
if [ -n "$POOL_STATE_FILE" ]; then
    critical_check 0 "POOL_STATE_FILE configured"
    if [ -f "$POOL_STATE_FILE" ]; then
        important_check 0 "Pool state file exists at configured path"
    else
        important_check 1 "Pool state file MISSING at $POOL_STATE_FILE"
    fi
else
    critical_check 1 "POOL_STATE_FILE not set (required for shared data mode)"
fi

# 3.2.2 WHITELIST_FILE configured
WHITELIST_FILE=$(get_env "WHITELIST_FILE")
info "Whitelist file: $WHITELIST_FILE"
if [ -n "$WHITELIST_FILE" ]; then
    critical_check 0 "WHITELIST_FILE configured"
    if [ -f "$WHITELIST_FILE" ]; then
        important_check 0 "Whitelist file exists at configured path"
    else
        important_check 1 "Whitelist file MISSING at $WHITELIST_FILE"
    fi
else
    critical_check 1 "WHITELIST_FILE not set (required for Phase 1.1 filtering)"
fi

echo ""

# ============================================================
# 3.3 DEX Configuration
# ============================================================
echo "-----------------------------------------------------------"
echo "3.3 DEX CONFIGURATION (V3)"
echo "-----------------------------------------------------------"

# 3.3.1 V3 Factory configured
V3_FACTORY=$(get_env "UNISWAP_V3_FACTORY")
info "V3 Factory: $V3_FACTORY"
if [ "$V3_FACTORY" = "0x1F98431c8aD98523631AE4a59f267346ea31F984" ]; then
    critical_check 0 "Uniswap V3 Factory address correct"
else
    critical_check 1 "Uniswap V3 Factory address"
fi

# 3.3.2 V3 Router configured
V3_ROUTER=$(get_env "UNISWAP_V3_ROUTER")
info "V3 Router: $V3_ROUTER"
if [ "$V3_ROUTER" = "0xE592427A0AEce92De3Edee1F18E0157C05861564" ]; then
    critical_check 0 "Uniswap V3 Router address correct"
else
    critical_check 1 "Uniswap V3 Router address"
fi

# 3.3.3 V3 Quoter configured (QuoterV1 for pre-checks + Multicall3 batch)
V3_QUOTER=$(get_env "UNISWAP_V3_QUOTER")
info "V3 Quoter: $V3_QUOTER"
if [ "$V3_QUOTER" = "0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6" ]; then
    critical_check 0 "Uniswap V3 QuoterV1 address correct"
else
    critical_check 1 "Uniswap V3 QuoterV1 address"
fi

echo ""

# ============================================================
# 3.4 Trading Parameters
# ============================================================
echo "-----------------------------------------------------------"
echo "3.4 TRADING PARAMETERS"
echo "-----------------------------------------------------------"

# 3.4.1 MIN_PROFIT_USD configured
MIN_PROFIT=$(get_env "MIN_PROFIT_USD")
info "Min profit: \$${MIN_PROFIT}"
if [ -n "$MIN_PROFIT" ]; then
    PROFIT_OK=$(python3 -c "print(1 if 0.05 <= float('$MIN_PROFIT') <= 100 else 0)" 2>/dev/null || echo "0")
    if [ "$PROFIT_OK" = "1" ]; then
        important_check 0 "Min profit threshold reasonable (\$$MIN_PROFIT)"
    else
        important_check 1 "Min profit threshold (\$$MIN_PROFIT - outside 0.05-100 range)"
    fi
else
    important_check 1 "Min profit threshold not set"
fi

# 3.4.2 MAX_TRADE_SIZE_USD configured
MAX_TRADE=$(get_env "MAX_TRADE_SIZE_USD")
info "Max trade size: \$${MAX_TRADE}"
if [ -n "$MAX_TRADE" ]; then
    TRADE_OK=$(python3 -c "print(1 if 10 <= float('$MAX_TRADE') <= 10000 else 0)" 2>/dev/null || echo "0")
    if [ "$TRADE_OK" = "1" ]; then
        important_check 0 "Max trade size reasonable (\$$MAX_TRADE)"
    else
        important_check 1 "Max trade size (\$$MAX_TRADE - outside 10-10000 range)"
    fi
else
    important_check 1 "Max trade size not set"
fi

# 3.4.3 MAX_SLIPPAGE_PERCENT configured
MAX_SLIPPAGE=$(get_env "MAX_SLIPPAGE_PERCENT")
info "Max slippage: ${MAX_SLIPPAGE}%"
if [ -n "$MAX_SLIPPAGE" ]; then
    SLIP_OK=$(python3 -c "print(1 if 0.1 <= float('$MAX_SLIPPAGE') <= 5 else 0)" 2>/dev/null || echo "0")
    if [ "$SLIP_OK" = "1" ]; then
        important_check 0 "Max slippage reasonable (${MAX_SLIPPAGE}%)"
    else
        important_check 1 "Max slippage (${MAX_SLIPPAGE}% - outside 0.1-5% range)"
    fi
else
    important_check 1 "Max slippage not set"
fi

# 3.4.4 MAX_GAS_PRICE_GWEI configured
MAX_GAS=$(get_env "MAX_GAS_PRICE_GWEI")
info "Max gas price: ${MAX_GAS} gwei"
if [ -n "$MAX_GAS" ] && [ "$MAX_GAS" -ge 10 ]; then
    important_check 0 "Max gas price configured (${MAX_GAS} gwei)"
else
    important_check 1 "Max gas price (${MAX_GAS} gwei)"
fi

echo ""

# ============================================================
# 3.5 Trading Pairs
# ============================================================
echo "-----------------------------------------------------------"
echo "3.5 TRADING PAIRS"
echo "-----------------------------------------------------------"

# 3.5.1 Trading pairs configured
TRADING_PAIRS=$(get_env "TRADING_PAIRS")
if [ -n "$TRADING_PAIRS" ]; then
    PAIR_COUNT=$(echo "$TRADING_PAIRS" | tr ',' '\n' | wc -l)
    info "Trading pairs configured: $PAIR_COUNT pairs"
    critical_check 0 "Trading pairs configured ($PAIR_COUNT pairs)"

    # List pairs
    echo "$TRADING_PAIRS" | tr ',' '\n' | while read pair; do
        SYMBOL=$(echo "$pair" | cut -d':' -f3)
        info "  - $SYMBOL"
    done
else
    info "Trading pairs: NOT SET"
    critical_check 1 "Trading pairs configured"
fi

echo ""

# ============================================================
# 3.6 Tax Logging
# ============================================================
echo "-----------------------------------------------------------"
echo "3.6 TAX LOGGING (IRS COMPLIANCE)"
echo "-----------------------------------------------------------"

# 3.6.1 Tax logging enabled
TAX_ENABLED=$(get_env "TAX_LOG_ENABLED")
info "Tax logging: $TAX_ENABLED"
if [ "$TAX_ENABLED" = "true" ]; then
    critical_check 0 "TAX_LOG_ENABLED=true"
else
    critical_check 1 "Tax logging not enabled"
fi

# 3.6.2 Tax directory configured
TAX_DIR=$(get_env "TAX_LOG_DIR")
info "Tax directory: $TAX_DIR"
if [ -n "$TAX_DIR" ]; then
    important_check 0 "TAX_LOG_DIR configured"
else
    important_check 1 "TAX_LOG_DIR not set"
fi

# 3.6.3 Logging level set
RUST_LOG=$(get_env "RUST_LOG")
info "RUST_LOG: $RUST_LOG"
if [ -n "$RUST_LOG" ]; then
    recommended_check 0 "Rust logging configured"
else
    recommended_check 1 "Rust logging not configured"
fi

echo ""

# ============================================================
# Summary
# ============================================================
echo "============================================================"
echo "  SECTION 3 SUMMARY"
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
    echo "*** SECTION 3: FAILED - Critical issues must be resolved ***"
    exit 1
else
    echo ""
    echo "=== SECTION 3: PASSED - All critical checks OK ==="
    exit 0
fi
