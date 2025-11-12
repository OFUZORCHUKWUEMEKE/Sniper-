use copy_tradin::{load_config, TransactionListener, UniversalParser};
use std::env;
use tokio::sync::mpsc;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging - simple version
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Solana Copy Trading Bot - Universal DEX Detection");

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
            info!("Please edit {} with your target wallet address", config_path);
            return Ok(());
        }
    };

    info!("Monitoring wallet: {}", config.target_wallet);
    info!("🌟 Using UNIVERSAL detection - works with ALL DEXs!");

    // Create channel for transactions
    let (tx_sender, mut tx_receiver) = mpsc::unbounded_channel();

    // Create universal parser
    let parser = UniversalParser::new(config.target_wallet);

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
                Ok(Some(swap_signal)) => {
                    info!("═══════════════════════════════════════════════");
                    info!("🎯 SWAP DETECTED (Universal Detection)!");
                    info!("═══════════════════════════════════════════════");
                    info!("Signature: {}", swap_signal.signature);
                    info!("Type: {}", swap_signal.swap_type);
                    info!("Input Token: {}", swap_signal.input_mint);
                    info!("Input Amount: {}", swap_signal.input_amount);
                    info!("Output Token: {}", swap_signal.output_mint);
                    info!("Output Amount: {}", swap_signal.output_amount);
                    if let Some(ref dex) = swap_signal.likely_dex {
                        info!("Likely DEX: {} (detected automatically)", dex);
                    }
                    info!("Timestamp: {}", swap_signal.timestamp);
                    info!("");
                    info!("🔗 View on Solscan:");
                    info!("   Transaction: {}", swap_signal.solscan_url());
                    info!("   Trader: {}", swap_signal.trader_solscan_url());
                    info!("═══════════════════════════════════════════════");
                    
                    // TODO: Pass to decision engine
                    // TODO: Execute trade via Jupiter
                }
                Ok(None) => {
                    info!("Transaction processed but no swap detected");
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