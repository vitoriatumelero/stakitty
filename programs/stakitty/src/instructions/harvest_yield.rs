use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::{constants::*, error::StakittyError, state::Pool};

/// Permissionless crank: moves yield realized by `settle_stake` out of the reserve,
/// `PROTOCOL_FEE_BPS` to the fee vault and the rest to the prize vault.
#[derive(Accounts)]
pub struct HarvestYield<'info> {
    #[account(mut, seeds = [POOL_SEED, &pool.pool_id.to_le_bytes()], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [RESERVE_SEED, pool.key().as_ref()], bump = pool.reserve_bump)]
    pub reserve: SystemAccount<'info>,
    #[account(mut, seeds = [PRIZE_SEED, pool.key().as_ref()], bump = pool.prize_bump)]
    pub prize_vault: SystemAccount<'info>,
    #[account(mut, seeds = [FEE_SEED, pool.key().as_ref()], bump = pool.fee_bump)]
    pub fee_vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn split_yield(amount: u64) -> Result<(u64, u64)> {
    let fee = u64::try_from(
        u128::from(amount)
            .checked_mul(u128::from(PROTOCOL_FEE_BPS))
            .and_then(|v| v.checked_div(u128::from(BPS_DENOMINATOR)))
            .ok_or(StakittyError::Overflow)?,
    )
    .map_err(|_| StakittyError::Overflow)?;
    let prize = amount.checked_sub(fee).ok_or(StakittyError::Overflow)?;
    Ok((fee, prize))
}

pub fn handle_harvest_yield(ctx: Context<HarvestYield>) -> Result<()> {
    let amount = ctx.accounts.pool.realized_yield;
    require!(amount > 0, StakittyError::NothingToHarvest);
    let (fee, prize) = split_yield(amount)?;

    let pool_key = ctx.accounts.pool.key();
    let reserve_seeds: &[&[u8]] = &[
        RESERVE_SEED,
        pool_key.as_ref(),
        &[ctx.accounts.pool.reserve_bump],
    ];
    for (to, lamports) in [
        (ctx.accounts.fee_vault.to_account_info(), fee),
        (ctx.accounts.prize_vault.to_account_info(), prize),
    ] {
        if lamports == 0 {
            continue;
        }
        transfer(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.key(),
                Transfer {
                    from: ctx.accounts.reserve.to_account_info(),
                    to,
                },
                &[reserve_seeds],
            ),
            lamports,
        )?;
    }

    let pool = &mut ctx.accounts.pool;
    pool.realized_yield = 0;
    pool.prize_available = pool
        .prize_available
        .checked_add(prize)
        .ok_or(StakittyError::Overflow)?;

    emit!(YieldHarvested {
        amount,
        fee,
        prize,
        prize_available: pool.prize_available,
        epoch: Clock::get()?.epoch,
    });
    Ok(())
}

#[event]
pub struct YieldHarvested {
    pub amount: u64,
    pub fee: u64,
    pub prize: u64,
    pub prize_available: u64,
    pub epoch: u64,
}
