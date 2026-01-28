#!/usr/bin/env bash
#
# Script Name: checklist_section3.sh
# Purpose: Pre-$100 Deployment Checklist - Section 3: Bot Configuration
# Author: AI-Generated
# Created: 2026-01-28
# Modified: 2026-01-28
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

# Paths
BOT_DIR="/home/botuser/bots/dexarb/src/rust-bot"
ENV_FILE="$BOT_DIR/.env"

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

# Helper to get .env value
get_env() {
    local key=$1
    grep "^${key}=" "$ENV_FILE" 2>/dev/null | cut -d'=' -f2- | tr -d '"' | tr -d "'"
}

echo ""
echo "============================================================"
echo "  PRE-\$100 DEPLOYMENT CHECKLIST"
echo "  Section 3: Bot Configuration (18 checks)"
echo "============================================================"
echo ""
echo "Date: $(date)"
echo "Config: $ENV_FILE"
echo ""

# Check .env exists
if [ ! -f "$ENV_FILE" ]; then
    echo "ERROR: .env file not found at $ENV_FILE"
    exit 1
fi

# ============================================================
# 3.1 Core Configuration
# ============================================================
echo "-----------------------------------------------------------"
echo "3.1 CORE CONFIGURATION"
echo "-----------------------------------------------------------"

# 3.1.1 RPC URL configured
RPC_URL=$(get_env "RPC_URL")
if [ -n "$RPC_URL" ]; then
    # Mask the API key
    RPC_MASKED=$(echo "$RPC_URL" | sed 's/\(v2\/\)[^/]*/\1***/')
    info "RPC URL: $RPC_MASKED"
    critical_check 0 "RPC URL configured"
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
if [ -n "$PRIVATE_KEY" ] && [ ${#PRIVATE_KEY} -eq 64 ]; then
    info "Private key: configured (64 chars)"
    critical_check 0 "Private key configured"
else
    info "Private key: MISSING or invalid"
    critical_check 1 "Private key configured"
fi

# 3.1.4 Poll interval configured
POLL_INTERVAL=$(get_env "POLL_INTERVAL_MS")
info "Poll interval: ${POLL_INTERVAL}ms"
if [ -n "$POLL_INTERVAL" ] && [ "$POLL_INTERVAL" -ge 1000 ]; then
    critical_check 0 "Poll interval configured (${POLL_INTERVAL}ms)"
else
    critical_check 1 "Poll interval configured"
fi

echo ""

# ============================================================
# 3.2 DEX Configuration
# ============================================================
echo "-----------------------------------------------------------"
echo "3.2 DEX CONFIGURATION"
echo "-----------------------------------------------------------"

# 3.2.1 Quickswap Router configured
QS_ROUTER=$(get_env "UNISWAP_ROUTER")
info "Quickswap Router: $QS_ROUTER"
if [ "$QS_ROUTER" = "0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff" ]; then
    critical_check 0 "Quickswap V2 Router address correct"
else
    critical_check 1 "Quickswap V2 Router address"
fi

# 3.2.2 Sushiswap Router configured
SUSHI_ROUTER=$(get_env "SUSHISWAP_ROUTER")
info "Sushiswap Router: $SUSHI_ROUTER"
if [ "$SUSHI_ROUTER" = "0x1b02dA8Cb0d097eB8D57A175b88c7D8b47997506" ]; then
    important_check 0 "Sushiswap V2 Router address correct"
else
    important_check 1 "Sushiswap V2 Router address"
fi

# 3.2.3 V3 Factory configured
V3_FACTORY=$(get_env "UNISWAP_V3_FACTORY")
info "V3 Factory: $V3_FACTORY"
if [ "$V3_FACTORY" = "0x1F98431c8aD98523631AE4a59f267346ea31F984" ]; then
    critical_check 0 "Uniswap V3 Factory address correct"
else
    critical_check 1 "Uniswap V3 Factory address"
fi

# 3.2.4 V3 Router configured
V3_ROUTER=$(get_env "UNISWAP_V3_ROUTER")
info "V3 Router: $V3_ROUTER"
if [ "$V3_ROUTER" = "0xE592427A0AEce92De3Edee1F18E0157C05861564" ]; then
    critical_check 0 "Uniswap V3 Router address correct"
else
    critical_check 1 "Uniswap V3 Router address"
fi

# 3.2.5 V3 Quoter configured
V3_QUOTER=$(get_env "UNISWAP_V3_QUOTER")
info "V3 Quoter: $V3_QUOTER"
if [ "$V3_QUOTER" = "0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6" ]; then
    important_check 0 "Uniswap V3 Quoter address correct"
else
    important_check 1 "Uniswap V3 Quoter address"
fi

echo ""

# ============================================================
# 3.3 Trading Parameters
# ============================================================
echo "-----------------------------------------------------------"
echo "3.3 TRADING PARAMETERS"
echo "-----------------------------------------------------------"

# 3.3.1 MIN_PROFIT_USD configured
MIN_PROFIT=$(get_env "MIN_PROFIT_USD")
info "Min profit: \$${MIN_PROFIT}"
if [ -n "$MIN_PROFIT" ]; then
    # Check it's a reasonable value (between 0.1 and 100)
    PROFIT_OK=$(python3 -c "print(1 if 0.1 <= float('$MIN_PROFIT') <= 100 else 0)" 2>/dev/null || echo "0")
    if [ "$PROFIT_OK" = "1" ]; then
        important_check 0 "Min profit threshold reasonable (\$$MIN_PROFIT)"
    else
        important_check 1 "Min profit threshold (\$$MIN_PROFIT - outside 0.1-100 range)"
    fi
else
    important_check 1 "Min profit threshold not set"
fi

# 3.3.2 MAX_TRADE_SIZE_USD configured
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

# 3.3.3 MAX_SLIPPAGE_PERCENT configured
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

# 3.3.4 MAX_GAS_PRICE_GWEI configured
MAX_GAS=$(get_env "MAX_GAS_PRICE_GWEI")
info "Max gas price: ${MAX_GAS} gwei"
if [ -n "$MAX_GAS" ] && [ "$MAX_GAS" -ge 10 ] && [ "$MAX_GAS" -le 500 ]; then
    important_check 0 "Max gas price reasonable (${MAX_GAS} gwei)"
else
    important_check 1 "Max gas price (${MAX_GAS} gwei)"
fi

echo ""

# ============================================================
# 3.4 Trading Pairs
# ============================================================
echo "-----------------------------------------------------------"
echo "3.4 TRADING PAIRS"
echo "-----------------------------------------------------------"

# 3.4.1 Trading pairs configured
TRADING_PAIRS=$(get_env "TRADING_PAIRS")
if [ -n "$TRADING_PAIRS" ]; then
    PAIR_COUNT=$(echo "$TRADING_PAIRS" | tr ',' '\n' | wc -l)
    info "Trading pairs configured: $PAIR_COUNT pairs"
    critical_check 0 "Trading pairs configured ($PAIR_COUNT pairs)"

    # List first few pairs
    echo "$TRADING_PAIRS" | tr ',' '\n' | head -3 | while read pair; do
        SYMBOL=$(echo "$pair" | cut -d':' -f3)
        info "  - $SYMBOL"
    done
    [ "$PAIR_COUNT" -gt 3 ] && info "  ... and $((PAIR_COUNT - 3)) more"
else
    info "Trading pairs: NOT SET"
    critical_check 1 "Trading pairs configured"
fi

# 3.4.2 USDC pairs exist (essential for bridged USDC.e trading)
USDC_PAIRS=$(echo "$TRADING_PAIRS" | grep -o "USDC" | wc -l)
if [ "$USDC_PAIRS" -gt 0 ]; then
    important_check 0 "USDC pairs configured ($USDC_PAIRS pairs)"
else
    important_check 1 "USDC pairs configured"
fi

echo ""

# ============================================================
# 3.5 Logging & Monitoring
# ============================================================
echo "-----------------------------------------------------------"
echo "3.5 LOGGING & MONITORING"
echo "-----------------------------------------------------------"

# 3.5.1 RUST_LOG configured
RUST_LOG=$(get_env "RUST_LOG")
info "RUST_LOG: $RUST_LOG"
if [ -n "$RUST_LOG" ]; then
    important_check 0 "Rust logging configured"
else
    important_check 1 "Rust logging not configured"
fi

# 3.5.2 Discord webhook configured
DISCORD_WEBHOOK=$(get_env "DISCORD_WEBHOOK")
if [ -n "$DISCORD_WEBHOOK" ] && [[ "$DISCORD_WEBHOOK" == https://discord.com/api/webhooks/* ]]; then
    info "Discord webhook: configured"
    recommended_check 0 "Discord webhook configured"
else
    info "Discord webhook: not set"
    recommended_check 1 "Discord webhook not configured"
fi

# 3.5.3 Log directory exists
LOG_DIR="/home/botuser/bots/dexarb/logs"
if [ -d "$LOG_DIR" ]; then
    LOG_COUNT=$(ls -1 "$LOG_DIR"/*.log 2>/dev/null | wc -l)
    info "Log directory: $LOG_DIR ($LOG_COUNT log files)"
    recommended_check 0 "Log directory exists"
else
    info "Log directory: NOT FOUND"
    recommended_check 1 "Log directory missing"
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
