//! Simplified types for DecisionLedger demonstration
//!
//! This contains only the essential types needed for the DecisionLedger system.

use crate::types::{PremintCandidate, Pubkey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Scored candidate with simplified structure for demo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredCandidate {
    /// Original candidate information
    pub base: PremintCandidate,
    /// The mint address
    pub mint: Pubkey,
    /// Predicted score (0-100)
    pub predicted_score: u8,
    /// Feature scores breakdown
    pub feature_scores: HashMap<String, f64>,
    /// Explanation of the score
    pub reason: String,
    /// Calculation time in microseconds
    pub calculation_time: u128,
    /// Whether anomaly was detected
    pub anomaly_detected: bool,
    /// Timestamp when scored
    pub timestamp: u64,
}

/// Represents the final financial outcome of a transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Outcome {
    /// Profit in SOL
    Profit(f64),
    /// Loss in SOL
    Loss(f64),
    /// No change (e.g., failed transaction, no confirmation)
    Neutral,
    /// Transaction sent but still waiting for result
    PendingConfirmation,
    /// Transaction failed during execution (e.g., timeout, reverted)
    FailedExecution,
    /// Candidate scored but transaction never sent
    NotExecuted,
}

impl Default for Outcome {
    fn default() -> Self {
        Outcome::NotExecuted
    }
}

/// Complete record of a PredictiveOracle decision and its transactional outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    /// Database record ID (set by DB)
    pub id: Option<i64>,
    /// The scored candidate that triggered this decision
    pub scored_candidate: ScoredCandidate,
    
    /// Transaction data (if sent)
    pub transaction_signature: Option<String>,
    /// Purchase price in SOL per token
    pub buy_price_sol: Option<f64>,
    /// Sale price in SOL per token
    pub sell_price_sol: Option<f64>,
    /// Amount of tokens bought
    pub amount_bought_tokens: Option<f64>,
    /// Amount of tokens sold
    pub amount_sold_tokens: Option<f64>,
    /// Total SOL spent on purchase
    pub initial_sol_spent: Option<f64>,
    /// Total SOL received from sale (net)
    pub final_sol_received: Option<f64>,
    
    /// When the decision was made (timestamp from PremintCandidate)
    pub timestamp_decision_made: u64,
    /// When the purchase transaction was sent
    pub timestamp_transaction_sent: Option<u64>,
    /// When the final outcome was evaluated
    pub timestamp_outcome_evaluated: Option<u64>,
    
    /// The final transaction outcome
    pub actual_outcome: Outcome,
    
    /// Market context snapshot at decision time (for later analysis)
    /// This will be populated by the MarketRegimeDetector in the future
    pub market_context_snapshot: HashMap<String, f64>,
}

// --- Communication Channels for DecisionLedger ---

/// Channel for sending new decisions to DecisionLedger
pub type DecisionRecordSender = tokio::sync::mpsc::Sender<TransactionRecord>;
pub type DecisionRecordReceiver = tokio::sync::mpsc::Receiver<TransactionRecord>;

/// Channel for sending outcome updates to DecisionLedger
/// (signature, outcome, buy_price, sell_price, sol_spent, sol_received, timestamp_evaluated)
pub type OutcomeUpdateSender = tokio::sync::mpsc::Sender<(String, Outcome, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<u64>)>;
pub type OutcomeUpdateReceiver = tokio::sync::mpsc::Receiver<(String, Outcome, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<u64>)>;