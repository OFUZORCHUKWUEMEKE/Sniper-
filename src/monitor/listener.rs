use crate::monitor::error::{MonitorError, MonitorResult};
use crate::monitor::websocket::WebSocketManager;
use crate::types::MonitorConfig;
use serde_json::Value;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::Signature;
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

const DEDUP_CACHE_SIZE: usize = 10_000;

/// Listens for transactions from WebSocket and fetches full transaction data
pub struct TransactionListener {
    ws_manager: WebSocketManager,
    rpc_client: Arc<RpcClient>,
    seen_signatures: HashSet<Signature>,
    tx_sender: mpsc::UnboundedSender<EncodedConfirmedTransactionWithStatusMeta>,
}

impl TransactionListener {
    /// Create a new transaction listener
    pub fn new(
        config: MonitorConfig,
        tx_sender: mpsc::UnboundedSender<EncodedConfirmedTransactionWithStatusMeta>,
    ) -> Self {
        let commitment = if config.use_confirmed_commitment {
            CommitmentConfig::confirmed()
        } else {
            CommitmentConfig::finalized()
        };

        // Use first RPC endpoint for now (can add failover later)
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            config.rpc_endpoints[0].clone(),
            commitment,
        ));

        Self {
            ws_manager: WebSocketManager::new(config),
            rpc_client,
            seen_signatures: HashSet::new(),
            tx_sender,
        }
    }

    /// Start listening for transactions
    pub async fn start(&mut self, target_address: solana_sdk::pubkey::Pubkey) -> MonitorResult<()> {
        info!("Starting transaction listener for {}", target_address);

        // Connect to WebSocket
        self.ws_manager.connect().await?;

        // Subscribe to logs (this gives us transaction signatures)
        self.ws_manager.subscribe_to_logs(&target_address).await?;

        info!("Listening for transactions...");

        // Start the listening loop
        loop {
            match self.listen_once().await {
                Ok(_) => {}
                Err(MonitorError::ConnectionFailed(_)) | Err(MonitorError::WebSocketError(_)) => {
                    warn!("Connection lost, attempting to reconnect...");

                    // Try to reconnect
                    if let Err(e) = self.ws_manager.reconnect().await {
                        error!("Failed to reconnect: {}", e);
                        return Err(e);
                    }

                    // Resubscribe after reconnection
                    self.ws_manager.subscribe_to_logs(&target_address).await?;
                }
                Err(e) => {
                    error!("Listener error: {}", e);
                    // Continue on other errors
                }
            }
        }
    }

    /// Listen for one message and process it
    async fn listen_once(&mut self) -> MonitorResult<()> {
        if let Some(message) = self.ws_manager.receive_message().await? {
            self.process_message(message).await?;
        }
        Ok(())
    }

    /// Process a WebSocket message
    async fn process_message(&mut self, message: Value) -> MonitorResult<()> {
        // Check if this is a logs notification
        if let Some(params) = message.get("params") {
            if let Some(result) = params.get("result") {
                if let Some(value) = result.get("value") {
                    self.process_log_notification(value).await?;
                }
            }
        }
        Ok(())
    }

    /// Process a log notification to extract transaction signature
    async fn process_log_notification(&mut self, value: &Value) -> MonitorResult<()> {
        // Extract signature from the log notification
        if let Some(sig_str) = value.get("signature").and_then(|s| s.as_str()) {
            debug!("Detected transaction: {}", sig_str);

            let signature = Signature::from_str(sig_str)
                .map_err(|e| MonitorError::ParseError(format!("Invalid signature: {}", e)))?;

            // Check for duplicates
            if self.is_duplicate(&signature) {
                debug!("Skipping duplicate transaction: {}", signature);
                return Ok(());
            }

            // Fetch full transaction details
            self.fetch_and_send_transaction(signature).await?;
        }

        Ok(())
    }

    /// Check if we've already processed this signature
    fn is_duplicate(&mut self, signature: &Signature) -> bool {
        if self.seen_signatures.contains(signature) {
            return true;
        }

        // Add to cache
        self.seen_signatures.insert(*signature);

        // Evict old entries if cache is too large
        if self.seen_signatures.len() > DEDUP_CACHE_SIZE {
            // Remove oldest 10% of entries (simple approach)
            let to_remove = DEDUP_CACHE_SIZE / 10;
            let signatures_to_remove: Vec<Signature> = self
                .seen_signatures
                .iter()
                .take(to_remove)
                .copied()
                .collect();

            for sig in signatures_to_remove {
                self.seen_signatures.remove(&sig);
            }
        }

        false
    }

    /// Fetch full transaction details from RPC and send to parser
    async fn fetch_and_send_transaction(&self, signature: Signature) -> MonitorResult<()> {
        info!("Fetching transaction: {}", signature);

        // Add small delay to ensure transaction is available
        sleep(Duration::from_millis(500)).await;

        // Try to fetch with retries
        let max_retries = 3;
        let mut retries = 0;

        loop {
            match self.rpc_client.get_transaction_with_config(
                &signature,
                solana_client::rpc_config::RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::JsonParsed),
                    commitment: Some(self.rpc_client.commitment()),
                    max_supported_transaction_version: Some(0), // Support v0 transactions
                },
            ) {
                Ok(transaction) => {
                    info!("Successfully fetched transaction: {}", signature);

                    // Send to parser via channel
                    if let Err(e) = self.tx_sender.send(transaction) {
                        error!("Failed to send transaction to parser: {}", e);
                        return Err(MonitorError::ChannelError);
                    }

                    return Ok(());
                }
                Err(e) => {
                    retries += 1;
                    if retries >= max_retries {
                        error!(
                            "Failed to fetch transaction after {} retries: {}",
                            max_retries, e
                        );
                        return Err(MonitorError::RpcError(e));
                    }

                    warn!(
                        "Retry {}/{} - Error fetching transaction: {}",
                        retries, max_retries, e
                    );
                    sleep(Duration::from_millis(1000 * retries)).await;
                }
            }
        }
    }

    /// Perform periodic health checks
    pub async fn health_check(&mut self) -> MonitorResult<()> {
        self.ws_manager.health_check().await
    }

    /// Close the listener
    pub async fn close(&mut self) -> MonitorResult<()> {
        info!("Closing transaction listener");
        self.ws_manager.close().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplication() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = MonitorConfig::default();
        let mut listener = TransactionListener::new(config, tx);

        let sig = Signature::default();

        assert!(!listener.is_duplicate(&sig));
        assert!(listener.is_duplicate(&sig));
    }
}
