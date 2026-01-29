# Next Steps - DEX Arbitrage Bot

## Current Status: V3 INTEGRATION COMPLETE ✓

**Date:** 2026-01-29

**Wallet:**
- USDC.e: ~$1,020
- MATIC: 8.21 (gas)
- Approvals: Set

---

## Session Summary (2026-01-29)

### V3 Integration to Live Bot

The main dexarb-bot now supports V3 pools (previously only paper-trading did):

1. **PoolStateManager** - Extended to store V3 pools alongside V2
2. **V3PoolSyncer** - Integrated into main.rs startup and loop
3. **OpportunityDetector** - New `check_pair_unified()` method compares all pool types
4. **LIVE_MODE** - Environment variable to enable real trades

### Key V3 Arbitrage Routes

| Route | Round-Trip Fee | Typical Spread | Est. Profit |
|-------|---------------|----------------|-------------|
| 0.05% ↔ 1.00% | 1.05% | ~2.24% | ~$10/trade |
| 0.05% ↔ 0.30% | 0.35% | ~1.43% | ~$9/trade |

---

## Immediate Next Steps

1. [x] Add V3 support to live bot (detector, syncer, state manager)
2. [x] Add LIVE_MODE environment variable
3. [x] Build and test compilation
4. [ ] Start live bot with V3 in tmux
5. [ ] Monitor for V3↔V3 opportunities
6. [ ] Validate execution on mainnet

---

## Commands

```bash
# Start live bot
tmux new-session -d -s live-bot
tmux send-keys -t live-bot "./target/release/dexarb-bot 2>&1 | tee data/bot_live.log" Enter

# Monitor for first trade
./scripts/monitor_trade.sh
```

---

## Backlog

### Monitoring
- [x] Hourly Discord reports
- [x] Spread logger
- [ ] Real-time alerts for high-value opportunities
- [ ] Daily summary reports

### Infrastructure
- [ ] Multi-RPC failover
- [ ] Staggered V3 sync (reduce RPC calls)

---

*Last updated: 2026-01-29*
