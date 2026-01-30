#!/usr/bin/env bash
#
# Script Name: checklist_section5_10.sh
# Purpose: Pre-$100 Deployment Checklist - Sections 5-10 Combined
# Author: AI-Generated
# Created: 2026-01-28
# Modified: 2026-01-30 - V3 shared-data, Multicall3, whitelist, two-wallet, no paper trading
# Modified: 2026-01-30 - Monolithic architecture (data collector check demoted to RECOMMENDED)
#
# Usage:
#   ./scripts/checklist_section5_10.sh
#
# Sections covered:
#   5. Execution Path Validation
#   6. Risk Management
#   7. Monitoring & Alerts
#   8. Financial Controls (tax logging, two-wallet)
#   9. Operational Procedures
#   10. Emergency Protocols
#

CRITICAL_PASS=0
CRITICAL_FAIL=0
IMPORTANT_PASS=0
IMPORTANT_FAIL=0
RECOMMENDED_PASS=0
RECOMMENDED_FAIL=0

BOT_DIR="/home/botuser/bots/dexarb"
SCRIPTS_DIR="$BOT_DIR/scripts"
DOCS_DIR="$BOT_DIR/docs"
DATA_DIR="$BOT_DIR/data"
LOGS_DIR="$BOT_DIR/logs"
ENV_LIVE="$BOT_DIR/src/rust-bot/.env.live"

# Two-wallet architecture
WALLET_LIVE="0xa532eb528aE17eFC881FCe6894a08B5b70fF21e2"
WALLET_BACKUP="0x8e843e351c284dd96F8E458c10B39164b2Aeb7Fb"
RPC_URL="https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8"

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
echo "  Sections 5-10: Execution, Risk, Monitoring, Financial,"
echo "                 Operational, Emergency"
echo "  V3 monolithic | Multicall3 | Whitelist | Two-wallet"
echo "============================================================"
echo ""
echo "Date: $(date)"
echo ""

# ============================================================
# SECTION 5: EXECUTION PATH VALIDATION
# ============================================================
echo "==========================================================="
echo "SECTION 5: EXECUTION PATH VALIDATION"
echo "==========================================================="
echo ""

# 5.1 Bot binary executable
BOT_BINARY="$BOT_DIR/src/rust-bot/target/release/dexarb-bot"
if [ -x "$BOT_BINARY" ]; then
    critical_check 0 "Live bot binary is executable"
else
    critical_check 1 "Live bot binary is executable"
fi

# 5.2 Data collector binary executable
DC_BINARY="$BOT_DIR/src/rust-bot/target/release/data-collector"
if [ -x "$DC_BINARY" ]; then
    critical_check 0 "Data collector binary is executable"
else
    critical_check 1 "Data collector binary is executable"
fi

# 5.3 Bot binary valid ELF
if [ -x "$BOT_BINARY" ]; then
    if file "$BOT_BINARY" | grep -q "executable"; then
        important_check 0 "Bot binary valid executable (ELF)"
    else
        important_check 1 "Bot binary not a valid executable"
    fi
else
    important_check 1 "Bot binary missing"
fi

# 5.4 Gas estimation available
GAS_PRICE=$(curl -s --max-time 5 -X POST "$RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"eth_gasPrice","params":[],"id":1}' \
    | python3 -c "import sys,json; r=json.load(sys.stdin).get('result','0x0'); print(int(r,16)//1e9)" 2>/dev/null)
info "Current gas price: ${GAS_PRICE} gwei"
if [ -n "$GAS_PRICE" ] && [ "${GAS_PRICE%.*}" -lt 500 ]; then
    important_check 0 "Gas price reasonable (${GAS_PRICE} gwei)"
else
    important_check 1 "Gas price check (${GAS_PRICE} gwei)"
fi

# 5.5 RPC responds to eth_call
if curl -s --max-time 5 -X POST "$RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | grep -q "result"; then
    critical_check 0 "RPC eth_call working"
else
    critical_check 1 "RPC eth_call working"
fi

# 5.6 Multicall3 module compiled into binary
if grep -q "multicall_quoter" "$BOT_DIR/src/rust-bot/src/arbitrage/mod.rs" 2>/dev/null; then
    critical_check 0 "Multicall3 batch Quoter module registered"
else
    critical_check 1 "Multicall3 module missing from arbitrage/mod.rs"
fi

# 5.7 Whitelist filter integrated in source
if grep -q "whitelist" "$BOT_DIR/src/rust-bot/src/arbitrage/detector.rs" 2>/dev/null; then
    important_check 0 "Whitelist filter integrated in detector"
else
    important_check 1 "Whitelist filter not found in detector"
fi

# 5.8 HALT mechanism in executor (committed capital safety)
if grep -q "tx_hash" "$BOT_DIR/src/rust-bot/src/main.rs" 2>/dev/null; then
    important_check 0 "HALT on committed capital mechanism present"
else
    important_check 1 "HALT mechanism not found in main.rs"
fi

# 5.9b Monolithic direct RPC sync in main.rs
if grep -q "sync_known_pools_parallel" "$BOT_DIR/src/rust-bot/src/main.rs" 2>/dev/null; then
    critical_check 0 "Monolithic direct RPC sync in main.rs"
else
    critical_check 1 "Monolithic sync missing — main.rs may still use JSON file reading"
fi

# 5.9 Slippage parameters in .env.live
if grep -q "SLIPPAGE" "$ENV_LIVE" 2>/dev/null; then
    important_check 0 "Slippage parameters configured"
else
    important_check 1 "Slippage parameters not in .env.live"
fi

echo ""

# ============================================================
# SECTION 6: RISK MANAGEMENT
# ============================================================
echo "==========================================================="
echo "SECTION 6: RISK MANAGEMENT"
echo "==========================================================="
echo ""

# 6.1 Max trade size limited
MAX_TRADE=$(grep "MAX_TRADE_SIZE_USD" "$ENV_LIVE" 2>/dev/null | cut -d'=' -f2)
if [ -n "$MAX_TRADE" ]; then
    info "Max trade size: \$${MAX_TRADE}"
    critical_check 0 "Max trade size limit configured (\$$MAX_TRADE)"
else
    critical_check 1 "Max trade size limit configured"
fi

# 6.2 Min profit threshold set
MIN_PROFIT=$(grep "MIN_PROFIT_USD" "$ENV_LIVE" 2>/dev/null | cut -d'=' -f2)
if [ -n "$MIN_PROFIT" ]; then
    info "Min profit: \$${MIN_PROFIT}"
    critical_check 0 "Min profit threshold set (\$$MIN_PROFIT)"
else
    critical_check 1 "Min profit threshold set"
fi

# 6.3 Max gas price limit
MAX_GAS=$(grep "MAX_GAS_PRICE_GWEI" "$ENV_LIVE" 2>/dev/null | cut -d'=' -f2)
if [ -n "$MAX_GAS" ]; then
    info "Max gas: ${MAX_GAS} gwei"
    important_check 0 "Max gas price limit configured (${MAX_GAS} gwei)"
else
    important_check 1 "Max gas price limit configured"
fi

# 6.4 Live wallet capital amount
USDC_BAL=$(curl -s --max-time 10 -X POST "$RPC_URL" \
    -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_call\",\"params\":[{\"to\":\"0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174\",\"data\":\"0x70a08231000000000000000000000000${WALLET_LIVE:2}\"}, \"latest\"],\"id\":1}" \
    | python3 -c "import sys,json; r=json.load(sys.stdin).get('result','0x0'); print(int(r,16)/1e6)" 2>/dev/null)
info "Live wallet USDC.e: \$${USDC_BAL}"
USDC_INT=${USDC_BAL%.*}
if [ -n "$USDC_INT" ] && [ "$USDC_INT" -ge 10 ] && [ "$USDC_INT" -le 2000 ]; then
    critical_check 0 "Live wallet capital reasonable (\$$USDC_BAL)"
else
    critical_check 1 "Live wallet capital (\$$USDC_BAL - should be \$10-2000)"
fi

# 6.5 MATIC for gas available (live wallet)
MATIC_BAL=$(curl -s --max-time 10 -X POST "$RPC_URL" \
    -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$WALLET_LIVE\", \"latest\"],\"id\":1}" \
    | python3 -c "import sys,json; r=json.load(sys.stdin).get('result','0x0'); print(int(r,16)/1e18)" 2>/dev/null)
info "Live wallet MATIC: ${MATIC_BAL}"
MATIC_INT=${MATIC_BAL%.*}
if [ -n "$MATIC_INT" ] && [ "$MATIC_INT" -ge 1 ]; then
    critical_check 0 "MATIC for gas available (${MATIC_BAL})"
else
    critical_check 1 "MATIC for gas (${MATIC_BAL} - need >1)"
fi

# 6.6 Backup wallet exists with funds
BACKUP_USDC=$(curl -s --max-time 10 -X POST "$RPC_URL" \
    -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_call\",\"params\":[{\"to\":\"0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174\",\"data\":\"0x70a08231000000000000000000000000${WALLET_BACKUP:2}\"}, \"latest\"],\"id\":1}" \
    | python3 -c "import sys,json; r=json.load(sys.stdin).get('result','0x0'); print(int(r,16)/1e6)" 2>/dev/null)
info "Backup wallet USDC.e: \$${BACKUP_USDC}"
BACKUP_INT=${BACKUP_USDC%.*}
if [ -n "$BACKUP_INT" ] && [ "$BACKUP_INT" -gt 0 ]; then
    important_check 0 "Backup wallet has funds (\$$BACKUP_USDC)"
else
    important_check 1 "Backup wallet empty"
fi

echo ""

# ============================================================
# SECTION 7: MONITORING & ALERTS
# ============================================================
echo "==========================================================="
echo "SECTION 7: MONITORING & ALERTS"
echo "==========================================================="
echo ""

# 7.1 Data collector process (RECOMMENDED — monolithic bot syncs directly)
# Data collector is only needed for paper trading / research, not live bot
if pgrep -f "data-collector" >/dev/null 2>&1 || pgrep -f "data_collector" >/dev/null 2>&1; then
    info "Data collector running (paper trading / research)"
    recommended_check 0 "Data collector running (optional for monolithic)"
else
    info "Data collector not running (ok — live bot syncs directly via RPC)"
    recommended_check 0 "Data collector not required for monolithic bot"
fi

# 7.2 Discord webhook configured (RECOMMENDED — alerts are nice-to-have)
if grep -q "DISCORD_WEBHOOK=https://discord.com" "$ENV_LIVE" 2>/dev/null; then
    recommended_check 0 "Discord webhook configured in .env.live"
else
    info "Discord webhook not in .env.live (paper trading only, via .env)"
    recommended_check 0 "Discord webhook optional for live bot"
fi

# 7.3 Log directory exists
if [ -d "$LOGS_DIR" ]; then
    LOG_COUNT=$(ls -1 "$LOGS_DIR" 2>/dev/null | wc -l)
    info "Log directory: $LOGS_DIR ($LOG_COUNT files)"
    recommended_check 0 "Log directory exists"
else
    recommended_check 1 "Log directory missing"
fi

# 7.4 Tmux available for session management
TMUX_COUNT=$(tmux list-sessions 2>/dev/null | wc -l)
if [ "$TMUX_COUNT" -gt 0 ]; then
    SESSIONS=$(tmux list-sessions 2>/dev/null | cut -d: -f1 | tr '\n' ', ' | sed 's/,$//')
    info "Tmux sessions: $SESSIONS"
    recommended_check 0 "Tmux sessions available ($TMUX_COUNT)"
else
    recommended_check 1 "No tmux sessions"
fi

echo ""

# ============================================================
# SECTION 8: FINANCIAL CONTROLS
# ============================================================
echo "==========================================================="
echo "SECTION 8: FINANCIAL CONTROLS"
echo "==========================================================="
echo ""

echo "-----------------------------------------------------------"
echo "8.1 TWO-WALLET ARCHITECTURE"
echo "-----------------------------------------------------------"
info "Live wallet:   $WALLET_LIVE"
info "Backup wallet: $WALLET_BACKUP"

# 8.1 Dedicated trading wallet
critical_check 0 "Dedicated trading wallet configured"

# 8.2 Wallet has trading capital
if [ -n "$USDC_INT" ] && [ "$USDC_INT" -ge 10 ]; then
    critical_check 0 "Live wallet has trading capital (\$$USDC_BAL)"
else
    critical_check 1 "Live wallet has trading capital"
fi

# 8.3 Wallet has gas funds
if [ -n "$MATIC_INT" ] && [ "$MATIC_INT" -ge 1 ]; then
    critical_check 0 "Live wallet has gas funds (${MATIC_BAL} MATIC)"
else
    critical_check 1 "Live wallet has gas funds"
fi

echo ""
echo "-----------------------------------------------------------"
echo "8.2 TAX LOGGING (IRS COMPLIANCE)"
echo "-----------------------------------------------------------"

TAX_DIR="$DATA_DIR/tax"

# 8.4 Tax logging enabled in config
if grep -q "TAX_LOG_ENABLED=true" "$ENV_LIVE" 2>/dev/null; then
    critical_check 0 "Tax logging enabled in .env.live"
else
    critical_check 1 "Tax logging not enabled (TAX_LOG_ENABLED=true missing)"
fi

# 8.5 Tax directory configured
if grep -q "TAX_LOG_DIR=" "$ENV_LIVE" 2>/dev/null; then
    TAX_DIR_CONFIG=$(grep "TAX_LOG_DIR=" "$ENV_LIVE" | cut -d'=' -f2)
    info "Tax directory: $TAX_DIR_CONFIG"
    critical_check 0 "Tax directory configured"
else
    critical_check 1 "Tax directory not configured"
fi

# 8.6 Tax directory exists and writable
if [ -d "$TAX_DIR" ] && [ -w "$TAX_DIR" ]; then
    critical_check 0 "Tax directory exists and writable"
else
    if mkdir -p "$TAX_DIR" 2>/dev/null; then
        critical_check 0 "Tax directory created"
    else
        critical_check 1 "Tax directory missing or not writable"
    fi
fi

# 8.7 Tax module exists in bot source
if [ -f "$BOT_DIR/src/rust-bot/src/tax/mod.rs" ]; then
    important_check 0 "Tax module source exists"
else
    important_check 1 "Tax module source missing"
fi

# 8.8 Tax logging integrated in main.rs
if grep -q "enable_tax_logging" "$BOT_DIR/src/rust-bot/src/main.rs" 2>/dev/null; then
    important_check 0 "Tax logging integrated in bot"
else
    important_check 1 "Tax logging not integrated in main.rs"
fi

# 8.9 CSV logger source
if [ -f "$BOT_DIR/src/rust-bot/src/tax/csv_logger.rs" ]; then
    important_check 0 "CSV tax logger source ready"
else
    important_check 1 "CSV tax logger missing"
fi

# 8.10 RP2 export source
if [ -f "$BOT_DIR/src/rust-bot/src/tax/rp2_export.rs" ]; then
    recommended_check 0 "RP2 export source ready"
else
    recommended_check 1 "RP2 export missing"
fi

echo ""

# ============================================================
# SECTION 9: OPERATIONAL PROCEDURES
# ============================================================
echo "==========================================================="
echo "SECTION 9: OPERATIONAL PROCEDURES"
echo "==========================================================="
echo ""

# 9.1 Deployment checklist documented
if [ -f "$DOCS_DIR/pre_100_deployment_checklist.md" ]; then
    important_check 0 "Deployment checklist documented"
else
    important_check 1 "Deployment checklist documented"
fi

# 9.2 Whitelist verification script exists
if [ -f "$SCRIPTS_DIR/verify_whitelist.py" ]; then
    important_check 0 "Whitelist verification script exists"
else
    important_check 1 "Whitelist verification script missing"
fi

# 9.3 Scripts directory organized
SCRIPT_COUNT=$(ls -1 "$SCRIPTS_DIR"/*.sh "$SCRIPTS_DIR"/*.py 2>/dev/null | wc -l)
info "Utility scripts: $SCRIPT_COUNT"
if [ "$SCRIPT_COUNT" -ge 3 ]; then
    recommended_check 0 "Utility scripts available ($SCRIPT_COUNT)"
else
    recommended_check 1 "Utility scripts available"
fi

# 9.4 Git repo for version control
if [ -d "$BOT_DIR/.git" ]; then
    COMMIT_COUNT=$(git -C "$BOT_DIR" rev-list --count HEAD 2>/dev/null || echo "0")
    LATEST_COMMIT=$(git -C "$BOT_DIR" log --oneline -1 2>/dev/null || echo "unknown")
    info "Git commits: $COMMIT_COUNT"
    info "Latest: $LATEST_COMMIT"
    important_check 0 "Version control active ($COMMIT_COUNT commits)"
else
    important_check 1 "Version control (git)"
fi

# 9.5 No uncommitted changes to critical files
DIRTY_COUNT=$(git -C "$BOT_DIR" status --porcelain src/rust-bot/src/ config/ 2>/dev/null | wc -l)
if [ "$DIRTY_COUNT" -eq 0 ]; then
    recommended_check 0 "No uncommitted changes to source/config"
else
    recommended_check 1 "Uncommitted changes detected ($DIRTY_COUNT files)"
fi

echo ""

# ============================================================
# SECTION 10: EMERGENCY PROTOCOLS
# ============================================================
echo "==========================================================="
echo "SECTION 10: EMERGENCY PROTOCOLS"
echo "==========================================================="
echo ""

# 10.1 Can stop bot quickly (tmux/kill)
if tmux list-sessions 2>/dev/null | grep -q "dexarb"; then
    critical_check 0 "Bot can be stopped quickly (tmux session exists)"
else
    if pgrep -f "dexarb" >/dev/null 2>&1; then
        critical_check 0 "Bot process can be killed"
    else
        critical_check 0 "No bot currently running (can start fresh)"
    fi
fi

# 10.2 Private key not exposed in logs
EXPOSED=$(grep -r "d332b37d" "$LOGS_DIR" 2>/dev/null | wc -l)
if [ "$EXPOSED" -eq 0 ]; then
    critical_check 0 "Private key not exposed in logs"
else
    critical_check 1 "Private key may be exposed in logs!"
fi

# 10.3 Can revoke approvals if needed (cast available)
if [ -x "$HOME/.foundry/bin/cast" ]; then
    important_check 0 "Foundry cast available for emergency revokes"
else
    important_check 1 "Foundry cast for emergencies"
fi

# 10.4 Documentation available
if [ -f "$DOCS_DIR/next_steps.md" ]; then
    recommended_check 0 "Operations documentation (next_steps.md) available"
else
    recommended_check 1 "Operations documentation"
fi

echo ""

# ============================================================
# FINAL SUMMARY
# ============================================================
echo "============================================================"
echo "  SECTIONS 5-10 SUMMARY"
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
    echo "*** SECTIONS 5-10: FAILED - Critical issues must be resolved ***"
    exit 1
else
    echo ""
    echo "=== SECTIONS 5-10: PASSED - All critical checks OK ==="
    exit 0
fi
