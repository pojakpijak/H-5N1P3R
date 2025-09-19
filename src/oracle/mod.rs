//! Oracle module - simplified for DecisionLedger demonstration
//!
//! This module contains only the essential components needed to demonstrate
//! the DecisionLedger operational memory system.

pub mod types;
pub mod decision_ledger;
pub mod transaction_monitor;

// Re-export main types
pub use types::{
    ScoredCandidate, TransactionRecord, Outcome,
    DecisionRecordSender, OutcomeUpdateSender,
};

// Re-export key components
pub use decision_ledger::DecisionLedger;
pub use transaction_monitor::{TransactionMonitor, MonitoredTransaction};