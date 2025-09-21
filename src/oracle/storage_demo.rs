//! Demonstration of storage abstraction flexibility
//!
//! This module shows how the LedgerStorage trait enables easy testing
//! and future backend implementations.

use anyhow::Result;
use async_trait::async_trait;
use crate::oracle::storage::LedgerStorage;
use crate::oracle::types::{TransactionRecord, Outcome};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// In-memory storage implementation for testing and demonstration
pub struct InMemoryLedger {
    records: Arc<Mutex<HashMap<i64, TransactionRecord>>>,
    next_id: Arc<Mutex<i64>>,
}

impl InMemoryLedger {
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
        }
    }
}

#[async_trait]
impl LedgerStorage for InMemoryLedger {
    async fn insert_record(&self, record: &TransactionRecord) -> Result<i64> {
        let mut next_id = self.next_id.lock().await;
        let id = *next_id;
        *next_id += 1;
        
        let mut records = self.records.lock().await;
        let mut record_with_id = record.clone();
        record_with_id.id = Some(id);
        records.insert(id, record_with_id);
        
        Ok(id)
    }

    async fn update_outcome(
        &self,
        signature: &str,
        outcome: &Outcome,
        buy_price_sol: Option<f64>,
        sell_price_sol: Option<f64>,
        final_sol_received: Option<f64>,
        timestamp_evaluated: u64,
    ) -> Result<()> {
        let mut records = self.records.lock().await;
        
        // Find record by signature and update it
        for record in records.values_mut() {
            if let Some(ref sig) = record.transaction_signature {
                if sig == signature {
                    record.actual_outcome = outcome.clone();
                    if buy_price_sol.is_some() {
                        record.buy_price_sol = buy_price_sol;
                    }
                    if sell_price_sol.is_some() {
                        record.sell_price_sol = sell_price_sol;
                    }
                    if final_sol_received.is_some() {
                        record.final_sol_received = final_sol_received;
                    }
                    record.timestamp_outcome_evaluated = Some(timestamp_evaluated);
                    break;
                }
            }
        }
        
        Ok(())
    }

    async fn get_records_since(&self, timestamp: u64) -> Result<Vec<TransactionRecord>> {
        let records = self.records.lock().await;
        
        let filtered: Vec<TransactionRecord> = records
            .values()
            .filter(|r| r.timestamp_decision_made >= timestamp)
            .cloned()
            .collect();
            
        Ok(filtered)
    }

    async fn get_losing_trades(&self, limit: u32) -> Result<Vec<TransactionRecord>> {
        let records = self.records.lock().await;
        
        let mut losing_trades: Vec<TransactionRecord> = records
            .values()
            .filter(|r| matches!(r.actual_outcome, Outcome::Loss(_) | Outcome::FailedExecution))
            .cloned()
            .collect();
            
        // Sort by timestamp (most recent first)
        losing_trades.sort_by(|a, b| b.timestamp_decision_made.cmp(&a.timestamp_decision_made));
        
        // Take only the requested number
        losing_trades.truncate(limit as usize);
        
        Ok(losing_trades)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::types::ScoredCandidate;
    use crate::types::PremintCandidate;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_storage_abstraction_flexibility() {
        // This test demonstrates how easy it is to swap storage implementations
        let in_memory_storage: Arc<dyn LedgerStorage> = Arc::new(InMemoryLedger::new());
        
        // Create a sample record
        let candidate = ScoredCandidate {
            base: PremintCandidate {
                mint: "TestToken123".to_string(),
                creator: "TestCreator".to_string(),
                program: "pump.fun".to_string(),
                slot: 12345,
                timestamp: 1640995200000,
                instruction_summary: Some("Test transaction".to_string()),
                is_jito_bundle: Some(true),
            },
            mint: "TestToken123".to_string(),
            predicted_score: 85,
            reason: "High potential test token".to_string(),
            feature_scores: {
                let mut scores = HashMap::new();
                scores.insert("liquidity".to_string(), 0.8);
                scores.insert("holder_distribution".to_string(), 0.7);
                scores
            },
            calculation_time: 150_000,
            anomaly_detected: false,
            timestamp: 1640995200000,
        };

        let record = TransactionRecord {
            id: None,
            scored_candidate: candidate,
            transaction_signature: Some("test_signature_123".to_string()),
            buy_price_sol: Some(0.001),
            sell_price_sol: None,
            amount_bought_tokens: Some(1000.0),
            amount_sold_tokens: None,
            initial_sol_spent: Some(1.0),
            final_sol_received: None,
            timestamp_decision_made: 1640995200000,
            timestamp_transaction_sent: Some(1640995210000),
            timestamp_outcome_evaluated: None,
            actual_outcome: Outcome::PendingConfirmation,
            market_context_snapshot: HashMap::new(),
        };

        // Test insertion
        let record_id = in_memory_storage.insert_record(&record).await.unwrap();
        assert_eq!(record_id, 1);

        // Test update
        in_memory_storage.update_outcome(
            "test_signature_123",
            &Outcome::Profit(0.5),
            None,
            Some(0.0015),
            Some(1.5),
            1640995220000,
        ).await.unwrap();

        // Test retrieval
        let records = in_memory_storage.get_records_since(1640995000000).await.unwrap();
        assert_eq!(records.len(), 1);
        assert!(matches!(records[0].actual_outcome, Outcome::Profit(_)));
        
        println!("✅ Storage abstraction test passed! The LedgerStorage trait enables:");
        println!("   - Easy unit testing with in-memory implementations");
        println!("   - Future database migrations (SQLite → PostgreSQL)");
        println!("   - Pluggable storage backends");
        println!("   - Clean separation of concerns");
    }
}