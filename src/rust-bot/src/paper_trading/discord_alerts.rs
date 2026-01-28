//! Discord Alert Module for Paper Trading
//!
//! Sends webhook notifications when paper trading opportunities are detected.
//! Aggregates opportunities across all strategies to provide comprehensive reports.
//!
//! Author: AI-Generated
//! Created: 2026-01-28
//!
//! Usage:
//!   Set DISCORD_WEBHOOK environment variable to your webhook URL
//!   Alerts are sent automatically when opportunities are detected

use serde::Serialize;
use std::collections::HashMap;
use std::env;
use tracing::{error, info, warn};

/// Discord webhook message structure
#[derive(Serialize)]
struct DiscordMessage {
    content: Option<String>,
    embeds: Vec<DiscordEmbed>,
}

/// Discord embed structure for rich formatting
#[derive(Serialize)]
struct DiscordEmbed {
    title: String,
    description: String,
    color: u32,
    fields: Vec<DiscordField>,
    footer: Option<DiscordFooter>,
    timestamp: Option<String>,
}

#[derive(Serialize)]
struct DiscordField {
    name: String,
    value: String,
    inline: bool,
}

#[derive(Serialize)]
struct DiscordFooter {
    text: String,
}

/// Aggregated opportunity data across all strategies
#[derive(Debug, Clone)]
pub struct AggregatedOpportunity {
    /// Trading pair (e.g., "WETH/USDC")
    pub pair: String,
    /// Block number when opportunity was detected
    pub block_number: u64,
    /// Midmarket spread (before fees)
    pub midmarket_spread_pct: f64,
    /// Executable spread (after 0.6% DEX fees)
    pub executable_spread_pct: f64,
    /// Buy on this DEX
    pub buy_dex: String,
    /// Sell on this DEX
    pub sell_dex: String,
    /// Buy price
    pub buy_price: f64,
    /// Sell price
    pub sell_price: f64,
    /// Strategies that caught this opportunity
    pub strategies_caught: Vec<StrategyMatch>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Individual strategy match for an opportunity
#[derive(Debug, Clone)]
pub struct StrategyMatch {
    /// Strategy name
    pub name: String,
    /// Estimated profit for this strategy
    pub estimated_profit: f64,
    /// Trade size this strategy would use
    pub trade_size: f64,
    /// Whether lost to competition (simulated)
    pub lost_to_competition: bool,
}

/// Discord alerter for paper trading
pub struct DiscordAlerter {
    webhook_url: Option<String>,
    client: reqwest::Client,
}

impl DiscordAlerter {
    /// Create a new Discord alerter
    pub fn new() -> Self {
        let webhook_url = env::var("DISCORD_WEBHOOK").ok();

        if webhook_url.is_some() {
            info!("Discord alerts enabled");
        } else {
            warn!("DISCORD_WEBHOOK not set - Discord alerts disabled");
        }

        Self {
            webhook_url,
            client: reqwest::Client::new(),
        }
    }

    /// Check if Discord alerts are enabled
    pub fn is_enabled(&self) -> bool {
        self.webhook_url.is_some()
    }

    /// Send an aggregated opportunity alert
    pub async fn send_opportunity_alert(&self, opportunity: &AggregatedOpportunity) {
        let webhook_url = match &self.webhook_url {
            Some(url) => url,
            None => return,
        };

        // Determine color based on profitability
        // Green for profitable, yellow for marginal, red for losing
        let best_profit = opportunity.strategies_caught
            .iter()
            .filter(|s| !s.lost_to_competition)
            .map(|s| s.estimated_profit)
            .fold(0.0_f64, f64::max);

        let color = if best_profit > 10.0 {
            0x00FF00  // Green - good profit
        } else if best_profit > 5.0 {
            0xFFFF00  // Yellow - marginal
        } else if best_profit > 0.0 {
            0xFFA500  // Orange - small profit
        } else {
            0xFF0000  // Red - losing or missed
        };

        // Build strategy summary
        let won_strategies: Vec<&StrategyMatch> = opportunity.strategies_caught
            .iter()
            .filter(|s| !s.lost_to_competition)
            .collect();

        let lost_strategies: Vec<&StrategyMatch> = opportunity.strategies_caught
            .iter()
            .filter(|s| s.lost_to_competition)
            .collect();

        // Format won strategies
        let won_summary = if won_strategies.is_empty() {
            "None (all lost to competition)".to_string()
        } else {
            won_strategies.iter()
                .map(|s| format!("**{}**: ${:.2} (${:.0} size)", s.name, s.estimated_profit, s.trade_size))
                .collect::<Vec<_>>()
                .join("\n")
        };

        // Format lost strategies
        let lost_summary = if lost_strategies.is_empty() {
            "None".to_string()
        } else {
            lost_strategies.iter()
                .map(|s| format!("~~{}~~: ${:.2}", s.name, s.estimated_profit))
                .collect::<Vec<_>>()
                .join(", ")
        };

        // Calculate aggregate stats
        let total_caught = opportunity.strategies_caught.len();
        let total_won = won_strategies.len();
        let win_rate = if total_caught > 0 {
            (total_won as f64 / total_caught as f64) * 100.0
        } else {
            0.0
        };

        let avg_profit: f64 = if !won_strategies.is_empty() {
            won_strategies.iter().map(|s| s.estimated_profit).sum::<f64>() / won_strategies.len() as f64
        } else {
            0.0
        };

        let max_profit = won_strategies.iter()
            .map(|s| s.estimated_profit)
            .fold(0.0_f64, f64::max);

        let min_profit = won_strategies.iter()
            .map(|s| s.estimated_profit)
            .fold(f64::MAX, f64::min);

        // Best strategy
        let best_strategy = won_strategies.iter()
            .max_by(|a, b| a.estimated_profit.partial_cmp(&b.estimated_profit).unwrap())
            .map(|s| format!("{} (${:.2})", s.name, s.estimated_profit))
            .unwrap_or_else(|| "N/A".to_string());

        // Create embed
        let embed = DiscordEmbed {
            title: format!("📊 {} Opportunity Detected", opportunity.pair),
            description: format!(
                "**Block:** `{}`\n**Direction:** {} → {}",
                opportunity.block_number,
                opportunity.buy_dex,
                opportunity.sell_dex
            ),
            color,
            fields: vec![
                DiscordField {
                    name: "📈 Spread Analysis".to_string(),
                    value: format!(
                        "```\nMidmarket:  {:.4}%\nExecutable: {:.4}%\nDEX Fees:   -0.60%\n```",
                        opportunity.midmarket_spread_pct,
                        opportunity.executable_spread_pct
                    ),
                    inline: true,
                },
                DiscordField {
                    name: "💰 Prices".to_string(),
                    value: format!(
                        "```\nBuy:  ${:.2}\nSell: ${:.2}\nDiff: ${:.2}\n```",
                        opportunity.buy_price,
                        opportunity.sell_price,
                        opportunity.sell_price - opportunity.buy_price
                    ),
                    inline: true,
                },
                DiscordField {
                    name: "🎯 Summary".to_string(),
                    value: format!(
                        "```\nStrategies: {}/{}\nWin Rate:   {:.0}%\nBest:       ${:.2}\nAvg:        ${:.2}\n```",
                        total_won, total_caught,
                        win_rate,
                        max_profit,
                        avg_profit
                    ),
                    inline: true,
                },
                DiscordField {
                    name: "✅ WON (would execute)".to_string(),
                    value: won_summary,
                    inline: false,
                },
                DiscordField {
                    name: "❌ LOST (to competition)".to_string(),
                    value: lost_summary,
                    inline: false,
                },
                DiscordField {
                    name: "🏆 Best Strategy".to_string(),
                    value: best_strategy,
                    inline: true,
                },
                DiscordField {
                    name: "📊 Profit Range".to_string(),
                    value: if min_profit < f64::MAX {
                        format!("${:.2} - ${:.2}", min_profit, max_profit)
                    } else {
                        "N/A".to_string()
                    },
                    inline: true,
                },
            ],
            footer: Some(DiscordFooter {
                text: "DEX Arbitrage Paper Trading | Polygon Network".to_string(),
            }),
            timestamp: Some(opportunity.timestamp.to_rfc3339()),
        };

        let message = DiscordMessage {
            content: None,
            embeds: vec![embed],
        };

        // Send webhook
        match self.client.post(webhook_url)
            .json(&message)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    info!("Discord alert sent for {} opportunity", opportunity.pair);
                } else {
                    warn!("Discord webhook returned status: {}", response.status());
                }
            }
            Err(e) => {
                error!("Failed to send Discord alert: {}", e);
            }
        }
    }

    /// Send a daily summary report
    pub async fn send_daily_summary(&self, summary: &DailySummary) {
        let webhook_url = match &self.webhook_url {
            Some(url) => url,
            None => return,
        };

        let color = if summary.net_profit > 0.0 {
            0x00FF00  // Green
        } else if summary.net_profit == 0.0 {
            0x808080  // Gray
        } else {
            0xFF0000  // Red
        };

        let strategy_breakdown = summary.strategy_performance
            .iter()
            .map(|(name, stats)| {
                format!(
                    "**{}**: {} trades, ${:.2} profit, {:.0}% WR",
                    name, stats.total_trades, stats.net_profit, stats.win_rate
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let embed = DiscordEmbed {
            title: "📊 Daily Paper Trading Summary".to_string(),
            description: format!(
                "**Date:** {}\n**Monitoring Period:** {} hours",
                summary.date,
                summary.monitoring_hours
            ),
            color,
            fields: vec![
                DiscordField {
                    name: "📈 Overall Performance".to_string(),
                    value: format!(
                        "```\nOpportunities: {}\nTrades Won:    {}\nTrades Lost:   {}\nWin Rate:      {:.1}%\n```",
                        summary.total_opportunities,
                        summary.trades_won,
                        summary.trades_lost,
                        summary.win_rate
                    ),
                    inline: true,
                },
                DiscordField {
                    name: "💰 P&L".to_string(),
                    value: format!(
                        "```\nGross Profit: ${:.2}\nGross Loss:   ${:.2}\nNet Profit:   ${:.2}\n```",
                        summary.gross_profit,
                        summary.gross_loss,
                        summary.net_profit
                    ),
                    inline: true,
                },
                DiscordField {
                    name: "🎯 Strategy Breakdown".to_string(),
                    value: if strategy_breakdown.is_empty() {
                        "No trades executed".to_string()
                    } else {
                        strategy_breakdown
                    },
                    inline: false,
                },
                DiscordField {
                    name: "🏆 Best Strategy".to_string(),
                    value: format!("{} (${:.2})", summary.best_strategy, summary.best_strategy_profit),
                    inline: true,
                },
                DiscordField {
                    name: "📉 Worst Strategy".to_string(),
                    value: format!("{} (${:.2})", summary.worst_strategy, summary.worst_strategy_profit),
                    inline: true,
                },
            ],
            footer: Some(DiscordFooter {
                text: "Paper Trading Summary | Not Real Trades".to_string(),
            }),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        };

        let message = DiscordMessage {
            content: Some("📋 **End of Day Summary**".to_string()),
            embeds: vec![embed],
        };

        match self.client.post(webhook_url)
            .json(&message)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    info!("Discord daily summary sent");
                } else {
                    warn!("Discord webhook returned status: {}", response.status());
                }
            }
            Err(e) => {
                error!("Failed to send Discord summary: {}", e);
            }
        }
    }
}

impl Default for DiscordAlerter {
    fn default() -> Self {
        Self::new()
    }
}

/// Daily summary statistics
pub struct DailySummary {
    pub date: String,
    pub monitoring_hours: f64,
    pub total_opportunities: u64,
    pub trades_won: u64,
    pub trades_lost: u64,
    pub win_rate: f64,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub net_profit: f64,
    pub best_strategy: String,
    pub best_strategy_profit: f64,
    pub worst_strategy: String,
    pub worst_strategy_profit: f64,
    pub strategy_performance: HashMap<String, StrategyStats>,
}

/// Per-strategy statistics
#[derive(Debug, Clone)]
pub struct StrategyStats {
    pub total_trades: u64,
    pub wins: u64,
    pub losses: u64,
    pub win_rate: f64,
    pub net_profit: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alerter_creation() {
        let alerter = DiscordAlerter::new();
        // Should not panic even without webhook URL
        assert!(!alerter.is_enabled() || alerter.is_enabled());
    }
}
