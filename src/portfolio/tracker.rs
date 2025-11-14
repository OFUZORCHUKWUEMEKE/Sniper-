use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Represents a single position in a token
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Create a new position
    pub fn new(
        token: Pubkey,
        amount: u64,
        payment_token: Pubkey,
        cost_basis: u64,
        entry_signature: String,
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

    /// Calculate unrealized P&L
    pub fn calculate_pnl(&self, current_value: u64) -> PnL {
        let profit = current_value as i64 - self.cost_basis as i64;
        let profit_percent = if self.cost_basis > 0 {
            (profit as f64 / self.cost_basis as f64) * 100.0
        } else {
            0.0
        };

        PnL {
            absolute: profit,
            percent: profit_percent,
        }
    }

    /// How long have we held this position (in seconds)
    pub fn holding_duration(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now - self.entry_time
    }

    /// Add more to the position (average up/down)
    pub fn add(&mut self, amount: u64, cost: u64) {
        let new_total_cost = self.cost_basis + cost;
        let new_total_amount = self.amount + amount;

        self.amount = new_total_amount;
        self.cost_basis = new_total_cost;

        // Recalculate average price
        self.avg_entry_price = if new_total_amount > 0 {
            new_total_cost as f64 / new_total_amount as f64
        } else {
            0.0
        };
    }

    /// Remove from position (partial or full exit)
    pub fn reduce(&mut self, amount: u64) -> Result<u64, String> {
        if amount > self.amount {
            return Err(format!(
                "Cannot reduce by {} - only have {}",
                amount, self.amount
            ));
        }

        let exit_percentage = amount as f64 / self.amount as f64;
        let cost_removed = (self.cost_basis as f64 * exit_percentage) as u64;

        self.amount -= amount;
        self.cost_basis -= cost_removed;

        Ok(cost_removed)
    }

    /// Is this position empty (fully exited)?
    pub fn is_empty(&self) -> bool {
        self.amount == 0
    }
}

/// Profit and Loss calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnL {
    pub absolute: i64, // Profit/loss in payment token units
    pub percent: f64,  // Profit/loss as percentage
}

/// Portfolio tracker - manages all positions
#[derive(Serialize, Deserialize)]
pub struct PortfolioTracker {
    /// All active positions (token mint -> Position)
    positions: HashMap<Pubkey, Position>,

    /// Closed positions history (for tracking realized P&L)
    closed_positions: Vec<ClosedPosition>,

    /// Total realized profit/loss
    total_realized_pnl: i64,
}

/// A closed (exited) position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedPosition {
    pub position: Position,
    pub exit_time: u64,
    pub exit_signature: String,
    pub exit_value: u64,
    pub realized_pnl: i64,
    pub realized_pnl_percent: f64,
}

impl PortfolioTracker {
    /// Create a new portfolio tracker
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            closed_positions: Vec::new(),
            total_realized_pnl: 0,
        }
    }

    /// Check if we have a position in this token
    pub fn has_position(&self, token: &Pubkey) -> bool {
        self.positions.contains_key(token)
    }

    /// Get a position if it exists
    pub fn get_position(&self, token: &Pubkey) -> Option<&Position> {
        self.positions.get(token)
    }

    /// Get all active positions
    pub fn get_all_positions(&self) -> Vec<&Position> {
        self.positions.values().collect()
    }

    /// Add a new position (BUY)
    pub fn open_position(
        &mut self,
        token: Pubkey,
        amount: u64,
        payment_token: Pubkey,
        cost: u64,
        signature: String,
    ) {
        if let Some(existing) = self.positions.get_mut(&token) {
            // Already have this token - add to position
            existing.add(amount, cost);
            tracing::info!(
                "➕ Added to position: {} tokens (new total: {})",
                amount,
                existing.amount
            );
        } else {
            // New position
            let position = Position::new(token, amount, payment_token, cost, signature);
            tracing::info!("🆕 Opened new position: {} tokens @ {} cost", amount, cost);
            self.positions.insert(token, position);
        }
    }

    /// Reduce or close a position (SELL)
    pub fn close_position(
        &mut self,
        token: &Pubkey,
        amount: u64,
        exit_value: u64,
        signature: String,
    ) -> Result<ClosedPosition, String> {
        let position = self
            .positions
            .get_mut(token)
            .ok_or_else(|| format!("No position found for token {}", token))?;

        // Calculate what portion we're closing
        let is_full_exit = amount >= position.amount;

        if is_full_exit {
            // Full exit - remove from active positions
            let closed_position = self.finalize_close(token, exit_value, signature)?;
            Ok(closed_position)
        } else {
            // Partial exit
            let cost_removed = position.reduce(amount)?;
            let partial_pnl = exit_value as i64 - cost_removed as i64;
            let pnl_percent = (partial_pnl as f64 / cost_removed as f64) * 100.0;

            tracing::info!(
                "📉 Partial exit: {} tokens, P&L: {} ({:.2}%)",
                amount,
                partial_pnl,
                pnl_percent
            );

            // Track partial exit as a closed position
            let exit_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let closed = ClosedPosition {
                position: position.clone(),
                exit_time,
                exit_signature: signature,
                exit_value,
                realized_pnl: partial_pnl,
                realized_pnl_percent: pnl_percent,
            };

            self.total_realized_pnl += partial_pnl;
            self.closed_positions.push(closed.clone());

            Ok(closed)
        }
    }

    /// Finalize a full position close
    fn finalize_close(
        &mut self,
        token: &Pubkey,
        exit_value: u64,
        signature: String,
    ) -> Result<ClosedPosition, String> {
        let position = self
            .positions
            .remove(token)
            .ok_or_else(|| format!("No position found for token {}", token))?;

        let realized_pnl = exit_value as i64 - position.cost_basis as i64;
        let realized_pnl_percent = if position.cost_basis > 0 {
            (realized_pnl as f64 / position.cost_basis as f64) * 100.0
        } else {
            0.0
        };

        let exit_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let closed_position = ClosedPosition {
            position: position.clone(),
            exit_time,
            exit_signature: signature,
            exit_value,
            realized_pnl,
            realized_pnl_percent,
        };

        self.total_realized_pnl += realized_pnl;
        self.closed_positions.push(closed_position.clone());

        tracing::info!(
            "🏁 CLOSED POSITION: Token {}, P&L: {} ({:.2}%), Held for {}s",
            token,
            realized_pnl,
            realized_pnl_percent,
            closed_position.exit_time - position.entry_time
        );

        Ok(closed_position)
    }

    /// Get portfolio statistics
    pub fn get_stats(&self) -> PortfolioStats {
        let active_positions_count = self.positions.len();
        let closed_positions_count = self.closed_positions.len();

        // Calculate total invested (current positions)
        let total_invested: u64 = self.positions.values().map(|p| p.cost_basis).sum();

        // Win rate
        let winning_trades = self
            .closed_positions
            .iter()
            .filter(|cp| cp.realized_pnl > 0)
            .count();

        let win_rate = if closed_positions_count > 0 {
            (winning_trades as f64 / closed_positions_count as f64) * 100.0
        } else {
            0.0
        };

        PortfolioStats {
            active_positions: active_positions_count,
            closed_positions: closed_positions_count,
            total_invested,
            total_realized_pnl: self.total_realized_pnl,
            win_rate,
        }
    }

    /// Get closed positions history
    pub fn get_history(&self) -> &[ClosedPosition] {
        &self.closed_positions
    }

    pub fn save(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("💾 Saving portfolio to {}", path);

        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;

        tracing::info!("✅ Portfolio saved successfully");
        Ok(())
    }

    /// Load portfolio from a JSON file
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        tracing::info!("📂 Loading portfolio from {}", path);

        if !Path::new(path).exists() {
            tracing::warn!("⚠️  No saved portfolio found, starting fresh");
            return Ok(Self::new());
        }

        let json = fs::read_to_string(path)?;
        let portfolio: Self = serde_json::from_str(&json)?;

        tracing::info!("✅ Portfolio loaded successfully");
        tracing::info!("   Active positions: {}", portfolio.positions.len());
        tracing::info!("   Closed positions: {}", portfolio.closed_positions.len());
        tracing::info!("   Total realized P&L: {}", portfolio.total_realized_pnl);

        Ok(portfolio)
    }

    /// Save portfolio with error handling
    pub fn save_safe(&self, path: &str) {
        if let Err(e) = self.save(path) {
            tracing::error!("❌ Failed to save portfolio: {}", e);
        }
    }
}

/// Portfolio statistics
#[derive(Debug, Clone)]
pub struct PortfolioStats {
    pub active_positions: usize,
    pub closed_positions: usize,
    pub total_invested: u64,
    pub total_realized_pnl: i64,
    pub win_rate: f64,
}

impl Default for PortfolioTracker {
    fn default() -> Self {
        Self::new()
    }
}
