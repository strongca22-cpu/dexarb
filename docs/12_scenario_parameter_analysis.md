# 12 Paper Trading Scenarios: Parameter Analysis & PnL Projections
## Testing Strategies with $5K Deployment Capital Ready

**Context**: You have $5K ready to deploy once paper trading proves the concept. These 12 configurations test different hypotheses to find the optimal strategy.

---

## Overview: What We're Testing

### **Core Hypothesis**
"DEX arbitrage on Polygon is profitable with $5K capital using optimal parameters"

### **Key Questions to Answer**
1. What profit threshold actually works? ($3 vs $5 vs $10?)
2. What's the optimal trade size? ($100 vs $500 vs $2000?)
3. Which pairs are most profitable? (WETH only? Multi-pair?)
4. How aggressive can we be on slippage? (0.3% vs 1.0%?)
5. Does faster polling = more profits? (50ms vs 200ms?)
6. What's the real competition rate? (30%? 70%?)
7. Can we stomach higher gas for more opportunities?
8. What daily win rate is achievable?

### **Expected Timeline**
- **Day 1-3**: Data collection (all configs running)
- **Day 4-5**: Clear patterns emerge
- **Day 6-7**: Winner becomes obvious
- **Day 8**: Deploy $5K to winning config

---

## Scenario 1: "The Conservative"
### Ultra-Safe, High-Confidence Trades Only

**Hypothesis**: "We can achieve 80%+ win rate with strict filters"

### **Parameters**
```rust
PaperTradingConfig {
    name: "Conservative",
    
    // Profit requirements (STRICT)
    min_profit_usd: 15.0,              // Need $15+ profit to trade
    max_trade_size_usd: 500.0,         // Small positions
    max_slippage_percent: 0.25,        // Very tight
    max_gas_price_gwei: 80,            // Wait for low gas
    
    // Pair selection (FOCUSED)
    pairs: vec!["WETH/USDC"],          // Most liquid only
    
    // Execution (PATIENT)
    poll_interval_ms: 100,
    
    // Risk management (STRICT)
    max_daily_trades: Some(5),         // Max 5 trades/day
    max_consecutive_losses: Some(2),   // Stop after 2 losses
    daily_loss_limit_usd: Some(30.0),  // Stop if -$30
    
    // Simulation (REALISTIC)
    simulate_competition: true,
    competition_rate: 0.80,            // Assume 80% of opps taken by others
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 15-25
Opportunities passed filters: 3-5
Executed (after competition): 1-2 trades

Wins: 1-2 (80-90% win rate)
Losses: 0-1

Average profit per win: $18-25
Average loss: -$3-5
Gas per trade: $0.40

Daily PnL: +$15-40
Monthly PnL: +$450-1,200
```

**Why Test This**:
- Establishes baseline win rate
- Tests if ultra-conservative can be profitable
- Validates detection logic works
- Low variance = clear signal

**When to Deploy This**:
✅ If you want consistent, reliable income
✅ If you're risk-averse
❌ If you want maximum profits (too few trades)

---

## Scenario 2: "The Moderate" (Expected Winner)
### Balanced Risk/Reward, Most Realistic

**Hypothesis**: "5 dollar minimum with moderate slippage gives best risk-adjusted returns"

### **Parameters**
```rust
PaperTradingConfig {
    name: "Moderate",
    
    // Profit requirements (BALANCED)
    min_profit_usd: 5.0,               // $5+ profit
    max_trade_size_usd: 1000.0,        // Medium positions
    max_slippage_percent: 0.5,         // Industry standard
    max_gas_price_gwei: 100,           // Normal Polygon gas
    
    // Pair selection (DIVERSIFIED)
    pairs: vec![
        "WETH/USDC",
        "WMATIC/USDC"
    ],
    
    // Execution (ACTIVE)
    poll_interval_ms: 100,
    
    // Risk management (MODERATE)
    max_daily_trades: Some(20),
    max_consecutive_losses: Some(5),
    daily_loss_limit_usd: Some(100.0),
    
    // Simulation (REALISTIC)
    simulate_competition: true,
    competition_rate: 0.60,            // 60% competition
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 50-80
Opportunities passed filters: 20-30
Executed (after competition): 8-12 trades

Wins: 5-8 (60-70% win rate)
Losses: 3-4

Average profit per win: $8-12
Average loss: -$2-4
Gas per trade: $0.50

Daily PnL: +$30-70
Monthly PnL: +$900-2,100
```

**Why Test This**:
- Most likely winner (balanced approach)
- Good trade frequency for data collection
- Matches industry-standard parameters
- Sustainable long-term

**When to Deploy This**:
✅ This is probably your winner
✅ Best risk/reward balance
✅ Most realistic for scaling

**$5K Deployment Projection**:
```
Capital: $5,000
Avg trade size: $800
Positions at risk: 1-2 simultaneously
Daily profit: $50-120
Monthly profit: $1,500-3,600
ROI: 30-72% monthly
```

---

## Scenario 3: "The Aggressive"
### High Frequency, Lower Profit Threshold

**Hypothesis**: "Volume compensates for lower win rate"

### **Parameters**
```rust
PaperTradingConfig {
    name: "Aggressive",
    
    // Profit requirements (LOOSE)
    min_profit_usd: 3.0,               // Lower threshold
    max_trade_size_usd: 1500.0,        // Larger positions
    max_slippage_percent: 1.0,         // High tolerance
    max_gas_price_gwei: 150,           // Trade even in congestion
    
    // Pair selection (WIDE NET)
    pairs: vec![
        "WETH/USDC",
        "WMATIC/USDC",
        "WBTC/USDC"
    ],
    
    // Execution (FAST)
    poll_interval_ms: 50,              // 20 Hz polling
    
    // Risk management (LOOSE)
    max_daily_trades: Some(50),
    max_consecutive_losses: Some(10),
    daily_loss_limit_usd: Some(200.0),
    
    // Simulation (OPTIMISTIC)
    simulate_competition: true,
    competition_rate: 0.40,            // Only 40% competition
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 120-180
Opportunities passed filters: 60-90
Executed (after competition): 36-54 trades

Wins: 18-27 (50% win rate)
Losses: 18-27

Average profit per win: $5-8
Average loss: -$3-5
Gas per trade: $0.60

Daily PnL: +$20-80 (high variance!)
Monthly PnL: +$600-2,400

Note: Higher variance, more stress
```

**Why Test This**:
- Tests if volume strategy works
- Validates competition assumptions
- Shows impact of lower thresholds
- Stress test for max throughput

**When to Deploy This**:
✅ If it somehow beats Moderate (unlikely)
❌ Probably too risky (50% win rate is stressful)
❌ Higher gas costs eat into profits

---

## Scenario 4: "The Whale"
### Large Positions, High Profits

**Hypothesis**: "Larger trades capture bigger absolute profits despite fewer opportunities"

### **Parameters**
```rust
PaperTradingConfig {
    name: "Whale",
    
    // Profit requirements (HIGH ABSOLUTE)
    min_profit_usd: 25.0,              // Need $25+
    max_trade_size_usd: 5000.0,        // Full capital per trade
    max_slippage_percent: 0.4,
    max_gas_price_gwei: 100,
    
    // Pair selection (LIQUID ONLY)
    pairs: vec!["WETH/USDC"],          // Deepest liquidity
    
    // Execution
    poll_interval_ms: 100,
    
    // Risk management (CAUTIOUS)
    max_daily_trades: Some(3),         // Only best opportunities
    max_consecutive_losses: Some(1),   // Stop after 1 loss
    daily_loss_limit_usd: Some(50.0),
    
    // Simulation
    simulate_competition: true,
    competition_rate: 0.75,
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 15-25
Opportunities passed filters: 2-4
Executed (after competition): 0-1 trades

Wins: 0-1 (70-80% win rate when trading)
Losses: 0

Average profit per win: $30-50
Average loss: -$8-12
Gas per trade: $0.50

Daily PnL: +$0-45 (highly variable, some days nothing)
Monthly PnL: +$300-1,000

Note: Fewer trades, but bigger when they hit
```

**Why Test This**:
- Tests if size matters
- Validates slippage on large trades
- Shows opportunity frequency at high thresholds
- Important for eventual scaling

**When to Deploy This**:
✅ If you prefer few, high-quality trades
❌ Probably too infrequent (boring, low data)
❌ All eggs in one basket risk

---

## Scenario 5: "The Micro Trader"
### Many Small Positions

**Hypothesis**: "Small size avoids slippage, enables high frequency"

### **Parameters**
```rust
PaperTradingConfig {
    name: "Micro Trader",
    
    // Profit requirements (LOW)
    min_profit_usd: 2.0,               // Just $2 is ok
    max_trade_size_usd: 100.0,         // Tiny positions
    max_slippage_percent: 0.5,
    max_gas_price_gwei: 100,
    
    // Pair selection
    pairs: vec!["WETH/USDC", "WMATIC/USDC"],
    
    // Execution
    poll_interval_ms: 100,
    
    // Risk management (MANY TRADES)
    max_daily_trades: Some(100),       // High frequency
    max_consecutive_losses: Some(15),
    daily_loss_limit_usd: Some(50.0),
    
    // Simulation
    simulate_competition: true,
    competition_rate: 0.50,
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 100-150
Opportunities passed filters: 50-75
Executed (after competition): 25-38 trades

Wins: 14-23 (55-60% win rate)
Losses: 11-15

Average profit per win: $3-5
Average loss: -$1.50-2.50
Gas per trade: $0.45

Daily PnL: +$10-50
Monthly PnL: +$300-1,500

Problem: Gas costs eat larger % of profit
```

**Why Test This**:
- Tests if micro trading viable
- Shows gas cost impact at small sizes
- Validates slippage assumptions
- Tests if frequency compensates

**When to Deploy This**:
❌ Probably not optimal (gas costs too high relative to profit)
✅ Good for learning (lots of data points)

**Key Insight**: 
```
Profit: $3
Gas: $0.50
Net: $2.50
Gas as % of profit: 16.7% ← Too high!

vs Moderate:
Profit: $10
Gas: $0.50
Net: $9.50
Gas as % of profit: 5.3% ← Better!
```

---

## Scenario 6: "WETH Specialist"
### Single Pair Focus, Deep Expertise

**Hypothesis**: "Specializing in most liquid pair beats diversification"

### **Parameters**
```rust
PaperTradingConfig {
    name: "WETH Specialist",
    
    // Profit requirements
    min_profit_usd: 5.0,
    max_trade_size_usd: 1200.0,
    max_slippage_percent: 0.5,
    max_gas_price_gwei: 100,
    
    // Pair selection (SINGLE PAIR)
    pairs: vec!["WETH/USDC"],          // Only WETH!
    
    // Execution
    poll_interval_ms: 100,
    
    // Risk management
    max_daily_trades: Some(15),
    max_consecutive_losses: Some(5),
    daily_loss_limit_usd: Some(75.0),
    
    // Simulation
    simulate_competition: true,
    competition_rate: 0.65,            // High competition on WETH
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 30-50
Opportunities passed filters: 12-20
Executed (after competition): 4-7 trades

Wins: 3-5 (65-75% win rate - high!)
Losses: 1-2

Average profit per win: $9-14
Average loss: -$2-4
Gas per trade: $0.50

Daily PnL: +$20-60
Monthly PnL: +$600-1,800

Note: Most competitive pair but best liquidity
```

**Why Test This**:
- WETH/USDC is THE most liquid pair on Polygon
- Tests if specialization beats diversification
- Highest competition but deepest pools
- Best for learning one pair well

**When to Deploy This**:
✅ If it beats Multi-Pair (focus is often better)
✅ Simpler to monitor/optimize
❌ If diversification proves better

---

## Scenario 7: "WMATIC Specialist"
### Alternative Pair, Lower Competition

**Hypothesis**: "Less competitive pair = higher win rate despite lower liquidity"

### **Parameters**
```rust
PaperTradingConfig {
    name: "WMATIC Specialist",
    
    // Profit requirements
    min_profit_usd: 5.0,
    max_trade_size_usd: 1000.0,
    max_slippage_percent: 0.6,         // Slightly higher (less liquid)
    max_gas_price_gwei: 100,
    
    // Pair selection (SINGLE PAIR)
    pairs: vec!["WMATIC/USDC"],        // Native token
    
    // Execution
    poll_interval_ms: 100,
    
    // Risk management
    max_daily_trades: Some(15),
    max_consecutive_losses: Some(5),
    daily_loss_limit_usd: Some(75.0),
    
    // Simulation
    simulate_competition: true,
    competition_rate: 0.45,            // Lower competition!
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 25-40
Opportunities passed filters: 10-16
Executed (after competition): 5-9 trades

Wins: 4-7 (70-80% win rate - higher!)
Losses: 1-2

Average profit per win: $7-12
Average loss: -$2-4
Gas per trade: $0.50

Daily PnL: +$25-70
Monthly PnL: +$750-2,100

Sweet spot: Less competition, still liquid enough
```

**Why Test This**:
- WMATIC less watched than WETH
- Lower competition hypothesis
- Still good liquidity
- May be hidden gem

**When to Deploy This**:
✅ If it beats WETH (higher win rate)
✅ Less stressful (less competition)
✅ Could be THE winner!

---

## Scenario 8: "The Diversifier"
### Multi-Pair Strategy

**Hypothesis**: "More pairs = more opportunities = more profit despite complexity"

### **Parameters**
```rust
PaperTradingConfig {
    name: "Diversifier",
    
    // Profit requirements
    min_profit_usd: 5.0,
    max_trade_size_usd: 1000.0,
    max_slippage_percent: 0.5,
    max_gas_price_gwei: 100,
    
    // Pair selection (MANY PAIRS)
    pairs: vec![
        "WETH/USDC",
        "WMATIC/USDC",
        "WBTC/USDC",
        "AAVE/USDC",
    ],
    
    // Execution
    poll_interval_ms: 100,
    
    // Risk management
    max_daily_trades: Some(30),        // More pairs = more trades
    max_consecutive_losses: Some(7),
    daily_loss_limit_usd: Some(150.0),
    
    // Simulation
    simulate_competition: true,
    competition_rate: 0.55,            // Mixed competition
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 80-120
Opportunities passed filters: 32-48
Executed (after competition): 14-22 trades

Wins: 8-13 (55-60% win rate)
Losses: 6-9

Average profit per win: $7-11
Average loss: -$2-4
Gas per trade: $0.50

Daily PnL: +$35-95
Monthly PnL: +$1,050-2,850

Trade-off: More opportunities but lower win rate per pair
```

**Why Test This**:
- Tests diversification hypothesis
- More opportunities = more data
- Risk spread across pairs
- Scalability test

**When to Deploy This**:
✅ If total PnL beats specialists (likely)
✅ More stable (diversification)
✅ Better for scaling later

---

## Scenario 9: "Speed Demon"
### High-Frequency Polling

**Hypothesis**: "Faster polling catches opportunities before others"

### **Parameters**
```rust
PaperTradingConfig {
    name: "Speed Demon",
    
    // Profit requirements
    min_profit_usd: 5.0,
    max_trade_size_usd: 1000.0,
    max_slippage_percent: 0.5,
    max_gas_price_gwei: 100,
    
    // Pair selection
    pairs: vec!["WETH/USDC", "WMATIC/USDC"],
    
    // Execution (FAST!)
    poll_interval_ms: 50,              // 20 Hz (vs normal 10 Hz)
    
    // Risk management
    max_daily_trades: Some(25),
    max_consecutive_losses: Some(6),
    daily_loss_limit_usd: Some(120.0),
    
    // Simulation
    simulate_competition: true,
    competition_rate: 0.55,            // Slightly less (we're faster!)
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 85-125 (more due to faster polling)
Opportunities passed filters: 34-50
Executed (after competition): 15-23 trades

Wins: 9-15 (60-65% win rate)
Losses: 6-8

Average profit per win: $8-12
Average loss: -$2-4
Gas per trade: $0.50

Daily PnL: +$45-105
Monthly PnL: +$1,350-3,150

CPU: Higher usage (20 Hz polling)
Network: More RPC calls (but still < limits)
```

**Why Test This**:
- Tests if speed matters (it should)
- Validates latency hypothesis
- Shows CPU/network cost of speed
- First-mover advantage test

**When to Deploy This**:
✅ If it meaningfully beats moderate (>20% better)
✅ If your VPS can handle it
❌ If gains don't justify complexity

---

## Scenario 10: "The Tortoise"
### Slow, Deliberate, Efficient

**Hypothesis**: "Slower polling reduces noise, catches only real opportunities"

### **Parameters**
```rust
PaperTradingConfig {
    name: "Tortoise",
    
    // Profit requirements
    min_profit_usd: 7.0,               // Higher threshold
    max_trade_size_usd: 1000.0,
    max_slippage_percent: 0.4,         // Tighter
    max_gas_price_gwei: 100,
    
    // Pair selection
    pairs: vec!["WETH/USDC", "WMATIC/USDC"],
    
    // Execution (SLOW)
    poll_interval_ms: 200,             // 5 Hz (vs normal 10 Hz)
    
    // Risk management
    max_daily_trades: Some(12),
    max_consecutive_losses: Some(4),
    daily_loss_limit_usd: Some(80.0),
    
    // Simulation
    simulate_competition: true,
    competition_rate: 0.70,            // Higher (we're slower)
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 35-55 (fewer due to slower polling)
Opportunities passed filters: 14-22
Executed (after competition): 4-7 trades

Wins: 3-5 (70-80% win rate - high!)
Losses: 1-2

Average profit per win: $10-15
Average loss: -$2-4
Gas per trade: $0.50

Daily PnL: +$25-65
Monthly PnL: +$750-1,950

CPU: Low usage
Network: Minimal RPC calls
```

**Why Test This**:
- Tests if slow/deliberate works
- Lower RPC usage (cost efficient)
- Higher quality opportunities only
- Less stress on system

**When to Deploy This**:
✅ If win rate compensates for fewer trades
✅ If you prefer simplicity
❌ If Speed Demon proves speed matters

---

## Scenario 11: "Gas Cowboy"
### Pay Any Gas Price

**Hypothesis**: "Paying premium gas captures more opportunities"

### **Parameters**
```rust
PaperTradingConfig {
    name: "Gas Cowboy",
    
    // Profit requirements
    min_profit_usd: 8.0,               // Need more to cover gas
    max_trade_size_usd: 1200.0,
    max_slippage_percent: 0.5,
    max_gas_price_gwei: 250,           // NO LIMIT!
    
    // Pair selection
    pairs: vec!["WETH/USDC", "WMATIC/USDC"],
    
    // Execution
    poll_interval_ms: 100,
    
    // Risk management
    max_daily_trades: Some(25),
    max_consecutive_losses: Some(6),
    daily_loss_limit_usd: Some(120.0),
    
    // Simulation
    simulate_competition: true,
    competition_rate: 0.50,            // Less (we pay to win!)
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 60-90
Opportunities passed filters: 30-45
Executed (after competition): 15-23 trades

Wins: 9-14 (60-65% win rate)
Losses: 6-9

Average profit per win: $10-15
Average loss: -$3-5
Gas per trade: $0.75 (higher!)

Daily PnL: +$35-90
Monthly PnL: +$1,050-2,700

Trade-off: More trades but higher gas costs
```

**Why Test This**:
- Tests gas price sensitivity
- On Polygon, gas is cheap (~$0.30-1.00)
- May be worth paying premium
- Shows cost/benefit of speed

**When to Deploy This**:
✅ If extra trades justify extra gas
❌ Probably not (Polygon gas already cheap)

---

## Scenario 12: "Penny Pincher"
### Low Gas Only

**Hypothesis**: "Patience pays - wait for cheap gas"

### **Parameters**
```rust
PaperTradingConfig {
    name: "Penny Pincher",
    
    // Profit requirements
    min_profit_usd: 4.0,               // Lower ok (gas is cheap)
    max_trade_size_usd: 1000.0,
    max_slippage_percent: 0.5,
    max_gas_price_gwei: 60,            // Only cheap gas!
    
    // Pair selection
    pairs: vec!["WETH/USDC", "WMATIC/USDC"],
    
    // Execution
    poll_interval_ms: 100,
    
    // Risk management
    max_daily_trades: Some(18),
    max_consecutive_losses: Some(5),
    daily_loss_limit_usd: Some(80.0),
    
    // Simulation
    simulate_competition: true,
    competition_rate: 0.65,
    simulate_slippage: true,
    simulate_gas_variance: true,
}
```

### **Expected Results**

**Daily Performance**:
```
Opportunities detected: 50-75
Opportunities passed filters: 20-30
Executed (after competition): 7-11 trades
Some skipped due to high gas!

Wins: 4-7 (60-65% win rate)
Losses: 3-4

Average profit per win: $6-10
Average loss: -$2-3
Gas per trade: $0.35 (lower!)

Daily PnL: +$20-55
Monthly PnL: +$600-1,650

Trade-off: Fewer trades but better margins
```

**Why Test This**:
- Tests gas price sensitivity
- Shows opportunity cost of waiting
- Validates margin optimization
- Cost efficiency test

**When to Deploy This**:
✅ If margins matter more than volume
❌ If missing opportunities costs more than gas savings

---

## Comparison Matrix

### **Expected 7-Day Results Summary**

| Scenario | Trades/Day | Win Rate | Daily PnL | Monthly PnL | Variance | Recommended |
|----------|------------|----------|-----------|-------------|----------|-------------|
| 1. Conservative | 1-2 | 80-90% | $15-40 | $450-1,200 | Low | ✅ Safe pick |
| **2. Moderate** | **8-12** | **60-70%** | **$30-70** | **$900-2,100** | **Medium** | **🏆 LIKELY WINNER** |
| 3. Aggressive | 36-54 | 50% | $20-80 | $600-2,400 | High | ❌ Too stressful |
| 4. Whale | 0-1 | 70-80% | $0-45 | $300-1,000 | Very High | ❌ Too infrequent |
| 5. Micro Trader | 25-38 | 55-60% | $10-50 | $300-1,500 | Medium | ❌ Gas costs too high |
| 6. WETH Specialist | 4-7 | 65-75% | $20-60 | $600-1,800 | Medium | ✅ Strong contender |
| 7. WMATIC Specialist | 5-9 | 70-80% | $25-70 | $750-2,100 | Medium | ✅ Strong contender |
| 8. Diversifier | 14-22 | 55-60% | $35-95 | $1,050-2,850 | Medium | ✅ Strong contender |
| 9. Speed Demon | 15-23 | 60-65% | $45-105 | $1,350-3,150 | Medium | ✅ If CPU allows |
| 10. Tortoise | 4-7 | 70-80% | $25-65 | $750-1,950 | Low | ✅ If high quality wins |
| 11. Gas Cowboy | 15-23 | 60-65% | $35-90 | $1,050-2,700 | Medium | ❓ Test needed |
| 12. Penny Pincher | 7-11 | 60-65% | $20-55 | $600-1,650 | Medium | ❓ Test needed |

---

## Deployment Strategy with $5K Capital

### **After 7 Days of Paper Trading**

#### **Step 1: Identify Winner** (Day 7)

Review metrics and identify top 3 performers:

**Likely Top 3**:
1. **Moderate** - Best risk/reward balance
2. **WMATIC Specialist** - Less competition, high win rate
3. **Speed Demon** - If speed proves advantageous

#### **Step 2: Deploy $1K Test** (Day 8-10)

```
Capital: $1,000 (20% of available)
Config: Winner from paper trading
Duration: 2-3 days
Goal: Validate paper results with real money

Expected: $30-70/day if Moderate wins
Stop if: 3 consecutive losses or -$100
```

#### **Step 3: Scale to $5K** (Day 11+)

```
If $1K test succeeds:
- Deploy full $5,000
- Use winning configuration exactly
- Monitor closely first 48 hours

Expected Daily: $150-350
Expected Monthly: $4,500-10,500 (90-210% ROI)
```

### **Capital Allocation Example** (Moderate Config)

```
Total Capital: $5,000

Per Trade:
- Trade size: $800-1,200
- Positions: 1-2 simultaneously
- Reserve: $2,000-3,000 (for gas + opportunities)

Daily Trades: 8-12
Daily Profit Target: $150-350
Monthly Target: $4,500-10,500

Risk Management:
- Stop if daily loss > $200
- Stop if 5 consecutive losses
- Withdraw profits weekly to cold storage
```

---

## Key Insights from Testing

### **What Paper Trading Will Reveal**

1. **Real Competition Rate**
   - Is it 50%? 70%? 90%?
   - Critical for setting expectations

2. **Optimal Profit Threshold**
   - $3? $5? $10?
   - Trade-off between volume and win rate

3. **Best Pairs**
   - WETH most liquid but most competitive?
   - WMATIC sweet spot?
   - Multi-pair worth complexity?

4. **Gas Price Impact**
   - Does paying premium gas help?
   - Is cheap gas worth waiting for?

5. **Speed Requirements**
   - Does 50ms polling beat 100ms?
   - Diminishing returns point?

6. **Realistic Win Rate**
   - Can we hit 60%? 70%?
   - What's achievable with competition?

7. **Trade Size Optimization**
   - Small positions better (low slippage)?
   - Large positions better (efficiency)?
   - Sweet spot likely $800-1,500

8. **Daily Profit Reality**
   - $50-100/day realistic?
   - $150-350/day with $5K?
   - Monthly ROI: 30-70%?

---

## Expected Winner Profile

### **Most Likely Winner: "Moderate+"**

```rust
// Winning config will likely be close to this
PaperTradingConfig {
    name: "Optimized Moderate",
    
    min_profit_usd: 5.0,              // Sweet spot
    max_trade_size_usd: 1000.0,       // Balanced
    max_slippage_percent: 0.5,        // Standard
    max_gas_price_gwei: 100,          // Normal
    
    pairs: vec![
        "WMATIC/USDC",                // Less competitive
        "WETH/USDC"                   // High volume
    ],
    
    poll_interval_ms: 100,            // 10 Hz sufficient
    
    max_daily_trades: Some(20),
    max_consecutive_losses: Some(5),
    daily_loss_limit_usd: Some(100.0),
    
    competition_rate: 0.60,           // Realistic
}
```

**Why This Will Win**:
- ✅ Balanced risk/reward
- ✅ Sustainable trade frequency
- ✅ Good liquidity
- ✅ Moderate competition
- ✅ Scalable parameters
- ✅ Not too aggressive

**$5K Deployment Projection**:
```
Daily trades: 8-12
Win rate: 60-65%
Daily profit: $150-350
Monthly profit: $4,500-10,500
Monthly ROI: 90-210%

After 3 months: $13,500-31,500 profit
```

---

## Action Plan

### **Week 1: Paper Trading**
```
☐ Deploy all 12 configurations
☐ Run 24/7 for 7 days
☐ Collect comprehensive metrics
☐ Export to CSV for analysis
```

### **Week 2: Analysis & Test Deploy**
```
☐ Identify winner (Day 7)
☐ Deploy $1K test (Day 8-10)
☐ Validate results
☐ Adjust parameters if needed
```

### **Week 3: Full Deployment**
```
☐ Deploy $5K with winning config
☐ Monitor intensely first 48h
☐ Scale gradually
☐ Withdraw profits weekly
```

### **Week 4+: Optimization**
```
☐ Fine-tune parameters based on real data
☐ Consider adding more pairs
☐ Plan Phase 2 (flash loans)
☐ Scale to $10K+ if successful
```

---

## Summary

**12 scenarios test**:
- 3 risk profiles (Conservative, Moderate, Aggressive)
- 2 size strategies (Micro, Whale)
- 3 pair strategies (WETH only, WMATIC only, Multi-pair)
- 2 speed strategies (Fast, Slow)
- 2 gas strategies (High, Low)

**Expected winner**: Moderate or WMATIC Specialist

**With $5K deployed**: $150-350/day, $4,500-10,500/month

**Next step**: Run paper trading for 7 days, let data decide! 🎯
