mod common;

use {
    common::*,
    solana_keypair::Keypair,
    solana_signer::Signer,
    stakitty::{
        error::StakittyError, state::SeasonStatus, MAX_WEIGHT_BPS, MIN_RESERVE_BPS,
        STAKE_PROGRAM_ID,
    },
};

const PRINCIPAL: u64 = 1_000 * SOL;

/// `bps` of PRINCIPAL in lamports.
fn share(bps: u64) -> u64 {
    PRINCIPAL * bps / 10_000
}

fn validators(env: &mut Env, n: usize) -> Vec<Validator> {
    (0..n).map(|_| listed_validator(env)).collect()
}

/// Pool with `PRINCIPAL` deposited and `n` listed validators.
fn funded_env(n: usize) -> (Env, Vec<Validator>) {
    let mut env = setup();
    let whale = new_user(&mut env, PRINCIPAL + SOL);
    deposit(&mut env, &whale, PRINCIPAL).unwrap();
    let vs = validators(&mut env, n);
    (env, vs)
}

fn sponsor_all(env: &mut Env, vs: &[&Validator], amounts: &[u64]) {
    for (v, amount) in vs.iter().zip(amounts) {
        sponsor(env, v, *amount).unwrap();
    }
}

fn end_current_season(env: &mut Env) {
    let season = season_state(env, pool_state(env).current_season);
    set_epoch(env, season.end_epoch);
}

fn stake_lamports(env: &Env, vote: &anchor_lang::prelude::Pubkey) -> u64 {
    match env.svm.get_account(&stake_pda(env, vote)) {
        Some(acc) if acc.owner == STAKE_PROGRAM_ID => acc.lamports,
        _ => 0,
    }
}

#[test]
fn sponsor_moves_sol_to_prize_vault_and_tallies() {
    let (mut env, vs) = funded_env(1);
    let reserve_before = lamports(&env, &env.reserve);
    let prize_before = lamports(&env, &env.prize_vault);

    sponsor(&mut env, &vs[0], 3 * SOL).unwrap();

    assert_eq!(lamports(&env, &env.prize_vault), prize_before + 3 * SOL);
    assert_eq!(lamports(&env, &env.reserve), reserve_before);
    let season = season_state(&env, 0);
    assert_eq!(season.total_paid, 3 * SOL);
    assert_eq!(season.sponsor_count, 1);
    assert_eq!(entry_state(&env, &vs[0].vote).total_paid, 3 * SOL);
}

#[test]
fn attack_sponsor_unlisted_validator_fails() {
    let mut env = setup();
    let withdrawer = Keypair::new();
    env.svm.airdrop(&withdrawer.pubkey(), 10 * SOL).unwrap();
    let vote = create_vote_account(&mut env, &withdrawer.pubkey());

    let ix = sponsor_ix(&env, &withdrawer.pubkey(), &vote, SOL);
    assert_custom_err(
        send(&mut env.svm, ix, &withdrawer, &[&withdrawer]),
        ACCOUNT_NOT_INITIALIZED,
    );
    assert_eq!(season_state(&env, 0).total_paid, 0);
}

#[test]
fn attack_sponsor_zero_amount_fails() {
    let (mut env, vs) = funded_env(1);

    assert_custom_err(
        sponsor(&mut env, &vs[0], 0),
        code(StakittyError::ZeroAmount),
    );
    assert_eq!(season_state(&env, 0).sponsor_count, 0);
}

#[test]
fn attack_sponsor_by_non_withdrawer_fails() {
    let (mut env, vs) = funded_env(1);
    let mallory = Keypair::new();
    env.svm.airdrop(&mallory.pubkey(), 10 * SOL).unwrap();

    let ix = sponsor_ix(&env, &mallory.pubkey(), &vs[0].vote, SOL);
    assert_custom_err(
        send(&mut env.svm, ix, &mallory, &[&mallory]),
        code(StakittyError::NotVoteWithdrawer),
    );
    assert_eq!(season_state(&env, 0).total_paid, 0);
}

#[test]
fn attack_close_with_fewer_than_four_validators_fails() {
    let (mut env, vs) = funded_env(3);
    let refs: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &refs, &[SOL, SOL, SOL]);
    end_current_season(&mut env);

    assert_custom_err(
        close_season(&mut env, &refs),
        code(StakittyError::NotEnoughValidators),
    );
    assert_eq!(season_state(&env, 0).status, SeasonStatus::Open);
    assert_eq!(pool_state(&env).current_season, 0);
}

#[test]
fn close_before_end_epoch_fails() {
    let (mut env, vs) = funded_env(4);
    let refs: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &refs, &[SOL; 4]);

    assert_custom_err(
        close_season(&mut env, &refs),
        code(StakittyError::SeasonNotEnded),
    );
}

#[test]
fn attack_close_omitting_a_sponsor_fails() {
    let (mut env, vs) = funded_env(5);
    let refs: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &refs, &[SOL; 5]);
    end_current_season(&mut env);

    // Dropping one payer would inflate everyone else's weight.
    assert_custom_err(
        close_season(&mut env, &refs[..4]),
        code(StakittyError::SponsorshipMismatch),
    );
}

#[test]
fn equal_payments_split_weight_evenly() {
    let (mut env, vs) = funded_env(4);
    let refs: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &refs, &[2 * SOL; 4]);
    end_current_season(&mut env);

    close_season(&mut env, &refs).unwrap();

    let season = season_state(&env, 0);
    assert_eq!(season.status, SeasonStatus::Closed);
    // Weights split the delegable 85%; the fixed 15% stays in the reserve.
    assert!(season.weights.iter().all(|w| w.weight_bps == 2_125));
    assert_eq!(season.reserve_bps, MIN_RESERVE_BPS);
    assert_eq!(pool_state(&env).current_season, 1);
    assert_eq!(season_state(&env, 1).status, SeasonStatus::Open);
}

#[test]
fn attack_cannot_exceed_35_percent() {
    let (mut env, vs) = funded_env(4);
    let refs: Vec<&Validator> = vs.iter().collect();
    // Whale pays 97% of the season.
    sponsor_all(
        &mut env,
        &refs,
        &[97 * SOL / 10, SOL / 10, SOL / 10, SOL / 10],
    );
    end_current_season(&mut env);
    close_season(&mut env, &refs).unwrap();

    let season = season_state(&env, 0);
    assert_eq!(season.weight_of(&vs[0].vote), MAX_WEIGHT_BPS);
    assert_eq!(season.weight_of(&vs[1].vote), 85);
    // Cap excess is not redistributed; it stays liquid on top of the fixed 15%.
    assert_eq!(season.reserve_bps, 10_000 - 3_500 - 3 * 85);

    rebalance(&mut env, &vs[0].vote, 0).unwrap();
    assert_eq!(stake_lamports(&env, &vs[0].vote), share(3_500));
}

#[test]
fn attack_rebalance_twice_in_same_season_fails() {
    let (mut env, vs) = funded_env(4);
    let refs: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &refs, &[SOL; 4]);
    end_current_season(&mut env);
    close_season(&mut env, &refs).unwrap();

    rebalance(&mut env, &vs[0].vote, 0).unwrap();
    let staked = stake_lamports(&env, &vs[0].vote);

    assert_custom_err(
        rebalance(&mut env, &vs[0].vote, 0),
        code(StakittyError::AlreadyRebalanced),
    );
    assert_eq!(stake_lamports(&env, &vs[0].vote), staked);
}

#[test]
fn rebalance_open_season_fails() {
    let (mut env, vs) = funded_env(4);
    let refs: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &refs, &[SOL; 4]);

    assert_custom_err(
        rebalance(&mut env, &vs[0].vote, 0),
        code(StakittyError::SeasonNotClosed),
    );
}

#[test]
fn rebalance_stakes_from_reserve_and_conserves_principal() {
    let (mut env, vs) = funded_env(4);
    let refs: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &refs, &[SOL; 4]);
    end_current_season(&mut env);
    close_season(&mut env, &refs).unwrap();
    let prize = lamports(&env, &env.prize_vault);

    for v in &vs {
        rebalance(&mut env, &v.vote, 0).unwrap();
        assert_eq!(stake_lamports(&env, &v.vote), share(2_125));
    }

    // 85% is staked, 15% is never delegated; the prize vault was never touched.
    assert_eq!(
        lamports(&env, &env.reserve),
        RESERVE_RENT_FLOOR + share(1_500)
    );
    assert_eq!(lamports(&env, &env.prize_vault), prize);
}

#[test]
fn stopped_payer_is_deactivated_and_returned_after_cooldown() {
    let (mut env, vs) = funded_env(5);
    let all: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &all, &[SOL; 5]);
    end_current_season(&mut env);
    close_season(&mut env, &all).unwrap();
    for v in &vs {
        rebalance(&mut env, &v.vote, 0).unwrap();
    }
    let quitter = &vs[4];
    let quitter_stake = stake_lamports(&env, &quitter.vote);
    assert_eq!(quitter_stake, share(1_700));

    // Season 1: the fifth validator stops paying.
    sponsor_all(&mut env, &all[..4], &[SOL; 4]);
    end_current_season(&mut env);
    close_season(&mut env, &all[..4]).unwrap();
    rebalance(&mut env, &quitter.vote, 1).unwrap();

    // Deactivated this epoch: still locked until the cooldown passes.
    let epoch = season_state(&env, 1).end_epoch;
    assert_custom_err(
        settle(&mut env, &quitter.vote),
        code(StakittyError::NothingToSettle),
    );

    set_epoch(&mut env, epoch + 1);
    let reserve_before = lamports(&env, &env.reserve);
    settle(&mut env, &quitter.vote).unwrap();

    assert_eq!(stake_lamports(&env, &quitter.vote), 0);
    assert_eq!(lamports(&env, &env.reserve), reserve_before + quitter_stake);
}

#[test]
fn weight_increase_waits_one_epoch_before_merge() {
    let (mut env, vs) = funded_env(4);
    let refs: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &refs, &[SOL; 4]);
    end_current_season(&mut env);
    close_season(&mut env, &refs).unwrap();
    // Only one validator rebalanced, so the reserve keeps liquidity for the next increase.
    rebalance(&mut env, &vs[0].vote, 0).unwrap();
    assert_eq!(stake_lamports(&env, &vs[0].vote), share(2_125));

    // Season 1: validator 0 pays 7/10 -> 59.5% of principal, capped at 35%.
    sponsor_all(&mut env, &refs, &[7 * SOL, SOL, SOL, SOL]);
    end_current_season(&mut env);
    close_season(&mut env, &refs).unwrap();
    rebalance(&mut env, &vs[0].vote, 1).unwrap();
    let transient = transient_pda(&env, &vs[0].vote);
    assert_eq!(lamports(&env, &transient), share(3_500) - share(2_125));

    // Same epoch: transient is still activating, merge must wait.
    assert_custom_err(
        settle(&mut env, &vs[0].vote),
        code(StakittyError::NothingToSettle),
    );

    let epoch = season_state(&env, 1).end_epoch;
    set_epoch(&mut env, epoch + 1);
    settle(&mut env, &vs[0].vote).unwrap();

    assert_eq!(stake_lamports(&env, &vs[0].vote), share(3_500));
    assert_eq!(lamports(&env, &transient), 0);
}

#[test]
fn weight_decrease_splits_and_returns_after_cooldown() {
    let (mut env, vs) = funded_env(4);
    let refs: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &refs, &[SOL; 4]);
    end_current_season(&mut env);
    close_season(&mut env, &refs).unwrap();
    rebalance(&mut env, &vs[0].vote, 0).unwrap();

    // Season 1: validator 0 pays 1/10 of the total -> 8.5% of principal.
    sponsor_all(&mut env, &refs, &[SOL, 3 * SOL, 3 * SOL, 3 * SOL]);
    end_current_season(&mut env);
    close_season(&mut env, &refs).unwrap();
    let reserve_before = lamports(&env, &env.reserve);
    rebalance(&mut env, &vs[0].vote, 1).unwrap();
    assert_eq!(stake_lamports(&env, &vs[0].vote), share(850));

    let epoch = season_state(&env, 1).end_epoch;
    set_epoch(&mut env, epoch + 1);
    settle(&mut env, &vs[0].vote).unwrap();

    // The split-off principal is back in the reserve, rent prefund included.
    assert_eq!(
        lamports(&env, &env.reserve),
        reserve_before + share(2_125) - share(850)
    );
    assert_eq!(lamports(&env, &transient_pda(&env, &vs[0].vote)), 0);
}

#[test]
fn attack_prefunding_stake_pda_cannot_block_rebalance() {
    let (mut env, vs) = funded_env(4);
    let refs: Vec<&Validator> = vs.iter().collect();
    sponsor_all(&mut env, &refs, &[SOL; 4]);
    end_current_season(&mut env);
    close_season(&mut env, &refs).unwrap();
    // A create_account-based flow would fail on a PDA that already holds lamports.
    let stake = stake_pda(&env, &vs[0].vote);
    env.svm.airdrop(&stake, SOL).unwrap();
    let reserve_before = lamports(&env, &env.reserve);

    rebalance(&mut env, &vs[0].vote, 0).unwrap();

    assert_eq!(stake_lamports(&env, &vs[0].vote), share(2_125));
    // The donation counts toward the target, so the reserve sends 1 SOL less.
    assert_eq!(
        lamports(&env, &env.reserve),
        reserve_before - (share(2_125) - SOL)
    );
}
