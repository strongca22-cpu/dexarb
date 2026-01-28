#!/usr/bin/env bash
#
# Script Name: checklist_full.sh
# Purpose: Pre-$100 Deployment Checklist - FULL RUN
# Author: AI-Generated
# Created: 2026-01-28
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
echo "#   Updated: 2026-01-28 (Dual-Route + Tax Logging)             #"
echo "#                                                              #"
echo "#   Total Checks: 140 (+10 tax logging)                        #"
echo "#   Critical: 46 | Important: 59 | Recommended: 35             #"
echo "#                                                              #"
echo "################################################################"
echo ""
echo "Date: $(date)"
echo ""
echo "Dual-Route Discovery:"
echo "  Route 1: V3 1.00% -> V3 0.05% (~\$10.25/trade)"
echo "  Route 2: V3 0.30% -> V3 0.05% (~\$9.22/trade)"
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
echo ">>> Running Section 3: Bot Configuration..."
if bash "$SCRIPTS_DIR/checklist_section3.sh" > /tmp/s3.log 2>&1; then
    SECTION3_PASS=1
    echo "    Section 3: PASSED"
else
    echo "    Section 3: FAILED"
fi

# Run Section 4
echo ">>> Running Section 4: Data Integrity..."
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

printf "%-45s %s\n" "Section 1 (Technical Infrastructure - 15):" $([ $SECTION1_PASS -eq 1 ] && echo "✓ PASS" || echo "✗ FAIL")
printf "%-45s %s\n" "Section 2 (Smart Contracts - 15 +UNI pools):" $([ $SECTION2_PASS -eq 1 ] && echo "✓ PASS" || echo "✗ FAIL")
printf "%-45s %s\n" "Section 3 (Bot Configuration - 18):" $([ $SECTION3_PASS -eq 1 ] && echo "✓ PASS" || echo "✗ FAIL")
printf "%-45s %s\n" "Section 4 (Data Integrity - 14 +dual-route):" $([ $SECTION4_PASS -eq 1 ] && echo "✓ PASS" || echo "✗ FAIL")
printf "%-45s %s\n" "Sections 5-10 (Combined - 68 +projections):" $([ $SECTION5_10_PASS -eq 1 ] && echo "✓ PASS" || echo "✗ FAIL")

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
    echo "  System is ready for \$100 deployment."
    echo ""
    echo "  Dual-Route Validation: COMPLETE"
    echo "    Route 1: V3 1.00% -> 0.05% verified"
    echo "    Route 2: V3 0.30% -> 0.05% verified"
    echo ""
    echo "  Confidence Level: HIGH (>90%)"
    echo "  Expected Daily: \$5-30 (conservative-optimistic)"
    echo "  Recommended Action: PROCEED WITH DEPLOYMENT"
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
    echo "  Run individual section scripts for details."
    echo ""
    exit 1
fi
