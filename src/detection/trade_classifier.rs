use crate::detection::types::{BalanceDelta, TransactionType};
use tracing::debug;

/// Classifies transactions based on balance change patterns
pub struct TradeClassifier {}

impl TradeClassifier {
    /// Create a new trade classifier
    pub fn new() -> Self {
        Self {}
    }

    /// Classify a transaction based on its balance deltas
    pub fn classify(&self, deltas: &[BalanceDelta]) -> TransactionType {
        let decreases = deltas.iter().filter(|d| d.is_decrease()).count();
        let increases = deltas.iter().filter(|d| d.is_increase()).count();

        debug!("Classifying transaction: {} decreases, {} increases", decreases, increases);

        let tx_type = match (decreases, increases) {
            // Simple swap: 1 sold, 1 bought
            (1, 1) => TransactionType::Swap,

            // Multi-hop swap: Multiple changes
            (n, m) if n >= 1 && m >= 1 => TransactionType::MultiHopSwap,

            // Transfer: Only decreases (sending tokens)
            (n, 0) if n > 0 => TransactionType::Transfer,

            // Receive: Only increases (receiving tokens)
            (0, n) if n > 0 => TransactionType::Receive,

            // Liquidity provision: 1 decrease (deposit), multiple increases (LP tokens)
            (1, n) if n > 1 => TransactionType::AddLiquidity,

            // Liquidity removal: Multiple decreases (LP tokens), 1 increase (withdrawal)
            (n, 1) if n > 1 => TransactionType::RemoveLiquidity,

            // Unknown pattern
            _ => TransactionType::Unknown,
        };

        debug!("Transaction classified as: {}", tx_type);
        tx_type
    }

    /// Quick check if a transaction should be copied
    pub fn should_copy(&self, deltas: &[BalanceDelta]) -> bool {
        self.classify(deltas).should_copy()
    }
}

impl Default for TradeClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;

    #[test]
    fn test_simple_swap_classification() {
        let classifier = TradeClassifier::new();

        let deltas = vec![
            BalanceDelta {
                mint: Pubkey::new_unique(),
                delta: -100,
                pre_balance: 1000,
                post_balance: 900,
                decimals: 6,
            },
            BalanceDelta {
                mint: Pubkey::new_unique(),
                delta: 50,
                pre_balance: 0,
                post_balance: 50,
                decimals: 9,
            },
        ];

        assert_eq!(classifier.classify(&deltas), TransactionType::Swap);
        assert!(classifier.should_copy(&deltas));
    }

    #[test]
    fn test_transfer_classification() {
        let classifier = TradeClassifier::new();

        let deltas = vec![BalanceDelta {
            mint: Pubkey::new_unique(),
            delta: -100,
            pre_balance: 1000,
            post_balance: 900,
            decimals: 6,
        }];

        assert_eq!(classifier.classify(&deltas), TransactionType::Transfer);
        assert!(!classifier.should_copy(&deltas));
    }

    #[test]
    fn test_receive_classification() {
        let classifier = TradeClassifier::new();

        let deltas = vec![BalanceDelta {
            mint: Pubkey::new_unique(),
            delta: 100,
            pre_balance: 0,
            post_balance: 100,
            decimals: 6,
        }];

        assert_eq!(classifier.classify(&deltas), TransactionType::Receive);
        assert!(!classifier.should_copy(&deltas));
    }
}