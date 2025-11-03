use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    // The transaction signature
    pub signature: Signature,
    /// Unix timestamp when the transaction was processed
    pub timestamp: i64,
    /// The DEX that was used for this trade
    pub dex: DexType,
    /// Source token mint address (token being sold)
    pub source_mint: Pubkey,
    /// Destination token mint address (token being bought)
    pub destination_mint: Pubkey,
    /// Amount of source token being sold (in smallest unit)
    pub amount_in: u64,
    /// Amount of destination token expected to receive (in smallest unit)
    pub amount_out: u64,
    /// Minimum amount out (used for slippage calculation)
    pub minimum_amount_out: u64,
    /// Calculated slippage tolerance in basis points (100 = 1%)
    pub slippage_bps: u16,
    /// Priority fee paid for this transaction in lamports
    pub priority_fee_lamports: u64,
    /// All account keys involved in the transaction
    pub accounts: Vec<Pubkey>,
    /// The wallet that initiated this trade (target wallet)
    pub trader: Pubkey,
}

impl TradeSignal {
    pub fn calculate_slippage(&self) -> f64 {
        if self.amount_out == 0 {
            return 0.0;
        }

        let slippage =
            ((self.amount_out - self.minimum_amount_out) as f64 / self.amount_out as f64) * 10000.0;
        slippage
    }

    /// Get a human-readable description of the trade
    pub fn description(&self) -> String {
        format!(
            "Swap {} {} for {} {} via {} (Slippage: {:.2}%)",
            self.amount_in,
            self.source_mint,
            self.minimum_amount_out,
            self.destination_mint,
            self.dex,
            self.slippage_bps as f64 / 100.0
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DexType {
    Jupiter,
    Raydium,
    Orca,
    Unknown,
}

impl fmt::Display for DexType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DexType::Jupiter => write!(f, "Jupiter"),
            DexType::Raydium => write!(f, "Raydium"),
            DexType::Orca => write!(f, "Orca"),
            DexType::Unknown => write!(f, "Unknown"),
        }
    }
}
// g
pub mod program_ids {
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    pub fn jupiter_v6() -> Pubkey {
        Pubkey::from_str("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4")
            .expect("Invalid Jupiter V6 pubkey")
    }

    /// Raydium AMM V4
    pub fn raydium_v4() -> Pubkey {
        Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8")
            .expect("Invalid Raydium V4 pubkey")
    }

    /// Orca Whirlpool
    pub fn orca_whirlpool() -> Pubkey {
        Pubkey::from_str("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc")
            .expect("Invalid Orca Whirlpool pubkey")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// The target wallet address to monitor
    pub target_wallet: Pubkey,

    /// List of RPC endpoint URLs (for failover)
    pub rpc_endpoints: Vec<String>,

    /// WebSocket endpoint URL
    pub websocket_endpoint: String,

    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,

    /// Maximum number of reconnection attempts before giving up
    pub max_reconnect_attempts: u32,

    /// Whether to use commitment level "confirmed" (faster) or "finalized" (safer)
    pub use_confirmed_commitment: bool,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            target_wallet: Pubkey::default(),
            rpc_endpoints: vec!["https://api.mainnet-beta.solana.com".to_string()],
            websocket_endpoint: "wss://api.mainnet-beta.solana.com".to_string(),
            connection_timeout_secs: 30,
            max_reconnect_attempts: 5,
            use_confirmed_commitment: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slippage_calculation() {
        let signal = TradeSignal {
            signature: Signature::default(),
            timestamp: 0,
            dex: DexType::Jupiter,
            source_mint: Pubkey::default(),
            destination_mint: Pubkey::default(),
            amount_in: 1000000,
            amount_out: 1000000,
            minimum_amount_out: 990000, // 1% slippage
            slippage_bps: 100,
            priority_fee_lamports: 1000,
            accounts: vec![],
            trader: Pubkey::default(),
        };

        let slippage = signal.calculate_slippage();
        assert!((slippage - 100.0).abs() < 0.01); // Within 1% tolerance
    }
}
