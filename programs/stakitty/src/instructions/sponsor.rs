use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::{
    constants::*,
    error::StakittyError,
    stake::vote_withdrawer,
    state::{Pool, Season, SeasonSponsorship, SeasonStatus, ValidatorEntry},
};

#[derive(Accounts)]
pub struct Sponsor<'info> {
    /// Must be the vote account's authorized withdrawer.
    #[account(mut)]
    pub withdrawer: Signer<'info>,
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [PRIZE_SEED, pool.key().as_ref()], bump = pool.prize_bump)]
    pub prize_vault: SystemAccount<'info>,
    /// CHECK: bound to the entry by `has_one`; owner and withdrawer read by `vote_withdrawer`.
    pub vote_account: UncheckedAccount<'info>,
    #[account(
        mut,
        has_one = pool,
        has_one = vote_account,
        seeds = [VALIDATOR_SEED, pool.key().as_ref(), vote_account.key().as_ref()],
        bump = validator_entry.bump
    )]
    pub validator_entry: Account<'info, ValidatorEntry>,
    #[account(
        mut,
        has_one = pool,
        seeds = [SEASON_SEED, pool.key().as_ref(), &pool.current_season.to_le_bytes()],
        bump = season.bump,
        constraint = season.status == SeasonStatus::Open @ StakittyError::SeasonNotOpen
    )]
    pub season: Account<'info, Season>,
    /// One payment per validator per season (`init`, not `init_if_needed`).
    #[account(
        init,
        payer = withdrawer,
        space = SeasonSponsorship::DISCRIMINATOR.len() + SeasonSponsorship::INIT_SPACE,
        seeds = [SPONSORSHIP_SEED, season.key().as_ref(), vote_account.key().as_ref()],
        bump
    )]
    pub sponsorship: Account<'info, SeasonSponsorship>,
    pub system_program: Program<'info, System>,
}

pub fn handle_sponsor(ctx: Context<Sponsor>, amount: u64) -> Result<()> {
    require!(amount > 0, StakittyError::ZeroAmount);
    require_keys_eq!(
        vote_withdrawer(&ctx.accounts.vote_account)?,
        ctx.accounts.withdrawer.key(),
        StakittyError::NotVoteWithdrawer
    );
    let epoch = Clock::get()?.epoch;
    require!(
        epoch < ctx.accounts.season.end_epoch,
        StakittyError::SeasonNotOpen
    );

    transfer(
        CpiContext::new(
            ctx.accounts.system_program.key(),
            Transfer {
                from: ctx.accounts.withdrawer.to_account_info(),
                to: ctx.accounts.prize_vault.to_account_info(),
            },
        ),
        amount,
    )?;

    let season = &mut ctx.accounts.season;
    season.total_paid = season
        .total_paid
        .checked_add(amount)
        .ok_or(StakittyError::Overflow)?;
    season.sponsor_count = season
        .sponsor_count
        .checked_add(1)
        .ok_or(StakittyError::Overflow)?;

    let entry = &mut ctx.accounts.validator_entry;
    entry.total_paid = entry
        .total_paid
        .checked_add(amount)
        .ok_or(StakittyError::Overflow)?;

    let sponsorship = &mut ctx.accounts.sponsorship;
    sponsorship.season = season.key();
    sponsorship.vote_account = entry.vote_account;
    sponsorship.amount = amount;
    sponsorship.bump = ctx.bumps.sponsorship;

    emit!(Sponsored {
        vote_account: entry.vote_account,
        season: season.index,
        amount,
        season_total: season.total_paid,
        epoch,
    });
    Ok(())
}

#[event]
pub struct Sponsored {
    pub vote_account: Pubkey,
    pub season: u32,
    pub amount: u64,
    pub season_total: u64,
    pub epoch: u64,
}
