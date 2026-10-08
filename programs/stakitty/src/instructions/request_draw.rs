use anchor_lang::prelude::*;
use ephemeral_vrf_sdk::anchor::vrf;
use ephemeral_vrf_sdk::consts::DEFAULT_QUEUE;
use ephemeral_vrf_sdk::instructions::{create_request_randomness_ix, RequestRandomnessParams};
use ephemeral_vrf_sdk::types::SerializableAccountMeta;

use crate::{
    constants::*,
    error::StakittyError,
    state::{Pool, Round, RoundStatus},
};

// `#[vrf]` adds program_identity, vrf_program, slot_hashes and system_program,
// plus the `invoke_signed_vrf` helper.
#[vrf]
#[derive(Accounts)]
pub struct RequestDraw<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(
        mut,
        has_one = pool,
        seeds = [ROUND_SEED, pool.key().as_ref(), &round.season_index.to_le_bytes()],
        bump = round.bump
    )]
    pub round: Account<'info, Round>,
    /// CHECK: pinned to MagicBlock's base-layer queue.
    #[account(mut, address = DEFAULT_QUEUE @ StakittyError::InvalidOracleQueue)]
    pub oracle_queue: UncheckedAccount<'info>,
}

/// Permissionless. Committed -> RandomnessRequested; the status flip makes a re-roll fail.
pub fn handle_request_draw(ctx: Context<RequestDraw>) -> Result<()> {
    let round_key = ctx.accounts.round.key();
    {
        let round = &mut ctx.accounts.round;
        require!(
            round.status == RoundStatus::Committed,
            StakittyError::InvalidRoundState
        );
        round.status = RoundStatus::RandomnessRequested;
    }

    let ix = create_request_randomness_ix(RequestRandomnessParams {
        payer: ctx.accounts.payer.key(),
        oracle_queue: ctx.accounts.oracle_queue.key(),
        callback_program_id: crate::ID,
        callback_discriminator: crate::instruction::ConsumeRandomness::DISCRIMINATOR.to_vec(),
        // Unique per round, so two rounds never share a request seed.
        caller_seed: round_key.to_bytes(),
        accounts_metas: Some(vec![SerializableAccountMeta {
            pubkey: round_key,
            is_signer: false,
            is_writable: true,
        }]),
        ..Default::default()
    });
    ctx.accounts
        .invoke_signed_vrf(&ctx.accounts.payer.to_account_info(), &ix)?;

    emit!(DrawRequested {
        round: round_key,
        season: ctx.accounts.round.season_index,
    });
    Ok(())
}

#[event]
pub struct DrawRequested {
    pub round: Pubkey,
    pub season: u32,
}
