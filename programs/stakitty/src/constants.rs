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

pub const BPS_DENOMINATOR: u16 = 10_000;

pub const VOTE_PROGRAM_ID: Pubkey = pubkey!("Vote111111111111111111111111111111111111111");
pub const STAKE_PROGRAM_ID: Pubkey = pubkey!("Stake11111111111111111111111111111111111111");
pub const STAKE_CONFIG_ID: Pubkey = pubkey!("StakeConfig11111111111111111111111111111111");
pub const STAKE_HISTORY_ID: Pubkey = pubkey!("SysvarStakeHistory1111111111111111111111111");
