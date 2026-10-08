use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::StakittyError,
    stake::{minimum_delegation, read_stake, StakeCtx, STAKE_ACCOUNT_SPACE},
    state::{Pool, Season, SeasonStatus, ValidatorEntry},
};

/// Permissionless crank: applies the latest closed season's weight to one validator.
/// Each stake change goes through the transient account, and a pending transient blocks the
/// next change until `settle_stake` runs after the epoch boundary.
#[derive(Accounts)]
pub struct Rebalance<'info> {
    #[account(seeds = [POOL_SEED, &pool.pool_id.to_le_bytes()], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(
        mut,
        has_one = pool,
        seeds = [SEASON_SEED, pool.key().as_ref(), &season.index.to_le_bytes()],
        bump = season.bump,
        constraint = season.status == SeasonStatus::Closed @ StakittyError::SeasonNotClosed,
        constraint = season.index.checked_add(1) == Some(pool.current_season) @ StakittyError::SeasonNotClosed
    )]
    pub season: Account<'info, Season>,
    #[account(
        mut,
        has_one = pool,
        has_one = vote_account,
        seeds = [VALIDATOR_SEED, pool.key().as_ref(), vote_account.key().as_ref()],
        bump = validator_entry.bump
    )]
    pub validator_entry: Account<'info, ValidatorEntry>,
    /// CHECK: bound to the entry by `has_one`; the Stake program validates it on delegate.
    pub vote_account: UncheckedAccount<'info>,
    #[account(mut, seeds = [RESERVE_SEED, pool.key().as_ref()], bump = pool.reserve_bump)]
    pub reserve: SystemAccount<'info>,
    /// CHECK: PDA; state read by `read_stake`, owned by the Stake program once created.
    #[account(
        mut,
        seeds = [STAKE_SEED, pool.key().as_ref(), vote_account.key().as_ref()],
        bump = validator_entry.stake_bump
    )]
    pub stake_account: UncheckedAccount<'info>,
    /// CHECK: PDA; same as `stake_account`.
    #[account(
        mut,
        seeds = [TRANSIENT_SEED, pool.key().as_ref(), vote_account.key().as_ref()],
        bump = validator_entry.transient_bump
    )]
    pub transient_stake: UncheckedAccount<'info>,
    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,
    /// CHECK: address-checked sysvar.
    #[account(address = STAKE_HISTORY_ID)]
    pub stake_history: UncheckedAccount<'info>,
    /// CHECK: address-checked; unused by the Stake program but still expected by delegate.
    #[account(address = STAKE_CONFIG_ID)]
    pub stake_config: UncheckedAccount<'info>,
    /// CHECK: address-checked CPI target.
    #[account(address = STAKE_PROGRAM_ID)]
    pub stake_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum RebalanceAction {
    None,
    Create,
    Increase,
    Decrease,
    Deactivate,
}

pub fn handle_rebalance(ctx: Context<Rebalance>) -> Result<()> {
    let accounts = &ctx.accounts;
    let season_index = accounts.season.index;
    require!(
        accounts.validator_entry.rebalanced_through <= season_index,
        StakittyError::AlreadyRebalanced
    );
    require!(
        read_stake(&accounts.transient_stake)?.is_none(),
        StakittyError::StakeTransitionPending
    );

    let stake_base = match accounts.season.stake_base {
        Some(base) => base,
        None => accounts.pool.total_principal,
    };
    let weight_bps = accounts.season.weight_of(&accounts.vote_account.key());
    let target = u64::try_from(
        u128::from(stake_base)
            .checked_mul(u128::from(weight_bps))
            .and_then(|v| v.checked_div(u128::from(BPS_DENOMINATOR)))
            .ok_or(StakittyError::Overflow)?,
    )
    .map_err(|_| StakittyError::Overflow)?;

    let stake_rent = accounts.rent.minimum_balance(STAKE_ACCOUNT_SPACE as usize);
    let min_delegation = minimum_delegation()?;
    // Smallest stake account worth holding: rent plus the minimum delegation.
    let min_account = stake_rent
        .checked_add(min_delegation)
        .ok_or(StakittyError::Overflow)?;
    // A target below the minimum is treated as zero; those lamports stay liquid.
    let target = if target < min_account { 0 } else { target };

    let pool_key = accounts.pool.key();
    let vote_key = accounts.vote_account.key();
    let reserve_seeds: &[&[u8]] = &[
        RESERVE_SEED,
        pool_key.as_ref(),
        &[accounts.pool.reserve_bump],
    ];
    let stake_seeds: &[&[u8]] = &[
        STAKE_SEED,
        pool_key.as_ref(),
        vote_key.as_ref(),
        &[accounts.validator_entry.stake_bump],
    ];
    let transient_seeds: &[&[u8]] = &[
        TRANSIENT_SEED,
        pool_key.as_ref(),
        vote_key.as_ref(),
        &[accounts.validator_entry.transient_bump],
    ];
    let reserve_info = accounts.reserve.to_account_info();
    let clock_info = accounts.clock.to_account_info();
    let history_info = accounts.stake_history.to_account_info();
    let system_info = accounts.system_program.to_account_info();
    let cpi = StakeCtx {
        reserve: &reserve_info,
        reserve_seeds,
        clock: &clock_info,
        stake_history: &history_info,
        system_program: &system_info,
    };
    let main = accounts.stake_account.to_account_info();
    let transient = accounts.transient_stake.to_account_info();
    let vote = accounts.vote_account.to_account_info();
    let config = accounts.stake_config.to_account_info();
    let rent_info = accounts.rent.to_account_info();

    let rent_floor = accounts.rent.minimum_balance(0);
    // New stake may only use lamports not owed to withdraw tickets.
    let liquid = accounts.pool.spendable(reserve_info.lamports(), rent_floor);
    // A decrease only borrows the split rent and returns it with the principal, so it may dip
    // into ticket-earmarked lamports; otherwise pending tickets could block their own unstake.
    let raw_liquid = reserve_info.lamports().saturating_sub(rent_floor);
    let current = read_stake(&main)?;
    let reserve_before = reserve_info.lamports();

    let action = match current {
        None if target == 0 => RebalanceAction::None,
        None => {
            // Lamports already sitting on the PDA (donations) count toward the target.
            let needed = target.saturating_sub(main.lamports());
            require!(needed <= liquid, StakittyError::InsufficientLiquidity);
            cpi.create(&main, stake_seeds, target, &rent_info)?;
            cpi.delegate(&main, &vote, &config)?;
            RebalanceAction::Create
        }
        Some(info) if info.is_deactivating() => {
            require!(target == 0, StakittyError::StakeNotSettled);
            RebalanceAction::None
        }
        Some(_) if target == 0 => {
            // Validator stopped paying (or fell under the minimum): full exit, ~1 epoch cooldown.
            cpi.deactivate(&main)?;
            RebalanceAction::Deactivate
        }
        Some(info) if target > info.lamports => {
            let delta = target - info.lamports;
            if delta < min_account {
                RebalanceAction::None
            } else {
                let needed = delta.saturating_sub(transient.lamports());
                require!(needed <= liquid, StakittyError::InsufficientLiquidity);
                cpi.create(&transient, transient_seeds, delta, &rent_info)?;
                cpi.delegate(&transient, &vote, &config)?;
                RebalanceAction::Increase
            }
        }
        Some(info) => {
            let delta = info.lamports - target;
            if delta < min_delegation.max(1) {
                RebalanceAction::None
            } else {
                // Split needs a rent-funded, stake-owned destination.
                let needed = stake_rent.saturating_sub(transient.lamports());
                require!(needed <= raw_liquid, StakittyError::InsufficientLiquidity);
                cpi.allocate(&transient, transient_seeds, stake_rent)?;
                cpi.split(&main, &transient, delta)?;
                cpi.deactivate(&transient)?;
                RebalanceAction::Decrease
            }
        }
    };

    // Every lamport that left the reserve went into this validator's stake accounts.
    let moved_in = reserve_before
        .checked_sub(reserve_info.lamports())
        .ok_or(StakittyError::Overflow)?;
    let season = &mut ctx.accounts.season;
    season.stake_base = Some(stake_base);
    let entry = &mut ctx.accounts.validator_entry;
    entry.rebalanced_through = season_index.checked_add(1).ok_or(StakittyError::Overflow)?;
    entry.stake_basis = entry
        .stake_basis
        .checked_add(moved_in)
        .ok_or(StakittyError::Overflow)?;

    emit!(Rebalanced {
        vote_account: vote_key,
        season: season_index,
        weight_bps,
        target_lamports: target,
        action,
        epoch: ctx.accounts.clock.epoch,
    });
    Ok(())
}

#[event]
pub struct Rebalanced {
    pub vote_account: Pubkey,
    pub season: u32,
    pub weight_bps: u16,
    pub target_lamports: u64,
    pub action: RebalanceAction,
    pub epoch: u64,
}
