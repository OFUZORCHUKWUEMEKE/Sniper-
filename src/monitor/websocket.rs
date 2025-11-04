use crate::monitor::error::{MonitorError, MonitorResult};
use crate::types::MonitorConfig;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use solana_sdk::pubkey::Pubkey;
use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

pub struct WebSocketManager {
    config: MonitorConfig,
    ws_stream: Option<WsStream>,
    reconnect_attempts: u32,
    subscription_id: Option<u64>,
}

impl WebSocketManager {
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            config,
            ws_stream: None,
            reconnect_attempts: 0,
            subscription_id: None,
        }
    }

    pub async fn connect(&mut self) -> MonitorResult<()> {
        info!(
            "Connecting to WebSocket: {}",
            self.config.websocket_endpoint
        );
        match connect_async(&self.config.websocket_endpoint).await {
            Ok((stream, response)) => {
                info!("WebSocket connected: {:?}", response.status());
                self.ws_stream = Some(stream);
                self.reconnect_attempts = 0;
                Ok(())
            }
            Err(e) => {
                error!("Failed to connect to WebSocket: {}", e);
                Err(MonitorError::ConnectionFailed(e.to_string()))
            }
        }
    }

    pub async fn subscribe_to_address(&mut self, address: &Pubkey) -> MonitorResult<()> {
        info!("Subscribing to address: {}", address);

        let commitment = if self.config.use_confirmed_commitment {
            "confirmed"
        } else {
            "finalized"
        };

        let subscribe_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "accountSubscribe",
            "params": [
                address.to_string(),
                {
                    "encoding": "jsonParsed",
                    "commitment": commitment
                }
            ]
        });

        self.send_message(&subscribe_request).await?;

        // Wait for subscription confirmation
        if let Some(response) = self.receive_message().await? {
            if let Some(result) = response.get("result") {
                if let Some(sub_id) = result.as_u64() {
                    self.subscription_id = Some(sub_id);
                    info!("Subscribed with ID: {}", sub_id);
                    return Ok(());
                }
            }

            if let Some(error) = response.get("error") {
                return Err(MonitorError::InvalidResponse(format!(
                    "Subscription failed: {:?}",
                    error
                )));
            }
        }

        Err(MonitorError::InvalidResponse(
            "No subscription confirmation received".to_string(),
        ))
    }

    pub async fn subscribe_to_logs(&mut self, address: &Pubkey) -> MonitorResult<()> {
        info!("Subscribing to logs for address: {}", address);

        let commitment = if self.config.use_confirmed_commitment {
            "confirmed"
        } else {
            "finalized"
        };
        // Subscribe to all transactions mentioning this account
        let subscribe_request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "logsSubscribe",
            "params": [
                {
                    "mentions": [address.to_string()]
                },
                {
                    "commitment": commitment
                }
            ]
        });
        self.send_message(&subscribe_request).await?;

        // Wait for subscription confirmation
        if let Some(response) = self.receive_message().await? {
            if let Some(result) = response.get("result") {
                if let Some(sub_id) = result.as_u64() {
                    info!("Logs subscription ID: {}", sub_id);
                    return Ok(());
                }
            }

            if let Some(error) = response.get("error") {
                warn!("Logs subscription failed: {:?}", error);
            }
        }

        Ok(())
    }

    /// Receive the next message from the WebSocket
    pub async fn receive_message(&mut self) -> MonitorResult<Option<Value>> {
        if let Some(stream) = &mut self.ws_stream {
            match stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    debug!("Received message: {}", text);
                    let value: Value = serde_json::from_str(&text)?;
                    Ok(Some(value))
                }
                Some(Ok(Message::Ping(_))) => {
                    debug!("Received ping");
                    Ok(None)
                }
                Some(Ok(Message::Pong(_))) => {
                    debug!("Received pong");
                    Ok(None)
                }
                Some(Ok(Message::Close(_))) => {
                    warn!("WebSocket closed by server");
                    Err(MonitorError::ConnectionFailed(
                        "Connection closed".to_string(),
                    ))
                }
                Some(Err(e)) => {
                    error!("WebSocket error: {}", e);
                    Err(MonitorError::WebSocketError(e))
                }
                None => {
                    warn!("WebSocket stream ended");
                    Err(MonitorError::ConnectionFailed("Stream ended".to_string()))
                }
                _ => Ok(None),
            }
        } else {
            Err(MonitorError::ConnectionFailed("Not connected".to_string()))
        }
    }

    /// Send a message through the WebSocket
    async fn send_message(&mut self, message: &Value) -> MonitorResult<()> {
        if let Some(stream) = &mut self.ws_stream {
            let text = serde_json::to_string(message)?;
            debug!("Sending message: {}", text);
            stream
                .send(Message::Text(text))
                .await
                .map_err(|e| MonitorError::WebSocketError(e))?;
            Ok(())
        } else {
            Err(MonitorError::ConnectionFailed("Not connected".to_string()))
        }
    }

    /// Attempt to reconnect with exponential backoff
    pub async fn reconnect(&mut self) -> MonitorResult<()> {
        if self.reconnect_attempts >= self.config.max_reconnect_attempts {
            error!("Max reconnection attempts reached");
            return Err(MonitorError::MaxReconnectAttemptsExceeded);
        }

        self.reconnect_attempts += 1;
        let backoff_secs = 2u64.pow(self.reconnect_attempts.min(5));

        warn!(
            "Reconnecting (attempt {}/{}) in {} seconds...",
            self.reconnect_attempts, self.config.max_reconnect_attempts, backoff_secs
        );

        sleep(Duration::from_secs(backoff_secs)).await;

        match self.connect().await {
            Ok(_) => {
                info!("Reconnected successfully");
                Ok(())
            }
            Err(e) => {
                error!("Reconnection failed: {}", e);
                Err(e)
            }
        }
    }
    /// Perform a health check by sending a ping
    pub async fn health_check(&mut self) -> MonitorResult<()> {
        if let Some(stream) = &mut self.ws_stream {
            stream
                .send(Message::Ping(vec![]))
                .await
                .map_err(|e| MonitorError::WebSocketError(e))?;
            debug!("Health check ping sent");
            Ok(())
        } else {
            Err(MonitorError::ConnectionFailed("Not connected".to_string()))
        }
    }

    /// Check if currently connected
    pub fn is_connected(&self) -> bool {
        self.ws_stream.is_some()
    }

    /// Close the WebSocket connection
    pub async fn close(&mut self) -> MonitorResult<()> {
        if let Some(mut stream) = self.ws_stream.take() {
            info!("Closing WebSocket connection");
            stream
                .close(None)
                .await
                .map_err(|e| MonitorError::WebSocketError(e))?;
        }
        Ok(())
    }
}
