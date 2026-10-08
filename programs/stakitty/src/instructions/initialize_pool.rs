use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::{
    constants::*,
    error::StakittyError,
    state::{Pool, Season, SeasonStatus},
};

/// `pool_id` is part of the pool seed, so a new id starts a fresh pool (e.g. to change
/// parameters on a test cluster) without touching an existing one.
#[derive(Accounts)]
#[instruction(pool_id: u16)]
pub struct InitializePool<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = Pool::DISCRIMINATOR.len() + Pool::INIT_SPACE,
        seeds = [POOL_SEED, &pool_id.to_le_bytes()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    /// System-owned PDA holding liquid SOL, so it can later fund stake accounts via CPI.
    #[account(mut, seeds = [RESERVE_SEED, pool.key().as_ref()], bump)]
    pub reserve: SystemAccount<'info>,
    /// Sponsor SOL only. Separate from the reserve so principal withdrawals cannot reach it.
    #[account(mut, seeds = [PRIZE_SEED, pool.key().as_ref()], bump)]
    pub prize_vault: SystemAccount<'info>,
    /// Protocol share of staking yield. Admin-only withdrawals.
    #[account(mut, seeds = [FEE_SEED, pool.key().as_ref()], bump)]
    pub fee_vault: SystemAccount<'info>,
    #[account(
        init,
        payer = authority,
        space = Season::DISCRIMINATOR.len() + Season::INIT_SPACE,
        seeds = [SEASON_SEED, pool.key().as_ref(), &0u32.to_le_bytes()],
        bump
    )]
    pub first_season: Account<'info, Season>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_pool(
    ctx: Context<InitializePool>,
    pool_id: u16,
    min_deposit: u64,
    season_length_epochs: u64,
) -> Result<()> {
    require!(min_deposit > 0, StakittyError::ZeroAmount);
    require!(season_length_epochs > 0, StakittyError::InvalidSeasonLength);

    let clock = Clock::get()?;
    let pool = &mut ctx.accounts.pool;
    pool.authority = ctx.accounts.authority.key();
    pool.pool_id = pool_id;
    pool.total_principal = 0;
    pool.min_deposit = min_deposit;
    pool.cumulative_weight = 0;
    pool.last_update_ts = clock.unix_timestamp;
    pool.current_season = 0;
    pool.season_length_epochs = season_length_epochs;
    pool.validator_count = 0;
    pool.pending_withdrawals = 0;
    pool.realized_yield = 0;
    pool.prize_available = 0;
    pool.committed_prizes = 0;
    pool.fee_bump = ctx.bumps.fee_vault;
    pool.bump = ctx.bumps.pool;
    pool.reserve_bump = ctx.bumps.reserve;
    pool.prize_bump = ctx.bumps.prize_vault;

    let season = &mut ctx.accounts.first_season;
    season.pool = pool.key();
    season.index = 0;
    season.start_epoch = clock.epoch;
    season.end_epoch = clock
        .epoch
        .checked_add(season_length_epochs)
        .ok_or(StakittyError::Overflow)?;
    season.start_ts = clock.unix_timestamp;
    season.start_pool_weight = 0;
    season.end_ts = 0;
    season.end_pool_weight = 0;
    season.total_paid = 0;
    season.sponsor_count = 0;
    season.status = SeasonStatus::Open;
    season.weights = Vec::new();
    season.reserve_bps = BPS_DENOMINATOR;
    season.stake_base = None;
    season.bump = ctx.bumps.first_season;

    // Make the vaults rent-exempt up front; these lamports are never principal, prize or fee.
    let rent_floor = Rent::get()?.minimum_balance(0);
    for vault in [
        &ctx.accounts.reserve,
        &ctx.accounts.prize_vault,
        &ctx.accounts.fee_vault,
    ] {
        let missing = rent_floor.saturating_sub(vault.lamports());
        if missing > 0 {
            transfer(
                CpiContext::new(
                    ctx.accounts.system_program.key(),
                    Transfer {
                        from: ctx.accounts.authority.to_account_info(),
                        to: vault.to_account_info(),
                    },
                ),
                missing,
            )?;
        }
    }
    Ok(())
}
