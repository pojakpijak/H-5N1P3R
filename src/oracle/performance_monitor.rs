//! PerformanceMonitor module - Pillar II (Observe)
//!
//! This module observes and measures Oracle performance by analyzing historical data
//! from the DecisionLedger to compute Key Performance Indicators (KPIs).

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, error};

use crate::oracle::types::{
    PerformanceReport, PerformanceReportSender, TransactionRecord, Outcome
};
use crate::oracle::storage::LedgerStorage;

/// PerformanceMonitor analyzes historical trading performance and generates reports
pub struct PerformanceMonitor {
    storage: Arc<dyn LedgerStorage>,
    report_sender: PerformanceReportSender,
    analysis_interval: Duration,
    time_window_hours: f64,
}

impl PerformanceMonitor {
    /// Create a new PerformanceMonitor
    pub fn new(
        storage: Arc<dyn LedgerStorage>,
        report_sender: PerformanceReportSender,
        analysis_interval_minutes: u64,
        time_window_hours: u64,
    ) -> Self {
        Self {
            storage,
            report_sender,
            analysis_interval: Duration::from_secs(analysis_interval_minutes * 60),
            time_window_hours: time_window_hours as f64,
        }
    }

    /// Main execution loop - periodically analyzes performance
    pub async fn run(self) {
        info!("PerformanceMonitor is running. Analysis every {} minutes.", 
              self.analysis_interval.as_secs() / 60);
        
        let mut interval = tokio::time::interval(self.analysis_interval);

        loop {
            interval.tick().await;
            info!("Performing periodic performance analysis...");
            
            match self.analyze_performance().await {
                Ok(report) => {
                    info!("Performance analysis complete. Profit Factor: {:.2}, Win Rate: {:.2}%", 
                          report.profit_factor, report.win_rate_percent);
                    
                    if let Err(e) = self.report_sender.send(report).await {
                        error!("Failed to send performance report: {}", e);
                    }
                }
                Err(e) => {
                    error!("Error during performance analysis: {}", e);
                }
            }
        }
    }

    /// Analyze performance using DecisionLedger data (public for testing)
    pub async fn analyze_performance(&self) -> Result<PerformanceReport> {
        let since_timestamp = (chrono::Utc::now() - chrono::Duration::hours(self.time_window_hours as i64))
            .timestamp_millis() as u64;
        
        // Query historical records from the DecisionLedger database
        let records = self.get_records_since(since_timestamp).await?;
        
        // Filter for closed trades (with definitive outcomes)
        let closed_trades: Vec<_> = records.iter()
            .filter(|r| matches!(r.actual_outcome, Outcome::Profit(_) | Outcome::Loss(_)))
            .collect();

        if closed_trades.is_empty() {
            return Ok(PerformanceReport::default());
        }

        let mut total_profit = 0.0;
        let mut total_loss = 0.0;
        let mut profitable_trades = 0;

        for trade in &closed_trades {
            match trade.actual_outcome {
                Outcome::Profit(p) => {
                    total_profit += p;
                    profitable_trades += 1;
                }
                Outcome::Loss(l) => {
                    total_loss += l.abs(); // Losses are stored as negative, take absolute value
                }
                _ => {} // Already filtered for Profit/Loss only
            }
        }

        let win_rate_percent = (profitable_trades as f64 / closed_trades.len() as f64) * 100.0;
        let profit_factor = if total_loss > 0.0 { 
            total_profit / total_loss 
        } else { 
            f64::INFINITY 
        };

        let report = PerformanceReport {
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            time_window_hours: self.time_window_hours,
            total_trades_evaluated: records.len(),
            win_rate_percent,
            profit_factor,
            net_profit_sol: total_profit - total_loss,
            average_profit_sol: if profitable_trades > 0 { 
                total_profit / profitable_trades as f64 
            } else { 
                0.0 
            },
            average_loss_sol: if closed_trades.len() > profitable_trades { 
                total_loss / (closed_trades.len() - profitable_trades) as f64 
            } else { 
                0.0 
            },
            max_drawdown_percent: 0.0, // TODO: Implement drawdown calculation
        };

        Ok(report)
    }

    /// Get records from DecisionLedger since timestamp using storage abstraction
    async fn get_records_since(&self, timestamp: u64) -> Result<Vec<TransactionRecord>> {
        self.storage.get_records_since(timestamp).await
    }
}