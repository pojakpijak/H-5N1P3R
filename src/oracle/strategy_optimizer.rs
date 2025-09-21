//! StrategyOptimizer module - Pillar II (Orient, Decide, Act)
//!
//! This module analyzes performance reports and optimizes Oracle strategy by adjusting
//! feature weights and thresholds based on historical trading outcomes.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn, error};

use crate::oracle::types::{
    FeatureWeights, OptimizedParameters, OptimizedParametersSender, PerformanceReportReceiver, ScoreThresholds,
    TransactionRecord, Outcome
};
use crate::oracle::storage::LedgerStorage;

/// StrategyOptimizer analyzes performance and dynamically adjusts Oracle parameters
pub struct StrategyOptimizer {
    storage: Arc<dyn LedgerStorage>,
    report_receiver: PerformanceReportReceiver,
    optimized_params_sender: OptimizedParametersSender,
    current_weights: FeatureWeights,
    current_thresholds: ScoreThresholds,
}

impl StrategyOptimizer {
    /// Create a new StrategyOptimizer
    pub fn new(
        storage: Arc<dyn LedgerStorage>,
        report_receiver: PerformanceReportReceiver,
        optimized_params_sender: OptimizedParametersSender,
        initial_weights: FeatureWeights,
        initial_thresholds: ScoreThresholds,
    ) -> Self {
        Self {
            storage,
            report_receiver,
            optimized_params_sender,
            current_weights: initial_weights,
            current_thresholds: initial_thresholds,
        }
    }

    /// Main execution loop - awaits performance reports and optimizes strategy
    pub async fn run(mut self) {
        info!("StrategyOptimizer is running, awaiting performance reports...");
        
        while let Some(report) = self.report_receiver.recv().await {
            info!("Received new performance report. Analyzing for potential optimizations...");
            
            // Basic optimization logic: if Profit Factor is weak, try to optimize
            // Use lower threshold for testing (3 trades minimum instead of 10)
            if report.profit_factor < 1.2 && report.total_trades_evaluated > 3 {
                warn!("Profit Factor is below threshold ({:.2}). Attempting to optimize strategy.", 
                      report.profit_factor);
                
                match self.find_optimizations().await {
                    Ok(Some(new_params)) => {
                        info!("Found new optimized parameters: {}", new_params.reason);
                        if let Err(e) = self.optimized_params_sender.send(new_params).await {
                            error!("Failed to send optimized parameters: {}", e);
                        }
                    }
                    Ok(None) => {
                        info!("No clear optimization path found in this cycle.");
                    }
                    Err(e) => {
                        error!("Error during strategy optimization: {}", e);
                    }
                }
            } else {
                info!("Current strategy performance is acceptable (PF: {:.2}). No optimization needed.", 
                      report.profit_factor);
            }
        }
    }

    /// Find potential optimizations based on losing trades analysis
    async fn find_optimizations(&mut self) -> Result<Option<OptimizedParameters>> {
        // Analyze losing trades to identify correlations with low feature scores
        let losing_trades = self.query_losing_trades().await?;
        
        if losing_trades.is_empty() {
            return Ok(None);
        }

        // Simple heuristic: find feature with lowest average score in losing trades
        let mut avg_scores_on_losses: HashMap<String, (f64, usize)> = HashMap::new();
        
        for trade in &losing_trades {
            for (feature, score) in &trade.scored_candidate.feature_scores {
                let entry = avg_scores_on_losses.entry(feature.clone()).or_insert((0.0, 0));
                entry.0 += score;
                entry.1 += 1;
            }
        }

        let mut worst_feature = ("".to_string(), 1.0); // (name, average_score)
        for (feature, (sum, count)) in avg_scores_on_losses {
            let avg = sum / count as f64;
            if avg < worst_feature.1 {
                worst_feature = (feature, avg);
            }
        }

        if worst_feature.0.is_empty() {
            return Ok(None);
        }

        // Generate hypothesis and new parameters
        let mut new_weights = self.current_weights.clone();
        let reason = format!(
            "Losses are correlated with low scores in '{}' (avg: {:.2}). Increasing its weight.",
            worst_feature.0, worst_feature.1
        );
        
        // Increase weight of worst-performing feature by 10% (simple logic, can be expanded)
        match worst_feature.0.as_str() {
            "liquidity" => new_weights.liquidity *= 1.1,
            "holder_distribution" => new_weights.holder_distribution *= 1.1,
            "volume_growth" => new_weights.volume_growth *= 1.1,
            "holder_growth" => new_weights.holder_growth *= 1.1,
            "price_change" => new_weights.price_change *= 1.1,
            "jito_bundle_presence" => new_weights.jito_bundle_presence *= 1.1,
            "creator_sell_speed" => new_weights.creator_sell_speed *= 1.1,
            "metadata_quality" => new_weights.metadata_quality *= 1.1,
            "social_activity" => new_weights.social_activity *= 1.1,
            _ => {
                warn!("Unknown feature '{}' found in analysis", worst_feature.0);
                return Ok(None);
            }
        }
        
        // Update current weights for next optimization cycle
        self.current_weights = new_weights.clone();

        Ok(Some(OptimizedParameters {
            new_weights,
            new_thresholds: self.current_thresholds.clone(), // Not changing thresholds for now
            reason,
        }))
    }

    /// Query database for losing trades using storage abstraction
    async fn query_losing_trades(&self) -> Result<Vec<TransactionRecord>> {
        // Get recent records (last 24 hours) and filter for losses
        let since_timestamp = (chrono::Utc::now() - chrono::Duration::hours(24))
            .timestamp_millis() as u64;
        
        let all_records = self.storage.get_records_since(since_timestamp).await?;
        
        // Filter for losing trades only
        let losing_trades: Vec<TransactionRecord> = all_records
            .into_iter()
            .filter(|record| matches!(record.actual_outcome, Outcome::Loss(_)))
            .take(50) // Limit to 50 most recent losses
            .collect();
        
        Ok(losing_trades)
    }
}