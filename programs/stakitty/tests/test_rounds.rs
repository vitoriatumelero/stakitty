mod common;

use {
    anchor_lang::prelude::Pubkey,
    common::*,
    solana_keypair::Keypair,
    solana_signer::Signer,
    stakitty::{error::StakittyError, state::RoundStatus, PRIZE_CLAIM_EPOCHS, STAKE_PROGRAM_ID},
};

const T0: i64 = 1_000_000;
const SEASON_SECONDS: i64 = 1_000;
const CONSTRAINT_ADDRESS: u32 = 2012;

struct RoundEnv {
    env: Env,
    alice: Keypair,
    bob: Keypair,
    vs: Vec<Validator>,
    /// Season 1 weights: alice 3 SOL and bob 1 SOL, both held for the whole season.
    tree: Tree,
}

fn close_with_sponsors(env: &mut Env, vs: &[Validator], at: i64) {
    for v in vs {
        sponsor(env, v, SOL).unwrap();
    }
    let index = pool_state(env).current_season;
    let end = season_state(env, index).end_epoch;
    set_epoch(env, end);
    set_time(env, at);
    let refs: Vec<&Validator> = vs.iter().collect();
    close_season(env, &refs).unwrap();
}

/// Seasons 0 and 1 closed; each had 4 sponsors paying 1 SOL.
fn round_env() -> RoundEnv {
    let mut env = setup_vrf();
    let alice = new_user(&mut env, 5 * SOL);
    let bob = new_user(&mut env, 5 * SOL);
    deposit(&mut env, &alice, 3 * SOL).unwrap();
    deposit(&mut env, &bob, SOL).unwrap();
    let vs: Vec<Validator> = (0..4).map(|_| listed_validator(&mut env)).collect();

    close_with_sponsors(&mut env, &vs, T0);
    close_with_sponsors(&mut env, &vs, T0 + SEASON_SECONDS);

    let secs = SEASON_SECONDS as u128;
    let tree = Tree::new(&[
        (alice.pubkey(), u128::from(3 * SOL) * secs),
        (bob.pubkey(), u128::from(SOL) * secs),
    ]);
    RoundEnv {
        env,
        alice,
        bob,
        vs,
        tree,
    }
}

/// Commits and draws season 1 with a ticket inside bob's range.
fn drawn_round() -> RoundEnv {
    let mut r = round_env();
    commit_round(&mut r.env, 1, &r.tree).unwrap();
    request_draw(&mut r.env, 1).unwrap();
    let bob_ticket = r.tree.leaves[1].1 + 7;
    consume_randomness_as(&mut r.env, vrf_identity(), 1, randomness_for(bob_ticket)).unwrap();
    r
}

#[test]
fn full_round_pays_previous_season_sponsorship_to_winner() {
    let mut r = round_env();
    commit_round(&mut r.env, 1, &r.tree).unwrap();
    let round = round_state(&r.env, 1);
    // Prize for season 1 = what sponsors paid in season 0.
    assert_eq!(round.prize, 4 * SOL);
    assert_eq!(round.total_weight, r.tree.total());
    assert_eq!(pool_state(&r.env).committed_prizes, 4 * SOL);

    request_draw(&mut r.env, 1).unwrap();
    assert_eq!(
        round_state(&r.env, 1).status,
        RoundStatus::RandomnessRequested
    );
    let bob_ticket = r.tree.leaves[1].1 + 7;
    consume_randomness_as(&mut r.env, vrf_identity(), 1, randomness_for(bob_ticket)).unwrap();
    let round = round_state(&r.env, 1);
    assert_eq!(round.status, RoundStatus::Settled);
    assert_eq!(round.winning_ticket, bob_ticket);

    let alice = r.alice.insecure_clone();
    assert_custom_err(
        claim_prize(&mut r.env, &alice, 1, &r.tree, 0),
        code(StakittyError::NotWinningLeaf),
    );

    let bob = r.bob.insecure_clone();
    let before = lamports(&r.env, &bob.pubkey());
    let vault_before = lamports(&r.env, &r.env.prize_vault);
    claim_prize(&mut r.env, &bob, 1, &r.tree, 1).unwrap();

    assert_eq!(lamports(&r.env, &bob.pubkey()), before + 4 * SOL);
    assert_eq!(lamports(&r.env, &r.env.prize_vault), vault_before - 4 * SOL);
    let round = round_state(&r.env, 1);
    assert_eq!(round.status, RoundStatus::Paid);
    assert_eq!(round.winner, bob.pubkey());
    assert_eq!(pool_state(&r.env).committed_prizes, 0);
    // Principal is untouched by the payout.
    assert_eq!(pool_state(&r.env).total_principal, 4 * SOL);
}

#[test]
fn attack_claim_prize_twice_fails() {
    let mut r = drawn_round();
    let bob = r.bob.insecure_clone();
    claim_prize(&mut r.env, &bob, 1, &r.tree, 1).unwrap();

    assert_custom_err(
        claim_prize(&mut r.env, &bob, 1, &r.tree, 1),
        code(StakittyError::InvalidRoundState),
    );
}

#[test]
fn attack_stealing_winners_leaf_fails() {
    let mut r = drawn_round();
    let mallory = Keypair::new();
    r.env.svm.airdrop(&mallory.pubkey(), SOL).unwrap();

    // Bob's range and proof, signed by Mallory: the leaf hash binds the owner.
    let (_, start, end) = r.tree.leaves[1];
    let ix = claim_prize_ix(&r.env, &mallory.pubkey(), 1, start, end, r.tree.proof(1));
    assert_custom_err(
        send(&mut r.env.svm, ix, &mallory, &[&mallory]),
        code(StakittyError::InvalidProof),
    );
    assert_eq!(round_state(&r.env, 1).status, RoundStatus::Settled);
}

#[test]
fn attack_inflated_range_fails() {
    let mut r = drawn_round();
    let alice = r.alice.insecure_clone();

    // Alice claims a range stretched over bob's ticket, with her real proof.
    let ix = claim_prize_ix(
        &r.env,
        &alice.pubkey(),
        1,
        0,
        r.tree.total(),
        r.tree.proof(0),
    );
    let admin = r.env.admin.insecure_clone();
    assert_custom_err(
        send(&mut r.env.svm, ix, &admin, &[&admin, &alice]),
        code(StakittyError::InvalidProof),
    );
}

#[test]
fn attack_commit_with_wrong_total_fails() {
    let mut r = round_env();
    let secs = SEASON_SECONDS as u128;
    // Inflating bob's weight breaks the sum against the on-chain accumulator.
    let forged = Tree::new(&[
        (r.alice.pubkey(), u128::from(3 * SOL) * secs),
        (r.bob.pubkey(), u128::from(2 * SOL) * secs),
    ]);

    assert_custom_err(
        commit_round(&mut r.env, 1, &forged),
        code(StakittyError::WeightMismatch),
    );
}

#[test]
fn attack_non_admin_commit_fails() {
    let mut r = round_env();
    let mallory = Keypair::new();
    r.env.svm.airdrop(&mallory.pubkey(), SOL).unwrap();
    let ix = commit_round_ix(
        &r.env,
        &mallory.pubkey(),
        1,
        r.tree.root(),
        r.tree.total(),
        2,
        true,
    );

    assert_custom_err(
        send(&mut r.env.svm, ix, &mallory, &[&mallory]),
        code(StakittyError::Unauthorized),
    );
}

#[test]
fn attack_commit_round_twice_fails() {
    let mut r = round_env();
    commit_round(&mut r.env, 1, &r.tree).unwrap();

    // Re-committing would let the admin swap the list after seeing the draw.
    assert!(commit_round(&mut r.env, 1, &r.tree).is_err());
    assert_eq!(round_state(&r.env, 1).merkle_root, r.tree.root());
}

#[test]
fn commit_without_previous_season_fails_after_season_zero() {
    let mut r = round_env();
    let admin = r.env.admin.insecure_clone();
    let ix = commit_round_ix(
        &r.env,
        &admin.pubkey(),
        1,
        r.tree.root(),
        r.tree.total(),
        2,
        false,
    );

    assert_custom_err(
        send(&mut r.env.svm, ix, &admin, &[&admin]),
        code(StakittyError::MissingPreviousSeason),
    );
}

#[test]
fn season_zero_round_has_no_sponsor_prize() {
    let mut r = round_env();
    let s0 = season_state(&r.env, 0);
    let secs = (s0.end_ts - s0.start_ts) as u128;
    let tree = Tree::new(&[
        (r.alice.pubkey(), u128::from(3 * SOL) * secs),
        (r.bob.pubkey(), u128::from(SOL) * secs),
    ]);

    commit_round(&mut r.env, 0, &tree).unwrap();
    assert_eq!(round_state(&r.env, 0).prize, 0);
}

#[test]
fn attack_redraw_same_round_fails() {
    let mut r = round_env();
    commit_round(&mut r.env, 1, &r.tree).unwrap();
    request_draw(&mut r.env, 1).unwrap();

    assert_custom_err(
        request_draw(&mut r.env, 1),
        code(StakittyError::InvalidRoundState),
    );
}

#[test]
fn attack_callback_from_wallet_fails() {
    let mut r = round_env();
    commit_round(&mut r.env, 1, &r.tree).unwrap();
    request_draw(&mut r.env, 1).unwrap();

    // Only the VRF program's scoped identity may deliver randomness.
    let wallet = Pubkey::new_unique();
    assert_custom_err(
        consume_randomness_as(&mut r.env, wallet, 1, randomness_for(0)),
        CONSTRAINT_ADDRESS,
    );
    assert_eq!(
        round_state(&r.env, 1).status,
        RoundStatus::RandomnessRequested
    );
}

#[test]
fn attack_callback_twice_cannot_reroll() {
    let mut r = drawn_round();
    let first = round_state(&r.env, 1).winning_ticket;

    assert_custom_err(
        consume_randomness_as(&mut r.env, vrf_identity(), 1, randomness_for(0)),
        code(StakittyError::InvalidRoundState),
    );
    assert_eq!(round_state(&r.env, 1).winning_ticket, first);
}

#[test]
fn unclaimed_prize_expires_into_next_round() {
    let mut r = drawn_round();
    let settled = round_state(&r.env, 1).settled_epoch;

    set_epoch(&mut r.env, settled + PRIZE_CLAIM_EPOCHS);
    assert_custom_err(
        expire_prize(&mut r.env, 1),
        code(StakittyError::ClaimWindowOpen),
    );

    set_epoch(&mut r.env, settled + PRIZE_CLAIM_EPOCHS + 1);
    let bob = r.bob.insecure_clone();
    assert_custom_err(
        claim_prize(&mut r.env, &bob, 1, &r.tree, 1),
        code(StakittyError::ClaimWindowClosed),
    );
    expire_prize(&mut r.env, 1).unwrap();
    assert_eq!(round_state(&r.env, 1).status, RoundStatus::Expired);
    let pool = pool_state(&r.env);
    assert_eq!(pool.committed_prizes, 0);
    assert_eq!(pool.prize_available, 4 * SOL);

    // Season 2: expired prize + season 1 sponsorship.
    let vs: Vec<Validator> =
        r.vs.iter()
            .map(|v| Validator {
                vote: v.vote,
                withdrawer: v.withdrawer.insecure_clone(),
            })
            .collect();
    // The claim window outlasted season 2; the admin extension reopens it for sponsors.
    extend_season(&mut r.env, PRIZE_CLAIM_EPOCHS).unwrap();
    let s2_start = season_state(&r.env, 2).start_ts;
    close_with_sponsors(&mut r.env, &vs, s2_start + SEASON_SECONDS);
    commit_round(&mut r.env, 2, &r.tree).unwrap();
    assert_eq!(round_state(&r.env, 2).prize, 8 * SOL);
    assert_eq!(pool_state(&r.env).prize_available, 0);
}

fn stake_lamports(env: &Env, vote: &Pubkey) -> u64 {
    match env.svm.get_account(&stake_pda(env, vote)) {
        Some(acc) if acc.owner == STAKE_PROGRAM_ID => acc.lamports,
        _ => 0,
    }
}

/// 1000 SOL staked over 5 validators; the fifth earns `reward` lamports, then stops paying
/// and its stake comes back to the reserve with the reward.
fn env_with_realized_yield(reward: u64) -> Env {
    let mut env = setup();
    let whale = new_user(&mut env, 1_001 * SOL);
    deposit(&mut env, &whale, 1_000 * SOL).unwrap();
    let vs: Vec<Validator> = (0..5).map(|_| listed_validator(&mut env)).collect();
    let all: Vec<&Validator> = vs.iter().collect();
    for v in &vs {
        sponsor(&mut env, v, SOL).unwrap();
    }
    let end = season_state(&env, 0).end_epoch;
    set_epoch(&mut env, end);
    close_season(&mut env, &all).unwrap();
    for v in &vs {
        rebalance(&mut env, &v.vote, 0).unwrap();
    }
    let quitter = &vs[4];
    assert_eq!(
        entry_state(&env, &quitter.vote).stake_basis,
        stake_lamports(&env, &quitter.vote)
    );
    // LiteSVM pays no inflation: credit the reward to the stake account directly.
    env.svm
        .airdrop(&stake_pda(&env, &quitter.vote), reward)
        .unwrap();

    for v in &vs[..4] {
        sponsor(&mut env, v, SOL).unwrap();
    }
    let end = season_state(&env, 1).end_epoch;
    set_epoch(&mut env, end);
    close_season(&mut env, &all[..4]).unwrap();
    rebalance(&mut env, &quitter.vote, 1).unwrap();
    set_epoch(&mut env, end + 1);
    settle(&mut env, &quitter.vote).unwrap();
    env
}

#[test]
fn settle_realizes_only_rewards_as_yield() {
    let reward = 2 * SOL;
    let env = env_with_realized_yield(reward);

    assert_eq!(pool_state(&env).realized_yield, reward);
    assert_eq!(pool_state(&env).total_principal, 1_000 * SOL);
}

#[test]
fn harvest_splits_yield_20_fee_80_prize() {
    let reward = 2 * SOL;
    let mut env = env_with_realized_yield(reward);
    let reserve_before = lamports(&env, &env.reserve);
    let fee_before = lamports(&env, &env.fee_vault);
    let prize_before = lamports(&env, &env.prize_vault);

    harvest_yield(&mut env).unwrap();

    assert_eq!(lamports(&env, &env.fee_vault), fee_before + reward / 5);
    assert_eq!(
        lamports(&env, &env.prize_vault),
        prize_before + reward * 4 / 5
    );
    assert_eq!(lamports(&env, &env.reserve), reserve_before - reward);
    let pool = pool_state(&env);
    assert_eq!(pool.realized_yield, 0);
    assert_eq!(pool.prize_available, reward * 4 / 5);

    assert_custom_err(
        harvest_yield(&mut env),
        code(StakittyError::NothingToHarvest),
    );
}

#[test]
fn attack_unharvested_yield_is_not_spendable_principal() {
    let reward = 2 * SOL;
    let mut env = env_with_realized_yield(reward);
    let spendable =
        |env: &Env| pool_state(env).spendable(lamports(env, &env.reserve), RESERVE_RENT_FLOOR);
    // Before the harvest the reward sits in the reserve but is excluded from what
    // withdrawals, tickets and new stake may use.
    let expected = lamports(&env, &env.reserve) - RESERVE_RENT_FLOOR - reward;
    assert_eq!(spendable(&env), expected);

    harvest_yield(&mut env).unwrap();
    assert_eq!(spendable(&env), expected);
}

#[test]
fn only_admin_withdraws_fees() {
    let reward = 5 * SOL;
    let mut env = env_with_realized_yield(reward);
    harvest_yield(&mut env).unwrap();
    let fees = reward / 5;

    let mallory = Keypair::new();
    env.svm.airdrop(&mallory.pubkey(), SOL).unwrap();
    let ix = withdraw_fees_ix(&env, &mallory.pubkey(), fees);
    assert_custom_err(
        send(&mut env.svm, ix, &mallory, &[&mallory]),
        code(StakittyError::Unauthorized),
    );

    let admin = env.admin.insecure_clone();
    let ix = withdraw_fees_ix(&env, &admin.pubkey(), fees + 1);
    assert_custom_err(
        send(&mut env.svm, ix, &admin, &[&admin]),
        code(StakittyError::InsufficientLiquidity),
    );

    let before = lamports(&env, &admin.pubkey());
    let ix = withdraw_fees_ix(&env, &admin.pubkey(), fees);
    send(&mut env.svm, ix, &admin, &[&admin]).unwrap();
    assert_eq!(lamports(&env, &admin.pubkey()), before + fees - 5_000);
    assert_eq!(lamports(&env, &env.fee_vault), RESERVE_RENT_FLOOR);
}
