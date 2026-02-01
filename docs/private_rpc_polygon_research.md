# Private / MEV-Protected RPC Options for Polygon

Research Date: 2026-01-31
Status: Active research — options tested against live Polygon mainnet

---

## Context

The dexarb bot submits atomic arbitrage transactions on Polygon. Transactions sent
through standard public RPC endpoints are visible in the mempool, allowing competing
MEV bots to frontrun or sandwich them. This document evaluates private/MEV-protected
RPC alternatives for Polygon specifically.

## Critical Finding: Limited Polygon Options

Unlike Ethereum mainnet (which has Flashbots Protect, MEV Blocker, etc.), Polygon has
very few private mempool services. Most major MEV protection solutions are Ethereum-only.
Flashbots does NOT operate on Polygon.

Polygon's ~2 second block time is a natural partial defense — the frontrunning window
is much shorter than Ethereum's 12 seconds.

---

## Options Tested (2026-01-31)

### 1. 1RPC by Automata Network — CURRENTLY IN USE

| Field | Value |
|-------|-------|
| Endpoint | `https://1rpc.io/matic` |
| Chain ID | 0x89 (137) |
| eth_sendRawTransaction | Supported (verified) |
| Cost | Free, no signup required |
| Status | Active, working |

**What it does:**
- TEE-based (Trusted Execution Environment) privacy relay
- Strips IP addresses, device fingerprints, timing metadata
- "Burn after relaying" — zero data retention
- 64+ chains supported, 20B+ relays processed

**What it does NOT do:**
- Does NOT provide MEV protection
- Does NOT use a private mempool — transactions still enter the public Polygon mempool
- Does NOT prevent frontrunning — competing bots can still see your transaction

**Honest assessment:** Provides metadata privacy (hides your identity from the RPC
operator), but offers zero protection against MEV bots. Strictly better than a naked
public RPC, but the bot's transactions are still publicly visible in the mempool.

**Current bot config:**
```
PRIVATE_RPC_URL=https://1rpc.io/matic
```

### 2. OMNIA Protocol — PUBLIC ENDPOINT BLOCKS TX SUBMISSION

| Field | Value |
|-------|-------|
| Public endpoint | `https://endpoints.omniatech.io/v1/matic/mainnet/public` |
| Custom endpoints | Via https://app.omniatech.io dashboard |
| eth_chainId | Works (returns 0x89) |
| eth_sendRawTransaction | **BLOCKED** — returns `-32603 Internal JSON-RPC error` (public endpoint) |
| Cost | Free public endpoints; premium features require $OMNIA token |
| Status | Active for reads, **does not accept tx submission** on public endpoint |

**Claims:**
- Private mempool routing via "Flashbots integration"
- MEV cashback (revenue sharing from auction houses)
- Frontrunning + sandwich protection
- "Reinforced transactions" (guaranteed execution)
- 33+ chains including Polygon

**Test results (2026-01-31):**
- `eth_chainId` → `0x89` (correct, Polygon mainnet)
- `eth_sendRawTransaction` with dummy tx → `-32603 Internal JSON-RPC error`
  (contrast: 1RPC returns proper node errors like `rlp: element is larger than containing list`)
- The public endpoint appears to block `eth_sendRawTransaction` entirely.
  This makes sense — MEV protection likely requires a custom/authenticated endpoint.

**Concerns:**
- Flashbots does NOT operate on Polygon, so the "Flashbots integration" claim
  likely applies to Ethereum only. Unclear what mechanism protects Polygon txs.
- Documentation mixes Ethereum-specific features with multi-chain marketing copy,
  making it hard to distinguish what actually works on Polygon.

**Next step:** Register at app.omniatech.io, generate a custom Polygon endpoint with
MEV protection enabled, and test whether eth_sendRawTransaction works through a
custom endpoint (it does not work on the public one).

### 3. bloXroute BDN — DOES NOT SUPPORT POLYGON TX SUBMISSION

| Field | Value |
|-------|-------|
| Endpoint | `https://api.blxrbdn.com` (API key required) |
| Alt endpoint | `https://polygon.rpc.blxrbdn.com` (returned empty response without auth) |
| Private tx method | `blxr_private_tx` (ETH only), `bsc_private_tx` (BSC only) |
| Polygon private tx | **Does not exist** — no `polygon_private_tx` in docs (404) |
| Regular tx (`blxr_tx`) | Supports Mainnet (ETH) and BSC-Mainnet only — **Polygon not listed** |
| Cost | Introductory (free), Professional ($300/mo), Enterprise ($1,250/mo) |
| Status | Active for ETH/BSC; **Polygon tx submission NOT supported** |

**What bloXroute actually offers on Polygon:**
- BDN network connectivity (faster block/tx propagation for Gateway nodes)
- Stream feeds (newTxs, pendingTxs, newBlocks)
- Does NOT offer transaction submission or private transactions on Polygon

**Documentation verified (2026-01-31):**
- `blxr_private_tx` docs: "Ethereum Mainnet only" — no Polygon mention
- `bsc_private_tx` docs: BSC only — no Polygon mention
- `blxr_tx` (regular submit): supports `Mainnet` and `BSC-Mainnet` only
- No `polygon_private_tx` page exists (returns 404)
- Auth header format: `base64(account_id:secret_hash)` — tested with account ID
  `6f62f509-cef3-4e13-bc30-90ccf0637323`, returned "Invalid account ID" (needs
  secret_hash from portal)

**Auth format (for reference):**
```
Authorization: base64(account_id:secret_hash)
# Get both values from bloXroute portal Account section
```

**Conclusion:** bloXroute markets BDN connectivity on Polygon but their Cloud-API
for transaction submission (including private transactions) is ETH/BSC only. Not
a viable option for Polygon arb bot tx submission.

### 4. Polygon FastLane (PFL) — SERVICE DOWN

| Field | Value |
|-------|-------|
| Documented endpoint | `https://polygon-rpc.fastlane.xyz` |
| Status | **NXDOMAIN** — domain does not resolve (tested 2026-01-31) |
| Alt endpoint | `https://rpc.fastlane.xyz` resolves but returns SSL 525 errors |
| Method | `pfl_addSearcherBundle` (non-standard) |
| Cost | Free submission |

**What it was:**
- Official Polygon MEV solution by FastLane Labs
- Validator-centric auction system with 65%+ coverage
- Designed for backrun auctions, not simple private relay
- Required deploying a Solver Contract (ISolverContract interface)
- Used EIP-712 signed messages, not raw transactions

**Current status:** Service appears deprecated or moved. The documented RPC endpoint
returns NXDOMAIN. GitHub repos (FastLane-Labs) still exist but may not be maintained.

**Integration complexity (if revived):**
- Requires deploying a smart contract on Polygon
- Uses EIP-712 signatures, not eth_sendRawTransaction
- Bundles are rebroadcast to public mempool (not truly private)
- Designed for backruns, not frontrunning protection

**Not recommended** unless the service comes back online with a simpler integration path.

### 5. Merkle — B2B ONLY

| Field | Value |
|-------|-------|
| Website | https://www.merkle.io/ |
| Polygon support | Claimed ("Ethereum, BSC, Polygon, Base, Solana & Arbitrum") |
| Cost | Unknown (B2B pricing, ~$0.20-0.30/tx on Ethereum) |
| Status | Active but requires business relationship |

**Assessment:** Enterprise-grade private mempool designed for wallets and RPC providers.
Likely not accessible to individual searchers/bots. Would require email/sales contact.

### 6. dRPC — MEV PROTECTION IS PAID ONLY

| Field | Value |
|-------|-------|
| Endpoint | `polygon.drpc.org` |
| Cost | MEV protection requires paid plan ($1+/month) |
| Status | Active |

**Assessment:** Standard RPC with optional MEV protection behind paywall. Not tested.

### 7. Shutter Network — EXPERIMENTAL

| Field | Value |
|-------|-------|
| Technology | Zero-knowledge proofs, pre-confirmation encryption |
| Polygon support | Claimed but unclear production status |
| Cost | Free up to 500 txs/day, 0.1% fee for unlimited |

**Assessment:** Mentioned for Polygon but unclear if fully operational. Not tested.

### 8. Marlin MEV-Bor Relay — DEPRECATED

| Field | Value |
|-------|-------|
| Endpoint | `bor.txrelay.marlin.org` (likely offline) |
| Status | Last update 2022, appears abandoned |

---

## Comparison Table (Verified Facts Only)

| Solution | Polygon Tx Submit | Drop-in RPC | Private Mempool | MEV Protection | Free | Tested |
|----------|:---:|:---:|:---:|:---:|:---:|:---:|
| 1RPC | **Yes** | Yes | No | No (metadata only) | Yes | **Yes** |
| OMNIA public | **No** (blocks sendRawTx) | No | N/A | N/A | Yes | **Yes** |
| OMNIA custom | Unknown | Unknown | Unverified | Unverified | Unknown | No |
| bloXroute | **No** (ETH/BSC only) | No | N/A | N/A | Free tier | **Yes** |
| FastLane | **No** (NXDOMAIN) | No | No (public rebroadcast) | Auction-based | Yes | **Yes** |
| Merkle | Claimed | Unknown | Unknown | Unknown | No | No |
| dRPC | Yes | Yes | Paid only | Paid only | No | No |

---

## Current Bot Configuration

```
# .env.polygon
PRIVATE_RPC_URL=https://1rpc.io/matic
```

The bot uses this URL for `eth_sendRawTransaction` when submitting atomic arb
transactions. All reads (pool sync, Quoter, estimateGas) stay on the Alchemy WS
provider. The 1RPC endpoint provides metadata privacy but not MEV protection.

Code path: `executor.rs` → `tx_client` (SignerMiddleware<Provider<Http>>) → 1RPC
→ public Polygon mempool → block inclusion.

Rollback: Comment out `PRIVATE_RPC_URL` to send via Alchemy WS (public mempool).

---

## Recommended Next Steps (Priority Order)

1. **Run with 1RPC** — Currently deployed. Free, working, provides metadata privacy.
   While not MEV protection, it's the only free drop-in option verified to work.

2. **Test OMNIA custom endpoint** — Register at app.omniatech.io, generate a custom
   Polygon endpoint with MEV protection enabled. The public endpoint blocks
   eth_sendRawTransaction, but a custom/authenticated endpoint may work differently.
   This is the most promising lead for actual MEV protection on Polygon.

3. **Monitor 1RPC performance** — Track whether transactions submitted through 1RPC
   get frontrun at the same rate as public RPC. If metadata privacy alone reduces
   targeting, it may be sufficient for Polygon's low-value arb opportunities.

4. **Consider Polygon's natural defense** — With ~2s block times and sub-$0.05 gas,
   the MEV extraction window is much smaller than Ethereum. The bot's atomic contract
   (revert-on-loss) already prevents sandwich losses. The main risk is being outbid
   on the same opportunity, not being sandwiched.

**Eliminated options:**
- bloXroute: Does not support Polygon tx submission (ETH/BSC only)
- FastLane: Service down (NXDOMAIN)
- Merkle: B2B only, no individual access

---

## FastLane Deep-Dive (Updated 2026-02-01)

Thorough research confirmed FastLane has abandoned Polygon:

- **Chainlink acquired Atlas IP** on Jan 22, 2026 (9 days before this research)
- **FastLane pivoted to Monad** throughout 2025 — all Twitter/GitHub activity is Monad-focused
- GitHub: Polygon-related repos (`sentry-patch`, `bdn-operations-relay`) last touched Oct-Nov 2024
- Active repos: all Monad-focused (`atlas-on-monad`, `fastlane-contracts` for Monad)
- Both `polygon-rpc.fastlane.xyz` and `beta-rpc.fastlane-labs.xyz` return **NXDOMAIN**
- Last Polygon forum update: Dec 2023
- Atlas now exclusively powers Chainlink SVR — Polygon not mentioned

**Conclusion:** No MEV auction infrastructure exists on Polygon. The only competitive mechanism is Priority Gas Auction (PGA) — bid higher gas in the public mempool.

---

## Alchemy Mempool Access (Updated 2026-02-01)

An alternative to private RPC: use Alchemy's `alchemy_pendingTransactions` to **see** the mempool and backrun opportunities, rather than trying to **hide** from it.

**Verified on free tier (2026-02-01):**
- `alchemy_pendingTransactions` — works via WS, supports `toAddress` filter + `hashesOnly`
- `eth_subscribe("logs")` — works, for real-time pool Swap/Sync events
- Measured: ~200 DEX pending txs/min through Alchemy's Polygon Bor nodes (99% QuickSwapV2)

**Bor node context:** Polygon runs on Bor (modified Geth). Each validator has its own Bor node with its own mempool. Transactions propagate via p2p gossip between Bor nodes. Alchemy runs its own Bor nodes, so `alchemy_pendingTransactions` only shows txs that reach Alchemy's nodes — a partial but significant view (Alchemy is a major provider).

**CU budget impact:**
| Subscription | CU/month | Fits free tier? |
|-------------|----------|----------------|
| Log events (23 pools) | ~1M | Yes |
| Pending V3 routers (full tx) | ~3.5M | Yes |
| Pending all routers (hashesOnly) | ~23M | Borderline |
| Pending all routers (full tx) | ~346M | No ($152/mo PAYG) |

---

## What We Are NOT Using (and Why)

- **Flashbots Protect** — Ethereum only, does not support Polygon
- **MEV Blocker** — Ethereum only
- **Blink** — Ethereum only
- **Marlin** — Deprecated (2022)
- **FastLane** — Dead on Polygon (pivoted to Monad, Chainlink acquired Atlas IP)
