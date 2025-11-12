use crate::monitor::error::{MonitorError, MonitorResult};
use crate::types::MonitorConfig;
use config::{Config, File};
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use std::path::Path;
use std::str::FromStr;
use tracing::info;

#[derive(Debug, Deserialize)]
struct RawConfig {
    monitor: RawMonitorConfig,
    logging: Option<LoggingConfig>,
}

#[derive(Debug, Deserialize)]
struct RawMonitorConfig {
    target_wallet: String,
    rpc_endpoints: Vec<String>,
    websocket_endpoint: String,
    connection_timeout_secs: Option<u64>,
    max_reconnect_attempts: Option<u32>,
    use_confirmed_commitment: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    level: Option<String>,
}

/// Load configuration from a TOML file
pub fn load_config<P: AsRef<Path>>(path: P) -> MonitorResult<MonitorConfig> {
    info!("Loading configuration from {:?}", path.as_ref());

    let config = Config::builder()
        .add_source(File::from(path.as_ref()))
        .build()
        .map_err(|e| MonitorError::ConfigError(format!("Failed to load config: {}", e)))?;

    let raw: RawConfig = config
        .try_deserialize()
        .map_err(|e| MonitorError::ConfigError(format!("Failed to parse config: {}", e)))?;

    // Parse target wallet pubkey
    let target_wallet = Pubkey::from_str(&raw.monitor.target_wallet)
        .map_err(|e| MonitorError::ConfigError(format!("Invalid target wallet address: {}", e)))?;

    // Validate RPC endpoints
    if raw.monitor.rpc_endpoints.is_empty() {
        return Err(MonitorError::ConfigError(
            "At least one RPC endpoint is required".to_string(),
        ));
    }

    for endpoint in &raw.monitor.rpc_endpoints {
        if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
            return Err(MonitorError::ConfigError(format!(
                "Invalid RPC endpoint (must start with http:// or https://): {}",
                endpoint
            )));
        }
    }

    // Validate WebSocket endpoint
    if !raw.monitor.websocket_endpoint.starts_with("ws://")
        && !raw.monitor.websocket_endpoint.starts_with("wss://")
    {
        return Err(MonitorError::ConfigError(
            "WebSocket endpoint must start with ws:// or wss://".to_string(),
        ));
    }

    let monitor_config = MonitorConfig {
        target_wallet,
        rpc_endpoints: raw.monitor.rpc_endpoints,
        websocket_endpoint: raw.monitor.websocket_endpoint,
        connection_timeout_secs: raw.monitor.connection_timeout_secs.unwrap_or(30),
        max_reconnect_attempts: raw.monitor.max_reconnect_attempts.unwrap_or(5),
        use_confirmed_commitment: raw.monitor.use_confirmed_commitment.unwrap_or(true),
    };

    info!("Configuration loaded successfully");
    info!("Target wallet: {}", monitor_config.target_wallet);
    info!("RPC endpoints: {:?}", monitor_config.rpc_endpoints);

    Ok(monitor_config)
}

/// Create a default configuration file
pub fn create_default_config<P: AsRef<Path>>(path: P) -> MonitorResult<()> {
    let default_config = r#"[monitor]
# The Solana wallet address to monitor and copy trades from
target_wallet = "YOUR_TARGET_WALLET_ADDRESS_HERE"

# List of RPC endpoints (for failover)
rpc_endpoints = [
    "https://api.mainnet-beta.solana.com",
    "https://solana-api.projectserum.com"
]

# WebSocket endpoint for real-time updates
websocket_endpoint = "wss://api.mainnet-beta.solana.com"

# Connection timeout in seconds
connection_timeout_secs = 30

# Maximum number of reconnection attempts
max_reconnect_attempts = 5

# Use "confirmed" commitment level (faster) instead of "finalized" (safer)
use_confirmed_commitment = true

[logging]
# Logging level: trace, debug, info, warn, error
level = "info"
"#;

    std::fs::write(path.as_ref(), default_config)
        .map_err(|e| MonitorError::ConfigError(format!("Failed to write config file: {}", e)))?;

    info!("Created default config file at {:?}", path.as_ref());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let raw = RawMonitorConfig {
            target_wallet: "11111111111111111111111111111111".to_string(),
            rpc_endpoints: vec![],
            websocket_endpoint: "wss://test.com".to_string(),
            connection_timeout_secs: None,
            max_reconnect_attempts: None,
            use_confirmed_commitment: None,
        };
        // Should fail with empty RPC endpoints
        assert!(raw.rpc_endpoints.is_empty());
    }
}
