# Pool Expansion Catalog — Polygon

**Date:** 2026-02-01
**Source:** Mempool pending swap data (603 decoded swaps) + factory contract queries (197 queries)
**Method:** Analyzed token pairs flowing through Alchemy mempool subscription, queried Uniswap V3 / SushiSwap V3 / QuickSwap V3 (Algebra) factory contracts, verified liquidity + quote depth via direct RPC.
**Verifier:** `scripts/verify_whitelist_enhanced.py` for USDC.e pools; direct slot0+liquidity+quoter for native USDC pools.

---

## Executive Summary

- **Current whitelist:** 23 active pools (16 V3 + 7 V2) across 6 USDC.e-quoted pairs
- **Factory discovery:** 61 new pools found not in whitelist/blacklist/observation
- **After verification:** 9 WHITELIST-quality, 3 MARGINAL, 49 DEAD/BLACKLIST
- **Critical finding:** All 10 USDC.e candidate pools = DEAD. All 9 viable pools use **native USDC** (`0x3c499c...`), not USDC.e (`0x2791Bca...`).
- **Blocker:** Bot's `QUOTE_TOKEN_ADDRESS` is USDC.e. Adding these pools requires dual-USDC support or quote token migration.

---

## Mempool Trading Volume by Pair

Top pairs observed in 603 decoded mempool swaps (known tokens only):

| Pair | Swaps | DEX Routes | In Whitelist? | In Simulator? |
|------|:-----:|------------|:---:|:---:|
| IMX/WETH | 67 | UniV3(3000) | no | no |
| USDC/USDC.e | 65 | UniV3(100), QS-V3 | no | no |
| 1INCH/USDT | 59 | UniV3(100) | no | no |
| USDC.e/WETH | 59 | UniV3(500), QS-V3 | YES | YES |
| USDC/WETH | 43 | UniV3(500,3000), QS-V3 | YES | YES |
| WBTC/WETH | 28 | UniV3(500), QS-V3 | no | no |
| USDT/WETH | 24 | UniV3(500), QS-V3 | no | no |
| USDT/WMATIC | 23 | UniV3(500) | no | no |
| WETH/WMATIC | 19 | UniV3(500,3000), QS-V3 | no | no |
| USDC/WMATIC | 17 | UniV3(500) | YES | YES |
| AAVE/WETH | 15 | UniV3(500) | no | no |
| SAND/USDT | 10 | UniV3(3000) | no | no |
| USDC.e/WMATIC | 7 | UniV3(500,3000), QS-V3 | YES | YES |

**Key insight:** WETH-quoted pairs (WBTC/WETH, USDT/WETH, WETH/WMATIC) have multi-DEX coverage = arb potential. But the bot only supports USDC-quoted pairs.

---

## Tier 1: Native USDC Pools — WHITELIST Quality (Requires QUOTE_TOKEN Change)

These 9 pools are deep, liquid, and pass the $5K depth test. They all use native USDC (`0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359`).

| Pair | DEX | Fee | Impact@$5K | Address | Notes |
|------|-----|-----|:---:|---------|-------|
| WETH/USDC | UniswapV3 | 0.05% | 0.4% | `0xa4d8c89f0c20efbe54cba9e7e7a7e509056228d9` | Parallel to existing USDC.e 0.05% pool |
| WMATIC/USDC | UniswapV3 | 0.05% | 0.7% | `0xb6e57ed85c4c9dbfef2a68711e9d6f36c56e0fcb` | Parallel to existing USDC.e 0.05% pool |
| WBTC/USDC | UniswapV3 | 0.05% | 0.3% | `0x32fae204835e08b9374493d6b4628fd1f87dd045` | Parallel to existing USDC.e 0.05% pool |
| WBTC/USDC | UniswapV3 | 0.30% | 0.7% | `0xe6ba22265aefe9dc392f544437acce2aedf8ef36` | New fee tier for WBTC |
| USDT/USDC | UniswapV3 | 0.01% | 0.0% | `0x31083a78e11b18e450fd139f9abea98cd53181b7` | Deep stablecoin |
| USDT/USDC | UniswapV3 | 0.05% | 0.0% | `0xee95696f77693af4f9b93850a9f48b4ad8e7a30a` | Deep stablecoin |
| DAI/USDC | UniswapV3 | 0.01% | 0.0% | `0xf369277650ad6654f25412ea8bfbd5942733babc` | Deep stablecoin |
| LINK/USDC | UniswapV3 | 0.30% | 0.7% | `0x79e4240e33c121402dfc9009de266356c91f241d` | First viable LINK pool for arb |
| AAVE/USDC | UniswapV3 | 0.30% | 4.3% | `0xc42bf5cd16d9eb1e892b66bb32a3892dcb7bb75c` | Entirely new pair |

### What Adding These Enables

- **LINK/USDC cross-DEX arb** — Currently monitoring-only (sole pool). Native USDC LINK pool + existing USDC.e pool = first arb pair for LINK.
- **AAVE/USDC** — New pair entirely. AAVE has 15 mempool swaps/session via AAVE/WETH route. Direct USDC pool adds arb surface.
- **WBTC/USDC fee=3000** — Adds a 0.30% fee tier counterpart. Different fee tiers between pools create persistent spreads (same pattern as SushiV3_030 ↔ UniV3_005 for WETH/USDC).
- **Stablecoin depth** — Native USDC stablecoin pools (USDT, DAI) have massive liquidity. Cross-USDC-variant arb becomes possible.

---

## Tier 2: Marginal Native USDC Pools

| Pair | DEX | Fee | Impact@$5K | Address |
|------|-----|-----|:---:|---------|
| WETH/USDC | UniswapV3 | 0.30% | 6.0% | `0x19c5505638383337d2972ce68b493ad78e315147` |
| WMATIC/USDC | UniswapV3 | 0.30% | 17.6% | `0x2db87c4831b2fec2e35591221455834193b50d1b` |
| AAVE/USDC (USDC.e) | UniswapV3 | 0.30% | 9.7% | `0xa236278bec0e0677a48527340cfb567b4e6e9adc` |

Usable at $100-$500 trade sizes. Consider for small-trade arb or as additional routing options.

---

## Tier 3: WETH-Quoted Pairs (Requires Bot Architecture Change)

These pairs have high mempool volume and multi-DEX coverage but require non-USDC quote token support in `detector.rs` and `executor.rs`.

| Pair | Swaps | DEX Routes | Arb Potential |
|------|:-----:|------------|:---:|
| WBTC/WETH | 28 | UniV3(500) + QuickswapV3 | YES — 2 DEXes |
| USDT/WETH | 24 | UniV3(500) + QuickswapV3 | YES — 2 DEXes |
| WETH/WMATIC | 19 | UniV3(500) + UniV3(3000) + QS-V3 | YES — 3 routes |
| IMX/WETH | 67 | UniV3(3000) only | No — single DEX |
| AAVE/WETH | 15 | UniV3(500) only | No — single DEX |

**71 multi-DEX swaps** across WBTC/WETH, USDT/WETH, WETH/WMATIC. These are discoverable via factory queries when ready.

---

## Tier 4: USDC/USDC.e Stablecoin Pair (Special Handling)

65 mempool swaps — highest volume non-WETH pair. Three UniswapV3 pools exist:

| Fee | Address | Liquidity | Notes |
|-----|---------|-----------|-------|
| 0.01% | `0xd36ec33c8bed5a9f7b6630855f1533455b98a418` | 24T | Deepest — stablecoin arb tier |
| 0.05% | `0xd9abecb39a5885d1e531ed3599adfed620e2fc8a` | 163T | Medium |
| 0.30% | `0x36f1f5d1fafb4b34bc42a39f06e5685d59a86166` | 1.1B | Thin |

Requires special bot handling: both tokens are USDC variants (6 decimals, 1:1 peg). Arb exists when large swaps create temporary depeg between USDC and USDC.e. Very low spread but very low gas cost on Polygon ($0.01).

---

## USDC.e Pools Verified as Dead

All discovered USDC.e pools (excluding those already tracked) have zero liquidity at current tick:

| Pair | Fee | Address | Status |
|------|-----|---------|--------|
| WBTC/USDC.e | 0.01% | `0xba91ae7312ace1137c15786177cbe687fd2d73d0` | liq=0 |
| LINK/USDC.e | 0.01% | `0x199892d638e35644dd896cc3b8dcb3c5c51af130` | liq=0 |
| UNI/USDC.e | 0.01% | `0x8800ebb4ea32996daf205ec10b5b5eb759b6137a` | liq=0 |
| AAVE/USDC.e | 0.01% | `0x6ee39efbe26e0c3da5effb78d9dbe9183fe0acb3` | liq=866M (below threshold) |
| AAVE/USDC.e | 0.05% | `0x693b52abdb6df2ea735eb19244a9e55374ebdf60` | quotes FAIL |
| SAND/USDC.e | 0.30% | `0xb69d18170a7d949777ead872cc6ba7cabb78fcfc` | liq=0 |
| CRV/USDC.e (×2) | 0.01/0.05% | `0xadd1...`, `0x9138...` | liq=0 |

**Conclusion:** Polygon V3 liquidity is migrating from USDC.e to native USDC. New pool deployments favor native USDC.

---

## Implementation Roadmap

### Phase A: Native USDC Support (prerequisite for all Tier 1 pools)

**Files to modify:**
- `.env.polygon` — Add `QUOTE_TOKEN_ADDRESS_NATIVE=0x3c499c...` or migrate `QUOTE_TOKEN_ADDRESS`
- `config.rs` — Parse dual quote tokens or migrate
- `detector.rs` — Handle both USDC variants in price comparison (both 6-decimal, near-parity)
- `executor.rs` — Trade size calc works unchanged (6 decimals)
- `types.rs` — Potentially add USDC variant to pair metadata
- `simulator.rs` — Add native USDC to `identify_pair()` constants
- `pools_whitelist.json` — Add Tier 1 pools with `"usdc_variant": "native"` tag

**Risk:** Low. Both USDC variants are 6 decimals and trade at 1:1 peg. The math is identical. Main work is plumbing — routing to correct token address.

### Phase B: WETH-Quoted Pairs (Tier 3)

Requires refactoring `detector.rs` to support non-USDC quote tokens. Profit calculation needs to handle WETH-denominated values (convert to USD via WETH/USDC price). Larger architectural change.

### Phase C: USDC/USDC.e Pair (Tier 4)

Special case — very narrow spreads, very high volume. Needs profitability analysis before implementation.

---

## Unknown Tokens Observed in Mempool

9 unidentified token addresses appeared in decoded swaps. Top by volume:

| Address | Swaps | Paired With |
|---------|:-----:|-------------|
| `0xeb51d9a39ad5eef215dc0bf39a8821ff804a0f01` | 62 | DAI, USDT |
| `0x49ddee75d588b79a3eb1225dd386644eeeeeaf08` | 24 | USDT |
| `0x311434160d7537be358930def317afb606c0d737` | 12 | WMATIC |
| `0x658cda444ac43b0a7da13d638700931319b64014` | 12 | WMATIC |
| `0xd2e57e7019a8faea8b3e4a3738ee5b269975008a` | 14 | USDC |

Could be identified via Polygonscan token lookup when expanding scope.

---

*Generated: 2026-02-01. Based on ~2h of mempool data + on-chain factory/quoter verification at block ~82406757.*
