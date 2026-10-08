use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::StakittyError,
    state::{Pool, Round, RoundStatus},
};

/// Permissionless after the claim window: the unclaimed prize goes back to the next round.
#[derive(Accounts)]
pub struct ExpirePrize<'info> {
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(
        mut,
        has_one = pool,
        seeds = [ROUND_SEED, pool.key().as_ref(), &round.season_index.to_le_bytes()],
        bump = round.bump,
        constraint = round.status == RoundStatus::Settled @ StakittyError::InvalidRoundState
    )]
    pub round: Account<'info, Round>,
}

pub fn handle_expire_prize(ctx: Context<ExpirePrize>) -> Result<()> {
    let epoch = Clock::get()?.epoch;
    let round = &mut ctx.accounts.round;
    let deadline = round
        .settled_epoch
        .checked_add(PRIZE_CLAIM_EPOCHS)
        .ok_or(StakittyError::Overflow)?;
    require!(epoch > deadline, StakittyError::ClaimWindowOpen);
    round.status = RoundStatus::Expired;

    let pool = &mut ctx.accounts.pool;
    pool.committed_prizes = pool
        .committed_prizes
        .checked_sub(round.prize)
        .ok_or(StakittyError::Overflow)?;
    pool.prize_available = pool
        .prize_available
        .checked_add(round.prize)
        .ok_or(StakittyError::Overflow)?;

    emit!(PrizeExpired {
        season: round.season_index,
        prize: round.prize,
        prize_available: pool.prize_available,
        epoch,
    });
    Ok(())
}

#[event]
pub struct PrizeExpired {
    pub season: u32,
    pub prize: u64,
    pub prize_available: u64,
    pub epoch: u64,
}
