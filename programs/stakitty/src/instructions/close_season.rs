use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::StakittyError,
    state::{Pool, Season, SeasonSponsorship, SeasonStatus, ValidatorWeight},
};

/// Permissionless once the season's end epoch is reached. Every `SeasonSponsorship` of the
/// season must be passed in `remaining_accounts`, sorted by vote account, so nobody can
/// shift weights by omitting a payer.
#[derive(Accounts)]
pub struct CloseSeason<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(
        mut,
        has_one = pool,
        seeds = [SEASON_SEED, pool.key().as_ref(), &pool.current_season.to_le_bytes()],
        bump = season.bump,
        constraint = season.status == SeasonStatus::Open @ StakittyError::SeasonNotOpen
    )]
    pub season: Account<'info, Season>,
    #[account(
        init,
        payer = payer,
        space = Season::DISCRIMINATOR.len() + Season::INIT_SPACE,
        seeds = [
            SEASON_SEED,
            pool.key().as_ref(),
            &pool.current_season.saturating_add(1).to_le_bytes()
        ],
        bump
    )]
    pub next_season: Account<'info, Season>,
    pub system_program: Program<'info, System>,
}

/// `paid / total` of the delegable share (100% minus the fixed reserve), in basis points of
/// total principal, capped. The cap excess is not redistributed: it stays liquid too.
pub fn capped_weight_bps(paid: u64, total: u64) -> Result<u16> {
    require!(total > 0, StakittyError::NotEnoughValidators);
    let delegable = BPS_DENOMINATOR
        .checked_sub(MIN_RESERVE_BPS)
        .ok_or(StakittyError::Overflow)?;
    let raw = u128::from(paid)
        .checked_mul(u128::from(delegable))
        .and_then(|v| v.checked_div(u128::from(total)))
        .ok_or(StakittyError::Overflow)?;
    let raw = u16::try_from(raw).map_err(|_| StakittyError::Overflow)?;
    Ok(raw.min(MAX_WEIGHT_BPS))
}

pub fn handle_close_season<'info>(ctx: Context<'info, CloseSeason<'info>>) -> Result<()> {
    let clock = Clock::get()?;
    let season = &mut ctx.accounts.season;
    require!(
        clock.epoch >= season.end_epoch,
        StakittyError::SeasonNotEnded
    );
    require!(
        season.sponsor_count >= MIN_SEASON_VALIDATORS,
        StakittyError::NotEnoughValidators
    );
    require!(
        ctx.remaining_accounts.len() == usize::from(season.sponsor_count),
        StakittyError::SponsorshipMismatch
    );

    let season_key = season.key();
    let mut weights = Vec::with_capacity(ctx.remaining_accounts.len());
    let mut sum_paid: u64 = 0;
    let mut sum_bps: u16 = 0;
    let mut previous: Option<Pubkey> = None;
    for info in ctx.remaining_accounts.iter() {
        let sponsorship = Account::<SeasonSponsorship>::try_from(info)?;
        require_keys_eq!(
            sponsorship.season,
            season_key,
            StakittyError::SponsorshipMismatch
        );
        // Strictly increasing keys rule out passing the same sponsorship twice.
        require!(
            previous.is_none_or(|p| p < sponsorship.vote_account),
            StakittyError::SponsorshipMismatch
        );
        previous = Some(sponsorship.vote_account);

        sum_paid = sum_paid
            .checked_add(sponsorship.amount)
            .ok_or(StakittyError::Overflow)?;
        let weight_bps = capped_weight_bps(sponsorship.amount, season.total_paid)?;
        sum_bps = sum_bps
            .checked_add(weight_bps)
            .ok_or(StakittyError::Overflow)?;
        weights.push(ValidatorWeight {
            vote_account: sponsorship.vote_account,
            weight_bps,
        });
    }
    require!(
        sum_paid == season.total_paid,
        StakittyError::SponsorshipMismatch
    );

    // Freeze the weight window: the round's Merkle tree must sum to end - start.
    let pool = &mut ctx.accounts.pool;
    pool.accrue(clock.unix_timestamp)?;
    season.end_ts = clock.unix_timestamp;
    season.end_pool_weight = pool.cumulative_weight;

    season.weights = weights;
    season.reserve_bps = BPS_DENOMINATOR
        .checked_sub(sum_bps)
        .ok_or(StakittyError::Overflow)?;
    season.status = SeasonStatus::Closed;

    pool.current_season = pool
        .current_season
        .checked_add(1)
        .ok_or(StakittyError::Overflow)?;

    let next = &mut ctx.accounts.next_season;
    next.pool = pool.key();
    next.index = pool.current_season;
    next.start_epoch = clock.epoch;
    next.end_epoch = clock
        .epoch
        .checked_add(pool.season_length_epochs)
        .ok_or(StakittyError::Overflow)?;
    next.start_ts = season.end_ts;
    next.start_pool_weight = season.end_pool_weight;
    next.end_ts = 0;
    next.end_pool_weight = 0;
    next.total_paid = 0;
    next.sponsor_count = 0;
    next.status = SeasonStatus::Open;
    next.weights = Vec::new();
    next.reserve_bps = BPS_DENOMINATOR;
    next.stake_base = None;
    next.bump = ctx.bumps.next_season;

    emit!(SeasonClosed {
        season: season.index,
        total_paid: season.total_paid,
        sponsor_count: season.sponsor_count,
        reserve_bps: season.reserve_bps,
        epoch: clock.epoch,
    });
    Ok(())
}

#[event]
pub struct SeasonClosed {
    pub season: u32,
    pub total_paid: u64,
    pub sponsor_count: u8,
    pub reserve_bps: u16,
    pub epoch: u64,
}
