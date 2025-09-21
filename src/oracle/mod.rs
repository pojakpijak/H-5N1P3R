//! Oracle module - Storage abstraction and Pillar II components
//!
//! This module contains the storage abstraction layer, SqliteLedger implementation (Pillar I)
//! and the PerformanceMonitor/StrategyOptimizer feedback loop (Pillar II).

pub mod types;
pub mod types_old; // Old types that are still in use
pub mod storage; // Storage abstraction layer
pub mod storage_demo; // Demonstration of storage flexibility
pub mod sqlite_ledger; // SQLite implementation of storage
pub mod decision_ledger; // Backward compatibility alias
pub mod transaction_monitor;
pub mod performance_monitor;
pub mod strategy_optimizer;
pub mod market_regime_detector; // Pillar III
pub mod data_sources; // For MarketRegimeDetector

// Re-export main types
pub use types::{
    ScoredCandidate, TransactionRecord, Outcome,
    DecisionRecordSender, OutcomeUpdateSender,
    FeatureWeights, ScoreThresholds,
    PerformanceReport, OptimizedParameters,
    PerformanceReportSender, PerformanceReportReceiver,
    OptimizedParametersSender, OptimizedParametersReceiver,
    // Pillar III types
    MarketRegime, RegimeSpecificParameters, OracleConfig,
};

// Re-export storage components
pub use storage::LedgerStorage;
pub use sqlite_ledger::SqliteLedger;

// Backward compatibility
pub use sqlite_ledger::SqliteLedger as DecisionLedger;

// Re-export key components
pub use transaction_monitor::{TransactionMonitor, MonitoredTransaction};
pub use performance_monitor::PerformanceMonitor;
pub use strategy_optimizer::StrategyOptimizer;
pub use market_regime_detector::MarketRegimeDetector; // Pillar III
pub use data_sources::OracleDataSources; // For MarketRegimeDetector