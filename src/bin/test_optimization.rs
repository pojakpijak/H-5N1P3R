//! Test to demonstrate Pillar II optimization when performance is poor

use anyhow::Result;
use h_5n1p3r::oracle::{
    DecisionLedger, TransactionRecord, Outcome, ScoredCandidate,
    PerformanceMonitor, StrategyOptimizer, FeatureWeights, ScoreThresholds,
    RuntimeOracleConfig, SharedRuntimeConfig,
};
use h_5n1p3r::types::PremintCandidate;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Testing Pillar II optimization with poor performance data");

    // Create channels
    let (decision_record_sender, decision_record_receiver) = mpsc::channel(100);
    let (outcome_update_sender, outcome_update_receiver) = mpsc::channel(100);
    let (perf_report_sender, perf_report_receiver) = mpsc::channel(16);
    let (opt_params_sender, mut opt_params_receiver) = mpsc::channel(16);

    // Initialize DecisionLedger
    let decision_ledger = DecisionLedger::new(
        decision_record_receiver,
        outcome_update_receiver,
    ).await?;

    // Get storage for the Pillar II components  
    let storage = decision_ledger.get_storage();

    // Start DecisionLedger
    let _ledger_handle = tokio::spawn(async move {
        decision_ledger.run().await;
    });

    // Add several losing trades with specific feature patterns
    create_poor_performance_data(decision_record_sender).await?;

    // Give data time to be recorded
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Manually trigger performance analysis and optimization
    let performance_monitor = PerformanceMonitor::new(
        storage.clone(),
        perf_report_sender.clone(),
        999, // Not used in manual test
        24,
    );

    // Run analysis manually and send poor performance report
    tokio::spawn(async move {
        // Wait a bit for data to be recorded
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        
        if let Ok(report) = performance_monitor.analyze_performance().await {
            info!("Performance analysis results: PF={:.2}, WR={:.1}%", 
                  report.profit_factor, report.win_rate_percent);
            
            // If performance is not poor enough, create a poor report manually
            let poor_report = if report.profit_factor >= 1.2 {
                use h_5n1p3r::oracle::PerformanceReport;
                PerformanceReport {
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    time_window_hours: 24.0,
                    total_trades_evaluated: 15,
                    win_rate_percent: 20.0, // Poor win rate
                    profit_factor: 0.6,     // Poor profit factor
                    average_profit_sol: 0.1,
                    average_loss_sol: 0.4,
                    net_profit_sol: -1.5,   // Net loss
                    max_drawdown_percent: 15.0,
                }
            } else {
                report
            };
            
            let _ = perf_report_sender.send(poor_report).await;
        }
    });

    // Initialize StrategyOptimizer
    let strategy_optimizer = StrategyOptimizer::new(
        storage,
        perf_report_receiver,
        opt_params_sender,
        FeatureWeights::default(),
        ScoreThresholds::default(),
    );

    // Create shared runtime configuration for hot-swap demonstration
    let runtime_config: SharedRuntimeConfig = Arc::new(RwLock::new(
        RuntimeOracleConfig::new(FeatureWeights::default(), ScoreThresholds::default())
    ));

    let _opt_handle = tokio::spawn(async move {
        strategy_optimizer.run().await;
    });

    // Hot-swap demonstration task
    let runtime_config_clone = runtime_config.clone();
    let _hotswap_handle = tokio::spawn(async move {
        while let Some(optimized_params) = opt_params_receiver.recv().await {
            let old_config = {
                let config = runtime_config_clone.read().await;
                (config.current_weights.liquidity, config.update_count)
            };
            
            // Apply hot-swap update
            {
                let mut config = runtime_config_clone.write().await;
                config.apply_optimization(optimized_params.clone());
            }
            
            let new_config = {
                let config = runtime_config_clone.read().await;
                (config.current_weights.liquidity, config.update_count)
            };
            
            info!("🎯 Optimization successful!");
            info!("Reason: {}", optimized_params.reason);
            info!("Original liquidity weight: {:.3}", old_config.0);
            info!("New liquidity weight: {:.3}", new_config.0);
            info!("Improvement: {:.1}%", (new_config.0 - old_config.0) / old_config.0 * 100.0);
            info!("🔄 Hot-swap completed! Update #{}", new_config.1);
            break;
        }
    });

    // Wait a moment for optimization to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    Ok(())
}

async fn create_poor_performance_data(sender: mpsc::Sender<TransactionRecord>) -> Result<()> {
    info!("Creating poor performance data to trigger optimization...");

    // Create several losing trades with consistently low liquidity scores
    for i in 1..=8 {
        let mut feature_scores = HashMap::new();
        // Low liquidity scores (the pattern we want to optimize)
        feature_scores.insert("liquidity".to_string(), 0.1 + (i as f64 * 0.05));
        feature_scores.insert("holder_distribution".to_string(), 0.7);
        feature_scores.insert("volume_growth".to_string(), 0.8);

        let candidate = PremintCandidate {
            mint: format!("LossToken{}Address", i),
            creator: format!("Creator{}Address", i),
            program: "pump.fun".to_string(),
            slot: 50000 + i,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            instruction_summary: Some("Losing token".to_string()),
            is_jito_bundle: Some(false),
        };

        let scored_candidate = ScoredCandidate {
            base: candidate.clone(),
            mint: candidate.mint.clone(),
            predicted_score: 65, // Mediocre score
            reason: format!("Token with low liquidity #{}", i),
            feature_scores: feature_scores.clone(),
            calculation_time: 120_000,
            anomaly_detected: false,
            timestamp: candidate.timestamp,
        };

        // Create a losing transaction record
        let loss_record = TransactionRecord {
            id: None,
            scored_candidate,
            transaction_signature: Some(format!("LossTx{}_{}", i, candidate.timestamp)),
            buy_price_sol: Some(0.001),
            sell_price_sol: Some(0.0005), // Lost half the value
            amount_bought_tokens: Some(1000.0),
            amount_sold_tokens: Some(1000.0),
            initial_sol_spent: Some(1.0),
            final_sol_received: Some(0.5), // 50% loss
            timestamp_decision_made: candidate.timestamp,
            timestamp_transaction_sent: Some(candidate.timestamp + 1000),
            timestamp_outcome_evaluated: Some(candidate.timestamp + 10000),
            actual_outcome: Outcome::Loss(-0.5), // 0.5 SOL loss
            market_context_snapshot: HashMap::new(),
        };

        sender.send(loss_record).await?;
    }
    
    // Add a few winning trades to make it more realistic
    for i in 1..=2 {
        let mut feature_scores = HashMap::new();
        // High scores for winning trades
        feature_scores.insert("liquidity".to_string(), 0.9);
        feature_scores.insert("holder_distribution".to_string(), 0.8);
        feature_scores.insert("volume_growth".to_string(), 0.9);

        let candidate = PremintCandidate {
            mint: format!("WinToken{}Address", i),
            creator: format!("WinCreator{}Address", i),
            program: "pump.fun".to_string(),
            slot: 60000 + i,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            instruction_summary: Some("Winning token".to_string()),
            is_jito_bundle: Some(false),
        };

        let scored_candidate = ScoredCandidate {
            base: candidate.clone(),
            mint: candidate.mint.clone(),
            predicted_score: 85, // High score
            reason: format!("Token with high liquidity #{}", i),
            feature_scores: feature_scores.clone(),
            calculation_time: 120_000,
            anomaly_detected: false,
            timestamp: candidate.timestamp,
        };

        // Create a winning transaction record
        let win_record = TransactionRecord {
            id: None,
            scored_candidate,
            transaction_signature: Some(format!("WinTx{}_{}", i, candidate.timestamp)),
            buy_price_sol: Some(0.001),
            sell_price_sol: Some(0.002), // Doubled the value
            amount_bought_tokens: Some(1000.0),
            amount_sold_tokens: Some(1000.0),
            initial_sol_spent: Some(1.0),
            final_sol_received: Some(2.0), // 100% profit
            timestamp_decision_made: candidate.timestamp,
            timestamp_transaction_sent: Some(candidate.timestamp + 1000),
            timestamp_outcome_evaluated: Some(candidate.timestamp + 10000),
            actual_outcome: Outcome::Profit(1.0), // 1.0 SOL profit
            market_context_snapshot: HashMap::new(),
        };

        sender.send(win_record).await?;
    }

    info!("Created 8 losing trades and 2 winning trades with poor overall performance");
    Ok(())
}