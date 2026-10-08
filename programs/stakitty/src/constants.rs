use anchor_lang::prelude::*;

#[constant]
pub const POOL_SEED: &[u8] = b"pool";

#[constant]
pub const RESERVE_SEED: &[u8] = b"reserve";

/// Sponsor SOL lives here, never in the reserve, so no withdraw path can reach it.
#[constant]
pub const PRIZE_SEED: &[u8] = b"prize";

#[constant]
pub const USER_SEED: &[u8] = b"user";

#[constant]
pub const VALIDATOR_SEED: &[u8] = b"validator";

#[constant]
pub const SEASON_SEED: &[u8] = b"season";

#[constant]
pub const SPONSORSHIP_SEED: &[u8] = b"sponsorship";

#[constant]
pub const STAKE_SEED: &[u8] = b"stake";

#[constant]
pub const TRANSIENT_SEED: &[u8] = b"transient";

/// Bounds the admin list so a season's weights fit in one account and one close tx.
#[constant]
pub const MAX_VALIDATORS: u8 = 16;

#[constant]
pub const MIN_SEASON_VALIDATORS: u8 = 4;

#[constant]
pub const MAX_WEIGHT_BPS: u16 = 3_500;

/// Share of principal never delegated: validator weights are scaled to the remaining 85%.
#[constant]
pub const MIN_RESERVE_BPS: u16 = 1_500;

#[constant]
pub const TICKET_SEED: &[u8] = b"ticket";

#[constant]
pub const ROUND_SEED: &[u8] = b"round";

/// Protocol fee vault. Only the admin can withdraw from it.
#[constant]
pub const FEE_SEED: &[u8] = b"fee";

/// Share of realized staking yield sent to the fee vault; the rest goes to prizes.
#[constant]
pub const PROTOCOL_FEE_BPS: u16 = 2_000;

/// Epochs a winner has to claim before the prize returns to the prize pool (~4 weeks).
#[constant]
pub const PRIZE_CLAIM_EPOCHS: u64 = 14;

/// Merkle proof depth bound: 2^24 leaves.
pub const MAX_PROOF_LEN: usize = 24;

pub const BPS_DENOMINATOR: u16 = 10_000;

pub const VOTE_PROGRAM_ID: Pubkey = pubkey!("Vote111111111111111111111111111111111111111");
pub const STAKE_PROGRAM_ID: Pubkey = pubkey!("Stake11111111111111111111111111111111111111");
pub const STAKE_CONFIG_ID: Pubkey = pubkey!("StakeConfig11111111111111111111111111111111");
pub const STAKE_HISTORY_ID: Pubkey = pubkey!("SysvarStakeHistory1111111111111111111111111");
