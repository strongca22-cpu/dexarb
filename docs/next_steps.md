# Next Steps - DEX Arbitrage Bot

Backlog of planned improvements and features.

---

## Discord Bot Commands (Planned)

**Purpose:** Allow manual report triggers via Discord commands (e.g., `!report`)

**Current state:** Hourly reports use webhook (one-way, send-only)

**Requirements:**
1. Create Discord Application at [Discord Developer Portal](https://discord.com/developers/applications)
2. Create Bot, get Bot Token
3. Invite bot to server with message read permissions
4. Implement listener script

**Options:**
- Prefix commands (`!report`) - simpler, uses `discord.py`
- Slash commands (`/report`) - more polished, requires command registration

**Implementation:**
- Separate script or integrate into `hourly_discord_report.py`
- Reuse existing `calculate_stats()` and `send_discord_report()` functions

**Priority:** Low - hourly reports sufficient for now

---

## Other Ideas

- [ ] Real-time alerts for high-value opportunities (>$50 profit)
- [ ] Daily summary report (aggregates all hourly data)
- [ ] Pool health monitoring alerts (TVL drops, staleness)

---

*Last updated: 2026-01-28*
