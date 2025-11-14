use crate::decision;
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signature::Signature};

/// Represents a change in token balance
#[derive(Debug, Clone)]
pub struct BalanceDelta {
    /// The token mint address
    pub mint: Pubkey,

    /// Change in balance (negative = sold, positive = bought)
    pub delta: i64,

    /// Balance before transaction
    pub pre_balance: u64,

    /// Balance after transaction
    pub post_balance: u64,

    /// Token decimals
    pub decimals: u8,
}

impl BalanceDelta {
    /// Get the absolute amount that changed
    pub fn abs_amount(&self) -> u64 {
        self.delta.unsigned_abs()
    }

    /// Check if this represents a token being sold
    pub fn is_decrease(&self) -> bool {
        self.delta < 0
    }

    /// Check if this represents a token being bought
    pub fn is_increase(&self) -> bool {
        self.delta > 0
    }

    /// Get human-readable amount
    pub fn ui_amount(&self) -> f64 {
        self.abs_amount() as f64 / 10_f64.powi(self.decimals as i32)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalSwapSignal {
    /// Transaction signature
    pub signature: Signature,

    /// Block timestamp
    pub timestamp: i64,

    /// The wallet that made the swap
    pub trader: Pubkey,

    /// Type of swap detected
    pub swap_type: SwapType,

    /// Token that was sold (input)
    pub input_mint: Pubkey,

    /// Amount of input token sold
    pub input_amount: u64,

    /// Token that was bought (output)
    pub output_mint: Pubkey,

    /// Amount of output token received
    pub output_amount: u64,

    /// Intermediate tokens (for multi-hop swaps)
    pub intermediate_tokens: Vec<Pubkey>,

    /// Likely DEX used (optional, for logging only)
    pub likely_dex: Option<String>,
}

impl UniversalSwapSignal {
    /// Get Solscan URL for this transaction
    pub fn solscan_url(&self) -> String {
        format!("https://solscan.io/tx/{}", self.signature)
    }

    /// Get Solscan URL for the trader
    pub fn trader_solscan_url(&self) -> String {
        format!("https://solscan.io/account/{}", self.trader)
    }

    /// Get human-readable description
    pub fn description(&self) -> String {
        format!(
            "{} swap: {} {} → {} {}",
            self.swap_type,
            self.input_amount,
            self.input_mint,
            self.output_amount,
            self.output_mint
        )
    }

    pub fn direction(&self) -> crate::decision::TradeDirection {
        decision::detect_direction(&self.input_mint, &self.output_mint)
    }

    /// Check if this is a buy signal we should copy
    pub fn is_buy(&self) -> bool {
        matches!(
            self.direction(),
            crate::decision::TradeDirection::Buy { .. }
        )
    }

    /// Check if this is a sell signal we should skip
    pub fn is_sell(&self) -> bool {
        matches!(
            self.direction(),
            decision::TradeDirection::Sell { .. }
        )
    }

    /// Check if this is a token-to-token swap
    pub fn is_swap(&self) -> bool {
        matches!(
            self.direction(),
            decision::TradeDirection::Swap { .. }
        )
    }
}

/// Type of swap detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwapType {
    /// Simple swap: A → B
    Simple,

    /// Multi-hop swap: A → B → C
    MultiHop,
}

impl std::fmt::Display for SwapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwapType::Simple => write!(f, "Simple"),
            SwapType::MultiHop => write!(f, "Multi-hop"),
        }
    }
}

/// Classification of transaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    /// Swap transaction (copy this!)
    Swap,

    /// Multi-hop swap (copy this!)
    MultiHopSwap,

    /// Simple transfer
    Transfer,

    /// Receiving tokens
    Receive,

    /// Adding liquidity
    AddLiquidity,

    /// Removing liquidity
    RemoveLiquidity,

    /// Unknown transaction type
    Unknown,
}

impl TransactionType {
    /// Check if this transaction type should be copied
    pub fn should_copy(&self) -> bool {
        matches!(self, TransactionType::Swap | TransactionType::MultiHopSwap)
    }
}

impl std::fmt::Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionType::Swap => write!(f, "Swap"),
            TransactionType::MultiHopSwap => write!(f, "Multi-hop Swap"),
            TransactionType::Transfer => write!(f, "Transfer"),
            TransactionType::Receive => write!(f, "Receive"),
            TransactionType::AddLiquidity => write!(f, "Add Liquidity"),
            TransactionType::RemoveLiquidity => write!(f, "Remove Liquidity"),
            TransactionType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Token balance information
#[derive(Debug, Clone)]
pub struct TokenBalance {
    pub mint: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub decimals: u8,
}
