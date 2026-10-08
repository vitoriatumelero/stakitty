use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::StakittyError,
    state::{Pool, Round, RoundStatus, Season, SeasonStatus},
};

/// Admin publishes the Merkle root of the season's user weights. The program cannot check
/// each leaf, only that the leaves sum to the on-chain pool accumulator over the season
/// (known risk: a wrong individual weight is caught only by off-chain audit of the list).
#[derive(Accounts)]
pub struct CommitRound<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED, &pool.pool_id.to_le_bytes()], bump = pool.bump, has_one = authority @ StakittyError::Unauthorized)]
    pub pool: Account<'info, Pool>,
    #[account(
        has_one = pool,
        seeds = [SEASON_SEED, pool.key().as_ref(), &season.index.to_le_bytes()],
        bump = season.bump,
        constraint = season.status == SeasonStatus::Closed @ StakittyError::SeasonNotClosed
    )]
    pub season: Account<'info, Season>,
    /// Season `index - 1`, whose sponsorship funds this round. Omitted for season 0.
    #[account(
        has_one = pool,
        seeds = [SEASON_SEED, pool.key().as_ref(), &season.index.saturating_sub(1).to_le_bytes()],
        bump = previous_season.bump
    )]
    pub previous_season: Option<Account<'info, Season>>,
    #[account(
        init,
        payer = authority,
        space = Round::DISCRIMINATOR.len() + Round::INIT_SPACE,
        seeds = [ROUND_SEED, pool.key().as_ref(), &season.index.to_le_bytes()],
        bump
    )]
    pub round: Account<'info, Round>,
    pub system_program: Program<'info, System>,
}

pub fn handle_commit_round(
    ctx: Context<CommitRound>,
    merkle_root: [u8; 32],
    total_weight: u128,
    leaf_count: u32,
) -> Result<()> {
    let season = &ctx.accounts.season;
    let expected = season
        .end_pool_weight
        .checked_sub(season.start_pool_weight)
        .ok_or(StakittyError::Overflow)?;
    require!(total_weight == expected, StakittyError::WeightMismatch);
    require!(
        total_weight > 0 && leaf_count > 0,
        StakittyError::NoParticipants
    );

    // Sponsorship paid in season s funds the round of season s+1.
    let sponsor_prize = if season.index == 0 {
        0
    } else {
        ctx.accounts
            .previous_season
            .as_ref()
            .ok_or(StakittyError::MissingPreviousSeason)?
            .total_paid
    };

    let pool = &mut ctx.accounts.pool;
    let prize = pool
        .prize_available
        .checked_add(sponsor_prize)
        .ok_or(StakittyError::Overflow)?;
    pool.prize_available = 0;
    pool.committed_prizes = pool
        .committed_prizes
        .checked_add(prize)
        .ok_or(StakittyError::Overflow)?;

    let round = &mut ctx.accounts.round;
    round.pool = pool.key();
    round.season_index = season.index;
    round.merkle_root = merkle_root;
    round.total_weight = total_weight;
    round.leaf_count = leaf_count;
    round.prize = prize;
    round.randomness = [0; 32];
    round.winning_ticket = 0;
    round.winner = Pubkey::default();
    round.status = RoundStatus::Committed;
    round.settled_epoch = 0;
    round.bump = ctx.bumps.round;

    emit!(RoundCommitted {
        season: season.index,
        merkle_root,
        total_weight,
        leaf_count,
        prize,
    });
    Ok(())
}

#[event]
pub struct RoundCommitted {
    pub season: u32,
    pub merkle_root: [u8; 32],
    pub total_weight: u128,
    pub leaf_count: u32,
    pub prize: u64,
}
