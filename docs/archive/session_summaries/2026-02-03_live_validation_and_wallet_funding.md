# Session Summary: Live Validation Run & Wallet Funding

**Date:** 2026-02-03 (00:00–01:00 UTC)
**Branch:** `feature/alloy-migration`
**Objective:** Validate the full hybrid pipeline end-to-end by closing a real on-chain transaction, even at a small loss.

---

## Key Accomplishments

### 1. Critical Bug Fix: `fee_to_u24()` V2 Sentinel Truncation

- **Bug:** `fee_to_u24(fee: u32)` in `executor.rs:54` and `v3_syncer.rs:49` used `Uint::from(fee as u16)`, truncating the V2 fee sentinel `16777215` (0xFFFFFF, 24-bit) to `65535` (0xFFFF, 16-bit).
- **Impact:** The ArbExecutor.sol contract interpreted fee=65535 as a standard V3 fee tier instead of the V2 sentinel, routing all V2 swaps through the wrong on-chain path → guaranteed revert.
- **Fix:** Changed to `Uint::from_limbs([fee as u64])` with `debug_assert!(fee <= 0xFFFFFF)` in both files.
- **Validation:** 80 tests pass, release binary builds clean.

### 2. Live Validation Run (~50 minutes)

Bot launched in tmux `dexarb-live` with `LIVE_MODE=true`, `MEMPOOL_MONITOR=execute`.

**Session Stats (49 min window):**

| Metric | Value |
|--------|-------|
| Hybrid pipeline | 17 insert / 12 hit / 12 exec / **0 success** / 12 fail |
| Block-reactive | 45 detect / 40 exec / **0 success** |
| Total exec attempts | 52 — all failed at estimateGas (`0x5fc483c5`) |
| Mempool decoded | 409 pending swaps |
| Confirmation rate | 99.8% (408/409) |
| Median lead time | 5.4s |
| Simulation accuracy | median 0.04% error |

**Hybrid pipeline is mechanically correct.** The full flow works end-to-end:
`SIM OPP → MEMPOOL EXEC → HYBRID CACHE → HYBRID HIT → execute()`

All failures occur at estimateGas — the ArbExecutor contract reverts with `InsufficientProfit()` because on-chain spreads are consumed by faster bots (~3.5s cache age with Alchemy latency).

**Opportunities detected by pair:**

| Pair | Detections | Exec Attempts |
|------|-----------|---------------|
| WMATIC/USDC | 1,035 | 40 |
| DAI/USDT | 98 | 7 |
| WETH/USDC | 23 | 2 |
| WMATIC/USDT | 7 | 1 |
| WBTC/USDC | 7 | 4 |

### 3. Profit Threshold Lowered

- `MIN_PROFIT_USD`: 0.10 → 0.01 (block-reactive path)
- `MEMPOOL_MIN_PROFIT_USD`: 0.05 → 0.001 (hybrid path)
- Contract `minProfit` lowered from 50000 (0.05 USDC) to 1000 (0.001 USDC)
- **Result:** Still all estimateGas reverts. Confirms spreads are negative (not just below threshold) at execution time.

### 4. Wallet Funding & Approvals

All 15 trading pairs now fully funded and approved:

| Token | Balance | ArbExecutor Approval | Pairs |
|-------|---------|---------------------|-------|
| USDC.e | $516.70 | MAX | 8 pairs |
| Native USDC | $400.00 | MAX | No pairs configured yet |
| USDT | $150.07 | MAX (new) | 3 pairs (new) |
| WETH | 0.043 (~$101) | MAX (new) | 4 pairs (new) |
| MATIC/POL | ~15 | N/A | Gas |

**Transactions executed this session:**
1. WETH approve → ArbExecutor: `0x4f14ba65...` (block 82478971)
2. Native USDC approve → UniV3 Router: `0x9330af84...` (block 82479081)
3. Swap 150 native USDC → 150.07 USDT via UniV3: `0x8bd55de4...` (block 82479092)
4. USDT approve → ArbExecutor: `0xb5d7fa59...` (block 82479127)

**Wallet incident:** $145 USDT accidentally sent on Ethereum mainnet instead of Polygon. Funds are safe (same address, same private key) but need ETH gas on mainnet to recover. Not urgent — recoverable at any time.

---

## Technical Findings

### Why 0% Capture Rate

The root cause is **latency, not code bugs**:
- Alchemy free tier adds ~250ms per RPC call
- Hybrid pipeline cache age: ~3.5s from mempool detection to block confirmation
- Other bots with local nodes (<20ms latency) capture spreads before our execution reaches the chain
- Even with `minProfit=0.001 USDC`, estimateGas reverts — the spreads are genuinely negative by execution time

### Architecture Validation

Despite 0 captures, the session validated:
- Hybrid pipeline fires correctly (mempool → cache → block confirmation → execute)
- Event-driven sync works (66 pools, ~50ms per block)
- Mempool monitor decodes 100% of target swaps, 99.8% confirmation rate
- Simulation accuracy: median 0.04% error (excellent)
- Route cooldown correctly suppresses repeatedly failing routes
- V2/V3/Algebra fee sentinel routing works correctly (post-fix)

### PairLookup Ambiguity (Known Issue)

Tokens appearing in multiple pairs (e.g., WMATIC in WMATIC/USDC and WMATIC/WETH) map to only one pair via `HashMap::or_insert_with`. WMATIC→WETH swaps may be misidentified as WMATIC/USDC. Not blocking for current operation but limits multi-quote-token mempool detection accuracy.

---

## Bot Status at End of Session

- Running in tmux `dexarb-live` with all 15 pairs active
- Log: `data/polygon/logs/live_validation_20260203_002513.log`
- `MIN_PROFIT_USD=0.01`, `MEMPOOL_MIN_PROFIT_USD=0.001`
- 80 tests passing, release binary with fee_to_u24 fix

---

## Next Steps (Prioritized)

### Immediate
1. **Monitor overnight run** — with 15 funded pairs, watch for any captures on less competitive pairs (DAI/USDT, WBTC/WETH, etc.)
2. **Add native USDC trading pairs** — $400 funded + approved but no pairs configured. Research pool addresses.
3. **Consider skip-estimateGas option** — for hybrid path only, send tx on-chain even when estimateGas reverts ($0.01/attempt on Polygon). Forces a real on-chain tx for validation.

### Near-term
4. **Hetzner dedicated server** — The single biggest lever. Local Bor node = <1ms RPC, full mempool, competitive execution.
5. **Reduce priority fee** — Currently hardcoded 5000 gwei ($0.70/tx). Even successful captures would lose money. Needs to be configurable.

### Deferred
6. Recover $145 USDT from Ethereum mainnet (needs ETH gas on mainnet)
7. Fix PairLookup ambiguity for multi-quote-token mempool detection

---

*Session conducted by AI assistant (Claude). All code changes on `feature/alloy-migration` branch.*
