pub mod constants;
pub mod error;
pub mod instructions;
pub mod merkle;
pub mod stake;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("8dFfRCbNDeYt9y96ZC8uCcU2BbXt2LwWEH91ZUN3BRQg");

#[program]
pub mod stakitty {
    use super::*;

    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        pool_id: u16,
        min_deposit: u64,
        season_length_epochs: u64,
    ) -> Result<()> {
        crate::instructions::initialize_pool::handle_initialize_pool(
            ctx,
            pool_id,
            min_deposit,
            season_length_epochs,
        )
    }

    pub fn open_account(ctx: Context<OpenAccount>) -> Result<()> {
        crate::instructions::open_account::handle_open_account(ctx)
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        crate::instructions::deposit::handle_deposit(ctx, amount)
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        crate::instructions::withdraw::handle_withdraw(ctx, amount)
    }

    pub fn add_validator(ctx: Context<AddValidator>) -> Result<()> {
        crate::instructions::add_validator::handle_add_validator(ctx)
    }

    pub fn sponsor(ctx: Context<Sponsor>, amount: u64) -> Result<()> {
        crate::instructions::sponsor::handle_sponsor(ctx, amount)
    }

    pub fn close_season<'info>(ctx: Context<'info, CloseSeason<'info>>) -> Result<()> {
        crate::instructions::close_season::handle_close_season(ctx)
    }

    pub fn rebalance(ctx: Context<Rebalance>) -> Result<()> {
        crate::instructions::rebalance::handle_rebalance(ctx)
    }

    pub fn settle_stake(ctx: Context<SettleStake>) -> Result<()> {
        crate::instructions::settle_stake::handle_settle_stake(ctx)
    }

    pub fn extend_season(ctx: Context<ExtendSeason>, additional_epochs: u64) -> Result<()> {
        crate::instructions::extend_season::handle_extend_season(ctx, additional_epochs)
    }

    pub fn request_withdraw(ctx: Context<RequestWithdraw>, amount: u64) -> Result<()> {
        crate::instructions::request_withdraw::handle_request_withdraw(ctx, amount)
    }

    pub fn claim_withdraw(ctx: Context<ClaimWithdraw>) -> Result<()> {
        crate::instructions::claim_withdraw::handle_claim_withdraw(ctx)
    }

    pub fn harvest_yield(ctx: Context<HarvestYield>) -> Result<()> {
        crate::instructions::harvest_yield::handle_harvest_yield(ctx)
    }

    pub fn withdraw_fees(ctx: Context<WithdrawFees>, amount: u64) -> Result<()> {
        crate::instructions::withdraw_fees::handle_withdraw_fees(ctx, amount)
    }

    pub fn commit_round(
        ctx: Context<CommitRound>,
        merkle_root: [u8; 32],
        total_weight: u128,
        leaf_count: u32,
    ) -> Result<()> {
        crate::instructions::commit_round::handle_commit_round(
            ctx,
            merkle_root,
            total_weight,
            leaf_count,
        )
    }

    pub fn request_draw(ctx: Context<RequestDraw>) -> Result<()> {
        crate::instructions::request_draw::handle_request_draw(ctx)
    }

    pub fn consume_randomness(ctx: Context<ConsumeRandomness>, randomness: [u8; 32]) -> Result<()> {
        crate::instructions::consume_randomness::handle_consume_randomness(ctx, randomness)
    }

    pub fn claim_prize(
        ctx: Context<ClaimPrize>,
        range_start: u128,
        range_end: u128,
        proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        crate::instructions::claim_prize::handle_claim_prize(ctx, range_start, range_end, proof)
    }

    pub fn expire_prize(ctx: Context<ExpirePrize>) -> Result<()> {
        crate::instructions::expire_prize::handle_expire_prize(ctx)
    }
}
