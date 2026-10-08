use anchor_lang::prelude::*;

use crate::error::StakittyError;

#[account]
#[derive(InitSpace)]
pub struct Pool {
    pub authority: Pubkey,
    /// Sum of every user's principal. Prize = assets - total_principal - rent, never raw lamports.
    pub total_principal: u64,
    pub min_deposit: u64,
    /// Pool-wide sum of principal x seconds; rounds compare user weights against it.
    pub cumulative_weight: u128,
    pub last_update_ts: i64,
    /// Index of the season currently open for sponsorship.
    pub current_season: u32,
    pub season_length_epochs: u64,
    pub validator_count: u8,
    /// Lamports owed to open withdraw tickets; earmarked in the reserve, not spendable.
    pub pending_withdrawals: u64,
    pub bump: u8,
    pub reserve_bump: u8,
    pub prize_bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct UserAccount {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub principal: u64,
    /// Principal x seconds held. A deposit made just before a draw adds almost no weight.
    pub cumulative_weight: u128,
    pub last_update_ts: i64,
    pub bump: u8,
}

/// Principal already deducted from the user, paid once the reserve holds enough liquid SOL.
#[account]
#[derive(InitSpace)]
pub struct WithdrawTicket {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub amount: u64,
    pub requested_epoch: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct ValidatorEntry {
    pub pool: Pubkey,
    pub vote_account: Pubkey,
    /// Lifetime sponsorship paid, across all seasons.
    pub total_paid: u64,
    /// `season.index + 1` of the last season whose weights were applied; 0 = never.
    pub rebalanced_through: u32,
    pub stake_bump: u8,
    pub transient_bump: u8,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum SeasonStatus {
    Open,
    Closed,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub struct ValidatorWeight {
    pub vote_account: Pubkey,
    pub weight_bps: u16,
}

#[account]
#[derive(InitSpace)]
pub struct Season {
    pub pool: Pubkey,
    pub index: u32,
    pub start_epoch: u64,
    pub end_epoch: u64,
    /// Sponsor SOL paid in this season. It sets season+1 stake and funds season+1 prizes.
    pub total_paid: u64,
    pub sponsor_count: u8,
    pub status: SeasonStatus,
    /// Final capped weights, written once by `close_season`.
    #[max_len(16)]
    pub weights: Vec<ValidatorWeight>,
    /// Share of principal left liquid: the fixed 15%, plus cap excess and rounding.
    pub reserve_bps: u16,
    /// `pool.total_principal` frozen at the first rebalance, so every validator sizes from one base.
    pub stake_base: Option<u64>,
    pub bump: u8,
}

impl Season {
    pub fn weight_of(&self, vote_account: &Pubkey) -> u16 {
        self.weights
            .iter()
            .find(|w| &w.vote_account == vote_account)
            .map_or(0, |w| w.weight_bps)
    }
}

#[account]
#[derive(InitSpace)]
pub struct SeasonSponsorship {
    pub season: Pubkey,
    pub vote_account: Pubkey,
    pub amount: u64,
    pub bump: u8,
}

/// `cumulative + balance * (now - last_ts)`. A clock that moves backwards adds nothing.
pub fn cumulative_at(cumulative: u128, balance: u64, last_ts: i64, now: i64) -> Result<u128> {
    let elapsed =
        u128::try_from(now.saturating_sub(last_ts).max(0)).map_err(|_| StakittyError::Overflow)?;
    u128::from(balance)
        .checked_mul(elapsed)
        .and_then(|added| cumulative.checked_add(added))
        .ok_or(StakittyError::Overflow.into())
}

impl Pool {
    /// Reserve lamports free for withdrawals and staking: minus the rent floor and open tickets.
    pub fn spendable(&self, reserve_lamports: u64, rent_floor: u64) -> u64 {
        reserve_lamports
            .saturating_sub(rent_floor)
            .saturating_sub(self.pending_withdrawals)
    }

    pub fn accrue(&mut self, now: i64) -> Result<()> {
        self.cumulative_weight = cumulative_at(
            self.cumulative_weight,
            self.total_principal,
            self.last_update_ts,
            now,
        )?;
        self.last_update_ts = self.last_update_ts.max(now);
        Ok(())
    }
}

impl UserAccount {
    pub fn accrue(&mut self, now: i64) -> Result<()> {
        self.cumulative_weight = cumulative_at(
            self.cumulative_weight,
            self.principal,
            self.last_update_ts,
            now,
        )?;
        self.last_update_ts = self.last_update_ts.max(now);
        Ok(())
    }
}
