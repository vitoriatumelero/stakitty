use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::{
    constants::*,
    error::StakittyError,
    state::{Pool, UserAccount},
};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED, &pool.pool_id.to_le_bytes()], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [RESERVE_SEED, pool.key().as_ref()], bump = pool.reserve_bump)]
    pub reserve: SystemAccount<'info>,
    #[account(
        mut,
        has_one = owner @ StakittyError::Unauthorized,
        has_one = pool,
        seeds = [USER_SEED, pool.key().as_ref(), owner.key().as_ref()],
        bump = user_account.bump
    )]
    pub user_account: Account<'info, UserAccount>,
    pub system_program: Program<'info, System>,
}

pub fn handle_withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
    require!(amount > 0, StakittyError::ZeroAmount);

    let now = Clock::get()?.unix_timestamp;
    let pool = &mut ctx.accounts.pool;
    let user = &mut ctx.accounts.user_account;
    pool.accrue(now)?;
    user.accrue(now)?;

    user.principal = user
        .principal
        .checked_sub(amount)
        .ok_or(StakittyError::InsufficientPrincipal)?;
    pool.total_principal = pool
        .total_principal
        .checked_sub(amount)
        .ok_or(StakittyError::Overflow)?;

    // Larger requests go through `request_withdraw`; lamports owed to tickets are off limits.
    let rent_floor = Rent::get()?.minimum_balance(0);
    let liquid = pool.spendable(ctx.accounts.reserve.lamports(), rent_floor);
    require!(amount <= liquid, StakittyError::InsufficientLiquidity);

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

    emit!(Withdrawn {
        pool: user.pool,
        user: user.owner,
        amount,
        principal: user.principal,
        total_principal: pool.total_principal,
        timestamp: now,
    });
    Ok(())
}

#[event]
pub struct Withdrawn {
    /// Several pools share the program; the round builder filters on this.
    pub pool: Pubkey,
    pub user: Pubkey,
    pub amount: u64,
    pub principal: u64,
    pub total_principal: u64,
    pub timestamp: i64,
}
