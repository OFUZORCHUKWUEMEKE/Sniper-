use thiserror::Error;

#[derive(Error, Debug)]
pub enum MonitorError {
    #[error("WebSocket connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Failed to parse transaction: {0}")]
    ParseError(String),

    #[error("RPC error: {0}")]
    RpcError(#[from] solana_client::client_error::ClientError),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("WebSocket error: {0}")]
    WebSocketError(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Channel send error")]
    ChannelError,

    #[error("Maximum reconnection attempts exceeded")]
    MaxReconnectAttemptsExceeded,

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    #[error("Timeout waiting for response")]
    Timeout,

    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// Result type for monitor operations
pub type MonitorResult<T> = Result<T, MonitorError>;
