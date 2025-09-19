//! Main entry point for the H-5N1P3R DecisionLedger demo
//!
//! This demonstrates the first pillar: Operational Memory - DecisionLedger

use anyhow::Result;
use h_5n1p3r::oracle::{
    DecisionLedger, TransactionMonitor, TransactionRecord, Outcome, MonitoredTransaction,
    ScoredCandidate, DecisionRecordSender, OutcomeUpdateSender,
};
use h_5n1p3r::types::PremintCandidate;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting H-5N1P3R DecisionLedger Demo");

    // Create communication channels
    let (decision_record_sender, decision_record_receiver) = mpsc::channel::<TransactionRecord>(100);
    let (outcome_update_sender, outcome_update_receiver) = mpsc::channel(100);
    let (monitor_tx_sender, monitor_tx_receiver) = mpsc::channel::<MonitoredTransaction>(100);

    // Initialize DecisionLedger
    let decision_ledger = DecisionLedger::new(
        decision_record_receiver,
        outcome_update_receiver,
    ).await?;

    // Initialize TransactionMonitor
    let transaction_monitor = TransactionMonitor::new(
        outcome_update_sender.clone(),
        1000, // Check every 1 second
    );

    // Start DecisionLedger and TransactionMonitor in background tasks
    let ledger_handle = tokio::spawn(async move {
        decision_ledger.run().await;
    });

    let monitor_handle = tokio::spawn(async move {
        transaction_monitor.run(monitor_tx_receiver).await;
    });

    // Demo: Create and record some decisions
    demo_decision_recording(decision_record_sender, monitor_tx_sender).await?;

    // Let the system run for a bit to process outcomes
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    info!("Demo completed. The DecisionLedger has recorded decisions and outcomes.");
    info!("Database file 'decisions.db' contains the persistent memory.");

    // The handles would normally run indefinitely in a real system
    ledger_handle.abort();
    monitor_handle.abort();

    Ok(())
}

/// Demonstrate recording decisions to the DecisionLedger
async fn demo_decision_recording(
    decision_sender: DecisionRecordSender,
    monitor_sender: mpsc::Sender<MonitoredTransaction>,
) -> Result<()> {
    info!("Creating demo PredictiveOracle decisions...");

    // Create some example decisions
    for i in 1..=3 {
        let candidate = PremintCandidate {
            mint: format!("DemoToken{}Address", i),
            creator: format!("DemoCreator{}Address", i),
            program: "pump.fun".to_string(),
            slot: 12345 + i,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            instruction_summary: Some("Token creation".to_string()),
            is_jito_bundle: Some(true),
        };

        let mut feature_scores = HashMap::new();
        feature_scores.insert("liquidity".to_string(), 0.7 + (i as f64 * 0.1));
        feature_scores.insert("holder_distribution".to_string(), 0.6);
        feature_scores.insert("volume_growth".to_string(), 0.8);

        let scored_candidate = ScoredCandidate {
            base: candidate.clone(),
            mint: candidate.mint.clone(),
            predicted_score: 70 + (i * 5) as u8,
            reason: format!("High potential token #{}", i),
            feature_scores: feature_scores.clone(),
            calculation_time: 150_000, // 150ms
            anomaly_detected: false,
            timestamp: candidate.timestamp,
        };

        // Record the initial decision
        let initial_record = TransactionRecord {
            id: None,
            scored_candidate: scored_candidate.clone(),
            transaction_signature: None,
            buy_price_sol: None,
            sell_price_sol: None,
            amount_bought_tokens: None,
            amount_sold_tokens: None,
            initial_sol_spent: None,
            final_sol_received: None,
            timestamp_decision_made: candidate.timestamp,
            timestamp_transaction_sent: None,
            timestamp_outcome_evaluated: None,
            actual_outcome: Outcome::NotExecuted,
            market_context_snapshot: HashMap::new(),
        };

        decision_sender.send(initial_record).await?;
        info!("Recorded decision for token: {}", candidate.mint);

        // For high-scoring candidates, simulate sending a transaction
        if scored_candidate.predicted_score >= 75 {
            let tx_signature = format!("SimulatedTx{}_{}", i, candidate.timestamp);
            let amount_bought = 1000.0;
            let sol_spent = 1.0;

            // Send transaction to monitor
            let monitored_tx = MonitoredTransaction {
                signature: tx_signature.clone(),
                mint: candidate.mint.clone(),
                amount_bought_tokens: amount_bought,
                initial_sol_spent: sol_spent,
                monitor_until: chrono::Utc::now().timestamp_millis() as u64 + 10_000, // 10 seconds
            };

            monitor_sender.send(monitored_tx).await?;

            // Record that transaction was sent
            let sent_record = TransactionRecord {
                id: None,
                scored_candidate: scored_candidate.clone(),
                transaction_signature: Some(tx_signature),
                buy_price_sol: Some(sol_spent / amount_bought),
                sell_price_sol: None,
                amount_bought_tokens: Some(amount_bought),
                amount_sold_tokens: None,
                initial_sol_spent: Some(sol_spent),
                final_sol_received: None,
                timestamp_decision_made: candidate.timestamp,
                timestamp_transaction_sent: Some(chrono::Utc::now().timestamp_millis() as u64),
                timestamp_outcome_evaluated: None,
                actual_outcome: Outcome::PendingConfirmation,
                market_context_snapshot: HashMap::new(),
            };

            decision_sender.send(sent_record).await?;
            info!("Sent transaction for token: {} (signature: {})", candidate.mint, tx_signature);
        }

        // Small delay between decisions
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    info!("All demo decisions recorded!");
    Ok(())
}