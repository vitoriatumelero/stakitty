use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::{constants::*, error::StakittyError, state::Pool};

#[derive(Accounts)]
pub struct WithdrawFees<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(seeds = [POOL_SEED], bump = pool.bump, has_one = authority @ StakittyError::Unauthorized)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [FEE_SEED, pool.key().as_ref()], bump = pool.fee_bump)]
    pub fee_vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handle_withdraw_fees(ctx: Context<WithdrawFees>, amount: u64) -> Result<()> {
    require!(amount > 0, StakittyError::ZeroAmount);
    let rent_floor = Rent::get()?.minimum_balance(0);
    let available = ctx.accounts.fee_vault.lamports().saturating_sub(rent_floor);
    require!(amount <= available, StakittyError::InsufficientLiquidity);

    let pool_key = ctx.accounts.pool.key();
    let fee_seeds: &[&[u8]] = &[FEE_SEED, pool_key.as_ref(), &[ctx.accounts.pool.fee_bump]];
    transfer(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.key(),
            Transfer {
                from: ctx.accounts.fee_vault.to_account_info(),
                to: ctx.accounts.authority.to_account_info(),
            },
            &[fee_seeds],
        ),
        amount,
    )?;

    emit!(FeesWithdrawn {
        authority: ctx.accounts.authority.key(),
        amount,
    });
    Ok(())
}

#[event]
pub struct FeesWithdrawn {
    pub authority: Pubkey,
    pub amount: u64,
}
