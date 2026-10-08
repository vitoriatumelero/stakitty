use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::StakittyError,
    stake::{read_stake, StakeCtx},
    state::{Pool, ValidatorEntry},
};

/// Permissionless crank, run after an epoch boundary: merges an activated transient into the
/// main stake, and returns cooled-down stake (principal + rewards) to the reserve.
#[derive(Accounts)]
pub struct SettleStake<'info> {
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(
        has_one = pool,
        has_one = vote_account,
        seeds = [VALIDATOR_SEED, pool.key().as_ref(), vote_account.key().as_ref()],
        bump = validator_entry.bump
    )]
    pub validator_entry: Account<'info, ValidatorEntry>,
    /// CHECK: only used as a PDA seed, bound to the entry by `has_one`.
    pub vote_account: UncheckedAccount<'info>,
    #[account(mut, seeds = [RESERVE_SEED, pool.key().as_ref()], bump = pool.reserve_bump)]
    pub reserve: SystemAccount<'info>,
    /// CHECK: PDA; state read by `read_stake`.
    #[account(
        mut,
        seeds = [STAKE_SEED, pool.key().as_ref(), vote_account.key().as_ref()],
        bump = validator_entry.stake_bump
    )]
    pub stake_account: UncheckedAccount<'info>,
    /// CHECK: PDA; state read by `read_stake`.
    #[account(
        mut,
        seeds = [TRANSIENT_SEED, pool.key().as_ref(), vote_account.key().as_ref()],
        bump = validator_entry.transient_bump
    )]
    pub transient_stake: UncheckedAccount<'info>,
    pub clock: Sysvar<'info, Clock>,
    /// CHECK: address-checked sysvar.
    #[account(address = STAKE_HISTORY_ID)]
    pub stake_history: UncheckedAccount<'info>,
    /// CHECK: address-checked CPI target.
    #[account(address = STAKE_PROGRAM_ID)]
    pub stake_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handle_settle_stake(ctx: Context<SettleStake>) -> Result<()> {
    let accounts = &ctx.accounts;
    let epoch = accounts.clock.epoch;
    let pool_key = accounts.pool.key();
    let reserve_seeds: &[&[u8]] = &[
        RESERVE_SEED,
        pool_key.as_ref(),
        &[accounts.pool.reserve_bump],
    ];
    let reserve_info = accounts.reserve.to_account_info();
    let clock_info = accounts.clock.to_account_info();
    let history_info = accounts.stake_history.to_account_info();
    let system_info = accounts.system_program.to_account_info();
    let cpi = StakeCtx {
        reserve: &reserve_info,
        reserve_seeds,
        clock: &clock_info,
        stake_history: &history_info,
        system_program: &system_info,
    };
    let main = accounts.stake_account.to_account_info();
    let transient = accounts.transient_stake.to_account_info();

    let mut merged: u64 = 0;
    let mut returned: u64 = 0;
    if let Some(info) = read_stake(&transient)? {
        if info.is_deactivating() {
            require!(
                epoch > info.deactivation_epoch,
                StakittyError::NothingToSettle
            );
            returned = cpi.withdraw_all(&transient)?;
        } else {
            require!(
                epoch > info.activation_epoch,
                StakittyError::NothingToSettle
            );
            merged = transient.lamports();
            cpi.merge(&main, &transient)?;
        }
    }
    if let Some(info) = read_stake(&main)? {
        if info.is_deactivating() && epoch > info.deactivation_epoch {
            returned = returned
                .checked_add(cpi.withdraw_all(&main)?)
                .ok_or(StakittyError::Overflow)?;
        }
    }
    require!(merged > 0 || returned > 0, StakittyError::NothingToSettle);

    emit!(StakeSettled {
        vote_account: accounts.vote_account.key(),
        merged_lamports: merged,
        returned_lamports: returned,
        epoch,
    });
    Ok(())
}

#[event]
pub struct StakeSettled {
    pub vote_account: Pubkey,
    pub merged_lamports: u64,
    pub returned_lamports: u64,
    pub epoch: u64,
}
