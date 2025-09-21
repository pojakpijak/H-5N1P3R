//! SqliteLedger module - SQLite implementation of the Oracle storage
//!
//! This module implements the LedgerStorage trait using SQLite as the backend.
//! It features a normalized database schema for better analytical query performance.

use anyhow::{Result, Context};
use async_trait::async_trait;
use sqlx::{sqlite::SqlitePoolOptions, FromRow, Pool, Sqlite};
use tracing::{info, error, debug};
use crate::oracle::storage::LedgerStorage;
use crate::oracle::types::{TransactionRecord, Outcome, DecisionRecordReceiver, OutcomeUpdateReceiver, ScoredCandidate};

const DB_FILE: &str = "./decisions.db";

/// Helper types for deserializing records from SQLite normalized schema

#[derive(FromRow)]
struct TradeRow {
    id: i64,
    mint: String,
    decision_timestamp: i64,
    signature: Option<String>,
    final_outcome: Option<String>,
    pnl_sol: Option<f64>,
    is_verified: bool,
}

#[derive(FromRow)]
struct DecisionFeatureRow {
    trade_id: i64,
    feature_name: String,
    feature_value: f64,
}

#[derive(FromRow)]
struct MarketContextRow {
    trade_id: i64,
    metric_name: String,
    metric_value: f64,
}

/// Legacy helper type for compatibility with get_records_since method
#[derive(FromRow)]
struct TransactionRecordRow {
    id: i64,
    mint: String,
    score: i64,
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

/// SqliteLedger provides persistent storage for Oracle decisions and outcomes using SQLite
pub struct SqliteLedger {
    pool: Pool<Sqlite>,
    // Keep these for backward compatibility with the existing run() method
    record_receiver: Option<DecisionRecordReceiver>,
    outcome_update_receiver: Option<OutcomeUpdateReceiver>,
}

impl SqliteLedger {
    /// Create a new SqliteLedger with SQLite backend
    pub async fn new(
        record_receiver: DecisionRecordReceiver,
        outcome_update_receiver: OutcomeUpdateReceiver,
    ) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}?mode=rwc", DB_FILE))
            .await
            .context("Failed to connect to SQLite database")?;

        // Create the normalized database schema
        Self::create_normalized_schema(&pool).await?;

        info!("SqliteLedger initialized and connected to {}", DB_FILE);

        Ok(Self { 
            pool, 
            record_receiver: Some(record_receiver), 
            outcome_update_receiver: Some(outcome_update_receiver)
        })
    }

    /// Create a new SqliteLedger without channel receivers (for direct use as storage)
    pub async fn new_storage_only() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}?mode=rwc", DB_FILE))
            .await
            .context("Failed to connect to SQLite database")?;

        // Create the normalized database schema
        Self::create_normalized_schema(&pool).await?;

        info!("SqliteLedger (storage-only) initialized and connected to {}", DB_FILE);

        Ok(Self { 
            pool, 
            record_receiver: None, 
            outcome_update_receiver: None
        })
    }

    /// Create the normalized database schema
    async fn create_normalized_schema(pool: &Pool<Sqlite>) -> Result<()> {
        // Main table transakcji/decyzji
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS trades (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                mint TEXT NOT NULL,
                decision_timestamp INTEGER NOT NULL,
                signature TEXT UNIQUE,
                final_outcome TEXT,
                pnl_sol REAL,
                is_verified BOOLEAN NOT NULL DEFAULT FALSE
            );
            "#
        )
        .execute(pool)
        .await
        .context("Failed to create trades table")?;

        // Tabela przechowująca wartości cech w momencie podjęcia decyzji
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS decision_features (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trade_id INTEGER NOT NULL,
                feature_name TEXT NOT NULL,
                feature_value REAL NOT NULL,
                FOREIGN KEY (trade_id) REFERENCES trades (id)
            );
            "#
        )
        .execute(pool)
        .await
        .context("Failed to create decision_features table")?;

        // Tabela przechowująca kontekst rynkowy w momencie podjęcia decyzji
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS market_context (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trade_id INTEGER NOT NULL,
                metric_name TEXT NOT NULL,
                metric_value REAL NOT NULL,
                FOREIGN KEY (trade_id) REFERENCES trades (id)
            );
            "#
        )
        .execute(pool)
        .await
        .context("Failed to create market_context table")?;

        // Create legacy table for backward compatibility with existing queries
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
        .execute(pool)
        .await
        .context("Failed to create transaction_records table for backward compatibility")?;

        Ok(())
    }

    /// Get a clone of the database pool for use by other components
    pub fn get_db_pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    /// Main execution loop - processes incoming decisions and outcome updates
    /// Only works if the channels were provided during construction
    pub async fn run(mut self) {
        let mut record_receiver = match self.record_receiver.take() {
            Some(receiver) => receiver,
            None => {
                error!("SqliteLedger::run() called but no record_receiver available");
                return;
            }
        };

        let mut outcome_update_receiver = match self.outcome_update_receiver.take() {
            Some(receiver) => receiver,
            None => {
                error!("SqliteLedger::run() called but no outcome_update_receiver available");
                return;
            }
        };

        info!("SqliteLedger is running...");
        loop {
            tokio::select! {
                Some(record) = record_receiver.recv() => {
                    if let Err(e) = self.insert_record(&record).await {
                        error!("Failed to insert transaction record: {:?}", e);
                    }
                },
                Some((signature, outcome, buy_price, sell_price, _sol_spent, sol_received, evaluated_at)) = outcome_update_receiver.recv() => {
                    if let Err(e) = self.update_outcome(&signature, &outcome, buy_price, sell_price, sol_received, evaluated_at.unwrap_or(chrono::Utc::now().timestamp_millis() as u64)).await {
                        error!("Failed to update outcome for signature {}: {:?}", signature, e);
                    }
                },
                else => {
                    info!("SqliteLedger channels closed. Shutting down.");
                    break;
                }
            }
        }
    }
}

#[async_trait]
impl LedgerStorage for SqliteLedger {
    /// Zapisuje nowy, kompletny rekord transakcji do bazy.
    /// Zwraca unikalny identyfikator (ID) zapisanego rekordu.
    async fn insert_record(&self, record: &TransactionRecord) -> Result<i64> {
        debug!("Inserting new transaction record for mint: {}", record.scored_candidate.mint);
        
        let mut tx = self.pool.begin().await?;

        // 1. Wstaw do tabeli `trades` i pobierz ID
        let trade_id = sqlx::query(
            r#"
            INSERT INTO trades (mint, decision_timestamp, signature, final_outcome, pnl_sol, is_verified)
            VALUES (?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&record.scored_candidate.mint)
        .bind(record.timestamp_decision_made as i64)
        .bind(&record.transaction_signature)
        .bind(serde_json::to_string(&record.actual_outcome)?)
        .bind(record.final_sol_received.map(|received| {
            record.initial_sol_spent.map(|spent| received - spent).unwrap_or(0.0)
        }))
        .bind(false)
        .execute(&mut *tx).await?.last_insert_rowid();

        // 2. W pętli, wstaw wszystkie `feature_scores` do `decision_features`
        for (name, value) in &record.scored_candidate.feature_scores {
            sqlx::query(
                "INSERT INTO decision_features (trade_id, feature_name, feature_value) VALUES (?, ?, ?)"
            )
            .bind(trade_id)
            .bind(name)
            .bind(value)
            .execute(&mut *tx).await?;
        }

        // 3. W pętli, wstaw `market_context_snapshot`
        for (metric_name, metric_value) in &record.market_context_snapshot {
            sqlx::query(
                "INSERT INTO market_context (trade_id, metric_name, metric_value) VALUES (?, ?, ?)"
            )
            .bind(trade_id)
            .bind(metric_name)
            .bind(metric_value)
            .execute(&mut *tx).await?;
        }

        // Also insert into legacy table for backward compatibility
        let feature_scores_json = serde_json::to_string(&record.scored_candidate.feature_scores)?;
        let market_context_json = serde_json::to_string(&record.market_context_snapshot)?;

        sqlx::query(
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
        .bind(serde_json::to_string(&record.actual_outcome)?)
        .bind(market_context_json)
        .bind(record.buy_price_sol)
        .bind(record.sell_price_sol)
        .bind(record.amount_bought_tokens)
        .bind(record.amount_sold_tokens)
        .bind(record.initial_sol_spent)
        .bind(record.final_sol_received)
        .bind(record.timestamp_transaction_sent.map(|t| t as i64))
        .bind(record.timestamp_outcome_evaluated.map(|t| t as i64))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(trade_id)
    }

    /// Aktualizuje wynik (Outcome) istniejącego rekordu na podstawie sygnatury transakcji.
    async fn update_outcome(
        &self,
        signature: &str,
        outcome: &Outcome,
        buy_price_sol: Option<f64>,
        sell_price_sol: Option<f64>,
        final_sol_received: Option<f64>,
        timestamp_evaluated: u64
    ) -> Result<()> {
        debug!("Updating outcome for signature: {}", signature);
        
        let mut tx = self.pool.begin().await?;

        // Update normalized tables
        let pnl_sol = if let (Some(received), Some(spent)) = (final_sol_received, sell_price_sol) {
            Some(received - spent)
        } else {
            None
        };

        sqlx::query(
            r#"
            UPDATE trades
            SET final_outcome = ?, pnl_sol = ?, is_verified = TRUE
            WHERE signature = ?
            "#
        )
        .bind(serde_json::to_string(outcome)?)
        .bind(pnl_sol)
        .bind(signature)
        .execute(&mut *tx)
        .await?;

        // Update legacy table for backward compatibility
        sqlx::query(
            r#"
            UPDATE transaction_records
            SET
                actual_outcome = ?,
                buy_price_sol = COALESCE(?, buy_price_sol),
                sell_price_sol = COALESCE(?, sell_price_sol),
                final_sol_received = COALESCE(?, final_sol_received),
                timestamp_outcome_evaluated = ?
            WHERE transaction_signature = ?;
            "#
        )
        .bind(serde_json::to_string(outcome)?)
        .bind(buy_price_sol)
        .bind(sell_price_sol)
        .bind(final_sol_received)
        .bind(timestamp_evaluated as i64)
        .bind(signature)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Pobiera wszystkie rekordy od określonego znacznika czasu.
    async fn get_records_since(&self, timestamp: u64) -> Result<Vec<TransactionRecord>> {
        // Use legacy table for backward compatibility
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
            let scored_candidate = ScoredCandidate {
                base: crate::types::PremintCandidate {
                    mint: row.mint.clone(),
                    creator: String::new(),
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

            records.push(TransactionRecord {
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
            });
        }
        
        Ok(records)
    }

    /// Pobiera N ostatnich przegranych transakcji do analizy przez optymalizator.
    async fn get_losing_trades(&self, limit: u32) -> Result<Vec<TransactionRecord>> {
        // Use legacy table for backward compatibility
        let rows: Vec<TransactionRecordRow> = sqlx::query_as(
            r#"
            SELECT * FROM transaction_records 
            WHERE actual_outcome LIKE '%Loss%' OR actual_outcome LIKE '%FailedExecution%'
            ORDER BY timestamp_decision_made DESC
            LIMIT ?;
            "#
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch losing trades")?;

        let mut records = Vec::new();
        for row in rows {
            let scored_candidate = ScoredCandidate {
                base: crate::types::PremintCandidate {
                    mint: row.mint.clone(),
                    creator: String::new(),
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

            records.push(TransactionRecord {
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
            });
        }
        
        Ok(records)
    }
}