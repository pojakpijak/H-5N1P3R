//! Storage abstraction layer for the Oracle system
//!
//! This module defines the contract for persistent storage operations
//! allowing for flexible backend implementations (SQLite, PostgreSQL, etc.)

use anyhow::Result;
use async_trait::async_trait;
use crate::oracle::types::{TransactionRecord, Outcome};

/// Kontrakt dla trwałej pamięci operacyjnej.
/// Definiuje operacje, które muszą być wspierane przez dowolny silnik bazy danych.
#[async_trait]
pub trait LedgerStorage: Send + Sync {
    /// Zapisuje nowy, kompletny rekord transakcji do bazy.
    /// Zwraca unikalny identyfikator (ID) zapisanego rekordu.
    async fn insert_record(&self, record: &TransactionRecord) -> Result<i64>;

    /// Aktualizuje wynik (Outcome) istniejącego rekordu na podstawie sygnatury transakcji.
    async fn update_outcome(
        &self,
        signature: &str,
        outcome: &Outcome,
        buy_price_sol: Option<f64>,
        sell_price_sol: Option<f64>,
        final_sol_received: Option<f64>,
        timestamp_evaluated: u64
    ) -> Result<()>;

    /// Pobiera wszystkie rekordy od określonego znacznika czasu.
    async fn get_records_since(&self, timestamp: u64) -> Result<Vec<TransactionRecord>>;

    /// Pobiera N ostatnich przegranych transakcji do analizy przez optymalizator.
    async fn get_losing_trades(&self, limit: u32) -> Result<Vec<TransactionRecord>>;
}