use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::{
    constants::*,
    error::StakittyError,
    state::{Pool, WithdrawTicket},
};

#[derive(Accounts)]
pub struct ClaimWithdraw<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [RESERVE_SEED, pool.key().as_ref()], bump = pool.reserve_bump)]
    pub reserve: SystemAccount<'info>,
    #[account(
        mut,
        close = owner,
        has_one = owner @ StakittyError::Unauthorized,
        has_one = pool,
        seeds = [TICKET_SEED, pool.key().as_ref(), owner.key().as_ref()],
        bump = ticket.bump
    )]
    pub ticket: Account<'info, WithdrawTicket>,
    pub system_program: Program<'info, System>,
}

pub fn handle_claim_withdraw(ctx: Context<ClaimWithdraw>) -> Result<()> {
    let amount = ctx.accounts.ticket.amount;
    // Tickets are paid from earmarked lamports, so only the rent floor is excluded here.
    let rent_floor = Rent::get()?.minimum_balance(0);
    let liquid = ctx.accounts.reserve.lamports().saturating_sub(rent_floor);
    require!(amount <= liquid, StakittyError::InsufficientLiquidity);

    let pool = &mut ctx.accounts.pool;
    pool.pending_withdrawals = pool
        .pending_withdrawals
        .checked_sub(amount)
        .ok_or(StakittyError::Overflow)?;

    let pool_key = pool.key();
    let reserve_seeds: &[&[u8]] = &[RESERVE_SEED, pool_key.as_ref(), &[pool.reserve_bump]];
    transfer(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.key(),
            Transfer {
                from: ctx.accounts.reserve.to_account_info(),
                to: ctx.accounts.owner.to_account_info(),
            },
            &[reserve_seeds],
        ),
        amount,
    )?;

    emit!(WithdrawClaimed {
        user: ctx.accounts.owner.key(),
        amount,
        pending_withdrawals: pool.pending_withdrawals,
        epoch: Clock::get()?.epoch,
    });
    Ok(())
}

#[event]
pub struct WithdrawClaimed {
    pub user: Pubkey,
    pub amount: u64,
    pub pending_withdrawals: u64,
    pub epoch: u64,
}
