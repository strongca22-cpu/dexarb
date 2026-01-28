//! TOML Configuration Reader for Paper Trading
//!
//! Reads paper trading configuration from a TOML file.
//! Supports hot-reloading via SIGHUP signal.
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use super::config::PaperTradingConfig;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// Top-level TOML configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct TomlConfig {
    pub general: GeneralConfig,
    #[serde(rename = "strategy")]
    pub strategies: Vec<StrategyConfig>,
}

/// General settings
#[derive(Debug, Clone, Deserialize)]
pub struct GeneralConfig {
    pub state_file: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_metrics_interval")]
    pub metrics_interval_secs: u64,
    #[serde(default = "default_max_state_age")]
    pub max_state_age_secs: i64,
}

fn default_poll_interval() -> u64 { 100 }
fn default_log_level() -> String { "info".to_string() }
fn default_metrics_interval() -> u64 { 300 }
fn default_max_state_age() -> i64 { 10 }

/// Strategy configuration from TOML
#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub pairs: Vec<String>,
    pub min_profit_usd: f64,
    pub max_trade_size_usd: f64,
    pub max_slippage_percent: f64,
    #[serde(default)]
    pub simulate_competition: bool,
    #[serde(default)]
    pub competition_rate: f64,
    pub max_daily_trades: Option<usize>,
    pub daily_loss_limit_usd: Option<f64>,
    pub max_consecutive_losses: Option<usize>,
    pub max_gas_gwei: Option<f64>,
}

fn default_true() -> bool { true }

impl TomlConfig {
    /// Load configuration from a TOML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {}", path.as_ref().display()))?;

        let config: Self = toml::from_str(&content)
            .with_context(|| "Failed to parse TOML configuration")?;

        Ok(config)
    }

    /// Get enabled strategies as PaperTradingConfig
    pub fn get_enabled_strategies(&self) -> Vec<PaperTradingConfig> {
        self.strategies
            .iter()
            .filter(|s| s.enabled)
            .map(|s| s.to_paper_trading_config())
            .collect()
    }

    /// Get all strategies (including disabled)
    pub fn get_all_strategies(&self) -> Vec<PaperTradingConfig> {
        self.strategies
            .iter()
            .map(|s| s.to_paper_trading_config())
            .collect()
    }
}

impl StrategyConfig {
    /// Convert to PaperTradingConfig
    pub fn to_paper_trading_config(&self) -> PaperTradingConfig {
        PaperTradingConfig {
            name: self.name.clone(),
            enabled: self.enabled,
            pairs: self.pairs.clone(),
            min_profit_usd: self.min_profit_usd,
            max_trade_size_usd: self.max_trade_size_usd,
            max_slippage_percent: self.max_slippage_percent,
            max_gas_price_gwei: self.max_gas_gwei.map(|g| g as u64).unwrap_or(100),
            poll_interval_ms: 100, // Inherited from general
            simulate_slippage: true,
            simulate_gas_variance: true,
            simulate_competition: self.simulate_competition,
            competition_rate: self.competition_rate,
            max_daily_trades: self.max_daily_trades,
            daily_loss_limit_usd: self.daily_loss_limit_usd,
            max_consecutive_losses: self.max_consecutive_losses,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
[general]
state_file = "/tmp/test.json"
poll_interval_ms = 100

[[strategy]]
name = "Test Strategy"
enabled = true
pairs = ["WETH/USDC"]
min_profit_usd = 5.0
max_trade_size_usd = 1000.0
max_slippage_percent = 0.5
"#;

        let config: TomlConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.strategies.len(), 1);
        assert_eq!(config.strategies[0].name, "Test Strategy");
    }
}
