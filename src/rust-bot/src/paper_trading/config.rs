//! Paper Trading Configuration
//!
//! Defines configuration presets for multi-configuration paper trading.
//! Each config represents a different trading strategy to test in parallel.
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use serde::{Deserialize, Serialize};

/// Configuration for a single paper trading strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTradingConfig {
    /// Name identifier for this configuration
    pub name: String,
    /// Whether this configuration is active
    pub enabled: bool,

    // Trading parameters
    /// Minimum profit threshold in USD to consider a trade
    pub min_profit_usd: f64,
    /// Maximum trade size in USD
    pub max_trade_size_usd: f64,
    /// Maximum acceptable slippage percentage
    pub max_slippage_percent: f64,
    /// Maximum gas price in gwei to execute
    pub max_gas_price_gwei: u64,

    // Pair selection
    /// Trading pairs to monitor (e.g., ["WETH/USDC", "WMATIC/USDC"])
    pub pairs: Vec<String>,

    // Timing
    /// Polling interval in milliseconds
    pub poll_interval_ms: u64,

    // Execution simulation
    /// Whether to simulate slippage losses
    pub simulate_slippage: bool,
    /// Whether to simulate gas cost variance
    pub simulate_gas_variance: bool,
    /// Whether to simulate competition (others taking opportunities)
    pub simulate_competition: bool,
    /// Rate at which opportunities are lost to competition (0.0 to 1.0)
    pub competition_rate: f64,

    // Risk management
    /// Maximum trades per day (None = unlimited)
    pub max_daily_trades: Option<usize>,
    /// Stop trading after N consecutive losses
    pub max_consecutive_losses: Option<usize>,
    /// Maximum daily loss in USD before stopping
    pub daily_loss_limit_usd: Option<f64>,
}

impl Default for PaperTradingConfig {
    fn default() -> Self {
        Self::moderate()
    }
}

impl PaperTradingConfig {
    /// Conservative configuration - high thresholds, low risk
    pub fn conservative() -> Self {
        Self {
            name: "Conservative".to_string(),
            enabled: true,
            min_profit_usd: 10.0,
            max_trade_size_usd: 500.0,
            max_slippage_percent: 0.3,
            max_gas_price_gwei: 80,
            pairs: vec!["WETH/USDC".to_string()],
            poll_interval_ms: 100,
            simulate_slippage: true,
            simulate_gas_variance: true,
            simulate_competition: true,
            competition_rate: 0.7, // Assume 70% of opps taken by others
            max_daily_trades: Some(10),
            max_consecutive_losses: Some(3),
            daily_loss_limit_usd: Some(50.0),
        }
    }

    /// Moderate configuration - balanced approach
    pub fn moderate() -> Self {
        Self {
            name: "Moderate".to_string(),
            enabled: true,
            min_profit_usd: 5.0,
            max_trade_size_usd: 1000.0,
            max_slippage_percent: 0.5,
            max_gas_price_gwei: 100,
            pairs: vec!["WETH/USDC".to_string(), "WMATIC/USDC".to_string()],
            poll_interval_ms: 100,
            simulate_slippage: true,
            simulate_gas_variance: true,
            simulate_competition: true,
            competition_rate: 0.5,
            max_daily_trades: Some(20),
            max_consecutive_losses: Some(5),
            daily_loss_limit_usd: Some(100.0),
        }
    }

    /// Aggressive configuration - lower thresholds, higher risk
    pub fn aggressive() -> Self {
        Self {
            name: "Aggressive".to_string(),
            enabled: true,
            min_profit_usd: 3.0,
            max_trade_size_usd: 2000.0,
            max_slippage_percent: 1.0,
            max_gas_price_gwei: 150,
            pairs: vec![
                "WETH/USDC".to_string(),
                "WMATIC/USDC".to_string(),
                "WBTC/USDC".to_string(),
            ],
            poll_interval_ms: 50,
            simulate_slippage: true,
            simulate_gas_variance: true,
            simulate_competition: true,
            competition_rate: 0.3, // More optimistic
            max_daily_trades: Some(50),
            max_consecutive_losses: Some(10),
            daily_loss_limit_usd: Some(200.0),
        }
    }

    /// Large trades configuration
    pub fn large_trades() -> Self {
        let mut config = Self::moderate();
        config.name = "Large Trades".to_string();
        config.max_trade_size_usd = 5000.0;
        config.min_profit_usd = 20.0;
        config
    }

    /// Small trades configuration
    pub fn small_trades() -> Self {
        let mut config = Self::moderate();
        config.name = "Small Trades".to_string();
        config.max_trade_size_usd = 100.0;
        config.min_profit_usd = 2.0;
        config
    }

    /// WETH only configuration
    pub fn weth_only() -> Self {
        let mut config = Self::moderate();
        config.name = "WETH Only".to_string();
        config.pairs = vec!["WETH/USDC".to_string()];
        config
    }

    /// WMATIC only configuration
    pub fn wmatic_only() -> Self {
        let mut config = Self::moderate();
        config.name = "WMATIC Only".to_string();
        config.pairs = vec!["WMATIC/USDC".to_string()];
        config
    }

    /// Multi-pair configuration
    pub fn multi_pair() -> Self {
        let mut config = Self::moderate();
        config.name = "Multi-Pair".to_string();
        config.pairs = vec![
            "WETH/USDC".to_string(),
            "WMATIC/USDC".to_string(),
            "WBTC/USDC".to_string(),
        ];
        config
    }

    /// Fast polling configuration (20 Hz)
    pub fn fast_polling() -> Self {
        let mut config = Self::moderate();
        config.name = "Fast Polling".to_string();
        config.poll_interval_ms = 50;
        config
    }

    /// Slow polling configuration (5 Hz)
    pub fn slow_polling() -> Self {
        let mut config = Self::moderate();
        config.name = "Slow Polling".to_string();
        config.poll_interval_ms = 200;
        config
    }

    /// High gas limit configuration
    pub fn high_gas() -> Self {
        let mut config = Self::moderate();
        config.name = "High Gas Limit".to_string();
        config.max_gas_price_gwei = 200;
        config.min_profit_usd = 8.0;
        config
    }

    /// Low gas limit configuration
    pub fn low_gas() -> Self {
        let mut config = Self::moderate();
        config.name = "Low Gas Limit".to_string();
        config.max_gas_price_gwei = 50;
        config.min_profit_usd = 3.0;
        config
    }

    /// Returns all 12 preset configurations
    pub fn all_presets() -> Vec<Self> {
        vec![
            Self::conservative(),
            Self::moderate(),
            Self::aggressive(),
            Self::large_trades(),
            Self::small_trades(),
            Self::weth_only(),
            Self::wmatic_only(),
            Self::multi_pair(),
            Self::fast_polling(),
            Self::slow_polling(),
            Self::high_gas(),
            Self::low_gas(),
        ]
    }
}
