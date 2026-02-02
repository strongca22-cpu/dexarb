# Session Summary: Node Migration Strategy + Bug Fixes

**Date:** 2026-02-01
**Session type:** Analysis, bug fixes, strategic planning
**Duration:** Extended session (continuation from A4 Phase 3 deployment)

---

## Key Outcomes

### 1. Two Critical Bugs Fixed

**Bug 1: PoolStateManager Key Collision**
- **Symptom:** Only 19 V3 pools active instead of expected 25
- **Root cause:** `DashMap<(DexType, String), V3PoolState>` — native USDC pools shared the same key `(DexType, "WETH/USDC")` as USDC.e pools, causing silent overwrite
- **Fix:** Changed key to `DashMap<Address, V3PoolState>` — pool address is globally unique
- **Result:** 25 V3 + 7 V2 = 32 pools now active
- **File:** `src/rust-bot/src/pool/state.rs`
- **Tests:** 73/73 pass, including new regression test `test_same_dex_pair_different_address_no_collision`

**Bug 2: chain_id=0 in Mempool Transaction Signing**
- **Symptom:** All 23 pre-fix mempool execution attempts had `gas_cost_usd=0.0000` — never hit chain
- **Error:** `"invalid sender: invalid chain id for signer: have 0 want 137"`
- **Root cause:** `execute_from_mempool()` skips `fill_transaction()` for speed, but that also sets chain_id on EIP-1559 tx body
- **Fix:** Added `inner.chain_id = Some(self.config.chain_id.into())` in both private and public RPC paths
- **Result:** First on-chain mempool tx `0xb63fcaf9...` confirmed (reverted with "Too little received" — expected, see below)
- **File:** `src/rust-bot/src/arbitrage/executor.rs`

### 2. Transaction Ordering Problem Identified

After fixing chain_id, 3 txs reached the chain:

| Tx | Submit Latency | Block Position | Result |
|----|---------------|----------------|--------|
| `0xb63f...` | 263ms | Index 25 (trigger at 106) | Revert — landed BEFORE trigger |
| `0x06f1...` | 231ms | Confirmed, reverted | Same ordering issue |
| `0xb884...` | 299ms | Confirmed, reverted | Same ordering issue |

**Critical finding:** Our tx landed at index 25, the trigger tx at index 106, all in the same block (82424555). We were FAST ENOUGH (same block), but landed in the WRONG POSITION. The predicted spread didn't exist yet because the trigger tx hadn't executed.

**Conclusion:** Polygon has no Flashbots-equivalent for guaranteed bundle ordering. Mempool backrunning without validator-level access is structurally limited on Polygon.

### 3. Strategic Pivot to Own Node

**Analysis of alternatives:**

| Approach | Assessment |
|----------|-----------|
| Continue current Alchemy setup | Structurally limited — ordering problem won't resolve |
| Migrate ethers-rs → alloy | 1-2ms savings irrelevant at 250ms RPC latency... BUT matters on a local node |
| Own Bor node (Hetzner dedicated) | Eliminates 250ms latency, unfiltered mempool, no rate limits |
| Validator integration | Requires $50K+ MATIC stake, private relationships |

**Decision: Hetzner dedicated server + Bor node + alloy migration**

Rationale:
- Cost is trivial (~$80-150/mo) vs potential returns
- Eliminates the dominant latency component (250ms → <1ms)
- Enables long-tail pool expansion (no RPC rate limits)
- alloy migration gives IPC transport + micro-latency wins
- Hybrid pipeline solves ordering problem (wait for block confirmation, submit pre-built tx)

### 4. Micro-Latency Analysis

On a local node, the latency regime changes fundamentally:

| Component | Current (Alchemy) | With local node |
|-----------|-------------------|-----------------|
| Block arrival | ~250ms | <10ms (validator-peered) |
| RPC round-trip | ~250ms | <1ms (IPC) |
| Local compute | ~10-15ms | ~3-7ms (optimized) |
| **Total** | **~263ms** | **~5-10ms** |

At this scale, ethers-rs → alloy saves 10-15% of total execution time. Other micro-optimizations (CPU pinning, pre-built tx templates, arena allocation) stack to potentially halve the local compute.

### 5. Documents Created

| Document | Path | Purpose |
|----------|------|---------|
| alloy port plan | `docs/alloy_port_plan.md` | 8-phase migration plan, type mapping tables, file-by-file survey, risk mitigation |
| next_steps.md | `docs/next_steps.md` (updated) | Strategic pivot: Hetzner + alloy + hybrid pipeline + long-tail expansion |
| This session summary | `docs/session_summaries/2026-02-01_node_migration_strategy.md` | Full record |

---

## Files Modified

| File | Change | Status |
|------|--------|--------|
| `src/rust-bot/src/pool/state.rs` | DashMap key: `(DexType, String)` → `Address` | **Uncommitted** |
| `src/rust-bot/src/arbitrage/executor.rs` | Added `chain_id` in mempool tx building (2 locations) | **Uncommitted** |
| `docs/next_steps.md` | Strategic pivot, new action plan A6-A15, hybrid pipeline, micro-latency stack | Committed (this session) |
| `docs/alloy_port_plan.md` | NEW — complete alloy migration plan | Committed (this session) |

---

## Decisions Made

1. **Proceed with Hetzner AX52/AX62** — higher-clock CPU (5800X/5900X) for single-core performance
2. **Frankfurt or Helsinki datacenter** — proximity to Polygon validators (many on Hetzner)
3. **alloy migration during Bor sync dead time** — efficient use of waiting period
4. **Keep current VPS running** — ethers-rs bot as baseline comparison and data collector
5. **Long-tail strategy** — don't compete on WETH/USDC with validator-integrated extractors; target 200+ pools across 20+ pairs where competition is lower
6. **Hybrid pipeline** — mempool for prediction, block confirmation for execution

---

## Open Items

1. **Uncommitted code changes** — pool/state.rs and executor.rs have unfixed bugs. Should commit before any other work.
2. **data_collector shared_state.rs** — has same key collision pattern (`format!("{}:{}", pool.dex, pool.pair.symbol)`). Lower priority (separate binary) but should fix eventually.
3. **Hetzner ordering** — user will handle via separate Claude chat (prompt provided)
4. **alloy version verification** — need to confirm latest alloy version supports all features (IPC, sol!, Alchemy subscriptions)
5. **Long-tail pool data** — need to quantify how many Polygon pools have recurring extractable spreads before committing to 200+ pool expansion

---

## Bot Status at Session End

- **Process:** `livebot_polygon` active in tmux
- **Pools:** 25 V3 + 7 V2 = 32 total (key collision fix deployed)
- **Mempool:** Execute mode, chain_id fix deployed
- **On-chain txs:** 3 submitted post-fix, all reverted (ordering issue)
- **Gas spend:** ~$0.0006 total (negligible)
- **Mempool stats (18:41 UTC):** 615 decoded, 98.0% confirmation, 5440ms median lead, 4 SIM OPPs, 43 validated, 0.050% median error
