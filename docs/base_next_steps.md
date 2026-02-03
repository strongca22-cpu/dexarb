# Base Bot — Next Steps & Session Summary

**Created:** 2026-02-03
**Session:** Base live launch + pool expansion scan
**Status:** Bot LIVE (diagnostic run), pool expansion research in progress

---

## Session Summary (2026-02-03)

### What was done

1. **Config changes to `.env.base`:**
   - `MEMPOOL_MONITOR=observe` -> `off` (Base has no public mempool; Coinbase sequencer is FCFS)
   - `POLL_INTERVAL_MS=3000` -> `2000` (Base 2s block times; 3s missed blocks)
   - Added `ROUTE_COOLDOWN_BLOCKS=10` (explicit; was already default in config.rs)

2. **Funding & on-chain setup:**
   - USDC bridge: $150 USDC to wallet `0x48091E...44fa` on Base — confirmed on-chain
   - Approve tx: USDC.approve(ArbExecutor, max_uint256) — tx `0x03d74c...d2c2` confirmed
   - ETH balance: 0.005685 ETH (~$18.76) for gas

3. **ArbExecutor contract review:**
   - Full security audit of ArbExecutor.sol (167 lines). Conclusion: **no bugs that risk funds.**
   - Pull model: contract calls `transferFrom` to pull USDC, executes both swap legs, returns all USDC. Reverts atomically if profit < minProfit.
   - Only risk is gas on failed txs (~$0.01-0.05 per revert on Base)

4. **Live launch:**
   - `LIVE_MODE=true` flipped
   - Bot running in `tmux: base-bot` — syncing every 2s block, 5/5 pools healthy
   - Gas killswitch in `tmux: base-killswitch` — kills bot if >$1 gas spent
   - Discord status reporter updated for Base LIVE, running in `tmux: discord-status`

5. **Pool expansion scan:**
   - Created `scripts/base_pool_scanner.py` — Base-specific pool discovery (separate from Polygon)
   - Full scan: 12 base tokens x 3 quote tokens across 3 factories (UniV3, SushiV3, Aerodrome CL)
   - Results: **150 pools found, 101 with liquidity, 15 pairs with cross-DEX arb potential**
   - CSV output: `data/base/pool_scan_results.csv`

### Key findings

- **Zero opportunities detected** in first ~15min (02:20-02:35 UTC). Expected — dry-run data showed 67/72 opportunities clustered at 14:00 UTC (US market open). Bot is healthy; market is quiet at 2AM UTC.
- **Aerodrome CL: zero pools found.** See Aerodrome section below — this is the biggest gap.
- **15 cross-DEX pairs identified** on UniV3 x SushiV3 alone. Current bot monitors only 1 pair (WETH/USDC).

---

## Pool Expansion Candidates

### Tier 1: High-value cross-DEX pairs (UniV3 + SushiV3, deep liquidity)

| Pair | Pools | DEXes | Notes |
|------|-------|-------|-------|
| WETH/USDC | 8 | 2 | **Already active** (5 pools in whitelist) |
| DAI/WETH | 8 | 2 | Excellent depth on both DEXes, 18-decimal quote |
| DAI/USDC | 7 | 2 | Stablecoin-to-stablecoin, tight spreads, high frequency |
| WETH/USDbC | 8 | 2 | Bridged USDC — may have spread vs native USDC |
| DEGEN/WETH | 6 | 2 | Meme token, volatile, frequent spread events |
| AERO/WETH | 6 | 2 | Aerodrome native token, deep liquidity on UniV3 |

### Tier 2: Good candidates (need liquidity depth assessment)

| Pair | Pools | DEXes | Notes |
|------|-------|-------|-------|
| cbETH/WETH | 5 | 2 | LST pair, deep UniV3, SushiV3 has 1 pool |
| wstETH/WETH | 5 | 2 | LST pair, very deep UniV3 0.01% tier |
| TOSHI/WETH | 5 | 2 | Meme token, check real liquidity depth |
| AERO/USDC | 5 | 2 | Cross-DEX arb on Base's native DEX token |
| BRETT/WETH | 4 | 2 | Meme token, mostly UniV3 |
| DEGEN/USDC | 4 | 2 | Meme via stablecoin quote |

### Tier 3: Marginal (thin liquidity, fewer pools)

| Pair | Pools | DEXes | Notes |
|------|-------|-------|-------|
| rETH/WETH | 3 | 2 | LST, decent UniV3 depth |
| DAI/USDbC | 7 | 2 | Stablecoin arb, but USDbC is declining |
| COMP/WETH | 4 | 1 | UniV3 only — no cross-DEX arb yet |
| SNX/WETH | 2 | 2 | Thin on both sides |

---

## Aerodrome: The Missing Piece

### The problem

Aerodrome is **Base's dominant DEX** (~40-50% of Base DEX volume). Our scanner found **zero** Aerodrome Slipstream (CL) pools despite trying the factory at `0x5e7BB104d84c7CB9B682AaC2F3d509f5F406809A` with tick spacings [1, 50, 100, 200].

### Why this matters

Most Base DEX volume flows through Aerodrome. Without it, we're only arbitraging between Uniswap V3 and SushiSwap V3 — which share similar market makers and have correlated pricing. Aerodrome's pricing is more independent, creating larger and more frequent spread opportunities.

### Root cause investigation needed

Aerodrome has **two pool types** that need separate integration:

1. **Aerodrome V2 (Solidly/Velodrome fork):**
   - Uses `vAMM` (volatile) and `sAMM` (stable) pool types
   - Factory: `0x420DD381b31aEf6683db6B902084cB0FFECe40Da`
   - Different router interface: `swapExactTokensForTokens` with `(amountIn, amountOutMin, routes[], to, deadline)` where `routes` includes `{from, to, stable, factory}`
   - NOT compatible with our current V2 or V3 router interfaces
   - This is where most Aerodrome WETH/USDC volume likely sits

2. **Aerodrome Slipstream (CL, concentrated liquidity):**
   - Factory at `0x5e7BB104d84c7CB9B682AaC2F3d509f5F406809A` (may need verification)
   - Uses tick spacings, not fee tiers — but the `getPool` ABI should be the same
   - Possible that Aerodrome uses different tick spacing values than [1, 50, 100, 200]
   - Router: `0xBE6D8f0d05cC4be24d5167a3eF062215bE6D18a5`
   - CL router interface may differ from standard V3 `exactInputSingle`

### Required work for Aerodrome integration

This is not a trivial addition. It requires:

1. **Research phase:**
   - Verify Aerodrome V2 factory address and pool discovery method
   - Verify Slipstream CL factory and enumerate valid tick spacings
   - Determine which pool type (V2 vs CL) holds WETH/USDC liquidity
   - Check Aerodrome's CL router ABI against standard V3

2. **ArbExecutor.sol changes (if Aerodrome V2):**
   - Need a new fee sentinel for Aerodrome V2 Solidly router
   - New `_swapSingle` branch for `IRouter.swapExactTokensForTokens` with routes struct
   - Redeploy the contract on Base

3. **Rust bot changes:**
   - New `DexType::AerodromeCL` and/or `DexType::AerodromeV2` variants
   - Pool syncer for Aerodrome pools (may use different event signatures)
   - Router address mapping and fee sentinel for executor
   - Quoter integration (Aerodrome may use a different quoter contract)

4. **If CL router is V3-compatible:**
   - Much simpler — just add as another V3-style DEX (like SushiV3 was added to Polygon)
   - Only need factory + router + quoter addresses
   - No ArbExecutor changes needed

**Recommendation:** Start by verifying whether Aerodrome CL uses a standard V3 router ABI. If yes, this is a config-only addition. If no, scope the router integration as a separate development phase.

---

## Bot Architecture Notes (for future sessions)

### What the Base bot runs on
- **Same binary** as Polygon: `dexarb-bot --chain base` loads `.env.base`
- **100% alloy** (migrated from ethers-rs on 2026-02-01)
- Atomic execution via ArbExecutor.sol (transferFrom pull model)
- Event-driven pool sync (eth_getLogs per block)
- V3 quoter pre-check (.call() simulation) + estimateGas implicit simulation + on-chain revert protection

### Trade sizing
- Currently **flat $100** (MAX_TRADE_SIZE_USD=100, no per-pool overrides in whitelist)
- Detector supports per-pool adaptive sizing via whitelist `max_trade_size_usd` field
- For thin SushiV3 pools, $100 is appropriate; for deep UniV3 pools, could increase

### Latency considerations
- Base sequencer is FCFS (Coinbase) — no MEV auction
- Bot runs on Alchemy WS from VPS (likely US region)
- Unknown latency to sequencer; competitive bots may be colocated
- The diagnostic run will reveal empirical capture rate

### Data collection
- Price logging: ENABLED (`data/base/price_history/prices_YYYYMMDD.csv`)
- Tax logging: ENABLED (`data/base/tax/`)
- Pool scanner output: `data/base/pool_scan_results.csv`

---

## Immediate Next Steps

### Priority 1: Monitor diagnostic run
- [ ] Wait for US/EU market hours (13:00-20:00 UTC) to see opportunity detection
- [ ] Check first execution attempts — capture rate, revert reasons, gas costs
- [ ] Review killswitch logs for gas spend trajectory

### Priority 2: Aerodrome research
- [ ] Verify Aerodrome V2 factory, discover WETH/USDC pool
- [ ] Test Aerodrome CL with expanded tick spacings (try 10, 20, 60, etc.)
- [ ] Check if Aerodrome CL router is V3-compatible (same `exactInputSingle` ABI)
- [ ] If V3-compatible: add to whitelist + config, rebuild, relaunch
- [ ] If not: scope the router integration (new ArbExecutor sentinel, new DexType)

### Priority 3: Pool expansion
- [ ] Run `scripts/base_pool_scanner.py` periodically to track liquidity changes
- [ ] Add DAI/WETH and DAI/USDC pairs to TRADING_PAIRS (deepest cross-DEX after WETH/USDC)
- [ ] Run depth assessment on expansion candidates (adapt `scripts/depth_assessment.py` for Base)
- [ ] Update `config/base/pools_whitelist.json` with new pools + per-pool trade size limits

### Priority 4: Infrastructure
- [ ] Research Base sequencer latency from current VPS
- [ ] Evaluate Hetzner or closer VPS for reduced RTT to sequencer
- [ ] Consider direct sequencer submission endpoint (if available)

---

## Wallet State (2026-02-03 02:25 UTC)

| Asset | Amount | Purpose |
|-------|--------|---------|
| USDC | $150.00 | Trading capital (MAX_TRADE_SIZE=$100, $50 buffer) |
| ETH | 0.005685 (~$18.76) | Gas for tx submission |
| USDC allowance | max_uint256 | ArbExecutor approved |

**Gas budget:** $1.00 killswitch (kills bot at 0.005382 ETH)

---

## Files Created/Modified This Session

| File | Action | Purpose |
|------|--------|---------|
| `.env.base` | Modified | MEMPOOL_MONITOR=off, POLL_INTERVAL_MS=2000, ROUTE_COOLDOWN_BLOCKS=10, LIVE_MODE=true |
| `scripts/base_pool_scanner.py` | Created | Base-specific pool discovery (UniV3, SushiV3, Aerodrome CL) |
| `scripts/base_gas_killswitch.sh` | Created | Gas spend monitor, kills bot at $1 threshold |
| `scripts/bot_status_discord.sh` | Modified | Base status: auto-detect LIVE/DRY-RUN, fixed log pattern |
| `data/base/pool_scan_results.csv` | Created | Full scan output (150 pools, 101 with liquidity) |
| `docs/base_next_steps.md` | Created | This file |
