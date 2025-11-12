//! Universal transaction detection module
//! 
//! This module provides DEX-agnostic swap detection by analyzing token balance changes
//! rather than parsing specific DEX instruction formats.

pub mod balance_analyzer;
pub mod swap_detector;
pub mod trade_classifier;
pub mod types;

use crate::monitor::error::MonitorResult;
use balance_analyzer::BalanceAnalyzer;
use solana_sdk::pubkey::Pubkey;
use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;
use swap_detector::SwapDetector;
use trade_classifier::TradeClassifier;
use types::UniversalSwapSignal;
use tracing::{info, warn};

/// Universal transaction parser that works with ANY DEX
pub struct UniversalParser {
    target_wallet: Pubkey,
    balance_analyzer: BalanceAnalyzer,
    swap_detector: SwapDetector,
    trade_classifier: TradeClassifier,
}

impl UniversalParser {
    /// Create a new universal parser
    pub fn new(target_wallet: Pubkey) -> Self {
        Self {
            target_wallet,
            balance_analyzer: BalanceAnalyzer::new(target_wallet),
            swap_detector: SwapDetector::new(),
            trade_classifier: TradeClassifier::new(),
        }
    }

    /// Parse a transaction and detect swaps universally
    pub fn parse(
        &self,
        transaction: EncodedConfirmedTransactionWithStatusMeta,
    ) -> MonitorResult<Option<UniversalSwapSignal>> {
        info!("🔍 Analyzing transaction with universal parser...");

        // Get transaction signature
        let signature = match &transaction.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Json(tx) => {
                tx.signatures
                    .first()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| {
                        crate::monitor::error::MonitorError::ParseError(
                            "No signature found".to_string(),
                        )
                    })?
            }
            _ => {
                warn!("Transaction not in JSON format");
                return Ok(None);
            }
        };

        // Get timestamp
        let timestamp = transaction.block_time.unwrap_or(0);

        // === STEP 1: Analyze Balance Changes ===
        let deltas = self.balance_analyzer.analyze(&transaction)?;

        if deltas.is_empty() {
            info!("⏭️  No token balance changes detected - skipping");
            return Ok(None);
        }

        info!("📊 Detected {} token balance changes", deltas.len());
        for delta in &deltas {
            info!(
                "   {} {} {} ({})",
                if delta.is_decrease() { "▼" } else { "▲" },
                delta.ui_amount(),
                delta.mint,
                if delta.is_decrease() {
                    "sold"
                } else {
                    "bought"
                }
            );
        }

        // === STEP 2: Classify Transaction Type ===
        let tx_type = self.trade_classifier.classify(&deltas);
        info!("📋 Transaction type: {}", tx_type);

        if !tx_type.should_copy() {
            info!("⏭️  Transaction type '{}' should not be copied - skipping", tx_type);
            return Ok(None);
        }

        // === STEP 3: Detect Swap Pattern ===
        let likely_dex = self.swap_detector.guess_dex(&transaction);
        
        let swap_signal = self.swap_detector.detect_swap(
            deltas,
            signature,
            timestamp,
            self.target_wallet,
            likely_dex.clone(),
        )?;

        if let Some(ref signal) = swap_signal {
            info!("🎯 ═══════════════════════════════════════════════");
            info!("🎯 UNIVERSAL SWAP DETECTED!");
            info!("🎯 ═══════════════════════════════════════════════");
            info!("   Type: {}", signal.swap_type);
            info!("   Input: {} ({})", signal.input_mint, signal.input_amount);
            info!("   Output: {} ({})", signal.output_mint, signal.output_amount);
            if let Some(ref dex) = signal.likely_dex {
                info!("   Likely DEX: {} (doesn't matter!)", dex);
            }
            info!("");
            info!("🔗 Links:");
            info!("   • Transaction: {}", signal.solscan_url());
            info!("   • Trader: {}", signal.trader_solscan_url());
            info!("   • Input Token: https://solscan.io/token/{}", signal.input_mint);
            info!("   • Output Token: https://solscan.io/token/{}", signal.output_mint);
            info!("🎯 ═══════════════════════════════════════════════");
        }

        Ok(swap_signal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universal_parser_creation() {
        let wallet = Pubkey::new_unique();
        let parser = UniversalParser::new(wallet);
        assert_eq!(parser.target_wallet, wallet);
    }
}