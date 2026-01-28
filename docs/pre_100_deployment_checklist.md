# Pre-$100 Deployment Comprehensive Checklist
## Complete System Validation Before Live Trading

**Purpose**: Systematic validation of all systems before committing real capital  
**Capital at Risk**: $100 (test amount)  
**Time Required**: 2-3 hours  
**Confidence Target**: >90% before proceeding  

---

## 📋 CHECKLIST OVERVIEW

### **Categories (10 Total)**

```
1. Technical Infrastructure ......... 15 checks
2. Smart Contract Verification ...... 15 checks  (↑3 for 0.30% pool)
3. Bot Configuration ................ 18 checks
4. Data Integrity ................... 14 checks  (↑4 for dual-route)
5. Execution Path Validation ........ 18 checks  (↑4 for Route 2)
6. Risk Management .................. 12 checks
7. Monitoring & Alerts .............. 10 checks
8. Financial Controls ............... 22 checks  (↑10 for tax logging)
9. Operational Procedures ........... 9 checks
10. Emergency Protocols ............. 7 checks

TOTAL: 140 checks (updated for tax logging validation)
CRITICAL: 46 checks (must pass ALL, +4 tax)
IMPORTANT: 59 checks (must pass 90%, +4 tax)
RECOMMENDED: 35 checks (should pass 80%, +2 tax)
```

### **Dual-Route Discovery Summary**

```
Route 1: V3 1.00% → V3 0.05%
├─ Spread: 2.24% midmarket, ~1.19% executable
├─ Profit: ~$10.25 per $1000 trade
└─ Frequency: ~209 detections/hour

Route 2: V3 0.30% → V3 0.05%
├─ Spread: 1.43% midmarket, ~0.68% executable
├─ Profit: ~$9.22 per $1000 trade
└─ Frequency: ~215 detections/hour

Combined: 424 opportunities/hour, $10.28 avg profit
Discovery Mode most profitable: 168 opps, $1,635.48 (paper)
```

---

## 1️⃣ TECHNICAL INFRASTRUCTURE (15 checks)

### **1.1 VPS/Server Health**

```bash
# Check 1.1.1: Server uptime and load
uptime
# Expected: >24 hours uptime, load <2.0

# Check 1.1.2: Available disk space
df -h
# Required: >50GB free on /home

# Check 1.1.3: Available memory
free -h
# Required: >2GB available RAM

# Check 1.1.4: CPU usage
top -b -n 1 | head -20
# Required: <80% average CPU usage
```

**Server Health Checklist**:
```
[ ] CRITICAL: Uptime >24 hours
[ ] CRITICAL: Disk space >50GB
[ ] CRITICAL: RAM >2GB available
[ ] IMPORTANT: CPU <80% usage
[ ] IMPORTANT: No high I/O wait
```

---

### **1.2 Network Connectivity**

```bash
# Check 1.2.1: Internet connectivity
ping -c 4 8.8.8.8

# Check 1.2.2: Polygon RPC reachability
curl -X POST https://polygon-rpc.com \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'

# Check 1.2.3: Alchemy RPC (if using)
curl -X POST $ALCHEMY_URL \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'

# Check 1.2.4: Latency test
time curl -X POST https://polygon-rpc.com \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
# Required: <2 seconds response time
```

**Network Checklist**:
```
[ ] CRITICAL: Primary RPC reachable
[ ] CRITICAL: Backup RPC reachable
[ ] IMPORTANT: Response time <2s
[ ] RECOMMENDED: Multi-RPC configured
```

---

### **1.3 Database Health**

```bash
# Check 1.3.1: PostgreSQL running
systemctl status postgresql

# Check 1.3.2: Database size
psql -d dexarb_db -c "
SELECT 
    pg_size_pretty(pg_database_size('dexarb_db')) as db_size,
    (SELECT count(*) FROM opportunities) as opp_count,
    (SELECT count(*) FROM executed_trades) as trade_count;"

# Check 1.3.3: Database performance
psql -d dexarb_db -c "
SELECT 
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC
LIMIT 5;"

# Check 1.3.4: Recent data
psql -d dexarb_db -c "
SELECT MAX(timestamp) as last_opportunity
FROM opportunities;"
# Required: <5 minutes ago
```

**Database Checklist**:
```
[ ] CRITICAL: PostgreSQL running
[ ] CRITICAL: Database accessible
[ ] CRITICAL: Recent data (<5 min)
[ ] IMPORTANT: Reasonable size (<10GB)
[ ] IMPORTANT: No lock contention
```

---

### **1.4 Bot Service Health**

```bash
# Check 1.4.1: Bot processes running
ps aux | grep dexarb

# Check 1.4.2: Service status
systemctl status dexarb-phase1

# Check 1.4.3: Recent log activity
journalctl -u dexarb-phase1 -n 50 --no-pager

# Check 1.4.4: No critical errors
journalctl -u dexarb-phase1 --since "1 hour ago" | grep -i "error\|critical\|fatal"
# Expected: No critical errors
```

**Service Checklist**:
```
[ ] CRITICAL: Bot service active
[ ] CRITICAL: No critical errors in logs
[ ] IMPORTANT: Recent log activity (<1 min)
[ ] IMPORTANT: Both collector and paper-trading running
```

---

## 2️⃣ SMART CONTRACT VERIFICATION (12 checks)

### **2.1 Router Addresses**

```bash
# Polygon Mainnet addresses to verify

# Check 2.1.1: Uniswap V2 Router (Quickswap)
QUICKSWAP_ROUTER="0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff"
cast code $QUICKSWAP_ROUTER --rpc-url https://polygon-rpc.com | wc -c
# Expected: >1000 (contract has code)

# Check 2.1.2: Uniswap V3 Router
V3_ROUTER="0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45"
cast code $V3_ROUTER --rpc-url https://polygon-rpc.com | wc -c
# Expected: >1000

# Check 2.1.3: Uniswap V3 Quoter
V3_QUOTER="0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6"
cast code $V3_QUOTER --rpc-url https://polygon-rpc.com | wc -c
# Expected: >1000
```

**Router Verification Checklist**:
```
[ ] CRITICAL: V2 router has code
[ ] CRITICAL: V3 router has code
[ ] CRITICAL: V3 quoter has code
[ ] CRITICAL: Addresses match documentation
```

**Router Addresses to Verify**:
```
Quickswap (V2): 0xa5E0829CaCEd8fFDD4De3c43696c57F7D7A678ff
V3 SwapRouter02: 0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45
V3 Quoter V2: 0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6
```

---

### **2.2 Token Addresses**

```bash
# Check 2.2.1: UNI token
UNI="0xb33EaAd8d922B1083446DC23f610c2567fB5180f"
cast call $UNI "symbol()(string)" --rpc-url https://polygon-rpc.com
# Expected: "UNI"

cast call $UNI "decimals()(uint8)" --rpc-url https://polygon-rpc.com
# Expected: 18

# Check 2.2.2: USDC token
USDC="0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"
cast call $USDC "symbol()(string)" --rpc-url https://polygon-rpc.com
# Expected: "USDC"

cast call $USDC "decimals()(uint8)" --rpc-url https://polygon-rpc.com
# Expected: 6

# Check 2.2.3: WMATIC token
WMATIC="0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270"
cast call $WMATIC "symbol()(string)" --rpc-url https://polygon-rpc.com
# Expected: "WMATIC"
```

**Token Verification Checklist**:
```
[ ] CRITICAL: UNI address correct, 18 decimals
[ ] CRITICAL: USDC address correct, 6 decimals
[ ] IMPORTANT: All tokens have code
[ ] IMPORTANT: Symbols match expected
```

---

### **2.3 Pool Addresses & TVL**

**NOTE: Two profitable routes discovered - all THREE fee tier pools required**

```bash
# Check 2.3.1: Find UNI/USDC V3 pools
# Visit Uniswap V3 info or compute addresses

# UNI/USDC 0.05% pool (DESTINATION for both routes)
POOL_005="0x________________"  # Fill in

# Check pool exists
cast call $POOL_005 "liquidity()(uint128)" --rpc-url https://polygon-rpc.com

# Check 2.3.2: UNI/USDC 0.30% pool (SOURCE for Route 2)
POOL_030="0x________________"  # Fill in

cast call $POOL_030 "liquidity()(uint128)" --rpc-url https://polygon-rpc.com

# Check 2.3.3: UNI/USDC 1.00% pool (SOURCE for Route 1)
POOL_100="0x________________"  # Fill in

cast call $POOL_100 "liquidity()(uint128)" --rpc-url https://polygon-rpc.com

# Check 2.3.4: Verify on Uniswap Info
open "https://info.uniswap.org/#/polygon/pools"
# Search for UNI/USDC pools, verify TVL for ALL THREE
```

**Pool Verification Checklist**:
```
[ ] CRITICAL: 0.05% pool address verified (destination pool)
[ ] CRITICAL: 0.30% pool address verified (Route 2 source)
[ ] CRITICAL: 1.00% pool address verified (Route 1 source)
[ ] CRITICAL: 0.05% pool TVL >$10M
[ ] CRITICAL: 0.30% pool TVL >$5M
[ ] CRITICAL: 1.00% pool TVL >$2M
[ ] IMPORTANT: 24h volume >$1M on all three pools
```

**Record Pool Addresses**:
```
Route 1: V3 1.00% → V3 0.05% (2.24% spread, ~$10.25/trade)
Route 2: V3 0.30% → V3 0.05% (1.43% spread, ~$9.22/trade)

UNI/USDC 0.05%: 0x_______________________
├─ TVL: $_____________
├─ 24h Volume: $_____________
└─ Role: DESTINATION (both routes)

UNI/USDC 0.30%: 0x_______________________
├─ TVL: $_____________
├─ 24h Volume: $_____________
└─ Role: SOURCE (Route 2)

UNI/USDC 1.00%: 0x_______________________
├─ TVL: $_____________
├─ 24h Volume: $_____________
└─ Role: SOURCE (Route 1)
```

---

### **2.4 Approval Status**

```bash
# Check 2.4.1: Current USDC approval for V3 router
cast call $USDC \
  "allowance(address,address)(uint256)" \
  $YOUR_WALLET \
  $V3_ROUTER \
  --rpc-url https://polygon-rpc.com

# Check 2.4.2: Current UNI approval for V3 router
cast call $UNI \
  "allowance(address,address)(uint256)" \
  $YOUR_WALLET \
  $V3_ROUTER \
  --rpc-url https://polygon-rpc.com
```

**Approval Checklist**:
```
[ ] CRITICAL: USDC approved for V3 router
[ ] CRITICAL: UNI approved for V3 router
[ ] RECOMMENDED: Approvals not unlimited (use exact amounts)
[ ] RECOMMENDED: Revoke approvals after testing
```

---

## 3️⃣ BOT CONFIGURATION (18 checks)

### **3.1 Core Configuration**

```bash
# Check 3.1.1: Review main config
cat config/paper_trading.toml

# Verify critical settings:
```

**Configuration Checklist**:
```
[ ] CRITICAL: RPC URL configured correctly
[ ] CRITICAL: Network = "polygon" (not "mainnet")
[ ] CRITICAL: Chain ID = 137
[ ] CRITICAL: Poll interval = 10000ms
[ ] IMPORTANT: Pool sync enabled
[ ] IMPORTANT: V3 integration enabled
```

**Record Configuration**:
```toml
[network]
name = "_____________"
chain_id = _____________
rpc_url = "_____________"

[collection]
poll_interval_ms = _____________
v2_pools_enabled = _____________
v3_pools_enabled = _____________

[filtering]
min_tvl_v2 = _____________
min_tvl_v3 = _____________
```

---

### **3.2 Pool Configuration**

```bash
# Check 3.2.1: Review pools.toml
cat config/pools.toml

# Check 3.2.2: Count configured pools
cat config/pools.toml | grep -c "\\[\\[pools\\]\\]"
```

**Pool Config Checklist**:
```
[ ] CRITICAL: UNI/USDC V3 0.05% configured
[ ] CRITICAL: UNI/USDC V3 1.00% configured
[ ] IMPORTANT: Pool addresses correct
[ ] IMPORTANT: Fee tiers correct (500, 10000)
[ ] RECOMMENDED: Only high-TVL pools enabled
```

**Record Configured Pools**:
```
V2 Pools: _____________
V3 Pools: _____________
Total: _____________
Active pairs: _____________
```

---

### **3.3 Strategy Configuration**

```bash
# Check 3.3.1: Review strategies
cat config/strategies.toml
```

**Strategy Checklist**:
```
[ ] IMPORTANT: Multiple strategies configured
[ ] IMPORTANT: Thresholds reasonable (>0.50%)
[ ] IMPORTANT: Trade sizes appropriate
[ ] RECOMMENDED: Discovery Mode threshold >0.20%
```

**Record Active Strategies**:
```
Strategy Name          | Min Spread | Trade Size
-----------------------|------------|------------
Aggressive            | _______%   | $_______
Altcoin Hunter        | _______%   | $_______
Diversifier           | _______%   | $_______
Discovery Mode        | _______%   | $_______
```

---

### **3.4 Execution Configuration**

```bash
# Check 3.4.1: Review execution.toml
cat config/execution.toml
```

**Execution Config Checklist**:
```
[ ] CRITICAL: Mode = "paper" initially (NOT "live" yet!)
[ ] CRITICAL: Test mode available
[ ] IMPORTANT: Gas price limits configured
[ ] IMPORTANT: Slippage tolerance configured
[ ] RECOMMENDED: Max per trade = $50-100
```

**Record Execution Settings**:
```toml
[execution]
mode = "_____________"  # Should be "paper" for now
test_mode = _____________

[limits]
max_per_trade = $_____________
max_daily_trades = _____________
gas_price_limit_gwei = _____________

[slippage]
max_slippage_pct = _____________%
```

---

## 4️⃣ DATA INTEGRITY (10 checks)

### **4.1 Opportunity Detection**

**NOTE: Two profitable routes discovered - verify BOTH are being detected**

```sql
-- Check 4.1.1: Recent opportunities detected
SELECT COUNT(*) as recent_opps
FROM opportunities
WHERE timestamp > NOW() - INTERVAL '10 minutes';
-- Expected: >200 opportunities (both routes combined)

-- Check 4.1.2: Opportunity distribution by route
SELECT
    pair,
    dex_from,
    dex_to,
    COUNT(*) as count,
    AVG(spread_pct) as avg_spread,
    AVG(expected_profit) as avg_profit,
    MAX(timestamp) as last_seen
FROM opportunities
WHERE timestamp > NOW() - INTERVAL '1 hour'
GROUP BY pair, dex_from, dex_to
ORDER BY count DESC;

-- Check 4.1.3: Verify BOTH routes detected
-- Route 1: V3 1.00% → V3 0.05% (expect ~209 detections/hour, 2.24% avg spread)
-- Route 2: V3 0.30% → V3 0.05% (expect ~215 detections/hour, 1.43% avg spread)

-- Check 4.1.4: Spread value distribution
SELECT
    FLOOR(spread_pct) as spread_bucket,
    COUNT(*) as count
FROM opportunities
WHERE timestamp > NOW() - INTERVAL '1 hour'
GROUP BY FLOOR(spread_pct)
ORDER BY spread_bucket;
-- Expected: Buckets at 1% and 2% (corresponding to both routes)
```

**Data Integrity Checklist**:
```
[ ] CRITICAL: Recent opportunities detected (<10 min)
[ ] CRITICAL: Spread values varying (not constant)
[ ] CRITICAL: Route 1 (1.00%→0.05%) actively detected
[ ] CRITICAL: Route 2 (0.30%→0.05%) actively detected
[ ] IMPORTANT: Route 1 avg spread ~2.24%
[ ] IMPORTANT: Route 2 avg spread ~1.43%
[ ] IMPORTANT: Combined >400 detections/hour
[ ] IMPORTANT: Timestamps updating continuously
[ ] RECOMMENDED: Avg profit >$9 per opportunity
```

**Expected Detection Metrics**:
```
Route 1 (V3 1.00% → V3 0.05%):
├─ Spread: ~2.24%
├─ Profit/trade: ~$10.25
└─ Detections/hour: ~209

Route 2 (V3 0.30% → V3 0.05%):
├─ Spread: ~1.43%
├─ Profit/trade: ~$9.22
└─ Detections/hour: ~215

Combined: ~424 detections/hour, $10.28 avg profit
```

---

### **4.2 Pool Sync Status**

```sql
-- Check 4.2.1: Pool sync freshness
SELECT 
    pair,
    dex,
    last_sync,
    NOW() - last_sync as time_since_sync,
    block_number
FROM pool_state
ORDER BY last_sync DESC
LIMIT 20;
-- Expected: All synced within last 30 seconds

-- Check 4.2.2: V2 vs V3 sync status
SELECT 
    CASE WHEN dex LIKE '%V3%' THEN 'V3' ELSE 'V2' END as pool_type,
    COUNT(*) as pool_count,
    AVG(EXTRACT(EPOCH FROM (NOW() - last_sync))) as avg_seconds_since_sync
FROM pool_state
GROUP BY CASE WHEN dex LIKE '%V3%' THEN 'V3' ELSE 'V2' END;
```

**Pool Sync Checklist**:
```
[ ] CRITICAL: All pools synced <60s ago
[ ] CRITICAL: Block numbers progressing
[ ] IMPORTANT: V3 pools syncing regularly
[ ] IMPORTANT: No stale pools (>5 min)
```

---

### **4.3 Spread Calculation Validation**

```sql
-- Check 4.3.1: Spread calculation sanity check
SELECT 
    pair,
    dex_from,
    dex_to,
    MIN(spread_pct) as min_spread,
    MAX(spread_pct) as max_spread,
    AVG(spread_pct) as avg_spread,
    STDDEV(spread_pct) as spread_stddev
FROM opportunities
WHERE timestamp > NOW() - INTERVAL '1 hour'
GROUP BY pair, dex_from, dex_to
HAVING COUNT(*) > 10;
-- Expected: Reasonable variance (stddev > 0.1)
```

**Spread Validation Checklist**:
```
[ ] CRITICAL: Spreads are varying (not constant)
[ ] CRITICAL: Spreads in reasonable range (0.5-5%)
[ ] IMPORTANT: Standard deviation >0.1%
[ ] IMPORTANT: No negative spreads
```

---

## 5️⃣ EXECUTION PATH VALIDATION (14 checks)

### **5.1 Dry Run Test**

**NOTE: Test BOTH profitable routes separately**

```bash
# Check 5.1.1: Simulate Route 1 execution (no real transaction)
./target/release/dexarb-bot \
  --dry-run \
  --pair UNI/USDC \
  --route "V3_1.00%->V3_0.05%" \
  --amount 50

# Expected output for Route 1:
# - Spread: ~2.24%
# - Est. Profit: ~$10.25 per $1000 trade
# - Gas: <$1

# Check 5.1.2: Simulate Route 2 execution (no real transaction)
./target/release/dexarb-bot \
  --dry-run \
  --pair UNI/USDC \
  --route "V3_0.30%->V3_0.05%" \
  --amount 50

# Expected output for Route 2:
# - Spread: ~1.43%
# - Est. Profit: ~$9.22 per $1000 trade
# - Gas: <$1

# Check 5.1.3: Compare routes side-by-side
./target/release/dexarb-bot \
  --dry-run \
  --pair UNI/USDC \
  --all-routes \
  --amount 50
```

**Dry Run Checklist**:
```
[ ] CRITICAL: Route 1 dry run completes without errors
[ ] CRITICAL: Route 2 dry run completes without errors
[ ] CRITICAL: Route 1 calculation correct (~2.24% spread)
[ ] CRITICAL: Route 2 calculation correct (~1.43% spread)
[ ] CRITICAL: Gas estimation reasonable (<$1) for both
[ ] IMPORTANT: Route 1 profit ~$10.25/trade matches expected
[ ] IMPORTANT: Route 2 profit ~$9.22/trade matches expected
[ ] IMPORTANT: Slippage estimation realistic (1-5%) for both
```

**Recommended Test Trade Plan**:
```
Test $100 capital allocation:
├─ Route 1 (1.00%→0.05%): $50 test trade
└─ Route 2 (0.30%→0.05%): $50 test trade

This validates BOTH routes with minimal risk.
```

---

### **5.2 Gas Estimation**

```bash
# Check 5.2.1: Estimate gas for V3 swap
cast estimate \
  --from $YOUR_WALLET \
  $V3_ROUTER \
  "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))" \
  "($UNI,$USDC,10000,$YOUR_WALLET,1000000000000000000,0,0,0)" \
  --rpc-url https://polygon-rpc.com

# Check 5.2.2: Current gas price
cast gas-price --rpc-url https://polygon-rpc.com
# Expected: <100 gwei on Polygon

# Check 5.2.3: Estimated gas cost
# Gas units × Gas price × MATIC price ÷ 1e9
# Typical: 200K gas × 30 gwei × $0.60 ÷ 1e9 = $0.36
```

**Gas Estimation Checklist**:
```
[ ] IMPORTANT: Gas estimate <300K units
[ ] IMPORTANT: Gas price <100 gwei
[ ] IMPORTANT: Total gas cost <$1 per trade
[ ] RECOMMENDED: Gas price monitoring working
```

**Record Gas Estimates**:
```
Single V3 swap: _____________ gas units
Gas price: _____________ gwei
Cost in MATIC: _____________ MATIC
Cost in USD: $_____________
```

---

### **5.3 Slippage Simulation**

```bash
# Check 5.3.1: Test slippage calculation
# In your code or via test script

# Expected behavior:
# - Calculate price impact for trade size
# - Compare to pool depth
# - Estimate slippage percentage
```

**Slippage Checklist**:
```
[ ] IMPORTANT: Slippage calculation implemented
[ ] IMPORTANT: Slippage based on pool liquidity
[ ] IMPORTANT: Max slippage configured (3-5%)
[ ] RECOMMENDED: Different slippage for V2/V3
```

**Test Slippage Scenarios**:
```
$50 trade in $10M pool:
└─ Expected: 0.5% slippage

$50 trade in $2M pool:
└─ Expected: 1-2% slippage

$500 trade in $10M pool:
└─ Expected: 1-2% slippage
```

---

### **5.4 Trade Simulation (Paper Trading)**

```bash
# Check 5.4.1: Review recent paper trades
psql -d dexarb_db -c "
SELECT 
    timestamp,
    pair,
    route,
    amount_usd,
    expected_profit,
    simulated_profit,
    slippage_pct
FROM paper_trades
WHERE timestamp > NOW() - INTERVAL '1 hour'
ORDER BY timestamp DESC
LIMIT 10;"
```

**Paper Trading Checklist**:
```
[ ] CRITICAL: Paper trades being recorded
[ ] IMPORTANT: Slippage being simulated
[ ] IMPORTANT: Profit calculations reasonable
[ ] IMPORTANT: Success rate >50% in paper trading
```

---

## 6️⃣ RISK MANAGEMENT (12 checks)

### **6.1 Capital Controls**

```toml
# Check 6.1.1: Review risk limits
[risk_management]
max_capital_at_risk = 100  # $100 total
max_per_trade = 50         # $50 per trade
max_simultaneous_trades = 2
reserve_for_gas = 10       # $10 reserve

[stop_loss]
enabled = true
max_daily_loss = 30        # Stop if lose $30 in one day
max_consecutive_losses = 5  # Stop after 5 losses in a row
```

**Capital Controls Checklist**:
```
[ ] CRITICAL: Max capital = $100
[ ] CRITICAL: Max per trade = $50
[ ] CRITICAL: Stop loss configured
[ ] IMPORTANT: Gas reserve allocated
[ ] IMPORTANT: Daily loss limit set
```

---

### **6.2 Trade Limits**

```toml
[trade_limits]
max_trades_per_hour = 20
max_trades_per_day = 100
min_profit_threshold = 0.10  # $0.10 minimum profit

[cooldown]
after_loss = 60              # 60s cooldown after loss
after_error = 300            # 5 min cooldown after error
```

**Trade Limits Checklist**:
```
[ ] IMPORTANT: Hourly trade limit set
[ ] IMPORTANT: Daily trade limit set
[ ] IMPORTANT: Minimum profit threshold
[ ] RECOMMENDED: Cooldown periods configured
```

---

### **6.3 Error Handling**

```bash
# Check 6.3.1: Review error handling in code
grep -r "\.unwrap()" src/ | wc -l
# Should be minimal - use proper error handling

# Check 6.3.2: Check for panic handlers
grep -r "panic!" src/ | wc -l
# Should be zero in production code

# Check 6.3.3: Test error scenarios
```

**Error Handling Checklist**:
```
[ ] CRITICAL: No unwrap() in critical paths
[ ] CRITICAL: No unhandled panics
[ ] IMPORTANT: Errors logged properly
[ ] IMPORTANT: Graceful degradation on errors
[ ] RECOMMENDED: Retry logic for transient errors
```

---

## 7️⃣ MONITORING & ALERTS (10 checks)

### **7.1 Logging Configuration**

```bash
# Check 7.1.1: Log files exist and are being written
ls -lh logs/
tail -20 logs/dexarb.log

# Check 7.1.2: Log rotation configured
cat /etc/logrotate.d/dexarb

# Check 7.1.3: Log level appropriate
grep "log_level" config/*.toml
```

**Logging Checklist**:
```
[ ] CRITICAL: Logs being written
[ ] CRITICAL: Log rotation configured
[ ] IMPORTANT: Log level = "info" or "debug"
[ ] IMPORTANT: Logs include timestamps
[ ] RECOMMENDED: Separate log files for errors
```

---

### **7.2 Monitoring Setup**

```bash
# Check 7.2.1: Monitoring script exists
ls -l scripts/monitor.sh

# Check 7.2.2: Metrics being collected
cat scripts/monitor.sh
# Should track:
# - Trade count
# - Win rate  
# - P&L
# - Error rate
# - Uptime
```

**Monitoring Checklist**:
```
[ ] IMPORTANT: Monitoring script exists
[ ] IMPORTANT: Metrics being logged
[ ] RECOMMENDED: Dashboard or visual monitoring
[ ] RECOMMENDED: Historical data retention
```

---

### **7.3 Alert Configuration**

```bash
# Check 7.3.1: Alert script exists
ls -l scripts/alert.sh

# Check 7.3.2: Alert conditions configured
cat scripts/alert.sh
# Should alert on:
# - Daily loss >$30
# - >5 consecutive failures
# - Critical errors
# - Bot stopped
```

**Alert Checklist**:
```
[ ] IMPORTANT: Alert mechanism exists
[ ] IMPORTANT: Alert on stop loss
[ ] IMPORTANT: Alert on critical errors
[ ] RECOMMENDED: Alert on unusual activity
[ ] RECOMMENDED: Multiple alert channels (email, SMS, etc.)
```

---

## 8️⃣ FINANCIAL CONTROLS (8 checks)

### **8.1 Wallet Setup**

```bash
# Check 8.1.1: Trading wallet created
echo $TRADING_WALLET
# Should be dedicated wallet, not main wallet

# Check 8.1.2: Wallet balance
cast balance $TRADING_WALLET --rpc-url https://polygon-rpc.com

# Check 8.1.3: USDC balance
cast call 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174 \
  "balanceOf(address)(uint256)" \
  $TRADING_WALLET \
  --rpc-url https://polygon-rpc.com
```

**Wallet Checklist**:
```
[ ] CRITICAL: Dedicated trading wallet
[ ] CRITICAL: Initial capital loaded ($110 = $100 + gas)
[ ] CRITICAL: Private key secured
[ ] IMPORTANT: No unnecessary funds in wallet
[ ] RECOMMENDED: Hardware wallet or secure key management
```

**Record Wallet Details**:
```
Trading Wallet: 0x_______________________
MATIC Balance: _____________ MATIC
USDC Balance: $_____________ USDC
Total USD Value: $_____________
```

---

### **8.2 Tax Logging (IRS Compliance)**

**NOTE: Tax logging is CRITICAL for IRS compliance. All crypto trades are taxable events.**

```bash
# Check 8.2.1: Tax logging enabled in .env
grep "TAX_LOG" src/rust-bot/.env
# Expected:
#   TAX_LOG_DIR=/home/botuser/bots/dexarb/data/tax
#   TAX_LOG_ENABLED=true

# Check 8.2.2: Tax directory exists and writable
ls -la data/tax/
touch data/tax/.test && rm data/tax/.test && echo "Writable: YES"

# Check 8.2.3: Tax module compiled in bot
grep -r "enable_tax_logging" src/rust-bot/src/main.rs
# Expected: Tax logging enabled after executor creation

# Check 8.2.4: Verify tax record fields (34+ IRS-required fields)
grep -A50 "pub struct TaxRecord" src/rust-bot/src/tax/mod.rs | head -60
# Must include: trade_id, timestamp, tax_year, transaction_type,
#   asset_sent/received, usd_values, cost_basis, gas_fees, tx_hash, etc.

# Check 8.2.5: CSV logger ready
ls -la src/rust-bot/src/tax/csv_logger.rs
# Creates: data/tax/trades_YYYY.csv (annual files)

# Check 8.2.6: JSON backup logger ready
ls -la src/rust-bot/src/tax/json_logger.rs
# Creates: data/tax/trades_YYYY.jsonl (redundant backup)

# Check 8.2.7: RP2 export available for tax software
ls -la src/rust-bot/src/tax/rp2_export.rs
# For: https://github.com/eprbell/rp2 (open-source tax calculator)

# Check 8.2.8: Tax export utility compiled
ls -la src/rust-bot/target/debug/tax-export 2>/dev/null || \
  echo "Run: cargo build --bin tax-export"
```

**Tax Record Fields Captured**:
```
IDENTIFICATION:
├─ trade_id (UUID)
├─ timestamp (RFC3339)
└─ tax_year

TRANSACTION:
├─ transaction_type (SWAP, BUY, SELL, TRANSFER, FEE)
├─ asset_sent / amount_sent
└─ asset_received / amount_received

USD VALUATIONS (IRS requires):
├─ usd_value_sent (fair market value)
├─ usd_value_received
├─ spot_price_sent
└─ spot_price_received

COST BASIS:
├─ cost_basis_usd
├─ proceeds_usd
├─ capital_gain_loss
├─ holding_period_days (0 for arbitrage)
└─ gain_type (SHORT_TERM for all arbitrage)

FEES (deductible):
├─ gas_fee_native (MATIC)
├─ gas_fee_usd
├─ dex_fee_percent
└─ total_fees_usd

BLOCKCHAIN AUDIT:
├─ blockchain ("Polygon")
├─ chain_id (137)
├─ transaction_hash
├─ block_number
└─ wallet_address

DEX ROUTING:
├─ dex_buy / dex_sell
└─ pool_address_buy / pool_address_sell
```

**Tax Logging Checklist**:
```
[ ] CRITICAL: TAX_LOG_ENABLED=true in .env
[ ] CRITICAL: TAX_LOG_DIR configured and writable
[ ] CRITICAL: Tax module integrated in main.rs
[ ] CRITICAL: All 34+ IRS fields captured per trade
[ ] IMPORTANT: CSV annual files created (trades_YYYY.csv)
[ ] IMPORTANT: JSON backup files created (trades_YYYY.jsonl)
[ ] IMPORTANT: USD valuations at trade time
[ ] IMPORTANT: Gas fees tracked for deductions
[ ] RECOMMENDED: RP2 export configured for tax software
[ ] RECOMMENDED: Tax export utility compiled
```

**Post-Trade Tax Verification**:
```bash
# After first real trade, verify logging:
cat data/tax/trades_2026.csv | head -2
# Should show header + first trade record

cat data/tax/trades_2026.jsonl | head -1 | python3 -m json.tool
# Should show complete JSON record with all fields

# Generate tax summary:
./target/debug/tax-export summary 2026
# Shows total trades, proceeds, cost basis, gains/losses
```

**Record Tax Configuration**:
```
Tax Log Directory: /home/botuser/bots/dexarb/data/tax
Tax Logging Enabled: [ ] Yes  [ ] No
CSV Logger Ready: [ ] Yes  [ ] No
JSON Logger Ready: [ ] Yes  [ ] No
RP2 Export Ready: [ ] Yes  [ ] No

Cost Basis Method: FIFO (First In, First Out)
Expected Tax Treatment: Short-term capital gains (held <1 year)
```

---

### **8.3 Accounting Setup**

```bash
# Check 8.3.1: P&L tracking implemented
# Should track:
# - Realized profit/loss
# - Unrealized profit/loss
# - Fees paid
# - Gas costs

# Check 8.3.2: Daily reconciliation script
ls -l scripts/reconcile.sh
```

**Accounting Checklist**:
```
[ ] IMPORTANT: P&L tracking implemented
[ ] IMPORTANT: Fee tracking
[ ] RECOMMENDED: Daily reconciliation
[ ] RECOMMENDED: Monthly summary reports
```

---

### **8.4 Profit Projections (Dual Routes)**

**Updated projections based on discovered opportunities (2026-01-28)**

```
TWO PROFITABLE ROUTES DISCOVERED:

Route 1: V3 1.00% → V3 0.05%
├─ Midmarket Spread: 2.24%
├─ Executable Spread: ~1.19% (after fees)
├─ Profit per $1000 trade: ~$10.25
└─ Detection frequency: ~209/hour

Route 2: V3 0.30% → V3 0.05%
├─ Midmarket Spread: 1.43%
├─ Executable Spread: ~0.68% (after fees)
├─ Profit per $1000 trade: ~$9.22
└─ Detection frequency: ~215/hour

Combined: 424 opportunities/hour, $10.28 avg profit
```

**Updated Profit Expectations**:
```
Capital Level  | Conservative | Expected   | Optimistic
---------------|--------------|------------|------------
$100 (test)    | $2-5/day     | $5-15/day  | $15-30/day
$500           | $10-25/day   | $25-60/day | $60-100/day
$1,000         | $20-50/day   | $50-120/day| $120-200/day
$5,000         | $100-250/day | $250-600/day| $600-1000/day
```

**Risk Diversification**:
```
With two independent routes:
├─ If Route 1 faces competition → Route 2 still profitable
├─ If Route 2 liquidity drops → Route 1 still available
└─ Combined ROI potential: 5-10% daily at scale
```

**Profit Projection Checklist**:
```
[ ] IMPORTANT: Both routes show positive expected value
[ ] IMPORTANT: Diversification reduces single-route risk
[ ] IMPORTANT: Gas costs factored into projections
[ ] RECOMMENDED: Start with conservative estimates
```

---

## 9️⃣ OPERATIONAL PROCEDURES (9 checks)

### **9.1 Startup Procedure**

```bash
# Check 9.1.1: Documented startup procedure
cat docs/STARTUP.md

# Check 9.1.2: Pre-flight checks script
ls -l scripts/preflight.sh

# Check 9.1.3: Startup script
ls -l scripts/start.sh
```

**Startup Checklist**:
```
[ ] IMPORTANT: Startup procedure documented
[ ] IMPORTANT: Pre-flight checks automated
[ ] RECOMMENDED: Startup script exists
[ ] RECOMMENDED: Health check after startup
```

---

### **9.2 Shutdown Procedure**

```bash
# Check 9.2.1: Documented shutdown procedure
cat docs/SHUTDOWN.md

# Check 9.2.2: Graceful shutdown implemented
# Bot should:
# - Finish current trades
# - Close positions
# - Save state
# - Clean shutdown

# Check 9.2.3: Shutdown script
ls -l scripts/stop.sh
```

**Shutdown Checklist**:
```
[ ] CRITICAL: Graceful shutdown implemented
[ ] IMPORTANT: Shutdown procedure documented
[ ] IMPORTANT: State saved on shutdown
[ ] RECOMMENDED: Backup before shutdown
```

---

### **9.3 Backup & Recovery**

```bash
# Check 9.3.1: Backup script exists
ls -l scripts/backup.sh

# Check 9.3.2: Recent backups exist
ls -lht backups/ | head

# Check 9.3.3: Recovery procedure documented
cat docs/RECOVERY.md

# Check 9.3.4: Test restore
# (Optional but recommended)
```

**Backup Checklist**:
```
[ ] IMPORTANT: Backup script exists
[ ] IMPORTANT: Recent backups (<24 hours)
[ ] IMPORTANT: Recovery procedure documented
[ ] RECOMMENDED: Test restore performed
[ ] RECOMMENDED: Off-site backup
```

---

## 🔟 EMERGENCY PROTOCOLS (7 checks)

### **10.1 Emergency Stop**

```bash
# Check 10.1.1: Emergency stop script
cat scripts/emergency_stop.sh

# Should:
# - Stop bot immediately
# - Cancel pending transactions
# - Alert operator
# - Log reason
```

**Emergency Stop Checklist**:
```
[ ] CRITICAL: Emergency stop script exists
[ ] CRITICAL: Can stop bot immediately
[ ] CRITICAL: Operator can trigger remotely
[ ] IMPORTANT: Alerts sent on emergency stop
```

---

### **10.2 Position Unwinding**

```bash
# Check 10.2.1: Unwind positions script
cat scripts/unwind_positions.sh

# Should:
# - Close all open positions
# - Convert tokens back to stable
# - Minimize loss
```

**Unwinding Checklist**:
```
[ ] IMPORTANT: Unwind script exists
[ ] IMPORTANT: Can close positions safely
[ ] RECOMMENDED: Dry-run tested
```

---

### **10.3 Incident Response**

```bash
# Check 10.3.1: Incident response plan
cat docs/INCIDENT_RESPONSE.md

# Should cover:
# - Who to contact
# - What to do
# - How to assess damage
# - How to recover
```

**Incident Response Checklist**:
```
[ ] IMPORTANT: Response plan documented
[ ] IMPORTANT: Contact information available
[ ] RECOMMENDED: Escalation procedures
[ ] RECOMMENDED: Post-mortem template
```

---

## 📊 FINAL SCORECARD

### **Category Summary**

```
CATEGORY                    | CHECKS | PASSED | STATUS
----------------------------|--------|--------|--------
1. Technical Infrastructure | 15     | ___/15 | [ ]
2. Smart Contracts          | 15     | ___/15 | [ ]  (added 3 for 0.30% pool)
3. Bot Configuration        | 18     | ___/18 | [ ]
4. Data Integrity           | 14     | ___/14 | [ ]  (added 4 for dual-route)
5. Execution Path           | 18     | ___/18 | [ ]  (added 4 for Route 2)
6. Risk Management          | 12     | ___/12 | [ ]
7. Monitoring & Alerts      | 10     | ___/10 | [ ]
8. Financial Controls       | 12     | ___/12 | [ ]  (added 4 for projections)
9. Operational Procedures   | 9      | ___/9  | [ ]
10. Emergency Protocols     | 7      | ___/7  | [ ]
----------------------------|--------|--------|--------
TOTAL                       | 130    | ___/130| [ ]
```

### **Dual-Route Validation Summary**

```
ROUTE VERIFICATION          | STATUS
----------------------------|--------
Route 1: V3 1.00% → 0.05%  | [ ] Verified on-chain
Route 2: V3 0.30% → 0.05%  | [ ] Verified on-chain
0.05% pool TVL >$10M       | [ ] Confirmed
0.30% pool TVL >$5M        | [ ] Confirmed
1.00% pool TVL >$2M        | [ ] Confirmed
Route 1 dry-run passed     | [ ] Tested
Route 2 dry-run passed     | [ ] Tested
Both routes detecting      | [ ] Active
```

---

### **Pass Criteria**

```
CRITICAL CHECKS (42 total):  (increased for dual-route)
└─ Must pass: 42/42 (100%)
└─ Status: ___/42

IMPORTANT CHECKS (55 total):  (increased for dual-route)
└─ Must pass: 50/55 (90%)
└─ Status: ___/55

RECOMMENDED CHECKS (33 total):
└─ Should pass: 26/33 (80%)
└─ Status: ___/33

DUAL-ROUTE CHECKS (8 total):  (NEW)
└─ Must pass: 8/8 (100%)
└─ Status: ___/8

OVERALL PASS: [ ] Yes  [ ] No

IF YES → Proceed to $100 deployment (test BOTH routes)
IF NO → Address failed checks first
```

---

## 🎯 DEPLOYMENT DECISION MATRIX

### **Based on Scorecard Results**

```
SCENARIO A: All Critical + 90%+ Important ✅
├─ Confidence: VERY HIGH (>95%)
├─ Decision: DEPLOY $100 immediately
├─ Expected: High probability of success
└─ Timeline: Start today

SCENARIO B: All Critical + 80-90% Important ✅
├─ Confidence: HIGH (85-95%)
├─ Decision: DEPLOY $100 with close monitoring
├─ Expected: Good probability of success
└─ Timeline: Start today, monitor hourly

SCENARIO C: All Critical + 70-80% Important ⚠️
├─ Confidence: MEDIUM (75-85%)
├─ Decision: DEPLOY $50 test first
├─ Expected: Moderate success probability
└─ Timeline: Start with smaller amount

SCENARIO D: Missing Critical Checks ❌
├─ Confidence: LOW (<75%)
├─ Decision: DO NOT DEPLOY
├─ Action: Fix critical issues first
└─ Timeline: Reassess after fixes
```

---

## ✅ PRE-DEPLOYMENT FINAL CHECKLIST

**Before funding wallet with $100**:

```
[ ] All 42 critical checks passed
[ ] 90%+ important checks passed
[ ] All 8 dual-route checks passed
[ ] Emergency stop procedures tested
[ ] Monitoring and alerts configured
[ ] Stop loss limits configured
[ ] Wallet secured and funded
[ ] Backup completed
[ ] Team/operator notified
[ ] Documentation reviewed
[ ] Ready to monitor first hour closely
```

**Dual-Route Validation**:
```
[ ] Route 1 (1.00%→0.05%) verified on-chain
[ ] Route 2 (0.30%→0.05%) verified on-chain
[ ] All THREE pool TVLs confirmed sufficient
[ ] Both routes dry-run tested
[ ] Both routes actively detecting opportunities
[ ] Test trades planned: $50 each route
```

**Deployment Authorization**:
```
Completed by: _____________
Date: _____________
Time: _____________
Confidence: ______%
Deployment Amount: $_____________
Test Allocation: Route 1 $___ / Route 2 $___
Expected Daily: $5-15 (conservative) / $15-30 (optimistic)
Stop Loss: $_____________

Signature: _____________
```

---

## 🚀 POST-DEPLOYMENT IMMEDIATE ACTIONS

**First 10 Minutes**:
```
[ ] Confirm bot started successfully
[ ] Watch for first opportunity detection
[ ] Verify BOTH routes detecting opportunities
[ ] Monitor first trade execution
[ ] Verify logging working
[ ] Confirm alerts functional
```

**First Hour**:
```
[ ] Track P&L per route (Route 1 vs Route 2)
[ ] Monitor win rate for each route
[ ] Verify ~200+ detections for each route
[ ] Watch for errors
[ ] Verify slippage reasonable
[ ] Check gas costs
[ ] Compare actual vs expected ($10.28 avg profit)
```

**First 24 Hours**:
```
[ ] Daily P&L review (expect $5-30)
[ ] Performance vs expectations per route
[ ] Route 1: ~$10.25/trade, ~209 detections/hr
[ ] Route 2: ~$9.22/trade, ~215 detections/hr
[ ] Any adjustments needed
[ ] Decision: continue, adjust, or stop
[ ] If successful: plan $500 scale-up
```

---

## 💡 FINAL NOTES

### **This Checklist is Your Safety Net**

- Don't skip checks to save time
- If something seems off, investigate
- When in doubt, don't deploy
- $100 is for testing, not profit
- Success = learning + validation
- Failure = lessons + improvement

### **Success Metrics for $100 Test**

```
MINIMUM SUCCESS:
├─ No critical errors
├─ No loss >$30
├─ Both routes execute successfully
├─ Learn execution patterns
└─ Validate system works

GOOD SUCCESS:
├─ Net profit >$0
├─ Win rate >45%
├─ Both routes profitable
├─ System stable
└─ Ready to scale

EXCELLENT SUCCESS:
├─ Net profit >$15 (dual-route expectation)
├─ Win rate >55%
├─ Both routes performing as expected
├─ No issues
└─ Scale to $500 immediately

UPDATED EXPECTATIONS (with dual routes):
├─ Route 1: ~$10.25 profit per $1000 trade
├─ Route 2: ~$9.22 profit per $1000 trade
├─ Combined detection: 424/hour
└─ Diversified risk across two independent paths
```

---

**Estimated Completion Time**: 2-3 hours  
**Worth Every Minute**: Absolutely! 🎯

**Your $100 is safe if you follow this checklist!** 🛡️
