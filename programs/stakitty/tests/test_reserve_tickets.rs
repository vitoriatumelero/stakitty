mod common;

use {
    common::*,
    solana_keypair::Keypair,
    solana_signer::Signer,
    stakitty::{error::StakittyError, state::SeasonStatus},
};

const PRINCIPAL: u64 = 1_000 * SOL;

fn share_of(total: u64, bps: u64) -> u64 {
    total * bps / 10_000
}

#[test]
fn admin_extension_unblocks_season_with_too_few_sponsors() {
    let mut env = setup();
    let vs: Vec<Validator> = (0..4).map(|_| listed_validator(&mut env)).collect();
    for v in &vs[..3] {
        sponsor(&mut env, v, SOL).unwrap();
    }
    let end = season_state(&env, 0).end_epoch;
    set_epoch(&mut env, end);
    let paying: Vec<&Validator> = vs[..3].iter().collect();
    assert_custom_err(
        close_season(&mut env, &paying),
        code(StakittyError::NotEnoughValidators),
    );
    // Past the end epoch, nobody else can join.
    assert_custom_err(
        sponsor(&mut env, &vs[3], SOL),
        code(StakittyError::SeasonNotOpen),
    );

    extend_season(&mut env, 2).unwrap();
    assert_eq!(season_state(&env, 0).end_epoch, end + 2);

    sponsor(&mut env, &vs[3], SOL).unwrap();
    let all: Vec<&Validator> = vs.iter().collect();
    assert_custom_err(
        close_season(&mut env, &all),
        code(StakittyError::SeasonNotEnded),
    );
    set_epoch(&mut env, end + 2);
    close_season(&mut env, &all).unwrap();
    assert_eq!(season_state(&env, 0).status, SeasonStatus::Closed);
}

#[test]
fn attack_non_admin_cannot_extend_season() {
    let mut env = setup();
    let mallory = Keypair::new();
    env.svm.airdrop(&mallory.pubkey(), SOL).unwrap();
    let end = season_state(&env, 0).end_epoch;

    let ix = extend_season_ix(&env, &mallory.pubkey(), 100);
    assert_custom_err(
        send(&mut env.svm, ix, &mallory, &[&mallory]),
        code(StakittyError::Unauthorized),
    );
    assert_eq!(season_state(&env, 0).end_epoch, end);
}

#[test]
fn extend_by_zero_epochs_fails() {
    let mut env = setup();
    assert_custom_err(
        extend_season(&mut env, 0),
        code(StakittyError::InvalidSeasonLength),
    );
}

/// 1000 SOL principal, 4 equal validators, all rebalanced: 850 staked, 150 liquid.
fn staked_env() -> (Env, Keypair, Vec<Validator>) {
    let mut env = setup();
    let whale = new_user(&mut env, PRINCIPAL + SOL);
    deposit(&mut env, &whale, PRINCIPAL).unwrap();
    let vs: Vec<Validator> = (0..4).map(|_| listed_validator(&mut env)).collect();
    let refs: Vec<&Validator> = vs.iter().collect();
    for v in &vs {
        sponsor(&mut env, v, SOL).unwrap();
    }
    let end = season_state(&env, 0).end_epoch;
    set_epoch(&mut env, end);
    close_season(&mut env, &refs).unwrap();
    for v in &vs {
        rebalance(&mut env, &v.vote, 0).unwrap();
    }
    assert_eq!(
        lamports(&env, &env.reserve),
        RESERVE_RENT_FLOOR + share_of(PRINCIPAL, 1_500)
    );
    (env, whale, vs)
}

#[test]
fn ticket_is_paid_after_next_rebalance_unstakes() {
    let (mut env, whale, vs) = staked_env();
    let ask = 400 * SOL;
    assert_custom_err(
        withdraw(&mut env, &whale, ask),
        code(StakittyError::InsufficientLiquidity),
    );

    request_withdraw(&mut env, &whale, ask).unwrap();
    let pool = pool_state(&env);
    assert_eq!(pool.total_principal, PRINCIPAL - ask);
    assert_eq!(pool.pending_withdrawals, ask);
    assert_eq!(user_state(&env, &whale.pubkey()).principal, PRINCIPAL - ask);
    // Only 150 SOL is liquid: the claim has to wait.
    assert_custom_err(
        claim_withdraw(&mut env, &whale),
        code(StakittyError::InsufficientLiquidity),
    );

    // Season 1 stake is sized from the reduced principal, so each validator shrinks.
    let refs: Vec<&Validator> = vs.iter().collect();
    for v in &vs {
        sponsor(&mut env, v, SOL).unwrap();
    }
    let end = season_state(&env, 1).end_epoch;
    set_epoch(&mut env, end);
    close_season(&mut env, &refs).unwrap();
    for v in &vs {
        rebalance(&mut env, &v.vote, 1).unwrap();
    }
    set_epoch(&mut env, end + 1);
    for v in &vs {
        settle(&mut env, &v.vote).unwrap();
    }

    let before = lamports(&env, &whale.pubkey());
    let ticket = ticket_pda(&env, &whale.pubkey());
    let ticket_rent = lamports(&env, &ticket);
    claim_withdraw(&mut env, &whale).unwrap();

    assert_eq!(lamports(&env, &whale.pubkey()), before + ask + ticket_rent);
    assert_eq!(lamports(&env, &ticket), 0);
    assert_eq!(pool_state(&env).pending_withdrawals, 0);
    // What is left liquid is exactly the fixed 15% of the remaining principal.
    assert_eq!(
        lamports(&env, &env.reserve),
        RESERVE_RENT_FLOOR + share_of(PRINCIPAL - ask, 1_500)
    );
}

#[test]
fn attack_instant_withdraw_cannot_spend_ticket_liquidity() {
    let (mut env, whale, _vs) = staked_env();
    request_withdraw(&mut env, &whale, 400 * SOL).unwrap();
    let bob = new_user(&mut env, 20 * SOL);
    deposit(&mut env, &bob, 10 * SOL).unwrap();

    // The reserve holds 160 SOL, but all of it is owed to the whale's ticket.
    assert_custom_err(
        withdraw(&mut env, &bob, 10 * SOL),
        code(StakittyError::InsufficientLiquidity),
    );
    assert_eq!(user_state(&env, &bob.pubkey()).principal, 10 * SOL);
}

#[test]
fn attack_claim_twice_fails() {
    let mut env = setup();
    let alice = new_user(&mut env, 2 * SOL);
    deposit(&mut env, &alice, SOL).unwrap();
    request_withdraw(&mut env, &alice, SOL / 2).unwrap();

    claim_withdraw(&mut env, &alice).unwrap();
    assert_custom_err(claim_withdraw(&mut env, &alice), ACCOUNT_NOT_INITIALIZED);
    assert_eq!(lamports(&env, &env.reserve), RESERVE_RENT_FLOOR + SOL / 2);
}

#[test]
fn attack_claim_someone_elses_ticket_fails() {
    let mut env = setup();
    let alice = new_user(&mut env, 2 * SOL);
    let mallory = new_user(&mut env, SOL);
    deposit(&mut env, &alice, SOL).unwrap();
    request_withdraw(&mut env, &alice, SOL).unwrap();

    let ix = claim_withdraw_ix(&env, &mallory.pubkey(), ticket_pda(&env, &alice.pubkey()));
    assert_custom_err(
        send(&mut env.svm, ix, &mallory, &[&mallory]),
        CONSTRAINT_SEEDS,
    );
    assert_eq!(pool_state(&env).pending_withdrawals, SOL);
}

#[test]
fn attack_request_more_than_principal_fails() {
    let mut env = setup();
    let alice = new_user(&mut env, 2 * SOL);
    deposit(&mut env, &alice, SOL).unwrap();

    assert_custom_err(
        request_withdraw(&mut env, &alice, SOL + 1),
        code(StakittyError::InsufficientPrincipal),
    );
    assert_custom_err(
        request_withdraw(&mut env, &alice, 0),
        code(StakittyError::ZeroAmount),
    );
    assert_eq!(pool_state(&env).pending_withdrawals, 0);
}

#[test]
fn second_ticket_while_one_is_open_fails() {
    let mut env = setup();
    let alice = new_user(&mut env, 2 * SOL);
    deposit(&mut env, &alice, SOL).unwrap();
    request_withdraw(&mut env, &alice, SOL / 4).unwrap();

    assert!(request_withdraw(&mut env, &alice, SOL / 4).is_err());
    assert_eq!(pool_state(&env).pending_withdrawals, SOL / 4);
    assert_eq!(user_state(&env, &alice.pubkey()).principal, SOL - SOL / 4);
}
