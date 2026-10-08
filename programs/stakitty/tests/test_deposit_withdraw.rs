mod common;

use {
    common::*,
    solana_keypair::Keypair,
    solana_signer::Signer,
    stakitty::{error::StakittyError, state::cumulative_at},
};

#[test]
fn deposit_below_one_sol_is_accepted() {
    let mut env = setup();
    let alice = new_user(&mut env, SOL);

    deposit(&mut env, &alice, SOL / 20).unwrap();

    assert_eq!(user_state(&env, &alice.pubkey()).principal, SOL / 20);
    assert_eq!(pool_state(&env).total_principal, SOL / 20);
    assert_eq!(lamports(&env, &env.reserve), RESERVE_RENT_FLOOR + SOL / 20);
}

#[test]
fn deposit_below_pool_minimum_fails() {
    let mut env = setup();
    let alice = new_user(&mut env, SOL);

    assert_custom_err(
        deposit(&mut env, &alice, MIN_DEPOSIT - 1),
        code(StakittyError::DepositTooSmall),
    );
    assert_eq!(pool_state(&env).total_principal, 0);
}

#[test]
fn withdraw_returns_sol_to_owner() {
    let mut env = setup();
    let alice = new_user(&mut env, SOL);
    deposit(&mut env, &alice, SOL / 2).unwrap();
    let before = lamports(&env, &alice.pubkey());

    withdraw(&mut env, &alice, SOL / 5).unwrap();

    assert_eq!(lamports(&env, &alice.pubkey()), before + SOL / 5);
    assert_eq!(
        user_state(&env, &alice.pubkey()).principal,
        SOL / 2 - SOL / 5
    );
    assert_eq!(pool_state(&env).total_principal, SOL / 2 - SOL / 5);
}

#[test]
fn full_withdraw_keeps_reserve_rent_exempt() {
    let mut env = setup();
    let alice = new_user(&mut env, SOL);
    deposit(&mut env, &alice, SOL / 2).unwrap();

    withdraw(&mut env, &alice, SOL / 2).unwrap();

    assert_eq!(lamports(&env, &env.reserve), RESERVE_RENT_FLOOR);
    assert_eq!(pool_state(&env).total_principal, 0);
}

#[test]
fn attack_withdraw_more_than_deposited_fails() {
    let mut env = setup();
    let alice = new_user(&mut env, 2 * SOL);
    deposit(&mut env, &alice, SOL / 2).unwrap();

    assert_custom_err(
        withdraw(&mut env, &alice, SOL / 2 + 1),
        code(StakittyError::InsufficientPrincipal),
    );
    assert_eq!(user_state(&env, &alice.pubkey()).principal, SOL / 2);
}

#[test]
fn attack_withdraw_twice_fails() {
    let mut env = setup();
    let alice = new_user(&mut env, SOL);
    deposit(&mut env, &alice, SOL / 4).unwrap();

    withdraw(&mut env, &alice, SOL / 4).unwrap();
    assert_custom_err(
        withdraw(&mut env, &alice, SOL / 4),
        code(StakittyError::InsufficientPrincipal),
    );
}

#[test]
fn attack_cannot_drain_other_users_deposits() {
    let mut env = setup();
    let alice = new_user(&mut env, 2 * SOL);
    let mallory = new_user(&mut env, SOL);
    deposit(&mut env, &alice, SOL).unwrap();
    deposit(&mut env, &mallory, SOL / 10).unwrap();

    // The reserve holds 1.1 SOL, but Mallory only owns 0.1 of it.
    assert_custom_err(
        withdraw(&mut env, &mallory, SOL / 5),
        code(StakittyError::InsufficientPrincipal),
    );
    assert_eq!(
        lamports(&env, &env.reserve),
        RESERVE_RENT_FLOOR + SOL + SOL / 10
    );
}

#[test]
fn attack_withdraw_from_someone_elses_account_fails() {
    let mut env = setup();
    let alice = new_user(&mut env, 2 * SOL);
    let mallory = new_user(&mut env, SOL);
    deposit(&mut env, &alice, SOL).unwrap();

    let alice_account = user_pda(&env, &alice.pubkey());
    let ix = withdraw_ix(&env, &mallory.pubkey(), alice_account, env.reserve, SOL);
    let res = send(&mut env.svm, ix, &mallory, &[&mallory]);

    // The user PDA is derived from the signer, so Alice's account fails the seeds check first.
    assert_custom_err(res, CONSTRAINT_SEEDS);
    assert_eq!(user_state(&env, &alice.pubkey()).principal, SOL);
}

#[test]
fn attack_fake_reserve_is_rejected() {
    let mut env = setup();
    let alice = new_user(&mut env, 2 * SOL);
    deposit(&mut env, &alice, SOL).unwrap();
    let fake_reserve = Keypair::new();
    env.svm.airdrop(&fake_reserve.pubkey(), 5 * SOL).unwrap();

    // Depositing into an attacker-chosen account must not credit principal.
    let ix = deposit_ix(
        &env,
        &alice.pubkey(),
        user_pda(&env, &alice.pubkey()),
        fake_reserve.pubkey(),
        SOL / 2,
    );
    assert_custom_err(send(&mut env.svm, ix, &alice, &[&alice]), CONSTRAINT_SEEDS);

    let ix = withdraw_ix(
        &env,
        &alice.pubkey(),
        user_pda(&env, &alice.pubkey()),
        fake_reserve.pubkey(),
        SOL / 2,
    );
    assert_custom_err(send(&mut env.svm, ix, &alice, &[&alice]), CONSTRAINT_SEEDS);
    assert_eq!(user_state(&env, &alice.pubkey()).principal, SOL);
}

#[test]
fn attack_withdraw_more_than_deposited_cannot_take_sponsor_prize() {
    let mut env = setup();
    let alice = new_user(&mut env, 2 * SOL);
    deposit(&mut env, &alice, SOL).unwrap();
    // Stand-in for sponsor payments (see test_sponsor_seasons for the real `sponsor` path).
    env.svm.airdrop(&env.prize_vault, 5 * SOL).unwrap();
    let prize_before = lamports(&env, &env.prize_vault);

    assert_custom_err(
        withdraw(&mut env, &alice, SOL + 1),
        code(StakittyError::InsufficientPrincipal),
    );
    assert_custom_err(
        withdraw(&mut env, &alice, 6 * SOL),
        code(StakittyError::InsufficientPrincipal),
    );

    // Pointing withdraw at the prize vault as if it were the reserve is rejected too.
    let prize_vault = env.prize_vault;
    let ix = withdraw_ix(
        &env,
        &alice.pubkey(),
        user_pda(&env, &alice.pubkey()),
        prize_vault,
        SOL,
    );
    assert_custom_err(send(&mut env.svm, ix, &alice, &[&alice]), CONSTRAINT_SEEDS);

    // A full legitimate withdraw still leaves the prize untouched.
    withdraw(&mut env, &alice, SOL).unwrap();
    assert_eq!(lamports(&env, &env.prize_vault), prize_before);
    assert_eq!(lamports(&env, &env.reserve), RESERVE_RENT_FLOOR);
    assert_eq!(pool_state(&env).total_principal, 0);
}

#[test]
fn attack_zero_withdraw_fails() {
    let mut env = setup();
    let alice = new_user(&mut env, SOL);
    deposit(&mut env, &alice, SOL / 10).unwrap();

    assert_custom_err(
        withdraw(&mut env, &alice, 0),
        code(StakittyError::ZeroAmount),
    );
}

#[test]
fn attack_reopening_account_fails() {
    let mut env = setup();
    let alice = new_user(&mut env, SOL);
    deposit(&mut env, &alice, SOL / 10).unwrap();

    let ix = open_ix(&env, &alice.pubkey());
    assert!(send(&mut env.svm, ix, &alice, &[&alice]).is_err());
    assert_eq!(user_state(&env, &alice.pubkey()).principal, SOL / 10);
}

#[test]
fn attack_last_minute_deposit_gets_proportional_weight() {
    let mut env = setup();
    let t0 = 1_000_000;
    set_time(&mut env, t0);
    let alice = new_user(&mut env, 2 * SOL);
    let mallory = new_user(&mut env, 2 * SOL);

    deposit(&mut env, &alice, SOL).unwrap();
    set_time(&mut env, t0 + 999);
    deposit(&mut env, &mallory, SOL).unwrap();
    let draw_time = t0 + 1_000;

    let a = user_state(&env, &alice.pubkey());
    let m = user_state(&env, &mallory.pubkey());
    let p = pool_state(&env);
    let alice_weight = cumulative_at(
        a.cumulative_weight,
        a.principal,
        a.last_update_ts,
        draw_time,
    )
    .unwrap();
    let mallory_weight = cumulative_at(
        m.cumulative_weight,
        m.principal,
        m.last_update_ts,
        draw_time,
    )
    .unwrap();
    let pool_weight = cumulative_at(
        p.cumulative_weight,
        p.total_principal,
        p.last_update_ts,
        draw_time,
    )
    .unwrap();

    assert_eq!(alice_weight, u128::from(SOL) * 1_000);
    assert_eq!(mallory_weight, u128::from(SOL));
    // Pool accumulator equals the sum of users': step 3 relies on this to verify the snapshot total.
    assert_eq!(pool_weight, alice_weight + mallory_weight);
}

#[test]
fn withdraw_stops_weight_accrual() {
    let mut env = setup();
    set_time(&mut env, 500);
    let alice = new_user(&mut env, 2 * SOL);
    deposit(&mut env, &alice, SOL).unwrap();
    set_time(&mut env, 600);
    withdraw(&mut env, &alice, SOL).unwrap();

    let a = user_state(&env, &alice.pubkey());
    let later = cumulative_at(a.cumulative_weight, a.principal, a.last_update_ts, 10_000).unwrap();
    assert_eq!(later, u128::from(SOL) * 100);
}

#[test]
fn new_pool_id_starts_a_separate_pool() {
    let mut env = setup();
    let alice = new_user(&mut env, 2 * SOL);
    deposit(&mut env, &alice, SOL).unwrap();
    let admin = env.admin.insecure_clone();

    // Same id again: the PDA already exists.
    let ix = initialize_pool_ix(&admin.pubkey(), POOL_ID, 5);
    assert!(send(&mut env.svm, ix, &admin, &[&admin]).is_err());

    let ix = initialize_pool_ix(&admin.pubkey(), 7, 5);
    send(&mut env.svm, ix, &admin, &[&admin]).unwrap();

    let other = pool_pda(7);
    assert_ne!(other, env.pool);
    let acc = env.svm.get_account(&other).unwrap();
    let fresh = <stakitty::state::Pool as anchor_lang::AccountDeserialize>::try_deserialize(
        &mut acc.data.as_slice(),
    )
    .unwrap();
    assert_eq!(fresh.pool_id, 7);
    assert_eq!(fresh.season_length_epochs, 5);
    assert_eq!(fresh.total_principal, 0);
    // The original pool is untouched.
    assert_eq!(pool_state(&env).total_principal, SOL);
}
