use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::{
    constants::*,
    error::StakittyError,
    merkle,
    state::{Pool, Round, RoundStatus},
};

#[derive(Accounts)]
pub struct ClaimPrize<'info> {
    #[account(mut)]
    pub winner: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED, &pool.pool_id.to_le_bytes()], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [PRIZE_SEED, pool.key().as_ref()], bump = pool.prize_bump)]
    pub prize_vault: SystemAccount<'info>,
    #[account(
        mut,
        has_one = pool,
        seeds = [ROUND_SEED, pool.key().as_ref(), &round.season_index.to_le_bytes()],
        bump = round.bump,
        constraint = round.status == RoundStatus::Settled @ StakittyError::InvalidRoundState
    )]
    pub round: Account<'info, Round>,
    pub system_program: Program<'info, System>,
}

pub fn handle_claim_prize(
    ctx: Context<ClaimPrize>,
    range_start: u128,
    range_end: u128,
    proof: Vec<[u8; 32]>,
) -> Result<()> {
    let round = &ctx.accounts.round;
    let epoch = Clock::get()?.epoch;
    let deadline = round
        .settled_epoch
        .checked_add(PRIZE_CLAIM_EPOCHS)
        .ok_or(StakittyError::Overflow)?;
    require!(epoch <= deadline, StakittyError::ClaimWindowClosed);
    require!(proof.len() <= MAX_PROOF_LEN, StakittyError::InvalidProof);
    require!(
        range_start <= round.winning_ticket
            && round.winning_ticket < range_end
            && range_end <= round.total_weight,
        StakittyError::NotWinningLeaf
    );
    let leaf = merkle::leaf_hash(&ctx.accounts.winner.key(), range_start, range_end);
    require!(
        merkle::verify(&proof, &round.merkle_root, leaf),
        StakittyError::InvalidProof
    );

    let prize = round.prize;
    if prize > 0 {
        let pool_key = ctx.accounts.pool.key();
        let prize_seeds: &[&[u8]] = &[
            PRIZE_SEED,
            pool_key.as_ref(),
            &[ctx.accounts.pool.prize_bump],
        ];
        transfer(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.key(),
                Transfer {
                    from: ctx.accounts.prize_vault.to_account_info(),
                    to: ctx.accounts.winner.to_account_info(),
                },
                &[prize_seeds],
            ),
            prize,
        )?;
    }

    let pool = &mut ctx.accounts.pool;
    pool.committed_prizes = pool
        .committed_prizes
        .checked_sub(prize)
        .ok_or(StakittyError::Overflow)?;
    let round = &mut ctx.accounts.round;
    round.status = RoundStatus::Paid;
    round.winner = ctx.accounts.winner.key();

    emit!(PrizeClaimed {
        season: round.season_index,
        winner: round.winner,
        prize,
        epoch,
    });
    Ok(())
}

#[event]
pub struct PrizeClaimed {
    pub season: u32,
    pub winner: Pubkey,
    pub prize: u64,
    pub epoch: u64,
}
