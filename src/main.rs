use copy_tradin::{
    PortfolioTracker, TradeDirection, TransactionListener, UniversalParser, load_config,
}; // ADD TradeDirection
use std::env;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info}; // ADD for thread-safe portfolio

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Solana Copy Trading Bot - Universal DEX Detection");

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
    // CREATE PORTFOLIO TRACKER
    const PORTFOLIO_FILE: &str = "portfolio.json";

    let portfolio = Arc::new(Mutex::new(match PortfolioTracker::load(PORTFOLIO_FILE) {
        Ok(portfolio) => {
            info!("📂 Loaded existing portfolio:");
            let stats = portfolio.get_stats();
            info!("   Active positions: {}", stats.active_positions);
            info!("   Closed positions: {}", stats.closed_positions);
            info!("   Total P&L: {}", stats.total_realized_pnl);
            portfolio
        }
        Err(e) => {
            info!("🆕 Starting with fresh portfolio: {}", e);
            PortfolioTracker::new()
        }
    }));

    info!("Monitoring wallet: {}", config.target_wallet);
    info!("🌟 Using UNIVERSAL detection - works with ALL DEXs!");

    let (tx_sender, mut tx_receiver) = mpsc::unbounded_channel();
    let parser = UniversalParser::new(config.target_wallet);
    let mut listener = TransactionListener::new(config.clone(), tx_sender);

    let portfolio = Arc::new(Mutex::new(PortfolioTracker::new()));
    let portfolio_clone = Arc::clone(&portfolio);

    let target_wallet = config.target_wallet;
    let listener_handle = tokio::spawn(async move {
        if let Err(e) = listener.start(target_wallet).await {
            error!("Listener error: {}", e);
        }
    });

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

                    // ✅ ADD DIRECTION ANALYSIS HERE
                    info!("");
                    match swap_signal.direction() {
                        TradeDirection::Buy { token, payment } => {
                            info!("🎯 DIRECTION: BUY (Entry Signal)");
                            info!("   Token:   {}", token);
                            info!("   Payment: {}", payment);
                            info!("✅ COPYABLE SIGNAL");

                            // TRACK THE POSITION
                            let mut portfolio = portfolio_clone.lock().unwrap();

                            if portfolio.has_position(&token) {
                                info!("📊 Already have position in this token - tracking as add");
                            } else {
                                info!("✅ NEW POSITION - Will track this");
                            }
                            // Simulate opening position (in reality, you'd execute the trade first)
                            portfolio.open_position(
                                token,
                                swap_signal.output_amount,
                                payment,
                                swap_signal.input_amount,
                                swap_signal.signature.to_string(), // ✅ .to_string()
                            );
                            portfolio.save_safe(PORTFOLIO_FILE);

                            // Show portfolio stats
                            let stats = portfolio.get_stats();
                            info!(
                                "💼 Portfolio: {} active positions, Total invested: {}",
                                stats.active_positions, stats.total_invested
                            );
                        }
                        TradeDirection::Sell { token, receives } => {
                            info!("📉 DIRECTION: SELL (Exit Signal)");
                            info!("   Selling:  {} (token being sold)", token);
                            info!("   For:      {} (receiving)", receives);
                            info!("⏭️  SKIP - Exit trade");
                            // CHECK IF WE HAVE THIS POSITION
                            let mut portfolio = portfolio_clone.lock().unwrap();

                            if portfolio.has_position(&token) {
                                info!("✅ WE OWN THIS! Copying the sell...");

                                // Close the position
                                match portfolio.close_position(
                                    &token,
                                    swap_signal.input_amount,
                                    swap_signal.output_amount,
                                    swap_signal.signature.to_string(),
                                ) {
                                    Ok(closed) => {
                                        info!("🏁 Position closed:");
                                        info!(
                                            "   P&L: {} ({:.2}%)",
                                            closed.realized_pnl, closed.realized_pnl_percent
                                        );
                                        portfolio.save_safe(PORTFOLIO_FILE);
                                        // Show updated stats
                                        let stats = portfolio.get_stats();
                                        info!(
                                            "💼 Portfolio: {} active, {} closed, Win rate: {:.1}%",
                                            stats.active_positions,
                                            stats.closed_positions,
                                            stats.win_rate
                                        );
                                    }
                                    Err(e) => {
                                        error!("Failed to close position: {}", e);
                                    }
                                }
                            } else {
                                info!("⏭️  SKIP - We don't own this token");
                            }
                        }
                        TradeDirection::Swap {
                            from_token,
                            to_token,
                        } => {
                            info!("🔄 DIRECTION: TOKEN SWAP");
                            info!("   From: {}", from_token);
                            info!("   To:   {}", to_token);
                        }
                    }

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

    tokio::select! {
        _ = listener_handle => {
            info!("Listener task ended");
        }
        _ = parser_handle => {
            info!("Parser task ended");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");

            // ✅ SAVE PORTFOLIO BEFORE SHUTDOWN
            let portfolio = portfolio.lock().unwrap();

            info!("");
            info!("💾 Saving portfolio...");
            portfolio.save_safe(PORTFOLIO_FILE);

            // Print final portfolio stats
            let stats = portfolio.get_stats();
            info!("");
            info!("📊 FINAL PORTFOLIO STATS:");
            info!("   Active positions:  {}", stats.active_positions);
            info!("   Closed positions:  {}", stats.closed_positions);
            info!("   Total realized P&L: {}", stats.total_realized_pnl);
            info!("   Win rate: {:.1}%", stats.win_rate);
        }
    }

    info!("Shutting down...");
    Ok(())
}
