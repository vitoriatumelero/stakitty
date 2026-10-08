use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::StakittyError,
    state::{Pool, UserAccount, WithdrawTicket},
};

/// For withdrawals larger than the liquid reserve. Principal (and its prize weight) leaves the
/// pool now, so the next rebalance sizes stake without it; `claim_withdraw` pays out once the
/// unstaked SOL is back in the reserve. One open ticket per user.
#[derive(Accounts)]
pub struct RequestWithdraw<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(
        mut,
        has_one = owner @ StakittyError::Unauthorized,
        has_one = pool,
        seeds = [USER_SEED, pool.key().as_ref(), owner.key().as_ref()],
        bump = user_account.bump
    )]
    pub user_account: Account<'info, UserAccount>,
    #[account(
        init,
        payer = owner,
        space = WithdrawTicket::DISCRIMINATOR.len() + WithdrawTicket::INIT_SPACE,
        seeds = [TICKET_SEED, pool.key().as_ref(), owner.key().as_ref()],
        bump
    )]
    pub ticket: Account<'info, WithdrawTicket>,
    pub system_program: Program<'info, System>,
}

pub fn handle_request_withdraw(ctx: Context<RequestWithdraw>, amount: u64) -> Result<()> {
    require!(amount > 0, StakittyError::ZeroAmount);

    let clock = Clock::get()?;
    let pool = &mut ctx.accounts.pool;
    let user = &mut ctx.accounts.user_account;
    pool.accrue(clock.unix_timestamp)?;
    user.accrue(clock.unix_timestamp)?;

    user.principal = user
        .principal
        .checked_sub(amount)
        .ok_or(StakittyError::InsufficientPrincipal)?;
    pool.total_principal = pool
        .total_principal
        .checked_sub(amount)
        .ok_or(StakittyError::Overflow)?;
    pool.pending_withdrawals = pool
        .pending_withdrawals
        .checked_add(amount)
        .ok_or(StakittyError::Overflow)?;

    let ticket = &mut ctx.accounts.ticket;
    ticket.owner = user.owner;
    ticket.pool = pool.key();
    ticket.amount = amount;
    ticket.requested_epoch = clock.epoch;
    ticket.bump = ctx.bumps.ticket;

    emit!(WithdrawRequested {
        user: user.owner,
        amount,
        principal: user.principal,
        pending_withdrawals: pool.pending_withdrawals,
        timestamp: clock.unix_timestamp,
        epoch: clock.epoch,
    });
    Ok(())
}

#[event]
pub struct WithdrawRequested {
    pub user: Pubkey,
    pub amount: u64,
    pub principal: u64,
    pub pending_withdrawals: u64,
    /// Same clock the weight accumulators use; the round builder replays it.
    pub timestamp: i64,
    pub epoch: u64,
}
