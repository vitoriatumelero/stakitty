use anchor_lang::prelude::*;
use ephemeral_vrf_sdk::anchor::vrf_callback;

use crate::{
    constants::*,
    error::StakittyError,
    state::{Round, RoundStatus},
};

// `#[vrf_callback]` adds `vrf_program_identity: Signer` pinned to scoped_vrf_identity(crate::ID).
#[vrf_callback]
#[derive(Accounts)]
pub struct ConsumeRandomness<'info> {
    #[account(
        mut,
        seeds = [ROUND_SEED, round.pool.as_ref(), &round.season_index.to_le_bytes()],
        bump = round.bump
    )]
    pub round: Account<'info, Round>,
}

/// Called only by the MagicBlock VRF program. Picks the winning ticket in `[0, total_weight)`.
pub fn handle_consume_randomness(
    ctx: Context<ConsumeRandomness>,
    randomness: [u8; 32],
) -> Result<()> {
    let round = &mut ctx.accounts.round;
    require!(
        round.status == RoundStatus::RandomnessRequested,
        StakittyError::InvalidRoundState
    );

    let mut low = [0u8; 16];
    low.copy_from_slice(&randomness[..16]);
    // Modulo bias is ~total_weight / 2^128: negligible.
    let winning_ticket = u128::from_le_bytes(low)
        .checked_rem(round.total_weight)
        .ok_or(StakittyError::NoParticipants)?;

    round.randomness = randomness;
    round.winning_ticket = winning_ticket;
    round.status = RoundStatus::Settled;
    round.settled_epoch = Clock::get()?.epoch;

    emit!(DrawSettled {
        round: round.key(),
        season: round.season_index,
        randomness,
        winning_ticket,
    });
    Ok(())
}

#[event]
pub struct DrawSettled {
    pub round: Pubkey,
    pub season: u32,
    pub randomness: [u8; 32],
    pub winning_ticket: u128,
}
