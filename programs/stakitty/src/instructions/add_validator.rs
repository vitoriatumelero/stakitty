use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::StakittyError,
    stake::vote_withdrawer,
    state::{Pool, ValidatorEntry},
};

#[derive(Accounts)]
pub struct AddValidator<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED, &pool.pool_id.to_le_bytes()], bump = pool.bump, has_one = authority @ StakittyError::Unauthorized)]
    pub pool: Account<'info, Pool>,
    /// CHECK: owner and layout validated by `vote_withdrawer`.
    pub vote_account: UncheckedAccount<'info>,
    #[account(
        init,
        payer = authority,
        space = ValidatorEntry::DISCRIMINATOR.len() + ValidatorEntry::INIT_SPACE,
        seeds = [VALIDATOR_SEED, pool.key().as_ref(), vote_account.key().as_ref()],
        bump
    )]
    pub validator_entry: Account<'info, ValidatorEntry>,
    pub system_program: Program<'info, System>,
}

pub fn handle_add_validator(ctx: Context<AddValidator>) -> Result<()> {
    vote_withdrawer(&ctx.accounts.vote_account)?;

    let pool = &mut ctx.accounts.pool;
    require!(
        pool.validator_count < MAX_VALIDATORS,
        StakittyError::ValidatorListFull
    );
    pool.validator_count = pool
        .validator_count
        .checked_add(1)
        .ok_or(StakittyError::Overflow)?;

    let pool_key = pool.key();
    let vote_key = ctx.accounts.vote_account.key();
    let (_, stake_bump) = Pubkey::find_program_address(
        &[STAKE_SEED, pool_key.as_ref(), vote_key.as_ref()],
        &crate::ID,
    );
    let (_, transient_bump) = Pubkey::find_program_address(
        &[TRANSIENT_SEED, pool_key.as_ref(), vote_key.as_ref()],
        &crate::ID,
    );

    let entry = &mut ctx.accounts.validator_entry;
    entry.pool = pool_key;
    entry.vote_account = vote_key;
    entry.total_paid = 0;
    entry.rebalanced_through = 0;
    entry.stake_basis = 0;
    entry.stake_bump = stake_bump;
    entry.transient_bump = transient_bump;
    entry.bump = ctx.bumps.validator_entry;
    Ok(())
}
