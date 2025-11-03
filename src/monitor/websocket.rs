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

        let commitment = if self.config.use_confirmed_commitment{
            "confirmed"
        }else{
            "finalized"
        };
        
    }
}
