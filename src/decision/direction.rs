use solana_sdk::pubkey::Pubkey;

pub struct Stablecoins;

impl Stablecoins {
    // USDC - most common
    pub const USDC: &'static str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

    // USDT
    pub const USDT: &'static str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";

    pub const USD1: &'static str = "USD1ttGY1N17NEEHLmELoaybftRBUSErhqYiQzvEmuB";

    pub fn is_stablecoin(mint: &Pubkey) -> bool {
        let mint_str = mint.to_string();

        mint_str == Self::USDC || mint_str == Self::USDT || mint_str == Self::USD1
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TradeDirection {
    Buy {
        token: Pubkey,   // The token being bought
        payment: Pubkey, // The stablecoin used to buy
    },

    Sell {
        token: Pubkey,    // The token being sold
        receives: Pubkey, // The stablecoin received
    },

    Swap {
        from_token: Pubkey,
        to_token: Pubkey,
    },
}

pub fn detect_direction(input_mint: &Pubkey, output_mint: &Pubkey) -> TradeDirection {
    let input_is_stable = Stablecoins::is_stablecoin(input_mint);
    let output_is_stable = Stablecoins::is_stablecoin(output_mint);

    match (input_is_stable, output_is_stable) {
        (true, false) => TradeDirection::Buy {
            token: *output_mint,
            payment: *input_mint,
        },

        (false, true) => TradeDirection::Sell {
            token: *output_mint,
            receives: *input_mint,
        },
        _ => TradeDirection::Swap {
            from_token: *input_mint,
            to_token: *output_mint,
        },
    }
}
