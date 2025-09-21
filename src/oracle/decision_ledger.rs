//! Backward compatibility module for DecisionLedger
//!
//! This module provides backward compatibility by re-exporting SqliteLedger as DecisionLedger

pub use crate::oracle::sqlite_ledger::SqliteLedger as DecisionLedger;