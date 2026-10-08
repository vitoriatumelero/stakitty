use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::StakittyError,
    state::{Pool, Season, SeasonStatus},
};

/// Admin escape hatch for a season stuck below the minimum number of sponsors.
#[derive(Accounts)]
pub struct ExtendSeason<'info> {
    pub authority: Signer<'info>,
    #[account(seeds = [POOL_SEED], bump = pool.bump, has_one = authority @ StakittyError::Unauthorized)]
    pub pool: Account<'info, Pool>,
    #[account(
        mut,
        has_one = pool,
        seeds = [SEASON_SEED, pool.key().as_ref(), &pool.current_season.to_le_bytes()],
        bump = season.bump,
        constraint = season.status == SeasonStatus::Open @ StakittyError::SeasonNotOpen
    )]
    pub season: Account<'info, Season>,
}

pub fn handle_extend_season(ctx: Context<ExtendSeason>, additional_epochs: u64) -> Result<()> {
    require!(additional_epochs > 0, StakittyError::InvalidSeasonLength);
    let season = &mut ctx.accounts.season;
    season.end_epoch = season
        .end_epoch
        .checked_add(additional_epochs)
        .ok_or(StakittyError::Overflow)?;

    emit!(SeasonExtended {
        season: season.index,
        end_epoch: season.end_epoch,
        sponsor_count: season.sponsor_count,
    });
    Ok(())
}

#[event]
pub struct SeasonExtended {
    pub season: u32,
    pub end_epoch: u64,
    pub sponsor_count: u8,
}
