//! Storage abstraction layer for the Oracle system
//!
//! This module defines the formal contract for data persistence operations,
//! allowing for clean separation between business logic and storage implementation.

use anyhow::{Result, Context};
use async_trait::async_trait;
use sqlx::{sqlite::SqlitePoolOptions, FromRow, Pool, Sqlite};
use tracing::{info, debug};
use std::sync::Arc;

use crate::oracle::types::{TransactionRecord, Outcome, ScoredCandidate};

/// Formal contract for persistent operational memory.
/// Defines operations that must be supported by any database engine.
#[async_trait]
pub trait LedgerStorage: Send + Sync {
    /// Saves a new, complete transaction record to the database.
    /// Returns the unique identifier (ID) of the saved record.
    async fn insert_record(&self, record: &TransactionRecord) -> Result<i64>;

    /// Updates the outcome of an existing transaction record identified by signature.
    async fn update_outcome(
        &self,
        signature: &str,
        outcome: Outcome,
        buy_price_sol: Option<f64>,
        sell_price_sol: Option<f64>,
        initial_sol_spent: Option<f64>,
        final_sol_received: Option<f64>,
        timestamp_evaluated: Option<u64>,
    ) -> Result<()>;

    /// Retrieves historical records since a given timestamp (for analysis).
    /// Returns records ordered by decision timestamp (ascending).
    async fn get_records_since(&self, timestamp: u64) -> Result<Vec<TransactionRecord>>;

    /// Retrieves a specific record by its database ID.
    async fn get_record_by_id(&self, id: i64) -> Result<Option<TransactionRecord>>;

    /// Retrieves records by transaction signature.
    async fn get_record_by_signature(&self, signature: &str) -> Result<Option<TransactionRecord>>;

    /// Gets the total count of records in the storage.
    async fn get_record_count(&self) -> Result<i64>;

    /// Health check for the storage backend.
    async fn health_check(&self) -> Result<bool>;
    
    /// Allows downcasting to concrete storage types for backward compatibility
    fn as_any(&self) -> &dyn std::any::Any;
}

const DB_FILE: &str = "./decisions.db";

/// Helper type for deserializing records from SQLite
#[derive(FromRow)]
struct TransactionRecordRow {
    id: i64,
    mint: String,
    score: i32,
    reason: String,
    feature_scores: String, // JSON
    calculation_time: i64,
    anomaly_detected: bool,
    timestamp_decision_made: i64,
    
    transaction_signature: Option<String>,
    buy_price_sol: Option<f64>,
    sell_price_sol: Option<f64>,
    amount_bought_tokens: Option<f64>,
    amount_sold_tokens: Option<f64>,
    initial_sol_spent: Option<f64>,
    final_sol_received: Option<f64>,

    timestamp_transaction_sent: Option<i64>,
    timestamp_outcome_evaluated: Option<i64>,
    actual_outcome: String, // Enum serialized to string
    market_context_snapshot: String, // JSON
}

/// SQLite implementation of the LedgerStorage trait.
/// This implementation uses the current flat table schema but is designed
/// to be easily replaceable with the normalized schema or PostgreSQL.
pub struct SqliteLedger {
    pool: Pool<Sqlite>,
}

impl SqliteLedger {
    /// Creates a new SqliteLedger with the current flat table schema.
    /// This maintains compatibility with the existing system.
    pub async fn new() -> Result<Arc<Self>> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}?mode=rwc", DB_FILE))
            .await
            .context("Failed to connect to SQLite database")?;

        // Create the transaction_records table if it doesn't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS transaction_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                mint TEXT NOT NULL,
                score INTEGER NOT NULL,
                reason TEXT NOT NULL,
                feature_scores TEXT NOT NULL,
                calculation_time INTEGER NOT NULL,
                anomaly_detected BOOLEAN NOT NULL,
                timestamp_decision_made INTEGER NOT NULL,

                transaction_signature TEXT,
                buy_price_sol REAL,
                sell_price_sol REAL,
                amount_bought_tokens REAL,
                amount_sold_tokens REAL,
                initial_sol_spent REAL,
                final_sol_received REAL,

                timestamp_transaction_sent INTEGER,
                timestamp_outcome_evaluated INTEGER,
                actual_outcome TEXT NOT NULL,
                market_context_snapshot TEXT NOT NULL
            );
            "#
        )
        .execute(&pool)
        .await
        .context("Failed to create transaction_records table")?;

        info!("SqliteLedger initialized and connected to {}", DB_FILE);

        Ok(Arc::new(Self { pool }))
    }

    /// Get a reference to the database pool for backward compatibility
    pub fn get_db_pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }
}

#[async_trait]
impl LedgerStorage for SqliteLedger {
    async fn insert_record(&self, record: &TransactionRecord) -> Result<i64> {
        debug!("Inserting new transaction record for mint: {}", record.scored_candidate.mint);
        
        let feature_scores_json = serde_json::to_string(&record.scored_candidate.feature_scores)?;
        let market_context_json = serde_json::to_string(&record.market_context_snapshot)?;

        let result = sqlx::query(
            r#"
            INSERT INTO transaction_records (
                mint, score, reason, feature_scores, calculation_time, anomaly_detected,
                timestamp_decision_made, transaction_signature, actual_outcome, market_context_snapshot,
                buy_price_sol, sell_price_sol, amount_bought_tokens, amount_sold_tokens,
                initial_sol_spent, final_sol_received, timestamp_transaction_sent, timestamp_outcome_evaluated
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
            "#
        )
        .bind(record.scored_candidate.mint.clone())
        .bind(record.scored_candidate.predicted_score as i64)
        .bind(record.scored_candidate.reason.clone())
        .bind(feature_scores_json)
        .bind(record.scored_candidate.calculation_time as i64)
        .bind(record.scored_candidate.anomaly_detected)
        .bind(record.timestamp_decision_made as i64)
        .bind(record.transaction_signature.clone())
        .bind(serde_json::to_string(&record.actual_outcome)?) // Serialize Outcome enum
        .bind(market_context_json)
        .bind(record.buy_price_sol)
        .bind(record.sell_price_sol)
        .bind(record.amount_bought_tokens)
        .bind(record.amount_sold_tokens)
        .bind(record.initial_sol_spent)
        .bind(record.final_sol_received)
        .bind(record.timestamp_transaction_sent.map(|t| t as i64))
        .bind(record.timestamp_outcome_evaluated.map(|t| t as i64))
        .execute(&self.pool)
        .await
        .context("Failed to insert record into DB")?;

        Ok(result.last_insert_rowid())
    }

    async fn update_outcome(
        &self,
        signature: &str,
        outcome: Outcome,
        buy_price_sol: Option<f64>,
        sell_price_sol: Option<f64>,
        initial_sol_spent: Option<f64>,
        final_sol_received: Option<f64>,
        timestamp_evaluated: Option<u64>,
    ) -> Result<()> {
        debug!("Updating outcome for signature: {}", signature);
        
        sqlx::query(
            r#"
            UPDATE transaction_records
            SET
                actual_outcome = ?,
                buy_price_sol = COALESCE(?, buy_price_sol),
                sell_price_sol = COALESCE(?, sell_price_sol),
                initial_sol_spent = COALESCE(?, initial_sol_spent),
                final_sol_received = COALESCE(?, final_sol_received),
                timestamp_outcome_evaluated = COALESCE(?, timestamp_outcome_evaluated)
            WHERE transaction_signature = ?;
            "#
        )
        .bind(serde_json::to_string(&outcome)?)
        .bind(buy_price_sol)
        .bind(sell_price_sol)
        .bind(initial_sol_spent)
        .bind(final_sol_received)
        .bind(timestamp_evaluated.map(|t| t as i64))
        .bind(signature)
        .execute(&self.pool)
        .await
        .context(format!("Failed to update outcome for signature {}", signature))?;
        
        Ok(())
    }

    async fn get_records_since(&self, timestamp: u64) -> Result<Vec<TransactionRecord>> {
        let rows: Vec<TransactionRecordRow> = sqlx::query_as(
            r#"
            SELECT * FROM transaction_records 
            WHERE timestamp_decision_made >= ? 
            ORDER BY timestamp_decision_made ASC;
            "#
        )
        .bind(timestamp as i64)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch historical records")?;

        let mut records = Vec::new();
        for row in rows {
            records.push(self.row_to_transaction_record(row)?);
        }
        
        Ok(records)
    }

    async fn get_record_by_id(&self, id: i64) -> Result<Option<TransactionRecord>> {
        let row: Option<TransactionRecordRow> = sqlx::query_as(
            "SELECT * FROM transaction_records WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch record by ID")?;

        match row {
            Some(row) => Ok(Some(self.row_to_transaction_record(row)?)),
            None => Ok(None),
        }
    }

    async fn get_record_by_signature(&self, signature: &str) -> Result<Option<TransactionRecord>> {
        let row: Option<TransactionRecordRow> = sqlx::query_as(
            "SELECT * FROM transaction_records WHERE transaction_signature = ?"
        )
        .bind(signature)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch record by signature")?;

        match row {
            Some(row) => Ok(Some(self.row_to_transaction_record(row)?)),
            None => Ok(None),
        }
    }

    async fn get_record_count(&self) -> Result<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM transaction_records")
            .fetch_one(&self.pool)
            .await
            .context("Failed to get record count")?;
        
        Ok(count.0)
    }

    async fn health_check(&self) -> Result<bool> {
        match sqlx::query("SELECT 1").execute(&self.pool).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SqliteLedger {
    /// Helper method to convert a database row to a TransactionRecord
    fn row_to_transaction_record(&self, row: TransactionRecordRow) -> Result<TransactionRecord> {
        // Reconstruct the TransactionRecord from the database row
        let scored_candidate = ScoredCandidate {
            base: crate::types::PremintCandidate {
                mint: row.mint.clone(),
                creator: String::new(), // TODO: Store base candidate as JSON
                program: String::new(),
                slot: 0,
                timestamp: row.timestamp_decision_made as u64,
                instruction_summary: None,
                is_jito_bundle: None,
            },
            mint: row.mint.clone(),
            predicted_score: row.score as u8,
            reason: row.reason,
            feature_scores: serde_json::from_str(&row.feature_scores)?,
            calculation_time: row.calculation_time as u128,
            anomaly_detected: row.anomaly_detected,
            timestamp: row.timestamp_decision_made as u64,
        };

        Ok(TransactionRecord {
            id: Some(row.id),
            scored_candidate,
            transaction_signature: row.transaction_signature,
            buy_price_sol: row.buy_price_sol,
            sell_price_sol: row.sell_price_sol,
            amount_bought_tokens: row.amount_bought_tokens,
            amount_sold_tokens: row.amount_sold_tokens,
            initial_sol_spent: row.initial_sol_spent,
            final_sol_received: row.final_sol_received,
            timestamp_decision_made: row.timestamp_decision_made as u64,
            timestamp_transaction_sent: row.timestamp_transaction_sent.map(|t| t as u64),
            timestamp_outcome_evaluated: row.timestamp_outcome_evaluated.map(|t| t as u64),
            actual_outcome: serde_json::from_str(&row.actual_outcome)?,
            market_context_snapshot: serde_json::from_str(&row.market_context_snapshot)?,
        })
    }
}