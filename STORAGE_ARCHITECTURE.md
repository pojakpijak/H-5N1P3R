# Storage Architecture Migration Guide

## Overview

The refactored storage architecture provides a clean separation between business logic and storage implementation, enabling seamless database migrations.

## Current Architecture

```rust
// Storage abstraction (database-agnostic)
trait LedgerStorage {
    async fn insert_record(&self, record: &TransactionRecord) -> Result<i64>;
    async fn update_outcome(&self, signature: &str, outcome: &Outcome, ...) -> Result<()>;
    async fn get_records_since(&self, timestamp: u64) -> Result<Vec<TransactionRecord>>;
    async fn get_losing_trades(&self, limit: u32) -> Result<Vec<TransactionRecord>>;
}

// Current implementation (SQLite)
struct SqliteLedger { ... }

// Usage in main application
let storage: Arc<dyn LedgerStorage> = Arc::new(SqliteLedger::new_storage_only().await?);
```

## Normalized Database Schema

### Current SQLite Schema

```sql
-- Main trades table
CREATE TABLE trades (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mint TEXT NOT NULL,
    decision_timestamp INTEGER NOT NULL,
    signature TEXT UNIQUE,
    final_outcome TEXT,
    pnl_sol REAL,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE
);

-- Feature scores (normalized)
CREATE TABLE decision_features (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trade_id INTEGER NOT NULL,
    feature_name TEXT NOT NULL,
    feature_value REAL NOT NULL,
    FOREIGN KEY (trade_id) REFERENCES trades (id)
);

-- Market context (normalized)
CREATE TABLE market_context (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trade_id INTEGER NOT NULL,
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    FOREIGN KEY (trade_id) REFERENCES trades (id)
);
```

## Future PostgreSQL Migration

To migrate to PostgreSQL, simply:

1. **Create PostgreSQL implementation:**

```rust
struct PostgresLedger {
    pool: Pool<Postgres>,
}

#[async_trait]
impl LedgerStorage for PostgresLedger {
    // Same interface, PostgreSQL-specific implementation
    async fn insert_record(&self, record: &TransactionRecord) -> Result<i64> {
        // PostgreSQL-optimized queries with RETURNING clause
        // JSON columns for complex data types
        // Full-text search capabilities
        // Advanced indexing strategies
    }
    // ... other methods
}
```

2. **Update main.rs:**

```rust
// Single line change!
let storage: Arc<dyn LedgerStorage> = Arc::new(PostgresLedger::new().await?);
```

## Testing Benefits

The abstraction enables clean unit testing:

```rust
// In-memory storage for fast tests
let test_storage: Arc<dyn LedgerStorage> = Arc::new(InMemoryLedger::new());

// Test business logic without database dependencies
async fn test_some_feature() {
    let storage = Arc::new(InMemoryLedger::new());
    
    // Test insertion
    let record_id = storage.insert_record(&test_record).await.unwrap();
    
    // Test retrieval
    let records = storage.get_records_since(timestamp).await.unwrap();
    
    // Assertions...
}
```

## Performance Benefits of Normalized Schema

### Before (Flat JSON):
```sql
-- Every analytical query required JSON parsing
SELECT mint, JSON_EXTRACT(feature_scores, '$.liquidity') as liquidity
FROM transaction_records 
WHERE JSON_EXTRACT(feature_scores, '$.liquidity') > 0.8;
```

### After (Normalized):
```sql
-- Direct indexed queries, dramatically faster
SELECT t.mint, df.feature_value as liquidity
FROM trades t
JOIN decision_features df ON t.id = df.trade_id
WHERE df.feature_name = 'liquidity' AND df.feature_value > 0.8;
```

## Strategic Benefits

1. **Technology Independence**: Business logic is decoupled from storage technology
2. **Easy Testing**: Mock implementations for unit tests
3. **Performance Optimization**: Normalized schema enables efficient analytical queries
4. **Future Scalability**: Can migrate to any database without code changes
5. **Backwards Compatibility**: Legacy table maintained during transition

## Migration Timeline

- **Phase 1** ✅: Storage abstraction implemented (current)
- **Phase 2**: PostgreSQL implementation (when needed)
- **Phase 3**: Data migration utilities
- **Phase 4**: Legacy table removal (after full migration)

This architecture provides a solid foundation for future growth while maintaining current operational simplicity.