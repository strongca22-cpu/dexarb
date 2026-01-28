//! Trader Metrics Tracking
//!
//! Tracks performance metrics for each paper trading configuration.
//! Enables comparison across strategies to find optimal parameters.
//!
//! Author: AI-Generated
//! Created: 2026-01-28

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Result of a simulated trade execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedTradeResult {
    /// Trading pair symbol
    pub pair: String,
    /// Whether the trade was profitable
    pub success: bool,
    /// Gross profit before costs
    pub profit_usd: f64,
    /// Gas cost in USD
    pub gas_cost_usd: f64,
    /// Net profit after all costs
    pub net_profit_usd: f64,
    /// Execution simulation time in ms
    pub execution_time_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Comprehensive metrics for a single paper trading configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderMetrics {
    /// Configuration name this tracks
    pub config_name: String,

    // Trade counts
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,

    // Profitability
    pub total_profit_usd: f64,
    pub total_loss_usd: f64,
    pub total_gas_usd: f64,
    pub net_profit_usd: f64,

    // Performance ratios
    pub win_rate: f64,
    pub avg_profit_per_trade: f64,
    pub avg_profit_per_win: f64,
    pub avg_loss_per_loss: f64,
    pub largest_win: f64,
    pub largest_loss: f64,

    // Opportunity tracking
    pub opportunities_detected: usize,
    pub opportunities_executed: usize,
    pub opportunities_missed: usize,
    pub missed_profit_usd: f64,

    // Risk metrics
    pub consecutive_losses: usize,
    pub max_consecutive_losses: usize,
    pub daily_trades_today: usize,
    pub daily_loss_today: f64,

    // Timing
    pub start_time: DateTime<Utc>,
    pub last_trade_time: Option<DateTime<Utc>>,
    pub last_reset_date: DateTime<Utc>,

    // Trade history (keep last N trades)
    pub recent_trades: Vec<SimulatedTradeResult>,
}

impl TraderMetrics {
    /// Maximum number of recent trades to keep in memory
    const MAX_RECENT_TRADES: usize = 100;

    /// Create new metrics tracker for a configuration
    pub fn new(config_name: String) -> Self {
        let now = Utc::now();
        Self {
            config_name,
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
            total_profit_usd: 0.0,
            total_loss_usd: 0.0,
            total_gas_usd: 0.0,
            net_profit_usd: 0.0,
            win_rate: 0.0,
            avg_profit_per_trade: 0.0,
            avg_profit_per_win: 0.0,
            avg_loss_per_loss: 0.0,
            largest_win: 0.0,
            largest_loss: 0.0,
            opportunities_detected: 0,
            opportunities_executed: 0,
            opportunities_missed: 0,
            missed_profit_usd: 0.0,
            consecutive_losses: 0,
            max_consecutive_losses: 0,
            daily_trades_today: 0,
            daily_loss_today: 0.0,
            start_time: now,
            last_trade_time: None,
            last_reset_date: now,
            recent_trades: Vec::new(),
        }
    }

    /// Record a completed trade result
    pub fn record_trade(&mut self, result: SimulatedTradeResult) {
        self.total_trades += 1;
        self.daily_trades_today += 1;
        self.last_trade_time = Some(Utc::now());
        self.total_gas_usd += result.gas_cost_usd;

        if result.success && result.net_profit_usd > 0.0 {
            self.winning_trades += 1;
            self.total_profit_usd += result.net_profit_usd;
            self.consecutive_losses = 0;

            if result.net_profit_usd > self.largest_win {
                self.largest_win = result.net_profit_usd;
            }
        } else {
            self.losing_trades += 1;
            let loss = result.net_profit_usd.abs();
            self.total_loss_usd += loss;
            self.daily_loss_today += loss;
            self.consecutive_losses += 1;

            if self.consecutive_losses > self.max_consecutive_losses {
                self.max_consecutive_losses = self.consecutive_losses;
            }

            if result.net_profit_usd < self.largest_loss {
                self.largest_loss = result.net_profit_usd;
            }
        }

        self.opportunities_executed += 1;
        self.recalculate_ratios();

        // Store in recent history
        self.recent_trades.push(result);
        if self.recent_trades.len() > Self::MAX_RECENT_TRADES {
            self.recent_trades.remove(0);
        }
    }

    /// Record a missed opportunity (lost to competition)
    pub fn record_missed_opportunity(&mut self, potential_profit: f64) {
        self.opportunities_detected += 1;
        self.opportunities_missed += 1;
        self.missed_profit_usd += potential_profit;
    }

    /// Record a detected opportunity (whether executed or not)
    pub fn record_detected_opportunity(&mut self) {
        self.opportunities_detected += 1;
    }

    /// Recalculate derived metrics
    fn recalculate_ratios(&mut self) {
        self.net_profit_usd = self.total_profit_usd - self.total_loss_usd;

        self.win_rate = if self.total_trades > 0 {
            self.winning_trades as f64 / self.total_trades as f64
        } else {
            0.0
        };

        self.avg_profit_per_trade = if self.total_trades > 0 {
            self.net_profit_usd / self.total_trades as f64
        } else {
            0.0
        };

        self.avg_profit_per_win = if self.winning_trades > 0 {
            self.total_profit_usd / self.winning_trades as f64
        } else {
            0.0
        };

        self.avg_loss_per_loss = if self.losing_trades > 0 {
            self.total_loss_usd / self.losing_trades as f64
        } else {
            0.0
        };
    }

    /// Reset daily counters (call at midnight)
    pub fn reset_daily(&mut self) {
        self.daily_trades_today = 0;
        self.daily_loss_today = 0.0;
        self.last_reset_date = Utc::now();
    }

    /// Check if we should reset daily counters
    pub fn check_daily_reset(&mut self) {
        let now = Utc::now();
        if now.date_naive() != self.last_reset_date.date_naive() {
            self.reset_daily();
        }
    }

    /// Get current daily trade count
    pub fn daily_trades(&self) -> usize {
        self.daily_trades_today
    }

    /// Get current daily loss
    pub fn daily_loss(&self) -> f64 {
        self.daily_loss_today
    }

    /// Get current consecutive losses
    pub fn consecutive_losses(&self) -> usize {
        self.consecutive_losses
    }

    /// Generate a summary string for logging
    pub fn summary(&self) -> String {
        format!(
            "{}: {} trades ({} wins, {} losses) | Win rate: {:.1}% | Net: ${:.2} | Avg: ${:.2}/trade",
            self.config_name,
            self.total_trades,
            self.winning_trades,
            self.losing_trades,
            self.win_rate * 100.0,
            self.net_profit_usd,
            self.avg_profit_per_trade
        )
    }
}

/// Aggregates metrics across all paper trading configurations
#[derive(Debug, Clone)]
pub struct MetricsAggregator {
    /// All tracked metrics by config name
    pub metrics: Vec<TraderMetrics>,
}

impl MetricsAggregator {
    pub fn new() -> Self {
        Self {
            metrics: Vec::new(),
        }
    }

    /// Add a new metrics tracker
    pub fn add(&mut self, metrics: TraderMetrics) {
        self.metrics.push(metrics);
    }

    /// Find the best performing configuration by net profit
    pub fn best_by_profit(&self) -> Option<&TraderMetrics> {
        self.metrics
            .iter()
            .max_by(|a, b| a.net_profit_usd.partial_cmp(&b.net_profit_usd).unwrap())
    }

    /// Find the best performing configuration by win rate
    pub fn best_by_win_rate(&self) -> Option<&TraderMetrics> {
        self.metrics
            .iter()
            .filter(|m| m.total_trades >= 10) // Need at least 10 trades
            .max_by(|a, b| a.win_rate.partial_cmp(&b.win_rate).unwrap())
    }

    /// Generate a comparison report
    pub fn generate_report(&self) -> String {
        let mut report = String::new();
        report.push_str("═══════════════════════════════════════════════════════\n");
        report.push_str("           PAPER TRADING PERFORMANCE REPORT           \n");
        report.push_str("═══════════════════════════════════════════════════════\n\n");

        for m in &self.metrics {
            report.push_str(&format!("Configuration: {}\n", m.config_name));
            report.push_str("─────────────────────────────────────────────────────\n");
            report.push_str(&format!(
                "Total Trades: {} (Wins: {}, Losses: {})\n",
                m.total_trades, m.winning_trades, m.losing_trades
            ));
            report.push_str(&format!("Win Rate: {:.1}%\n", m.win_rate * 100.0));
            report.push_str(&format!("Net Profit: ${:.2}\n", m.net_profit_usd));
            report.push_str(&format!("Avg Profit/Trade: ${:.2}\n", m.avg_profit_per_trade));
            report.push_str(&format!("Largest Win: ${:.2}\n", m.largest_win));
            report.push_str(&format!("Largest Loss: ${:.2}\n", m.largest_loss));
            report.push_str(&format!(
                "Opportunities: {} detected, {} executed, {} missed\n",
                m.opportunities_detected, m.opportunities_executed, m.opportunities_missed
            ));
            report.push_str(&format!("Missed Potential: ${:.2}\n", m.missed_profit_usd));
            report.push_str("\n");
        }

        if let Some(best) = self.best_by_profit() {
            report.push_str("═══════════════════════════════════════════════════════\n");
            report.push_str(&format!("BEST PERFORMER: {}\n", best.config_name));
            report.push_str(&format!("   Net Profit: ${:.2}\n", best.net_profit_usd));
            report.push_str(&format!("   Win Rate: {:.1}%\n", best.win_rate * 100.0));
            report.push_str("═══════════════════════════════════════════════════════\n");
        }

        report
    }
}

impl Default for MetricsAggregator {
    fn default() -> Self {
        Self::new()
    }
}
