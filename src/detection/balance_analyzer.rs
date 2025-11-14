use crate::detection::types::{BalanceDelta, TokenBalance};
use crate::monitor::error::{MonitorError, MonitorResult};
use solana_sdk::pubkey::Pubkey;
use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{debug, warn};

/// Analyzes token balance changes in transactions
pub struct BalanceAnalyzer {
    target_wallet: Pubkey,
}

impl BalanceAnalyzer {
    /// Create a new balance analyzer
    pub fn new(target_wallet: Pubkey) -> Self {
        Self { target_wallet }
    }

    /// Extract all token balances before the transaction
    pub fn extract_pre_balances(
        &self,
        transaction: &EncodedConfirmedTransactionWithStatusMeta,
    ) -> MonitorResult<HashMap<Pubkey, TokenBalance>> {
        let meta = transaction
            .transaction
            .meta
            .as_ref()
            .ok_or_else(|| MonitorError::ParseError("No transaction metadata".to_string()))?;

        let mut balances = HashMap::new();

        // Handle OptionSerializer - it may be None, Some, or Skip
        let pre_balances = match &meta.pre_token_balances {
            solana_transaction_status::option_serializer::OptionSerializer::Some(balances) => {
                balances
            }
            solana_transaction_status::option_serializer::OptionSerializer::None
            | solana_transaction_status::option_serializer::OptionSerializer::Skip => {
                return Ok(balances); // No token balances
            }
        };

        for balance_info in pre_balances {
            // Parse mint
            let mint = Pubkey::from_str(&balance_info.mint)
                .map_err(|e| MonitorError::ParseError(format!("Invalid mint address: {}", e)))?;

            // Parse owner - handle OptionSerializer
            let owner = match &balance_info.owner {
                solana_transaction_status::option_serializer::OptionSerializer::Some(o) => {
                    Pubkey::from_str(o).ok().unwrap_or_default()
                }
                _ => Pubkey::default(),
            };

            // Only track balances for target wallet
            if owner != self.target_wallet {
                continue;
            }

            // Parse amount
            let amount = balance_info
                .ui_token_amount
                .amount
                .parse::<u64>()
                .map_err(|e| MonitorError::ParseError(format!("Invalid amount: {}", e)))?;

            let decimals = balance_info.ui_token_amount.decimals;

            balances.insert(
                mint,
                TokenBalance {
                    mint,
                    owner,
                    amount,
                    decimals,
                },
            );

            debug!("Pre-balance: {} = {}", mint, amount);
        }

        Ok(balances)
    }

    /// Extract all token balances after the transaction
    pub fn extract_post_balances(
        &self,
        transaction: &EncodedConfirmedTransactionWithStatusMeta,
    ) -> MonitorResult<HashMap<Pubkey, TokenBalance>> {
        let meta = transaction
            .transaction
            .meta
            .as_ref()
            .ok_or_else(|| MonitorError::ParseError("No transaction metadata".to_string()))?;

        let mut balances = HashMap::new();

        // Handle OptionSerializer - it may be None, Some, or Skip
        let post_balances = match &meta.post_token_balances {
            solana_transaction_status::option_serializer::OptionSerializer::Some(balances) => {
                balances
            }
            solana_transaction_status::option_serializer::OptionSerializer::None
            | solana_transaction_status::option_serializer::OptionSerializer::Skip => {
                return Ok(balances); // No token balances
            }
        };

        for balance_info in post_balances {
            // Parse mint
            let mint = Pubkey::from_str(&balance_info.mint)
                .map_err(|e| MonitorError::ParseError(format!("Invalid mint address: {}", e)))?;

            // Parse owner - handle OptionSerializer
            let owner = match &balance_info.owner {
                solana_transaction_status::option_serializer::OptionSerializer::Some(o) => {
                    Pubkey::from_str(o).ok().unwrap_or_default()
                }
                _ => Pubkey::default(),
            };

            // Only track balances for target wallet
            if owner != self.target_wallet {
                continue;
            }

            // Parse amount
            let amount = balance_info
                .ui_token_amount
                .amount
                .parse::<u64>()
                .map_err(|e| MonitorError::ParseError(format!("Invalid amount: {}", e)))?;

            let decimals = balance_info.ui_token_amount.decimals;

            balances.insert(
                mint,
                TokenBalance {
                    mint,
                    owner,
                    amount,
                    decimals,
                },
            );

            debug!("Post-balance: {} = {}", mint, amount);
        }

        Ok(balances)
    }

    /// Calculate the deltas between pre and post balances
    pub fn calculate_deltas(
        &self,
        pre_balances: HashMap<Pubkey, TokenBalance>,
        post_balances: HashMap<Pubkey, TokenBalance>,
    ) -> Vec<BalanceDelta> {
        // Get all unique token mints
        let all_mints: std::collections::HashSet<_> = pre_balances
            .keys()
            .chain(post_balances.keys())
            .cloned()
            .collect();

        let mut deltas = Vec::new();

        for mint in all_mints {
            let pre_balance = pre_balances.get(&mint);
            let post_balance = post_balances.get(&mint);

            let (pre_amount, decimals) = match pre_balance {
                Some(bal) => (bal.amount, bal.decimals),
                None => (0, post_balance.map(|b| b.decimals).unwrap_or(9)),
            };

            let post_amount = post_balance.map(|b| b.amount).unwrap_or(0);

            // Calculate delta
            let delta = post_amount as i64 - pre_amount as i64;

            // Skip if no change
            if delta == 0 {
                continue;
            }

            // Skip native SOL/WSOL (we only care about SPL token swaps)
            // Note: Wrapped SOL (WSOL) has mint: So11111111111111111111111111111111111111112
            // We skip it because SOL changes are usually just for fees
            let wsol_mint = "So11111111111111111111111111111111111111112";
            if mint.to_string() == wsol_mint {
                debug!(
                    "Skipping SOL/WSOL balance change (likely fees): {} lamports",
                    delta
                );
                continue;
            }

            let balance_delta = BalanceDelta {
                mint,
                delta,
                pre_balance: pre_amount,
                post_balance: post_amount,
                decimals,
            };

            debug!(
                "Delta detected: {} {} ({})",
                if balance_delta.is_decrease() {
                    "-"
                } else {
                    "+"
                },
                balance_delta.ui_amount(),
                mint
            );

            deltas.push(balance_delta);
        }

        // Sort by delta (decreases first, then increases)
        deltas.sort_by_key(|d| d.delta);

        deltas
    }

    /// Analyze a transaction and extract balance deltas
    pub fn analyze(
        &self,
        transaction: &EncodedConfirmedTransactionWithStatusMeta,
    ) -> MonitorResult<Vec<BalanceDelta>> {
        let pre_balances = self.extract_pre_balances(transaction)?;
        let post_balances = self.extract_post_balances(transaction)?;

        if pre_balances.is_empty() && post_balances.is_empty() {
            warn!("No token balances found in transaction");
            return Ok(Vec::new());
        }

        let deltas = self.calculate_deltas(pre_balances, post_balances);

        Ok(deltas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_delta_calculations() {
        let delta = BalanceDelta {
            mint: Pubkey::new_unique(),
            delta: -100_000_000, // Sold 100 tokens (with 6 decimals)
            pre_balance: 1_000_000_000,
            post_balance: 900_000_000,
            decimals: 6,
        };

        assert!(delta.is_decrease());
        assert!(!delta.is_increase());
        assert_eq!(delta.abs_amount(), 100_000_000);
        assert_eq!(delta.ui_amount(), 100.0);
    }
}
