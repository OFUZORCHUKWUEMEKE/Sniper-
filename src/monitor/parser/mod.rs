use crate::monitor::error::{MonitorError, MonitorResult};
use crate::types::{DexType, TradeSignal, program_ids};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, UiInstruction, UiMessage, UiParsedInstruction, UiParsedMessage, UiTransaction
};
use std::str::FromStr;
use tracing::{debug, error, info, warn};

pub mod jupiter;
pub mod orca;
pub mod raydium;

/// Main transaction parser that routes to specific DEX parsers
pub struct TransactionParser {
    target_wallet: Pubkey,
}

impl TransactionParser {
    /// Create a new transaction parser
    pub fn new(target_wallet: Pubkey) -> Self {
        Self { target_wallet }
    }

    /// Parse a transaction and extract trade signal if it's a swap
    pub fn parse(
        &self,
        transaction: EncodedConfirmedTransactionWithStatusMeta,
    ) -> MonitorResult<Option<TradeSignal>> {
        info!("Parsing transaction...");

        // Extract transaction and metadata
        let ui_transaction = match transaction.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Json(tx) => tx,
            _ => {
                warn!("Transaction not in JSON format");
                return Ok(None);
            }
        };

        // Get message
        let message = match ui_transaction.message {
            UiMessage::Parsed(msg) => msg,
            _ => {
                warn!("Message not parsed");
                return Ok(None);
            }
        };

        // Get signature
        let signature = match ui_transaction.signatures.first() {
            Some(sig_str) => Signature::from_str(sig_str)
                .map_err(|e| MonitorError::ParseError(format!("Invalid signature: {}", e)))?,
            None => {
                warn!("No signature found");
                return Ok(None);
            }
        };

        // Get timestamp
        let timestamp = transaction.block_time.unwrap_or(0);

        // Extract all account keys
        let account_keys: Vec<Pubkey> = message
            .account_keys
            .iter()
            .filter_map(|key| match Pubkey::from_str(&key.pubkey) {
                Ok(pubkey) => Some(pubkey),
                Err(_) => {
                    warn!("Invalid pubkey: {}", key.pubkey);
                    None
                }
            })
            .collect();

        // Check if target wallet is involved
        if !account_keys.contains(&self.target_wallet) {
            debug!("Target wallet not involved in transaction");
            return Ok(None);
        }

        // Identify DEX type from instructions
        let dex_type = self.identify_dex(&message.instructions, &account_keys)?;

        if dex_type == DexType::Unknown {
            debug!("Unknown DEX type, skipping");
            return Ok(None);
        }

        info!("Detected {} swap", dex_type);

        // Extract priority fee
        let priority_fee = self.extract_priority_fee(&message);

        // Route to appropriate parser based on DEX type
        let trade_signal = match dex_type {
            DexType::Jupiter => jupiter::parse_jupiter_swap(
                &message.instructions,
                &account_keys,
                signature,
                timestamp,
                priority_fee,
                self.target_wallet,
            )?,
            DexType::Raydium => raydium::parse_raydium_swap(
                &message.instructions,
                &account_keys,
                signature,
                timestamp,
                priority_fee,
                self.target_wallet,
            )?,
            DexType::Orca => orca::parse_orca_swap(
                &message.instructions,
                &account_keys,
                signature,
                timestamp,
                priority_fee,
                self.target_wallet,
            )?,
            DexType::Unknown => None,
        };

        if let Some(ref signal) = trade_signal {
            info!("Successfully parsed trade: {}", signal.description());
        }

        Ok(trade_signal)
    }

    /// Identify which DEX is being used based on program IDs in instructions
    fn identify_dex(
        &self,
        instructions: &[UiInstruction],
        account_keys: &[Pubkey],
    ) -> MonitorResult<DexType> {
        let jupiter_id = program_ids::jupiter_v6();
        let raydium_id = program_ids::raydium_v4();
        let orca_id = program_ids::orca_whirlpool();

        for instruction in instructions {
            let program_id = match instruction {
                UiInstruction::Parsed(UiParsedInstruction::Parsed(parsed)) => {
                    // parsed.program is a String, not a HashMap
                    // Use the program field directly
                    Pubkey::from_str(&parsed.program).ok()
                }
                UiInstruction::Compiled(compiled) => {
                    // For compiled instructions, use program_id_index
                    account_keys
                        .get(compiled.program_id_index as usize)
                        .copied()
                }
                _ => None,
            };

            if let Some(pid) = program_id {
                if pid == jupiter_id {
                    return Ok(DexType::Jupiter);
                } else if pid == raydium_id {
                    return Ok(DexType::Raydium);
                } else if pid == orca_id {
                    return Ok(DexType::Orca);
                }
            }
        }

        Ok(DexType::Unknown)
    }

    fn extract_priority_fee(&self, message: &UiParsedMessage) -> u64 {
        //                              ^^^^^^^^^^^^^^^^^^^^^
        // Changed from &UiTransaction to &UiParsedMessage

        // Parse ComputeBudget instructions from message.instructions
        // TODO: Implement priority fee extraction
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let parser = TransactionParser::new(Pubkey::default());
        assert_eq!(parser.target_wallet, Pubkey::default());
    }
}
