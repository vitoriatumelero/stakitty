pub mod constants;
pub mod error;
pub mod instructions;
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
        min_deposit: u64,
        season_length_epochs: u64,
    ) -> Result<()> {
        crate::instructions::initialize_pool::handle_initialize_pool(
            ctx,
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
}
