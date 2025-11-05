use copy_tradin::{TransactionListener, TransactionParser, load_config};
use std::env;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Solana Copy Trading Bot - Monitor (Task 1)");

    // Load configuration
    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let config = match load_config(&config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to load config: {}", e);
            info!("Creating default config file...");
            copy_tradin::create_default_config(&config_path)?;
            info!(
                "Please edit {} with your target wallet address",
                config_path
            );
            return Ok(());
        }
    };

    info!("Monitoring wallet: {}", config.target_wallet);

    // Create channel for transactions
    let (tx_sender, mut tx_receiver) = mpsc::unbounded_channel();

    // Create parser
    let parser = TransactionParser::new(config.target_wallet);

    // Create and start listener
    let mut listener = TransactionListener::new(config.clone(), tx_sender);

    // Spawn listener task
    let target_wallet = config.target_wallet;
    let listener_handle = tokio::spawn(async move {
        if let Err(e) = listener.start(target_wallet).await {
            error!("Listener error: {}", e);
        }
    });

    // Spawn parser task
    let parser_handle = tokio::spawn(async move {
        info!("Parser ready, waiting for transactions...");

        while let Some(transaction) = tx_receiver.recv().await {
            match parser.parse(transaction) {
                Ok(Some(trade_signal)) => {
                    info!("═══════════════════════════════════════════════");
                    info!("🎯 TRADE DETECTED!");
                    info!("═══════════════════════════════════════════════");
                    info!("Signature: {}", trade_signal.signature);
                    info!("DEX: {}", trade_signal.dex);
                    info!("Source Token: {}", trade_signal.source_mint);
                    info!("Dest Token: {}", trade_signal.destination_mint);
                    info!("Amount In: {}", trade_signal.amount_in);
                    info!("Min Amount Out: {}", trade_signal.minimum_amount_out);
                    info!("Slippage: {:.2}%", trade_signal.slippage_bps as f64 / 100.0);
                    info!(
                        "Priority Fee: {} lamports",
                        trade_signal.priority_fee_lamports
                    );
                    info!("Timestamp: {}", trade_signal.timestamp);
                    info!("═══════════════════════════════════════════════");
                }
                Ok(None) => {
                    info!("Transaction processed but no trade signal extracted");
                }
                Err(e) => {
                    error!("Failed to parse transaction: {}", e);
                }
            }
        }
    });

    info!("Bot is running. Press Ctrl+C to stop.");

    // Wait for either task to complete (or Ctrl+C)
    tokio::select! {
        _ = listener_handle => {
            info!("Listener task ended");
        }
        _ = parser_handle => {
            info!("Parser task ended");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
    }

    info!("Shutting down...");
    Ok(())
}
