use crate::detection::types::{BalanceDelta, SwapType, UniversalSwapSignal};
use crate::monitor::error::MonitorResult;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use tracing::{debug, info, warn};

/// Detects swap patterns from balance deltas
pub struct SwapDetector {}

impl SwapDetector {
    /// Create a new swap detector
    pub fn new() -> Self {
        Self {}
    }

    /// Detect if balance deltas represent a swap
    pub fn detect_swap(
        &self,
        deltas: Vec<BalanceDelta>,
        signature: Signature,
        timestamp: i64,
        target_wallet: Pubkey,
        likely_dex: Option<String>,
    ) -> MonitorResult<Option<UniversalSwapSignal>> {
        if deltas.is_empty() {
            debug!("No balance deltas, not a swap");
            return Ok(None);
        }

        // Separate increases and decreases
        let decreases: Vec<_> = deltas.iter().filter(|d| d.is_decrease()).collect();
        let increases: Vec<_> = deltas.iter().filter(|d| d.is_increase()).collect();

        info!("Balance changes: {} decreases, {} increases", decreases.len(), increases.len());

        // === PATTERN 1: Simple Swap ===
        // Exactly 1 token decreased (sold) and 1 token increased (bought)
        if decreases.len() == 1 && increases.len() == 1 {
            let input = decreases[0];
            let output = increases[0];

            info!("✅ Simple swap detected:");
            info!("   Input:  {} {} ({})", input.ui_amount(), input.mint, input.mint);
            info!("   Output: {} {} ({})", output.ui_amount(), output.mint, output.mint);

            return Ok(Some(UniversalSwapSignal {
                signature,
                timestamp,
                trader: target_wallet,
                swap_type: SwapType::Simple,
                input_mint: input.mint,
                input_amount: input.abs_amount(),
                output_mint: output.mint,
                output_amount: output.abs_amount(),
                intermediate_tokens: vec![],
                likely_dex,
            }));
        }

        // === PATTERN 2: Multi-Hop Swap ===
        // Multiple tokens changed (e.g., USDC → SOL → BONK)
        // Strategy: First decrease = input, Last increase = output
        if decreases.len() >= 1 && increases.len() >= 1 {
            let input = decreases.first().unwrap();
            let output = increases.last().unwrap();

            // Collect intermediate tokens
            let intermediate_tokens: Vec<Pubkey> = deltas
                .iter()
                .filter(|d| d.mint != input.mint && d.mint != output.mint)
                .map(|d| d.mint)
                .collect();

            info!("✅ Multi-hop swap detected:");
            info!("   Input:  {} {} ({})", input.ui_amount(), input.mint, input.mint);
            info!("   Output: {} {} ({})", output.ui_amount(), output.mint, output.mint);
            info!("   Hops:   {} intermediate tokens", intermediate_tokens.len());

            return Ok(Some(UniversalSwapSignal {
                signature,
                timestamp,
                trader: target_wallet,
                swap_type: SwapType::MultiHop,
                input_mint: input.mint,
                input_amount: input.abs_amount(),
                output_mint: output.mint,
                output_amount: output.abs_amount(),
                intermediate_tokens,
                likely_dex,
            }));
        }

        // === PATTERN 3: Not a Swap ===
        // Examples:
        // - Only decreases (transfers/burns)
        // - Only increases (receives/mints)
        // - Complex patterns (liquidity provision, etc.)
        warn!("⏭️  Not a swap pattern (decreases: {}, increases: {})", decreases.len(), increases.len());
        Ok(None)
    }

    /// Try to guess which DEX was used (for logging only)
    pub fn guess_dex(
        &self,
        transaction: &solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta,
    ) -> Option<String> {
        // Try to identify DEX from program IDs in instructions
        use crate::types::program_ids;

        let tx = match &transaction.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Json(tx) => tx,
            _ => return None,
        };

        let message = match &tx.message {
            solana_transaction_status::UiMessage::Parsed(msg) => msg,
            _ => return None,
        };

        // Check each instruction for known program IDs
        for instruction in &message.instructions {
            let program_id_str = match instruction {
                solana_transaction_status::UiInstruction::Parsed(parsed) => {
                    match parsed {
                        solana_transaction_status::UiParsedInstruction::Parsed(p) => {
                            Some(p.program_id.clone())
                        }
                        solana_transaction_status::UiParsedInstruction::PartiallyDecoded(p) => {
                            Some(p.program_id.clone())
                        }
                    }
                }
                solana_transaction_status::UiInstruction::Compiled(compiled) => {
                    // Would need to look up program ID from account keys
                    None
                }
            };

            if let Some(program_id) = program_id_str {
                // Check against known DEX program IDs
                if program_id == program_ids::jupiter_v6().to_string() {
                    return Some("Jupiter".to_string());
                }
                if program_id == program_ids::raydium_v4().to_string() {
                    return Some("Raydium".to_string());
                }
                if program_id == program_ids::orca_whirlpool().to_string() {
                    return Some("Orca".to_string());
                }
            }
        }

        Some("Unknown DEX".to_string())
    }
}

impl Default for SwapDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_swap_detection() {
        let detector = SwapDetector::new();
        
        // Simulate: Sold 100 USDC, bought 50,000 BONK
        let deltas = vec![
            BalanceDelta {
                mint: Pubkey::new_unique(), // USDC
                delta: -100_000_000,
                pre_balance: 1_000_000_000,
                post_balance: 900_000_000,
                decimals: 6,
            },
            BalanceDelta {
                mint: Pubkey::new_unique(), // BONK
                delta: 50_000_000_000,
                pre_balance: 0,
                post_balance: 50_000_000_000,
                decimals: 9,
            },
        ];

        let result = detector.detect_swap(
            deltas,
            Signature::new_unique(),
            0,
            Pubkey::new_unique(),
            None,
        ).unwrap();

        assert!(result.is_some());
        let signal = result.unwrap();
        assert_eq!(signal.swap_type, SwapType::Simple);
        assert_eq!(signal.input_amount, 100_000_000);
        assert_eq!(signal.output_amount, 50_000_000_000);
    }
}