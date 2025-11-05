use crate::monitor::error::{MonitorError, MonitorResult};
use crate::types::{DexType, TradeSignal};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::UiInstruction;
use std::str::FromStr;
use tracing::{debug, warn};

pub fn parse_jupiter_swap(
    instructions: &[UiInstruction],
    account_keys: &[Pubkey],
    signature: Signature,
    timestamp: i64,
    priority_fee: u64,
    trader: Pubkey,
) -> MonitorResult<Option<TradeSignal>> {
    debug!("Parsing Jupiter swap");
    for instruction in instructions {
        if let Some(signal) = try_parse_instruction(
            instruction,
            account_keys,
            signature,
            timestamp,
            priority_fee,
            trader,
        )? {
            return Ok(Some(signal));
        }
    }

    Ok(None)
}

fn try_parse_instruction(
    instruction: &UiInstruction,
    account_keys: &[Pubkey],
    signature: Signature,
    timestamp: i64,
    priority_fee: u64,
    trader: Pubkey,
) -> MonitorResult<Option<TradeSignal>> {
    match instruction {
        UiInstruction::Compiled(compiled) => {
            // For compiled instructions, we have program_id_index and accounts indices
            let program_id = account_keys
                .get(compiled.program_id_index as usize)
                .copied()
                .ok_or_else(|| MonitorError::ParseError("Invalid program_id_index".to_string()))?;

            // Check if this is Jupiter
            let jupiter_id = crate::types::program_ids::jupiter_v6();
            if program_id != jupiter_id {
                return Ok(None);
            }

            // Extract account indices for token accounts
            // Jupiter account layout (approximate):
            // 0: Token program
            // 1: User source token account
            // 2: User destination token account
            // 3+: Various pool/vault accounts

            if compiled.accounts.len() < 3 {
                warn!("Not enough accounts for Jupiter swap");
                return Ok(None);
            }

            // Get source and destination token accounts
            let source_token_account = account_keys.get(compiled.accounts[1] as usize).copied();
            let dest_token_account = account_keys.get(compiled.accounts[2] as usize).copied();

            if source_token_account.is_none() || dest_token_account.is_none() {
                warn!("Could not extract token accounts");
                return Ok(None);
            }

            // Parse instruction data to extract amounts
            let data = bs58::decode(&compiled.data)
                .into_vec()
                .map_err(|e| MonitorError::ParseError(format!("Failed to decode data: {}", e)))?;

            // Jupiter instruction data format (simplified):
            // First 8 bytes: instruction discriminator
            // Next 8 bytes: amount_in (u64)
            // Next 8 bytes: minimum_amount_out (u64)

            if data.len() < 24 {
                warn!("Instruction data too short");
                return Ok(None);
            }

            let amount_in =
                u64::from_le_bytes(data[8..16].try_into().map_err(|_| {
                    MonitorError::ParseError("Failed to parse amount_in".to_string())
                })?);

            let minimum_amount_out = u64::from_le_bytes(data[16..24].try_into().map_err(|_| {
                MonitorError::ParseError("Failed to parse minimum_amount_out".to_string())
            })?);

            // For Jupiter, we need to get the actual token mints
            // This requires additional RPC calls or parsing token account data
            // For now, we'll use placeholder mints
            // TODO: Fetch token account info to get actual mints

            let source_mint = source_token_account.unwrap(); // Placeholder
            let dest_mint = dest_token_account.unwrap(); // Placeholder

            // Calculate slippage in basis points
            let slippage_bps = if minimum_amount_out > 0 {
                let expected_out = amount_in; // This should be calculated from price
                ((expected_out.saturating_sub(minimum_amount_out)) * 10000 / expected_out.max(1))
                    as u16
            } else {
                0
            };

            let trade_signal = TradeSignal {
                signature,
                timestamp,
                dex: DexType::Jupiter,
                source_mint,
                destination_mint: dest_mint,
                amount_in,
                amount_out: amount_in, // Should be calculated
                minimum_amount_out,
                slippage_bps,
                priority_fee_lamports: priority_fee,
                accounts: account_keys.to_vec(),
                trader,
            };

            debug!(
                "Parsed Jupiter swap: {} -> {}",
                amount_in, minimum_amount_out
            );
            Ok(Some(trade_signal))
        }
        _ => Ok(None),
    }
}
