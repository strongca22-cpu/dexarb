# Session Summary: Pool Expansion Scan — Native USDC Discovery

**Date:** 2026-02-01 (session ~08:00–09:30 UTC)
**Status:** Documentation complete, whitelist v1.5, AAVE/USDC added to TRADING_PAIRS
**Scope:** Future expansion catalog — no bot code changes this session

---

## Objective

Analyze mempool trading data and factory contracts to identify pools not yet in the whitelist/blacklist. Catalog all viable expansion candidates for future phases, once the narrow-and-deep approach (A4 Phase 3 execution) is proving results.

---

## Methodology

### 1. Mempool Pair Analysis

Analyzed 603 decoded pending swaps from `pending_swaps_20260201.csv`:

| Token | Appearances | Top Pairs |
|-------|------------|-----------|
| WETH | 258 | IMX/WETH (67), USDC.e/WETH (59), USDC/WETH (43), WBTC/WETH (28) |
| USDT | 191 | 1INCH/USDT (59), USDT/WETH (24), USDT/WMATIC (23) |
| WMATIC | 109 | WETH/WMATIC (19), USDT/WMATIC (23) |
| USDC.e | 131 | USDC.e/WETH (59), USDC/USDC.e (65) |
| USDC (native) | 120 | USDC/WETH (43), USDC/USDC.e (65) |

**Key insight:** WETH is the most traded token (258 appearances). Many high-volume pairs are WETH-quoted (WBTC/WETH, USDT/WETH, WETH/WMATIC), not USDC-quoted. The bot's current `detector.rs` only supports USDC-quoted pairs.

### 2. Factory Contract Queries

Queried 197 pool addresses across 3 factory contracts:

| Factory | Contract | Method |
|---------|----------|--------|
| Uniswap V3 | `0x1F98431c8aD98523631AE4a59f267346ea31F984` | `getPool(tokenA, tokenB, fee)` |
| SushiSwap V3 | `0x917933899c6a5F8E37F31E050010466EdF8Adde7` | `getPool(tokenA, tokenB, fee)` |
| QuickSwap V3 (Algebra) | `0x411b0fAcC3489691f28ad58c47006AF5E3Ab3A28` | `poolByPair(tokenA, tokenB)` |

Tokens queried: WETH, WMATIC, WBTC, USDT, DAI, LINK, AAVE, UNI, IMX, SAND, CRV
Fee tiers: 100, 500, 3000, 10000 (UniV3/SushiV3); dynamic (Algebra)
Both USDC.e and native USDC variants checked for each combination.

**Result:** 61 pool addresses discovered that are NOT in the existing whitelist/blacklist/observation lists.

### 3. Liquidity Verification

**Round 1 — Enhanced verifier (`verify_whitelist_enhanced.py`) on USDC.e pools:**
- 13 USDC.e candidate pools checked
- ALL 13 = BLACKLIST (zero liquidity at current tick, or quote failure)
- AAVE pools had liquidity but verifier couldn't quote (AAVE not in TOKEN_ADDRESSES config)

**Round 2 — Direct RPC verification on native USDC + AAVE pools:**
- Custom script querying slot0 + liquidity + QuoterV1
- 31 pools checked total
- Result: 9 WHITELIST, 3 MARGINAL, 19 DEAD/BLACKLIST

---

## Critical Finding: USDC.e → Native USDC Migration

**Every newly-discovered USDC.e pool is dead.** Liquidity on Polygon has migrated from bridged USDC.e (`0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174`) to native USDC (`0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359`).

All 9 viable new pools use native USDC. The bot currently only trades USDC.e pools (23 active). Supporting native USDC pools requires:
1. Dual quote-token support in `detector.rs` (both USDC addresses)
2. Native USDC token approval on ArbExecutor
3. Cross-USDC-variant opportunity detection (e.g., buy on USDC.e pool, sell on native USDC pool)

---

## Tier 1: Whitelist-Quality Native USDC Pools (9)

| Pool | Pair | DEX | Fee | Liquidity | Quote $500 |
|------|------|-----|-----|-----------|------------|
| `0xa4d8c89f` | WETH/USDC | UniswapV3 | 500 | 2.2×10¹⁸ | $499.61 |
| `0xb6e57ed3` | WMATIC/USDC | UniswapV3 | 500 | 6.8×10¹⁷ | $499.41 |
| `0x32fae2ee` | WBTC/USDC | UniswapV3 | 500 | 3.1×10¹⁵ | $498.91 |
| `0xe6ba22ac` | WBTC/USDC | UniswapV3 | 3000 | 7.1×10¹⁵ | $496.67 |
| `0x31083a98` | USDT/USDC | UniswapV3 | 100 | 1.2×10¹² | $499.99 |
| `0xee9569f4` | USDT/USDC | UniswapV3 | 500 | 3.5×10¹⁰ | $499.97 |
| `0xf36927e0` | DAI/USDC | UniswapV3 | 100 | 3.8×10²¹ | $499.99 |
| `0x79e42484` | LINK/USDC | UniswapV3 | 3000 | 5.7×10¹⁸ | $498.32 |
| `0xc42bf508` | AAVE/USDC | UniswapV3 | 3000 | 9.6×10¹⁸ | $497.84 |

## Tier 2: Marginal (3)

| Pool | Pair | DEX | Fee | Issue |
|------|------|-----|-----|-------|
| Various | UNI/USDC | UniswapV3 | 3000 | $495.23 quote (borderline) |
| Various | CRV/USDC | - | - | Low liquidity |
| Various | SAND/USDC | - | - | Quote failure |

## Tier 3: WETH-Quoted Pairs (Future — Requires Bot Architecture Changes)

High mempool volume but require `detector.rs` changes for non-USDC quote tokens:
- WBTC/WETH (28 swaps in sample)
- USDT/WETH (24 swaps)
- WETH/WMATIC (19 swaps, 3 DEX routes)

## Tier 4: Stablecoin Bridge (USDC/USDC.e)

- UniV3 fee=100 pool has 24T liquidity
- Potential bridge arb: buy on native pool, sell via USDC.e pool (or vice versa)
- Unique opportunity type — not currently supported

---

## Files Created/Modified

| File | Change |
|------|--------|
| `docs/pool_expansion_catalog.md` | NEW — Full catalog with tiers, addresses, implementation roadmap |
| `config/polygon/pools_whitelist.json` | v1.4 → v1.5: Added `native_usdc_candidates` section (9 pools, status=candidate_native_usdc) |
| `src/rust-bot/.env.polygon` | Added AAVE/USDC as 8th trading pair |
| `docs/next_steps.md` | Updated status header (whitelist v1.5), completed work table, key files, footer |

---

## Implementation Roadmap (Future — After A4 Phase 3 Proves Viable)

### Phase A: Native USDC Support
- Add native USDC address to `detector.rs` and `executor.rs`
- Dual quote-token detection (USDC.e + native USDC)
- ArbExecutor approval for native USDC
- Activate 9 candidate pools (change status to "active" in whitelist)

### Phase B: WETH-Quoted Pairs
- Add WETH as alternative quote token in `detector.rs`
- Price conversion: WETH→USD via WETH/USDC pool price
- New pairs: WBTC/WETH, USDT/WETH, WETH/WMATIC
- Requires profit calculation rework (currently assumes USD quote)

### Phase C: USDC/USDC.e Bridge Arb
- Cross-variant stablecoin arbitrage
- Unique detection logic (1:1 peg deviation)
- Low-risk, high-frequency potential

---

## Key Takeaways

1. **Liquidity migration is real.** USDC.e pools are dying; native USDC pools are where liquidity is moving on Polygon.
2. **9 quality pools ready to activate** once dual-USDC support is built.
3. **WETH-quoted pairs are the biggest volume** in the mempool but require architectural changes to support.
4. **This is future work.** Priority remains A4 Phase 3 (execution from mempool signals) — the narrow-and-deep approach on existing USDC.e pools.
5. **Whitelist v1.5** safely documents candidates without affecting the running bot (status != "active").

---

*Next: Continue collecting A4 Phase 2 simulation data (24h target), then build Phase 3 execution pipeline.*
