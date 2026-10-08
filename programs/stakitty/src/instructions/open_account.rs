use anchor_lang::prelude::*;

use crate::{
    constants::*,
    state::{Pool, UserAccount},
};

#[derive(Accounts)]
pub struct OpenAccount<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(seeds = [POOL_SEED, &pool.pool_id.to_le_bytes()], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(
        init,
        payer = owner,
        space = UserAccount::DISCRIMINATOR.len() + UserAccount::INIT_SPACE,
        seeds = [USER_SEED, pool.key().as_ref(), owner.key().as_ref()],
        bump
    )]
    pub user_account: Account<'info, UserAccount>,
    pub system_program: Program<'info, System>,
}

pub fn handle_open_account(ctx: Context<OpenAccount>) -> Result<()> {
    let user = &mut ctx.accounts.user_account;
    user.owner = ctx.accounts.owner.key();
    user.pool = ctx.accounts.pool.key();
    user.principal = 0;
    user.cumulative_weight = 0;
    user.last_update_ts = Clock::get()?.unix_timestamp;
    user.bump = ctx.bumps.user_account;
    Ok(())
}
