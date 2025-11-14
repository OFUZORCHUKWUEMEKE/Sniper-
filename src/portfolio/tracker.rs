use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Position {
    /// The token mint address
    pub token: Pubkey,

    /// Amount we own (in smallest unit)
    pub amount: u64,

    /// What we paid for it (stablecoin mint)
    pub payment_token: Pubkey,

    /// How much we spent (in smallest unit)
    pub cost_basis: u64,

    /// When we entered the position
    pub entry_time: u64,

    /// The transaction signature when we bought
    pub entry_signature: String,

    /// Average entry price (cost per token)
    pub avg_entry_price: f64,
}

impl Position {
    pub fn new(
        token: Pubkey,
        amount: u64,
        payment_token: Pubkey,
        cost_basis: u64,
        entry_time: u64,
        entry_signature: String,
        avg_entry_price: f64,
    ) -> Self {
        let entry_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let avg_entry_price = if amount > 0 {
            cost_basis as f64 / amount as f64
        } else {
            0.0
        };

        Self {
            token,
            amount,
            payment_token,
            cost_basis,
            entry_time,
            entry_signature,
            avg_entry_price,
        }
    }
}
