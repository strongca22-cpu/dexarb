# QuickSwap V3 (Algebra) Integration Reference

**Date:** 2026-01-30  
**Status:** Ready for integration  
**DEX:** QuickSwap V3 (Algebra Protocol)  
**Chain:** Polygon

---

## Contract Addresses (Polygon Mainnet)

| Contract | Address |
|----------|---------|
| Factory | `0x411b0fAcC3489691f28ad58c47006AF5E3Ab3A28` |
| PoolDeployer | `0x2D98E2FA9da15aa6dC9581AB097Ced7af697CB92` |
| SwapRouter | `0xf5b509bB0909a69B1c207E495f687a596C168E12` |
| QuoterV2 | `0xa15F0D7377B2A0C0c10db057f641beD21028FC89` |
| NonfungiblePositionManager | `0x8eF88E4c7CfbbaC1C163f7eddd4B578792201de6` |

**Constants:**
```
POOL_INIT_CODE_HASH = 0x6ec6c9c8091d160c0aa74b2b14ba9c1717e95093bd3ac085cee99a49aab294a4
```

---

## Critical Differences from Uniswap V3

### 1. **NO FEE TIERS - Single Pool Per Pair**
- **Uniswap V3:** Multiple pools per pair (0.01%, 0.05%, 0.3%, 1%)
- **Algebra:** ONE pool per pair with **dynamic fees**
- Fee automatically adjusts based on volatility/volume (avg 0.1-0.15%)

### 2. **No Fee Parameters in Code**
```rust
// Uniswap V3
factory.getPool(token0, token1, fee)
quoter.quoteExactInputSingle(tokenIn, tokenOut, fee, ...)

// QuickSwap V3 (Algebra)
factory.poolByPair(token0, token1)  // NO fee parameter
quoter.quoteExactInputSingle(tokenIn, tokenOut, ...)  // NO fee parameter
```

### 3. **Pool Address Calculation**
```rust
// Use poolDeployer (NOT factory) and NO fee in CREATE2
let pool_address = keccak256(
    abi.encodePacked(
        hex"ff",
        pool_deployer,  // 0x2D98E2FA9da15aa6dC9581AB097Ced7af697CB92
        keccak256(abi.encode(token0, token1)),  // NO fee
        POOL_INIT_CODE_HASH
    )
);
```

### 4. **Interface Names**
- `IUniswapV3Pool` → `IAlgebraPool`
- `uniswapV3SwapCallback` → `algebraSwapCallback`
- Otherwise mostly compatible

---

## Dynamic Fees vs Static Fees: Arbitrage Edge

**Key Insight:** QuickSwap's dynamic fees create persistent arbitrage opportunities against fixed-fee DEXs.

### How It Works

1. **QuickSwap fee adjusts based on recent volatility** (updates every N blocks)
2. **Uni/Sushi have fixed fees** (0.05%, 0.3%, etc.)
3. **Price moves faster than fee adjustments** → spread opportunity

### Example Scenario
```
Time 0:
- WETH/USDC experiences volatility spike
- QuickSwap fee: 0.15% (still low from previous calm period)
- UniswapV3 0.05% pool: 0.05% (fixed)
- UniswapV3 0.30% pool: 0.30% (fixed)

Time 1 (price movement):
- WETH price jumps 0.5%
- Arb: Buy WETH on QuickSwap (0.15%) → Sell on Uni 0.05% pool
- Net spread: ~0.30% (0.5% - 0.15% - 0.05%)

Time 2 (fee adjustment):
- QuickSwap fee adjusts to 0.25% (higher due to volatility)
- Opportunity window closes
```

### Why This Creates Edge

**Static fee pools** (Uni, Sushi):
- Predictable cost structure
- Liquidity fragments across multiple fee tiers
- Each tier optimizes for different scenarios

**Dynamic fee pool** (QuickSwap):
- **Lags behind volatility** (adjustment delay)
- **All liquidity in one pool** (no fragmentation)
- Fee can be "wrong" for current market conditions

**Arbitrage Sweet Spots:**
1. **Volatility spikes** - QuickSwap fee hasn't caught up yet
2. **Calm periods after volatility** - QuickSwap fee hasn't come down yet
3. **Asymmetric movements** - One-directional price moves create larger spreads

**Expected frequency:** 2-5 opportunities per day per major pair during volatile sessions.

---

## Recommended Initial Pools

Priority pools based on liquidity and cross-DEX overlap:

```json
[
  {
    "dex_type": "QuickswapV3",
    "token0": "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270",
    "token1": "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359",
    "symbol": "WMATIC/USDC",
    "priority": "highest",
    "note": "Most liquid, has active farming"
  },
  {
    "dex_type": "QuickswapV3",
    "token0": "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619",
    "token1": "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359",
    "symbol": "WETH/USDC",
    "priority": "high"
  },
  {
    "dex_type": "QuickswapV3",
    "token0": "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270",
    "token1": "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619",
    "symbol": "WMATIC/WETH",
    "priority": "medium"
  },
  {
    "dex_type": "QuickswapV3",
    "token0": "0x1BFD67037B42Cf73acF2047067bd4F2C47D9BfD6",
    "token1": "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359",
    "symbol": "WBTC/USDC",
    "priority": "medium"
  },
  {
    "dex_type": "QuickswapV3",
    "token0": "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359",
    "token1": "0xc2132D05D31c914a87C6611C10748AEb04B58e8F",
    "symbol": "USDC/USDT",
    "priority": "low",
    "note": "Stablecoin pair - lower spreads but very liquid"
  }
]
```

---

## Integration Checklist

### Code Changes Required

- [ ] **1. Add DexType variant**
  ```rust
  // src/pool.rs or types.rs
  pub enum DexType {
      UniswapV3,
      SushiswapV3,
      QuickswapV3,  // ADD
  }
  ```

- [ ] **2. Update Quoter addresses**
  ```rust
  // src/arbitrage/multicall_quoter.rs
  DexType::QuickswapV3 => Address::from_str("0xa15F0D7377B2A0C0c10db057f641beD21028FC89")?
  ```

- [ ] **3. Pool address calculation** (no fee parameter)
  ```rust
  // Ensure CREATE2 uses poolDeployer, not factory
  // Ensure NO fee field in calculation
  ```

- [ ] **4. Whitelist pools**
  ```bash
  # Add 4-5 pools to config/pools_whitelist.json
  # Note: NO "fee" field for QuickSwap pools
  ```

- [ ] **5. ArbExecutor.sol routing**
  ```solidity
  // contracts/src/ArbExecutor.sol
  // Add router check for 0xf5b509bB0909a69B1c207E495f687a596C168E12
  // Use algebraSwapCallback interface
  ```

- [ ] **6. Multicall3 quoter logic** (handle missing fee field)
  ```rust
  // src/arbitrage/multicall_quoter.rs
  // QuickSwap quoter calls don't include fee param
  ```

### Testing Steps

- [ ] **7. Verify pool addresses**
  ```bash
  python scripts/verify_whitelist.py --dex quickswap
  ```

- [ ] **8. Test sync**
  ```bash
  # Ensure pools sync correctly (check logs for QuickswapV3)
  tmux attach -t livebot
  # Look for: "Synced 16 pools (12 active)"
  ```

- [ ] **9. Monitor cross-DEX detection**
  ```bash
  # Check for QuickSwap ↔ Uniswap opportunities in logs
  grep "QuickswapV3" ~/bots/dexarb/data/livebot.log
  ```

- [ ] **10. Live test (watch mode)**
  ```bash
  # Let bot run in watch mode (kills on first trade)
  # Observe if QuickSwap pools appear in opportunity detection
  ```

---

## Expected Impact

**Before QuickSwap:**
- 12 pools (10 Uni + 2 Sushi)
- 66 cross-DEX pair combinations

**After QuickSwap:**
- 16-17 pools (10 Uni + 2 Sushi + 4-5 Quick)
- ~120-136 cross-DEX pair combinations
- **+82% more comparison pairs**

**Opportunity Types:**
1. **QuickSwap ↔ UniswapV3** (most common - dynamic vs 0.05%/0.3%)
2. **QuickSwap ↔ SushiswapV3** (less common but larger spreads)
3. **Three-way** (Quick → Uni → Sushi → Quick)

---

## Resources

| Resource | URL |
|----------|-----|
| Algebra Docs (Architecture) | https://docs.algebra.finance/algebra-integral-documentation/overview-faq/partners/algebra-v1.0/quickswap-polygon |
| Migration from Uni V3 | https://docs.algebra.finance/algebra-integral/integration-of-algebra-integral-protocol/migration-from-uniswapv3 |
| QuickSwap Contracts | https://docs.quickswap.exchange/overview/contracts-and-addresses |
| Dynamic Fee Explanation | https://medium.com/@crypto_algebra/why-decentralized-exchanges-prefer-algebra-over-uniswap-v3-c431cbd2d8c5 |
| PolygonScan QuoterV2 | https://polygonscan.com/address/0xa15F0D7377B2A0C0c10db057f641beD21028FC89 |

---

## Notes

- QuickSwap V3 launched on Polygon in 2022 (mature, battle-tested)
- Same math as Uniswap V3 (concentrated liquidity)
- Main edge: **dynamic fees lag price volatility** = arbitrage window
- Integration complexity: **LOW** (very similar to Uni V3, just remove fee params)
- Estimated implementation time: **2-4 hours**

---

*Last updated: 2026-01-30*
