# Alloy Port Plan — ethers-rs → alloy Migration

**Created:** 2026-02-01
**Target:** Execute during Hetzner node setup (dead time while Bor syncs)
**Scope:** Full migration of `src/rust-bot/` from ethers-rs 2.0 to alloy
**Branch:** `feature/alloy-migration`

---

## Why Migrate

1. **ethers-rs is maintenance-only** — no new features, no bug fixes for edge cases
2. **alloy is the successor** — same team (Paradigm/Foundry), actively developed
3. **Micro-latency matters on a local node** — 1-2ms savings (10-15% of hot path) when network latency drops from 250ms to <1ms
4. **Native `rustls` TLS** — eliminates OpenSSL dependency (simpler cross-compilation)
5. **`sol!` macro** — compile-time ABI binding catches contract interface errors at build time
6. **Better async primitives** — fewer allocations, zero-copy patterns, modern tokio integration
7. **IPC transport** — first-class Unix socket support for local Bor node communication

---

## Current ethers-rs Footprint

### Cargo.toml Dependency
```toml
ethers = { version = "2.0", features = ["ws", "rustls", "abigen"] }
```

### Files Importing ethers (26 files total)

| Category | Files | Primary ethers Usage |
|----------|-------|---------------------|
| **Core types** | `types.rs`, `config.rs` | `Address`, `U256`, `Address::from_str()` |
| **Providers** | `main.rs`, `monitor.rs`, `data_collector/`, `paper_trading/` | `Provider<Ws>`, `Provider<Http>`, `subscribe_blocks()` |
| **Signing** | `main.rs`, `executor.rs` | `LocalWallet`, `SignerMiddleware` |
| **ABI/Contracts** | `executor.rs`, `syncer.rs`, `v2_syncer.rs`, `v3_syncer.rs` | `abigen!` macro (8 contract interfaces) |
| **ABI encoding** | `multicall_quoter.rs`, `decoder.rs` | `ethers::abi::{encode, decode}`, `ParamType`, `Token` |
| **Event filtering** | `main.rs` | `Filter`, `keccak256`, `H256`, log parsing |
| **Tx building** | `executor.rs` | `Eip1559TransactionRequest`, `TypedTransaction` |
| **Light usage** | `state.rs`, `calculator.rs`, `detector.rs`, `whitelist.rs`, `simulator.rs`, `mempool/types.rs` | `Address`, `U256`, `TxHash` only |

### abigen! Contracts (8 interfaces)

| Contract | File | Methods Used |
|----------|------|-------------|
| `IUniswapV2Router02` | executor.rs | `swapExactTokensForTokens`, `getAmountsOut` |
| `ISwapRouter` | executor.rs | `exactInputSingle` (V3) |
| `IQuoter` | executor.rs | `quoteExactInputSingle` (V3 QuoterV1) |
| `IQuoterV2` | executor.rs | `quoteExactInputSingle` (SushiSwap struct params) |
| `IAlgebraSwapRouter` | executor.rs | `exactInputSingle` (no fee param) |
| `IAlgebraQuoter` | executor.rs | `quoteExactInputSingle` (Algebra) |
| `IERC20` | executor.rs | `approve`, `allowance`, `balanceOf`, `decimals` |
| `IArbExecutor` | executor.rs | `executeArb` (custom contract) |
| `IUniswapV2Factory` | syncer.rs | `getPair`, `allPairs`, `allPairsLength` |
| `IUniswapV2Pair` | syncer.rs, v2_syncer.rs | `getReserves`, `token0`, `token1` |
| `UniswapV3Factory` | v3_syncer.rs | `getPool` |
| `UniswapV3Pool` | v3_syncer.rs | `slot0`, `liquidity`, `fee`, `token0`, `token1` |
| `AlgebraPool` | v3_syncer.rs | `globalState`, `liquidity`, `token0`, `token1` |
| `ERC20Metadata` | v3_syncer.rs | `decimals` |

---

## Migration Mapping

### Type Primitives

| ethers-rs | alloy | Notes |
|-----------|-------|-------|
| `ethers::types::Address` | `alloy::primitives::Address` | Same 20-byte type, different module path |
| `ethers::types::U256` | `alloy::primitives::U256` | alloy uses ruint (faster), different API surface |
| `ethers::types::H256` | `alloy::primitives::B256` | Renamed: H256 → B256 |
| `ethers::types::TxHash` | `alloy::primitives::TxHash` | Type alias for B256 |
| `ethers::types::U64` | `u64` | alloy uses native u64 for block numbers |
| `ethers::types::Bytes` | `alloy::primitives::Bytes` | Same concept |

### U256 API Changes (BREAKING — requires care)

```rust
// ethers-rs
U256::from(100)
U256::from_big_endian(&bytes)
value.low_u128()
value.as_u128()
U256::zero()
U256::one()

// alloy (ruint-based)
U256::from(100)
U256::from_be_bytes::<32>(bytes)       // fixed-size array
value.to::<u128>()                     // panics if overflow
value.try_into::<u128>()              // safe conversion
U256::ZERO
U256::from(1)
```

**Key differences:**
- `U256::from_big_endian()` → `U256::from_be_slice()` or `U256::from_be_bytes()`
- `.low_u128()` → `.to::<u128>()` (panics on overflow) or manual masking
- Arithmetic operators work the same way (`+`, `-`, `*`, `/`)
- Comparison operators work the same way

### Provider & Transport

| ethers-rs | alloy | Notes |
|-----------|-------|-------|
| `Provider::<Ws>::connect(url)` | `ProviderBuilder::new().on_ws(WsConnect::new(url)).await` | Builder pattern |
| `Provider::<Http>::try_from(url)` | `ProviderBuilder::new().on_http(url.parse()?)` | Builder pattern |
| N/A | `ProviderBuilder::new().on_ipc(IpcConnect::new(path))` | NEW: Unix socket to local node |
| `provider.subscribe_blocks()` | `provider.subscribe_blocks().await` | Similar API |
| `provider.get_logs(&filter)` | `provider.get_logs(&filter).await` | Similar API |
| `provider.get_transaction(hash)` | `provider.get_transaction_by_hash(hash).await` | Renamed |
| `provider.get_transaction_receipt(hash)` | `provider.get_transaction_receipt(hash).await` | Same |
| `provider.send_raw_transaction(bytes)` | `provider.send_raw_transaction(&bytes).await` | Ref instead of owned |
| `provider.estimate_gas(&tx, None)` | `provider.estimate_gas(&tx).await` | Simplified |
| `provider.call(&tx, None)` | `provider.call(&tx).await` | Simplified |

### Wallet & Signing

| ethers-rs | alloy | Notes |
|-----------|-------|-------|
| `key.parse::<LocalWallet>()?.with_chain_id(id)` | `PrivateKeySigner::from_str(key)?` | Chain ID set on tx, not wallet |
| `SignerMiddleware::new(provider, wallet)` | `ProviderBuilder::new().wallet(wallet).on_ws(...)` | Built into provider |
| `wallet.sign_transaction(&tx)` | `wallet.sign_transaction(&tx).await` | Same concept |
| `wallet.address()` | `wallet.address()` | Same |

### Contract Interaction

| ethers-rs | alloy | Notes |
|-----------|-------|-------|
| `abigen!(Name, abi_json)` | `sol!("path/to/abi.json")` or inline `sol!` | Compile-time, catches errors |
| `Contract::new(addr, provider)` | Generated struct constructor | Type-safe |
| `.call().await?` | `.call().await?` | Similar |
| `.send().await?.wait_for_confirmations(1)` | `.send().await?.watch().await?` | Different confirmation API |

**sol! macro example (replaces abigen!):**
```rust
sol! {
    #[sol(rpc)]
    interface IArbExecutor {
        function executeArb(
            address token0, address token1,
            address routerBuy, address routerSell,
            uint24 feeBuy, uint24 feeSell,
            uint256 amountIn, uint256 minProfit
        ) external returns (uint256 profit);
    }
}

// Usage:
let contract = IArbExecutor::new(address, &provider);
let result = contract.executeArb(t0, t1, rb, rs, fb, fs, amt, min).send().await?;
```

### ABI Encoding/Decoding

| ethers-rs | alloy | Notes |
|-----------|-------|-------|
| `ethers::abi::encode(&[Token::Address(a)])` | `sol_data::Address::abi_encode(&a)` | Type-safe encoding |
| `ethers::abi::decode(&[ParamType::Address], data)` | `sol_data::Address::abi_decode(data)` | Type-safe decoding |
| `Token::Address(a)` | Direct type use | No Token enum needed |
| `Token::Uint(U256::from(x))` | Direct U256 use | Cleaner |
| `Token::Tuple(vec![...])` | Struct with `#[derive(SolType)]` | Compile-time struct |
| `ethers::utils::keccak256(bytes)` | `alloy::primitives::keccak256(bytes)` | Same output |

### Event Filtering

| ethers-rs | alloy | Notes |
|-----------|-------|-------|
| `Filter::new().from_block(n).address(addrs).topic0(topics)` | `Filter::new().from_block(n).address(addrs).event_signature(topics)` | Minor rename |
| `log.topics[0]` | `log.topics()[0]` | Method instead of field |
| `log.data` | `log.data().data` | Nested access |
| `log.address` | `log.address()` | Method |

### Transaction Building

| ethers-rs | alloy | Notes |
|-----------|-------|-------|
| `Eip1559TransactionRequest { ... }` | `TransactionRequest::default().with_...()` | Builder pattern |
| `tx.chain_id = Some(id.into())` | `tx = tx.with_chain_id(id)` | Builder |
| `tx.max_fee_per_gas = Some(fee)` | `tx = tx.with_max_fee_per_gas(fee)` | Builder |
| `tx.gas = Some(limit)` | `tx = tx.with_gas_limit(limit)` | Builder |
| `TypedTransaction::Eip1559(tx)` | `TransactionRequest` (unified) | No enum wrapping |

---

## Migration Phases

### Phase 0: Preparation (before migration starts)
- [ ] Create `feature/alloy-migration` branch
- [ ] Snapshot current test results (73/73 pass baseline)
- [ ] Read alloy docs: https://docs.rs/alloy/latest/alloy/
- [ ] Verify alloy version supports all needed features (IPC, sol!, subscriptions)

### Phase 1: Cargo.toml + Type Primitives (~30 min)
**Lowest risk. Mechanical find-and-replace.**

Replace Cargo.toml dependency:
```toml
# Remove
ethers = { version = "2.0", features = ["ws", "rustls", "abigen"] }

# Add (updated for alloy 1.5.2)
alloy = { version = "1.5", features = [
    "provider-ws",    # WebSocket transport
    "provider-ipc",   # IPC transport (NEW — for local Bor node)
    "pubsub",         # Block/tx subscriptions
    "dyn-abi",        # Dynamic ABI encoding/decoding (replaces ethers::abi)
    "json",           # JSON serialization for ABI
] }
# Note: "default" features include: essentials (contract, provider-http, rpc-types, signer-local),
# reqwest-rustls-tls, sol-types, json-abi, etc. — no need to list them explicitly.
```

Files to update (type imports only — 10 files):
1. `types.rs` — `Address`, `U256`
2. `config.rs` — `Address`
3. `state.rs` — `Address`
4. `calculator.rs` — `Address`, `U256`
5. `detector.rs` — `Address`, `U256`
6. `whitelist.rs` — `Address`
7. `simulator.rs` — `Address`, `TxHash`, `U256`
8. `mempool/types.rs` — `Address`, `TxHash`, `U256`
9. `data_collector/shared_state.rs` — `Address`, `U256`
10. `cooldown.rs` — (if any)

**Build + test after this phase.** Many files will compile with just import path changes.

### Phase 2: U256 API Surface (~1-2 hours)
**Medium risk. Requires careful attention to overflow behavior.**

Key changes across all files using U256 arithmetic:
- `U256::from_big_endian(&bytes)` → `U256::from_be_slice(&bytes)`
- `.low_u128()` → `.to::<u128>()` or manual mask
- `U256::zero()` → `U256::ZERO`
- `U256::one()` → `U256::from(1)`

Files affected:
1. `types.rs` — V2 constant product math, V3 sqrt price
2. `calculator.rs` — price impact calculations
3. `main.rs` — event data parsing (`U256::from_big_endian`)
4. `executor.rs` — gas price calculations, amount conversions
5. `multicall_quoter.rs` — calldata encoding
6. `decoder.rs` — calldata decoding
7. `simulator.rs` — AMM math

**Build + test after this phase.** Arithmetic correctness is critical.

### Phase 3: ABI Encoding/Decoding (~1-2 hours)
**Medium risk. decoder.rs and multicall_quoter.rs are the most complex.**

Replace `ethers::abi::{encode, decode, ParamType, Token}` with alloy's `sol_data` types.

Key files:
1. `multicall_quoter.rs` — Multicall3 calldata encoding, quoter return decoding
2. `decoder.rs` — Mempool calldata decoding (11 function selectors)

**Note:** The raw selector-based decoding in `decoder.rs` can largely stay as-is (raw byte manipulation). Only the `ethers::abi::decode()` calls need to change to alloy's `sol_data` decoders.

**Build + test after this phase.**

### Phase 4: Contract Interfaces — sol! Macro (~2-3 hours)
**Higher risk. Replaces all abigen! with sol! macro. Most impactful change.**

Replace 14 `abigen!` macro invocations across 4 files:
1. `executor.rs` — 8 contracts (routers, quoters, ERC20, ArbExecutor)
2. `syncer.rs` — 2 contracts (V2 factory, V2 pair)
3. `v2_syncer.rs` — 2 contracts (V2 pair, ERC20)
4. `v3_syncer.rs` — 4 contracts (V3 factory, V3 pool, Algebra pool, ERC20)

**Approach:** Create a `src/contracts/` module with all sol! definitions in one place, replacing scattered abigen! calls. This centralizes ABI management.

```rust
// src/contracts/mod.rs
sol! {
    #[sol(rpc)]
    interface IERC20 {
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function balanceOf(address account) external view returns (uint256);
        function decimals() external view returns (uint8);
    }

    #[sol(rpc)]
    interface IArbExecutor {
        function executeArb(
            address token0, address token1,
            address routerBuy, address routerSell,
            uint24 feeBuy, uint24 feeSell,
            uint256 amountIn, uint256 minProfit
        ) external returns (uint256 profit);
    }

    // ... all other interfaces
}
```

**Build + test after this phase.** Contract call semantics may differ.

### Phase 5: Provider & Transport (~1-2 hours)
**High risk. Core infrastructure change.**

Files affected:
1. `main.rs` — WS provider setup, block subscription, event filtering
2. `executor.rs` — `TradeExecutor<M: Middleware>` → alloy provider pattern
3. `monitor.rs` — Mempool WS subscription
4. `v2_syncer.rs` — `V2PoolSyncer<P: Middleware>` generic
5. `v3_syncer.rs` — `V3PoolSyncer<P: Middleware>` generic
6. `data_collector/mod.rs` — Provider setup
7. `bin/data_collector.rs` — Provider setup

**Key architectural change:** ethers-rs uses `Middleware` trait for generic provider abstraction. alloy uses `Provider` trait. All `<M: Middleware>` generics become `<P: Provider>` (or concrete types for simplicity).

**Consider:** On the Hetzner node, we may want IPC transport. Design the provider setup to support `Ws | Ipc` via alloy's `BoxTransport` or feature flags.

**Build + test after this phase.**

### Phase 6: Transaction Building & Signing (~1-2 hours)
**High risk. Directly affects on-chain execution.**

Files affected:
1. `executor.rs` — `execute_from_mempool()`, `execute_atomic()`, wallet setup
2. `main.rs` — Wallet initialization

Key changes:
- `Eip1559TransactionRequest` → `TransactionRequest` builder
- `LocalWallet` → `PrivateKeySigner`
- Manual chain_id setting → builder `.with_chain_id()`
- `fill_transaction` → alloy's `fill` or manual nonce/gas setting
- `send_raw_transaction` → similar API

**Build + test. Then test on Polygon testnet (Mumbai) before mainnet.**

### Phase 7: IPC Transport (NEW capability)
**Only possible after migration. This is the latency win.**

Add IPC support for local Bor node:
```rust
// .env.polygon (on Hetzner)
RPC_URL=ipc:///var/run/bor/bor.ipc
RPC_WS_URL=ws://localhost:8546  # fallback

// main.rs — detect transport from URL scheme
let provider = if config.rpc_url.starts_with("ipc://") {
    ProviderBuilder::new().on_ipc(IpcConnect::new(path)).await?
} else if config.rpc_url.starts_with("ws://") {
    ProviderBuilder::new().on_ws(WsConnect::new(url)).await?
} else {
    ProviderBuilder::new().on_http(url.parse()?)?
};
```

### Phase 8: Final Validation
- [ ] All 73+ tests pass
- [ ] Clean `cargo build --release`
- [ ] Deploy to current VPS in monitor mode (read-only, no execution)
- [ ] Compare block arrival times, event parsing, mempool decoding with ethers-rs baseline
- [ ] Execute one test trade on Polygon mainnet
- [ ] Verify mempool pipeline end-to-end
- [ ] Deploy to Hetzner with IPC transport

---

## Risk Mitigation

1. **Keep ethers-rs branch alive** — Don't delete `main` branch. If alloy has a showstopper, revert.
2. **Phase-by-phase testing** — Build + test after every phase. Don't batch changes.
3. **Current VPS stays live** — ethers-rs bot runs on current VPS while alloy version is validated on Hetzner.
4. **Same contract ABIs** — The on-chain contracts (ArbExecutor, pools) don't change. Only the Rust-side interface changes.
5. **Parallel deployment** — Run alloy version in monitor-only mode alongside ethers-rs production bot until confident.

---

## Alloy Version Selection

**Updated 2026-02-01:** alloy `1.5.2` is the latest stable release (was 0.9.x when plan was written).
Rust minimum: 1.88 (we have 1.93.0). All needed features confirmed available:
- `provider-ws` — WebSocket transport (replaces ethers `ws` feature)
- `provider-ipc` — Unix socket transport for local Bor node
- `signer-local` — PrivateKeySigner (replaces LocalWallet)
- `contract` — includes sol! macro, dyn-abi, json-abi, providers
- `pubsub` — block subscriptions, pending tx subscriptions
- `rpc-types` — Transaction, Block, Log, TransactionReceipt
- `dyn-abi` — dynamic ABI encode/decode (replaces ethers::abi)
- `sol-types` — sol! macro for compile-time ABI

**Note:** alloy 1.x `default` features include `essentials` (contract + provider-http + rpc-types + signer-local) and `reqwest-rustls-tls`. We need to add `provider-ws`, `provider-ipc`, and `pubsub` explicitly.

**Checked via:** `cargo search alloy` → 1.5.2, `cargo info alloy` → features confirmed, `cargo add --dry-run` → all features resolve.

---

## Estimated Scope

| Phase | Files Changed | Risk | Dependency |
|-------|--------------|------|------------|
| 1. Cargo + types | 10-12 | Low | None |
| 2. U256 API | 7 | Medium | Phase 1 |
| 3. ABI encode/decode | 2 | Medium | Phase 1 |
| 4. Contract interfaces | 4-5 | Medium-High | Phase 1 |
| 5. Provider/transport | 6-7 | High | Phases 1-4 |
| 6. Tx building/signing | 2 | High | Phases 1-5 |
| 7. IPC transport | 2 | Low | Phase 5 |
| 8. Validation | 0 | N/A | All phases |

**Total files modified:** ~20 (out of ~26 that import ethers)

---

*This plan should be executed as a dedicated Claude Code session with full focus on the migration. Do not mix with feature work. Each phase ends with a successful build + test run.*
