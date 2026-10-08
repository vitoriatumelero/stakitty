#![allow(dead_code)]

use {
    anchor_lang::{
        prelude::{Clock, Pubkey},
        solana_program::{
            instruction::{AccountMeta, Instruction},
            system_program,
        },
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    ephemeral_vrf_sdk::consts::{scoped_vrf_identity, DEFAULT_QUEUE, VRF_PROGRAM_ID},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signature::Signature,
    solana_signer::Signer,
    solana_stake_interface::stake_history::{StakeHistory, StakeHistoryEntry},
    solana_transaction::versioned::VersionedTransaction,
    stakitty::{
        error::StakittyError,
        merkle,
        state::{Pool, Round, Season, UserAccount, ValidatorEntry},
        FEE_SEED, POOL_SEED, PRIZE_SEED, RESERVE_SEED, ROUND_SEED, SEASON_SEED, SPONSORSHIP_SEED,
        STAKE_CONFIG_ID, STAKE_HISTORY_ID, STAKE_PROGRAM_ID, STAKE_SEED, TICKET_SEED,
        TRANSIENT_SEED, USER_SEED, VALIDATOR_SEED, VOTE_PROGRAM_ID,
    },
};

pub const SOL: u64 = 1_000_000_000;
pub const POOL_ID: u16 = 0;
pub const MIN_DEPOSIT: u64 = SOL / 1_000;
pub const SEASON_EPOCHS: u64 = 3;
pub const RESERVE_RENT_FLOOR: u64 = 890_880;
pub const CONSTRAINT_SEEDS: u32 = 2006;
pub const ACCOUNT_NOT_INITIALIZED: u32 = 3012;
/// VoteStateV3 size; version tag 2 = V3.
const VOTE_ACCOUNT_LEN: usize = 3762;

pub struct Env {
    pub svm: LiteSVM,
    pub admin: Keypair,
    pub pool: Pubkey,
    pub reserve: Pubkey,
    pub prize_vault: Pubkey,
    pub fee_vault: Pubkey,
}

pub struct Validator {
    pub vote: Pubkey,
    pub withdrawer: Keypair,
}

pub fn send(
    svm: &mut LiteSVM,
    ix: Instruction,
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<(), String> {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let res = svm
        .send_transaction(tx)
        .map(|_| ())
        .map_err(|e| format!("{:?}\n{}", e.err, e.meta.logs.join("\n")));
    // Identical retries (e.g. "withdraw twice") must not be deduped as the same signature.
    svm.expire_blockhash();
    res
}

pub fn assert_custom_err(res: Result<(), String>, code: u32) {
    let err = res.expect_err("transaction should have failed");
    assert!(
        err.contains(&format!("Custom({code})")),
        "expected Custom({code}), got {err}"
    );
}

pub fn code(err: StakittyError) -> u32 {
    err.into()
}

pub fn season_pda(pool: &Pubkey, index: u32) -> Pubkey {
    Pubkey::find_program_address(
        &[SEASON_SEED, pool.as_ref(), &index.to_le_bytes()],
        &stakitty::id(),
    )
    .0
}

pub fn setup() -> Env {
    setup_with(LiteSVM::new())
}

/// Same pool, plus MagicBlock's VRF program and oracle queue copied from devnet, with
/// sigverify off so a test can act as the VRF identity PDA in `consume_randomness`.
pub fn setup_vrf() -> Env {
    let mut svm = LiteSVM::new().with_sigverify(false);
    svm.add_program(
        VRF_PROGRAM_ID,
        include_bytes!("../fixtures/ephemeral_vrf.so"),
    )
    .unwrap();
    let queue = include_bytes!("../fixtures/oracle_queue.bin").to_vec();
    let lamports = svm.minimum_balance_for_rent_exemption(queue.len());
    svm.set_account(
        DEFAULT_QUEUE,
        solana_account::Account {
            lamports,
            data: queue,
            owner: VRF_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
    setup_with(svm)
}

fn setup_with(mut svm: LiteSVM) -> Env {
    let program_id = stakitty::id();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/stakitty.so"
    ));
    svm.add_program(program_id, bytes).unwrap();

    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 100 * SOL).unwrap();
    let pool = pool_pda(POOL_ID);
    let reserve = Pubkey::find_program_address(&[RESERVE_SEED, pool.as_ref()], &program_id).0;
    let prize_vault = Pubkey::find_program_address(&[PRIZE_SEED, pool.as_ref()], &program_id).0;
    let fee_vault = Pubkey::find_program_address(&[FEE_SEED, pool.as_ref()], &program_id).0;

    let ix = Instruction::new_with_bytes(
        program_id,
        &stakitty::instruction::InitializePool {
            pool_id: POOL_ID,
            min_deposit: MIN_DEPOSIT,
            season_length_epochs: SEASON_EPOCHS,
        }
        .data(),
        stakitty::accounts::InitializePool {
            authority: admin.pubkey(),
            pool,
            reserve,
            prize_vault,
            fee_vault,
            first_season: season_pda(&pool, 0),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, ix, &admin, &[&admin]).unwrap();
    Env {
        svm,
        admin,
        pool,
        reserve,
        prize_vault,
        fee_vault,
    }
}

pub fn user_pda(env: &Env, owner: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[USER_SEED, env.pool.as_ref(), owner.as_ref()],
        &stakitty::id(),
    )
    .0
}

pub fn open_ix(env: &Env, owner: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::OpenAccount {}.data(),
        stakitty::accounts::OpenAccount {
            owner: *owner,
            pool: env.pool,
            user_account: user_pda(env, owner),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

pub fn new_user(env: &mut Env, lamports: u64) -> Keypair {
    let user = Keypair::new();
    env.svm.airdrop(&user.pubkey(), lamports).unwrap();
    let ix = open_ix(env, &user.pubkey());
    send(&mut env.svm, ix, &user, &[&user]).unwrap();
    user
}

pub fn deposit_ix(
    env: &Env,
    owner: &Pubkey,
    user_account: Pubkey,
    reserve: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::Deposit { amount }.data(),
        stakitty::accounts::Deposit {
            owner: *owner,
            pool: env.pool,
            reserve,
            user_account,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

pub fn withdraw_ix(
    env: &Env,
    owner: &Pubkey,
    user_account: Pubkey,
    reserve: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::Withdraw { amount }.data(),
        stakitty::accounts::Withdraw {
            owner: *owner,
            pool: env.pool,
            reserve,
            user_account,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// Admin pays fees so the owner's lamport delta is exactly the amount moved.
pub fn deposit(env: &mut Env, user: &Keypair, amount: u64) -> Result<(), String> {
    let ix = deposit_ix(
        env,
        &user.pubkey(),
        user_pda(env, &user.pubkey()),
        env.reserve,
        amount,
    );
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin, user])
}

pub fn withdraw(env: &mut Env, user: &Keypair, amount: u64) -> Result<(), String> {
    let ix = withdraw_ix(
        env,
        &user.pubkey(),
        user_pda(env, &user.pubkey()),
        env.reserve,
        amount,
    );
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin, user])
}

pub fn pool_state(env: &Env) -> Pool {
    let acc = env.svm.get_account(&env.pool).unwrap();
    Pool::try_deserialize(&mut acc.data.as_slice()).unwrap()
}

pub fn user_state(env: &Env, owner: &Pubkey) -> UserAccount {
    let acc = env.svm.get_account(&user_pda(env, owner)).unwrap();
    UserAccount::try_deserialize(&mut acc.data.as_slice()).unwrap()
}

pub fn season_state(env: &Env, index: u32) -> Season {
    let acc = env.svm.get_account(&season_pda(&env.pool, index)).unwrap();
    Season::try_deserialize(&mut acc.data.as_slice()).unwrap()
}

pub fn entry_pda(env: &Env, vote: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[VALIDATOR_SEED, env.pool.as_ref(), vote.as_ref()],
        &stakitty::id(),
    )
    .0
}

pub fn entry_state(env: &Env, vote: &Pubkey) -> ValidatorEntry {
    let acc = env.svm.get_account(&entry_pda(env, vote)).unwrap();
    ValidatorEntry::try_deserialize(&mut acc.data.as_slice()).unwrap()
}

pub fn stake_pda(env: &Env, vote: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[STAKE_SEED, env.pool.as_ref(), vote.as_ref()],
        &stakitty::id(),
    )
    .0
}

pub fn transient_pda(env: &Env, vote: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[TRANSIENT_SEED, env.pool.as_ref(), vote.as_ref()],
        &stakitty::id(),
    )
    .0
}

pub fn sponsorship_pda(env: &Env, season: u32, vote: &Pubkey) -> Pubkey {
    let season = season_pda(&env.pool, season);
    Pubkey::find_program_address(
        &[SPONSORSHIP_SEED, season.as_ref(), vote.as_ref()],
        &stakitty::id(),
    )
    .0
}

pub fn lamports(env: &Env, key: &Pubkey) -> u64 {
    env.svm.get_account(key).map(|a| a.lamports).unwrap_or(0)
}

pub fn set_time(env: &mut Env, unix_timestamp: i64) {
    let mut clock: Clock = env.svm.get_sysvar();
    clock.unix_timestamp = unix_timestamp;
    env.svm.set_sysvar(&clock);
}

/// Moves the clock to `epoch` and backfills StakeHistory for every prior epoch, which the
/// Stake program requires to be contiguous. Zero activating/deactivating cluster stake means
/// any transition started in an earlier epoch is complete: one epoch of warmup or cooldown.
pub fn set_epoch(env: &mut Env, epoch: u64) {
    let mut clock: Clock = env.svm.get_sysvar();
    clock.epoch = epoch;
    clock.slot = epoch * 432_000;
    env.svm.set_sysvar(&clock);

    let mut history = StakeHistory::default();
    for past in 0..epoch {
        history.add(
            past,
            StakeHistoryEntry {
                effective: 1_000_000 * SOL,
                activating: 0,
                deactivating: 0,
            },
        );
    }
    env.svm.set_sysvar(&history);
}

/// Writes a minimal initialized V3 vote account. Everything after the withdrawer is zero,
/// which is a valid empty vote state (no votes, no credits).
pub fn create_vote_account(env: &mut Env, withdrawer: &Pubkey) -> Pubkey {
    let vote = Pubkey::new_unique();
    let mut data = vec![0u8; VOTE_ACCOUNT_LEN];
    data[0..4].copy_from_slice(&2u32.to_le_bytes());
    data[4..36].copy_from_slice(Pubkey::new_unique().as_ref());
    data[36..68].copy_from_slice(withdrawer.as_ref());
    let lamports = env.svm.minimum_balance_for_rent_exemption(VOTE_ACCOUNT_LEN);
    env.svm
        .set_account(
            vote,
            solana_account::Account {
                lamports,
                data,
                owner: VOTE_PROGRAM_ID,
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();
    vote
}

pub fn add_validator_ix(env: &Env, vote: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::AddValidator {}.data(),
        stakitty::accounts::AddValidator {
            authority: env.admin.pubkey(),
            pool: env.pool,
            vote_account: *vote,
            validator_entry: entry_pda(env, vote),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// Creates a vote account, lists it, and funds its withdrawer.
pub fn listed_validator(env: &mut Env) -> Validator {
    let withdrawer = Keypair::new();
    env.svm.airdrop(&withdrawer.pubkey(), 50 * SOL).unwrap();
    let vote = create_vote_account(env, &withdrawer.pubkey());
    let ix = add_validator_ix(env, &vote);
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin]).unwrap();
    Validator { vote, withdrawer }
}

pub fn sponsor_ix(env: &Env, signer: &Pubkey, vote: &Pubkey, amount: u64) -> Instruction {
    let season = pool_state(env).current_season;
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::Sponsor { amount }.data(),
        stakitty::accounts::Sponsor {
            withdrawer: *signer,
            payer: *signer,
            pool: env.pool,
            prize_vault: env.prize_vault,
            vote_account: *vote,
            validator_entry: entry_pda(env, vote),
            season: season_pda(&env.pool, season),
            sponsorship: sponsorship_pda(env, season, vote),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

pub fn sponsor(env: &mut Env, v: &Validator, amount: u64) -> Result<(), String> {
    let ix = sponsor_ix(env, &v.withdrawer.pubkey(), &v.vote, amount);
    send(&mut env.svm, ix, &v.withdrawer, &[&v.withdrawer])
}

pub fn close_season_ix(env: &Env, sponsorships: &[Pubkey]) -> Instruction {
    let index = pool_state(env).current_season;
    let mut metas = stakitty::accounts::CloseSeason {
        payer: env.admin.pubkey(),
        pool: env.pool,
        season: season_pda(&env.pool, index),
        next_season: season_pda(&env.pool, index + 1),
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    metas.extend(
        sponsorships
            .iter()
            .map(|k| AccountMeta::new_readonly(*k, false)),
    );
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::CloseSeason {}.data(),
        metas,
    )
}

/// Closes the current season passing every sponsorship of `paying`, sorted by vote account.
pub fn close_season(env: &mut Env, paying: &[&Validator]) -> Result<(), String> {
    let index = pool_state(env).current_season;
    let mut votes: Vec<Pubkey> = paying.iter().map(|v| v.vote).collect();
    votes.sort();
    let sponsorships: Vec<Pubkey> = votes
        .iter()
        .map(|v| sponsorship_pda(env, index, v))
        .collect();
    let ix = close_season_ix(env, &sponsorships);
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin])
}

pub fn rebalance(env: &mut Env, vote: &Pubkey, season: u32) -> Result<(), String> {
    let ix = Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::Rebalance {}.data(),
        stakitty::accounts::Rebalance {
            pool: env.pool,
            season: season_pda(&env.pool, season),
            validator_entry: entry_pda(env, vote),
            vote_account: *vote,
            reserve: env.reserve,
            stake_account: stake_pda(env, vote),
            transient_stake: transient_pda(env, vote),
            clock: clock_id(),
            rent: rent_id(),
            stake_history: STAKE_HISTORY_ID,
            stake_config: STAKE_CONFIG_ID,
            stake_program: STAKE_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin])
}

pub fn settle(env: &mut Env, vote: &Pubkey) -> Result<(), String> {
    let ix = Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::SettleStake {}.data(),
        stakitty::accounts::SettleStake {
            pool: env.pool,
            validator_entry: entry_pda(env, vote),
            vote_account: *vote,
            reserve: env.reserve,
            stake_account: stake_pda(env, vote),
            transient_stake: transient_pda(env, vote),
            clock: clock_id(),
            stake_history: STAKE_HISTORY_ID,
            stake_program: STAKE_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin])
}

pub fn clock_id() -> Pubkey {
    anchor_lang::prelude::pubkey!("SysvarC1ock11111111111111111111111111111111")
}

pub fn rent_id() -> Pubkey {
    anchor_lang::prelude::pubkey!("SysvarRent111111111111111111111111111111111")
}

pub fn ticket_pda(env: &Env, owner: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[TICKET_SEED, env.pool.as_ref(), owner.as_ref()],
        &stakitty::id(),
    )
    .0
}

pub fn extend_season_ix(env: &Env, authority: &Pubkey, additional_epochs: u64) -> Instruction {
    let index = pool_state(env).current_season;
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::ExtendSeason { additional_epochs }.data(),
        stakitty::accounts::ExtendSeason {
            authority: *authority,
            pool: env.pool,
            season: season_pda(&env.pool, index),
        }
        .to_account_metas(None),
    )
}

pub fn extend_season(env: &mut Env, additional_epochs: u64) -> Result<(), String> {
    let admin = env.admin.insecure_clone();
    let ix = extend_season_ix(env, &admin.pubkey(), additional_epochs);
    send(&mut env.svm, ix, &admin, &[&admin])
}

pub fn request_withdraw(env: &mut Env, user: &Keypair, amount: u64) -> Result<(), String> {
    let ix = Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::RequestWithdraw { amount }.data(),
        stakitty::accounts::RequestWithdraw {
            owner: user.pubkey(),
            pool: env.pool,
            user_account: user_pda(env, &user.pubkey()),
            ticket: ticket_pda(env, &user.pubkey()),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin, user])
}

pub fn claim_withdraw_ix(env: &Env, owner: &Pubkey, ticket: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::ClaimWithdraw {}.data(),
        stakitty::accounts::ClaimWithdraw {
            owner: *owner,
            pool: env.pool,
            reserve: env.reserve,
            ticket,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// Admin pays fees so the owner's lamport delta is the payout plus the ticket's rent refund.
pub fn claim_withdraw(env: &mut Env, user: &Keypair) -> Result<(), String> {
    let ix = claim_withdraw_ix(env, &user.pubkey(), ticket_pda(env, &user.pubkey()));
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin, user])
}

pub fn round_pda(pool: &Pubkey, season: u32) -> Pubkey {
    Pubkey::find_program_address(
        &[ROUND_SEED, pool.as_ref(), &season.to_le_bytes()],
        &stakitty::id(),
    )
    .0
}

pub fn round_state(env: &Env, season: u32) -> Round {
    let acc = env.svm.get_account(&round_pda(&env.pool, season)).unwrap();
    Round::try_deserialize(&mut acc.data.as_slice()).unwrap()
}

/// Merkle tree over `(owner, weight)` leaves, laid out as contiguous ticket ranges.
pub struct Tree {
    pub leaves: Vec<(Pubkey, u128, u128)>,
    layers: Vec<Vec<[u8; 32]>>,
}

impl Tree {
    pub fn new(weights: &[(Pubkey, u128)]) -> Self {
        let mut start = 0u128;
        let leaves: Vec<(Pubkey, u128, u128)> = weights
            .iter()
            .map(|(owner, w)| {
                let leaf = (*owner, start, start + w);
                start += w;
                leaf
            })
            .collect();
        let mut layers = vec![leaves
            .iter()
            .map(|(o, a, b)| merkle::leaf_hash(o, *a, *b))
            .collect::<Vec<_>>()];
        while layers.last().unwrap().len() > 1 {
            let next = layers
                .last()
                .unwrap()
                .chunks(2)
                .map(|pair| {
                    if pair.len() == 2 {
                        merkle::node_hash(&pair[0], &pair[1])
                    } else {
                        pair[0]
                    }
                })
                .collect();
            layers.push(next);
        }
        Tree { leaves, layers }
    }

    pub fn root(&self) -> [u8; 32] {
        self.layers.last().unwrap()[0]
    }

    pub fn total(&self) -> u128 {
        self.leaves.last().map_or(0, |l| l.2)
    }

    /// Siblings bottom-up; a promoted odd node contributes no sibling at that level.
    pub fn proof(&self, mut index: usize) -> Vec<[u8; 32]> {
        let mut proof = Vec::new();
        for layer in &self.layers[..self.layers.len() - 1] {
            let sibling = index ^ 1;
            if sibling < layer.len() {
                proof.push(layer[sibling]);
            }
            index /= 2;
        }
        proof
    }
}

pub fn commit_round_ix(
    env: &Env,
    authority: &Pubkey,
    season: u32,
    root: [u8; 32],
    total_weight: u128,
    leaf_count: u32,
    with_previous: bool,
) -> Instruction {
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::CommitRound {
            merkle_root: root,
            total_weight,
            leaf_count,
        }
        .data(),
        stakitty::accounts::CommitRound {
            authority: *authority,
            pool: env.pool,
            season: season_pda(&env.pool, season),
            previous_season: with_previous.then(|| season_pda(&env.pool, season.saturating_sub(1))),
            round: round_pda(&env.pool, season),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

pub fn commit_round(env: &mut Env, season: u32, tree: &Tree) -> Result<(), String> {
    let admin = env.admin.insecure_clone();
    let ix = commit_round_ix(
        env,
        &admin.pubkey(),
        season,
        tree.root(),
        tree.total(),
        tree.leaves.len() as u32,
        season > 0,
    );
    send(&mut env.svm, ix, &admin, &[&admin])
}

pub fn request_draw(env: &mut Env, season: u32) -> Result<(), String> {
    let admin = env.admin.insecure_clone();
    let ix = Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::RequestDraw {}.data(),
        stakitty::accounts::RequestDraw {
            payer: admin.pubkey(),
            pool: env.pool,
            round: round_pda(&env.pool, season),
            oracle_queue: DEFAULT_QUEUE,
            program_identity: Pubkey::find_program_address(&[b"identity"], &stakitty::id()).0,
            vrf_program: VRF_PROGRAM_ID,
            slot_hashes: anchor_lang::prelude::pubkey!(
                "SysvarS1otHashes111111111111111111111111111"
            ),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, ix, &admin, &[&admin])
}

/// Sends `consume_randomness` claiming `identity` as signer. Needs `setup_vrf` (sigverify off):
/// a real transaction can only carry that signature via the VRF program's CPI.
pub fn consume_randomness_as(
    env: &mut Env,
    identity: Pubkey,
    season: u32,
    randomness: [u8; 32],
) -> Result<(), String> {
    let ix = Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::ConsumeRandomness { randomness }.data(),
        stakitty::accounts::ConsumeRandomness {
            vrf_program_identity: identity,
            round: round_pda(&env.pool, season),
        }
        .to_account_metas(None),
    );
    let msg = Message::new_with_blockhash(
        &[ix],
        Some(&env.admin.pubkey()),
        &env.svm.latest_blockhash(),
    );
    let signers = usize::from(msg.header.num_required_signatures);
    let tx = VersionedTransaction {
        signatures: vec![Signature::default(); signers],
        message: VersionedMessage::Legacy(msg),
    };
    let res = env
        .svm
        .send_transaction(tx)
        .map(|_| ())
        .map_err(|e| format!("{:?}\n{}", e.err, e.meta.logs.join("\n")));
    env.svm.expire_blockhash();
    res
}

pub fn vrf_identity() -> Pubkey {
    scoped_vrf_identity(&stakitty::id())
}

/// Randomness whose low 16 bytes decode to `ticket` (assuming `ticket < total_weight`).
pub fn randomness_for(ticket: u128) -> [u8; 32] {
    let mut r = [0u8; 32];
    r[..16].copy_from_slice(&ticket.to_le_bytes());
    r
}

pub fn claim_prize_ix(
    env: &Env,
    winner: &Pubkey,
    season: u32,
    start: u128,
    end: u128,
    proof: Vec<[u8; 32]>,
) -> Instruction {
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::ClaimPrize {
            range_start: start,
            range_end: end,
            proof,
        }
        .data(),
        stakitty::accounts::ClaimPrize {
            winner: *winner,
            pool: env.pool,
            prize_vault: env.prize_vault,
            round: round_pda(&env.pool, season),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// Claims with leaf `index` of `tree`, signed by `user`. Admin pays fees.
pub fn claim_prize(
    env: &mut Env,
    user: &Keypair,
    season: u32,
    tree: &Tree,
    index: usize,
) -> Result<(), String> {
    let (_, start, end) = tree.leaves[index];
    let ix = claim_prize_ix(env, &user.pubkey(), season, start, end, tree.proof(index));
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin, user])
}

pub fn expire_prize(env: &mut Env, season: u32) -> Result<(), String> {
    let ix = Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::ExpirePrize {}.data(),
        stakitty::accounts::ExpirePrize {
            pool: env.pool,
            round: round_pda(&env.pool, season),
        }
        .to_account_metas(None),
    );
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin])
}

pub fn harvest_yield(env: &mut Env) -> Result<(), String> {
    let ix = Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::HarvestYield {}.data(),
        stakitty::accounts::HarvestYield {
            pool: env.pool,
            reserve: env.reserve,
            prize_vault: env.prize_vault,
            fee_vault: env.fee_vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let admin = env.admin.insecure_clone();
    send(&mut env.svm, ix, &admin, &[&admin])
}

pub fn withdraw_fees_ix(env: &Env, authority: &Pubkey, amount: u64) -> Instruction {
    Instruction::new_with_bytes(
        stakitty::id(),
        &stakitty::instruction::WithdrawFees { amount }.data(),
        stakitty::accounts::WithdrawFees {
            authority: *authority,
            pool: env.pool,
            fee_vault: env.fee_vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

pub fn pool_pda(pool_id: u16) -> Pubkey {
    Pubkey::find_program_address(&[POOL_SEED, &pool_id.to_le_bytes()], &stakitty::id()).0
}

/// `initialize_pool` for an arbitrary id, with every PDA derived from that pool.
pub fn initialize_pool_ix(admin: &Pubkey, pool_id: u16, season_length_epochs: u64) -> Instruction {
    let program_id = stakitty::id();
    let pool = pool_pda(pool_id);
    let vault = |seed: &[u8]| Pubkey::find_program_address(&[seed, pool.as_ref()], &program_id).0;
    Instruction::new_with_bytes(
        program_id,
        &stakitty::instruction::InitializePool {
            pool_id,
            min_deposit: MIN_DEPOSIT,
            season_length_epochs,
        }
        .data(),
        stakitty::accounts::InitializePool {
            authority: *admin,
            pool,
            reserve: vault(RESERVE_SEED),
            prize_vault: vault(PRIZE_SEED),
            fee_vault: vault(FEE_SEED),
            first_season: season_pda(&pool, 0),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}
