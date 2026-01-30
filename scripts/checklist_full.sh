#!/usr/bin/env bash
#
# Script Name: checklist_full.sh
# Purpose: Pre-$100 Deployment Checklist - FULL RUN
# Author: AI-Generated
# Created: 2026-01-28
# Modified: 2026-01-30 - V3 shared-data, Multicall3, whitelist, two-wallet architecture
#
# Usage:
#   ./scripts/checklist_full.sh
#
# Runs all sections and provides combined summary
#

SCRIPTS_DIR="/home/botuser/bots/dexarb/scripts"

echo ""
echo "################################################################"
echo "#                                                              #"
echo "#   PRE-\$100 DEPLOYMENT CHECKLIST - FULL VALIDATION           #"
echo "#   Updated: 2026-01-30                                        #"
echo "#                                                              #"
echo "#   Architecture: V3 shared-data (JSON-based)                  #"
echo "#   Features: Multicall3 batch Quoter, whitelist v1.1,         #"
echo "#             two-wallet, tax logging, HALT safety             #"
echo "#   Config: .env.live (live bot), .env (data collector)        #"
echo "#                                                              #"
echo "################################################################"
echo ""
echo "Date: $(date)"
echo ""
echo "Bot Architecture:"
echo "  Data collector -> JSON state file -> Live bot"
echo "  Multicall3 batch pre-screen -> Executor (Quoter + swap)"
echo "  Whitelist v1.1 strict enforcement (10 pools, 7 blacklisted)"
echo ""

SECTION1_PASS=0
SECTION2_PASS=0
SECTION3_PASS=0
SECTION4_PASS=0
SECTION5_10_PASS=0

# Run Section 1
echo ""
echo ">>> Running Section 1: Technical Infrastructure..."
if bash "$SCRIPTS_DIR/checklist_section1.sh" > /tmp/s1.log 2>&1; then
    SECTION1_PASS=1
    echo "    Section 1: PASSED"
else
    echo "    Section 1: FAILED"
fi

# Run Section 2
echo ">>> Running Section 2: Smart Contract Verification..."
if bash "$SCRIPTS_DIR/checklist_section2.sh" > /tmp/s2.log 2>&1; then
    SECTION2_PASS=1
    echo "    Section 2: PASSED"
else
    echo "    Section 2: FAILED"
fi

# Run Section 3
echo ">>> Running Section 3: Bot Configuration (.env.live)..."
if bash "$SCRIPTS_DIR/checklist_section3.sh" > /tmp/s3.log 2>&1; then
    SECTION3_PASS=1
    echo "    Section 3: PASSED"
else
    echo "    Section 3: FAILED"
fi

# Run Section 4
echo ">>> Running Section 4: Data Integrity (JSON state + whitelist)..."
if bash "$SCRIPTS_DIR/checklist_section4.sh" > /tmp/s4.log 2>&1; then
    SECTION4_PASS=1
    echo "    Section 4: PASSED"
else
    echo "    Section 4: FAILED"
fi

# Run Sections 5-10
echo ">>> Running Sections 5-10: Execution, Risk, Monitoring, Finance, Ops, Emergency..."
if bash "$SCRIPTS_DIR/checklist_section5_10.sh" > /tmp/s5_10.log 2>&1; then
    SECTION5_10_PASS=1
    echo "    Sections 5-10: PASSED"
else
    echo "    Sections 5-10: FAILED"
fi

echo ""
echo "################################################################"
echo "#                    FINAL SUMMARY                             #"
echo "################################################################"
echo ""

TOTAL_SECTIONS=$((SECTION1_PASS + SECTION2_PASS + SECTION3_PASS + SECTION4_PASS + SECTION5_10_PASS))

printf "%-52s %s\n" "Section 1 (Infrastructure, state file, binaries):" $([ $SECTION1_PASS -eq 1 ] && echo "PASS" || echo "FAIL")
printf "%-52s %s\n" "Section 2 (Contracts, Multicall3, whitelist pools):" $([ $SECTION2_PASS -eq 1 ] && echo "PASS" || echo "FAIL")
printf "%-52s %s\n" "Section 3 (Config .env.live, V3, shared data):" $([ $SECTION3_PASS -eq 1 ] && echo "PASS" || echo "FAIL")
printf "%-52s %s\n" "Section 4 (JSON state, whitelist integrity):" $([ $SECTION4_PASS -eq 1 ] && echo "PASS" || echo "FAIL")
printf "%-52s %s\n" "Sections 5-10 (Execution, risk, finance, ops):" $([ $SECTION5_10_PASS -eq 1 ] && echo "PASS" || echo "FAIL")

echo ""
echo "----------------------------------------------------------------"
echo ""

if [ $TOTAL_SECTIONS -eq 5 ]; then
    echo "  ██████╗  █████╗ ███████╗███████╗███████╗██████╗ "
    echo "  ██╔══██╗██╔══██╗██╔════╝██╔════╝██╔════╝██╔══██╗"
    echo "  ██████╔╝███████║███████╗███████╗█████╗  ██║  ██║"
    echo "  ██╔═══╝ ██╔══██║╚════██║╚════██║██╔══╝  ██║  ██║"
    echo "  ██║     ██║  ██║███████║███████║███████╗██████╔╝"
    echo "  ╚═╝     ╚═╝  ╚═╝╚══════╝╚══════╝╚══════╝╚═════╝ "
    echo ""
    echo "  ALL CRITICAL CHECKS PASSED!"
    echo "  System is ready for deployment."
    echo ""
    echo "  Architecture verified:"
    echo "    - V3 shared-data (JSON pool state)"
    echo "    - Multicall3 batch Quoter pre-screening"
    echo "    - Whitelist v1.1 strict enforcement"
    echo "    - Two-wallet architecture"
    echo "    - Tax logging (IRS compliance)"
    echo "    - HALT on committed capital"
    echo ""
    echo "  Recommended Action: PROCEED WITH DEPLOYMENT"
    echo ""
    echo "  Detail logs: /tmp/s1.log through /tmp/s5_10.log"
    echo ""
    exit 0
else
    echo "  ███████╗ █████╗ ██╗██╗     ███████╗██████╗ "
    echo "  ██╔════╝██╔══██╗██║██║     ██╔════╝██╔══██╗"
    echo "  █████╗  ███████║██║██║     █████╗  ██║  ██║"
    echo "  ██╔══╝  ██╔══██║██║██║     ██╔══╝  ██║  ██║"
    echo "  ██║     ██║  ██║██║███████╗███████╗██████╔╝"
    echo "  ╚═╝     ╚═╝  ╚═╝╚═╝╚══════╝╚══════╝╚═════╝ "
    echo ""
    echo "  SOME SECTIONS FAILED!"
    echo "  Review failed sections before deployment."
    echo ""
    echo "  Failed sections: $((5 - TOTAL_SECTIONS))"
    echo "  Detail logs: /tmp/s1.log through /tmp/s5_10.log"
    echo "  Run individual section scripts for verbose output."
    echo ""
    exit 1
fi
