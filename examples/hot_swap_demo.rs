//! Hot-Swap Configuration Demo
//!
//! This example demonstrates how to use the runtime configuration system
//! for hot-swapping Oracle parameters without system restart.

use anyhow::Result;
use h_5n1p3r::oracle::{
    FeatureWeights, ScoreThresholds, RuntimeOracleConfig, SharedRuntimeConfig, OptimizedParameters
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🔧 Hot-Swap Configuration Demo");
    
    // Create initial configuration
    let initial_weights = FeatureWeights::default();
    let initial_thresholds = ScoreThresholds::default();
    
    let runtime_config: SharedRuntimeConfig = Arc::new(RwLock::new(
        RuntimeOracleConfig::new(initial_weights.clone(), initial_thresholds.clone())
    ));
    
    info!("📊 Initial Configuration:");
    info!("  Liquidity weight: {:.3}", initial_weights.liquidity);
    info!("  Holder distribution weight: {:.3}", initial_weights.holder_distribution);
    info!("  Volume growth weight: {:.3}", initial_weights.volume_growth);
    
    // Simulate a configuration reader (like an Oracle component)
    let config_reader = runtime_config.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));
        loop {
            interval.tick().await;
            let config = config_reader.read().await;
            info!("🔍 Current liquidity weight: {:.3} (Updates: {})", 
                  config.current_weights.liquidity, config.update_count);
        }
    });
    
    // Wait a moment
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    
    // Simulate an optimization update
    info!("🔄 Applying hot-swap optimization...");
    let optimized_params = OptimizedParameters {
        new_weights: FeatureWeights {
            liquidity: 0.250,
            holder_distribution: 0.160,
            volume_growth: 0.180,
            holder_growth: 0.110,
            price_change: 0.100,
            jito_bundle_presence: 0.050,
            creator_sell_speed: 0.100,
            metadata_quality: 0.100,
            social_activity: 0.050,
        },
        new_thresholds: initial_thresholds,
        reason: "Demo optimization: increased liquidity focus".to_string(),
    };
    
    // Apply the hot-swap
    {
        let mut config = runtime_config.write().await;
        let old_liquidity = config.current_weights.liquidity;
        config.apply_optimization(optimized_params);
        let new_liquidity = config.current_weights.liquidity;
        
        info!("✅ Hot-swap completed!");
        info!("  Liquidity weight: {:.3} → {:.3}", old_liquidity, new_liquidity);
        info!("  Update count: {}", config.update_count);
    }
    
    // Wait to see the updated configuration in the reader
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    
    // Show final configuration
    let final_config = runtime_config.read().await;
    info!("🎯 Final Configuration Summary:");
    info!("  Total updates: {}", final_config.update_count);
    info!("  Last update: {}ms ago", 
          chrono::Utc::now().timestamp_millis() as u64 - final_config.last_update_timestamp);
    info!("  Liquidity weight: {:.3}", final_config.current_weights.liquidity);
    
    info!("✨ Hot-swap demo completed successfully!");
    
    Ok(())
}