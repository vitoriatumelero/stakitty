/**
 * End-to-end run against a local Surfnet (no VRF: there is no oracle offline).
 *
 *   surfpool start --offline --ci --no-deploy
 *   anchor deploy --provider.cluster http://127.0.0.1:8899
 *   RPC_URL=http://127.0.0.1:8899 WALLET=~/.config/solana/id.json npm run e2e:surfpool
 *
 * Flow: pool -> 2 users -> 5 real vote accounts -> season 0 (5 sponsors) -> rebalance ->
 * season 1 (4 sponsors, bob withdraws mid-season) -> quitter deactivated -> settle ->
 * season 1 list rebuilt from events, checked against the accumulator, committed.
 */
import assert from 'node:assert/strict';
import { BN } from '@anchor-lang/core';
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_CLOCK_PUBKEY,
  SYSVAR_RENT_PUBKEY,
  sendAndConfirmTransaction,
  Transaction,
  VoteInit,
  VoteProgram,
} from '@solana/web3.js';
import { leafHash, verify } from './lib/merkle.ts';
import { buildRound, commitRound, connect, loadKeypair, POOL_ID, poolPda, PROGRAM_ID, roundPda, seasonPda } from './lib/stakitty.ts';

const STAKE_PROGRAM = new PublicKey('Stake11111111111111111111111111111111111111');
const STAKE_CONFIG = new PublicKey('StakeConfig11111111111111111111111111111111');
const STAKE_HISTORY = new PublicKey('SysvarStakeHistory1111111111111111111111111');
const SEASON_EPOCHS = 2;

const rpcUrl = process.env.RPC_URL;
const walletPath = process.env.WALLET;
if (!rpcUrl || !walletPath) throw new Error('set RPC_URL and WALLET');
const admin = loadKeypair(walletPath.replace(/^~/, process.env.HOME ?? '~'));
const program = connect(rpcUrl, admin);
const connection = program.provider.connection;

const pool = poolPda();
const pda = (...seeds: (Buffer | PublicKey)[]): PublicKey =>
  PublicKey.findProgramAddressSync(
    seeds.map((s) => (s instanceof PublicKey ? s.toBuffer() : s)),
    PROGRAM_ID,
  )[0];
const reserve = pda(Buffer.from('reserve'), pool);
const prizeVault = pda(Buffer.from('prize'), pool);
const feeVault = pda(Buffer.from('fee'), pool);

const sol = (n: number): BN => new BN(Math.round(n * LAMPORTS_PER_SOL));
const log = (msg: string): void => console.log(`- ${msg}`);

async function rpc<T>(method: string, params: unknown[]): Promise<T> {
  const res = await fetch(rpcUrl as string, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
  });
  const body = (await res.json()) as { result?: T; error?: unknown };
  if (body.error) throw new Error(`${method}: ${JSON.stringify(body.error)}`);
  return body.result as T;
}

async function travelToEpoch(epoch: number): Promise<void> {
  await rpc('surfnet_timeTravel', [{ absoluteEpoch: epoch }]);
  // Let the clock settle on the new epoch before the next transaction.
  for (let i = 0; i < 50 && (await connection.getEpochInfo()).epoch < epoch; i++) {
    await new Promise((r) => setTimeout(r, 100));
  }
}

async function funded(lamports: number): Promise<Keypair> {
  const kp = Keypair.generate();
  const sig = await connection.requestAirdrop(kp.publicKey, lamports);
  await connection.confirmTransaction(sig, 'confirmed');
  return kp;
}

interface Validator {
  vote: PublicKey;
  withdrawer: Keypair;
}

async function createVoteAccount(): Promise<Validator> {
  const withdrawer = await funded(20 * LAMPORTS_PER_SOL);
  const node = Keypair.generate();
  const vote = Keypair.generate();
  const lamports = await connection.getMinimumBalanceForRentExemption(VoteProgram.space);
  const tx = VoteProgram.createAccount({
    fromPubkey: withdrawer.publicKey,
    votePubkey: vote.publicKey,
    voteInit: new VoteInit(node.publicKey, node.publicKey, withdrawer.publicKey, 0),
    lamports,
  });
  await sendAndConfirmTransaction(connection, tx, [withdrawer, vote, node], { commitment: 'confirmed' });
  return { vote: vote.publicKey, withdrawer };
}

async function stakeState(account: PublicKey): Promise<string> {
  const info = await connection.getAccountInfo(account);
  if (!info || !info.owner.equals(STAKE_PROGRAM)) return 'none';
  const data = info.data;
  const activation = data.readBigUInt64LE(164);
  const deactivation = data.readBigUInt64LE(172);
  const sol = (info.lamports / LAMPORTS_PER_SOL).toFixed(3);
  return deactivation === 0xffff_ffff_ffff_ffffn
    ? `${sol} SOL delegated (activation epoch ${activation})`
    : `${sol} SOL deactivating (epoch ${deactivation})`;
}

async function openAndDeposit(user: Keypair, amount: BN): Promise<void> {
  const userAccount = pda(Buffer.from('user'), pool, user.publicKey);
  await program.methods
    .openAccount()
    .accountsStrict({ owner: user.publicKey, pool, userAccount, systemProgram: SystemProgram.programId })
    .signers([user])
    .rpc();
  await program.methods
    .deposit(amount)
    .accountsStrict({ owner: user.publicKey, pool, reserve, userAccount, systemProgram: SystemProgram.programId })
    .signers([user])
    .rpc();
}

async function sponsorAll(validators: Validator[], season: number, amount: BN): Promise<void> {
  for (const v of validators) {
    await program.methods
      .sponsor(amount)
      .accountsStrict({
        withdrawer: v.withdrawer.publicKey,
        payer: v.withdrawer.publicKey,
        pool,
        prizeVault,
        voteAccount: v.vote,
        validatorEntry: pda(Buffer.from('validator'), pool, v.vote),
        season: seasonPda(season),
        sponsorship: pda(Buffer.from('sponsorship'), seasonPda(season), v.vote),
        systemProgram: SystemProgram.programId,
      })
      .signers([v.withdrawer])
      .rpc();
  }
}

async function closeSeason(season: number, paying: Validator[]): Promise<void> {
  const votes = paying.map((v) => v.vote).sort((a, b) => Buffer.compare(a.toBuffer(), b.toBuffer()));
  await program.methods
    .closeSeason()
    .accountsStrict({
      payer: admin.publicKey,
      pool,
      season: seasonPda(season),
      nextSeason: seasonPda(season + 1),
      systemProgram: SystemProgram.programId,
    })
    .remainingAccounts(
      votes.map((vote) => ({
        pubkey: pda(Buffer.from('sponsorship'), seasonPda(season), vote),
        isSigner: false,
        isWritable: false,
      })),
    )
    .rpc();
}

function stakeAccounts(vote: PublicKey): { stakeAccount: PublicKey; transientStake: PublicKey } {
  return {
    stakeAccount: pda(Buffer.from('stake'), pool, vote),
    transientStake: pda(Buffer.from('transient'), pool, vote),
  };
}

async function rebalance(vote: PublicKey, season: number): Promise<void> {
  await program.methods
    .rebalance()
    .accountsStrict({
      pool,
      season: seasonPda(season),
      validatorEntry: pda(Buffer.from('validator'), pool, vote),
      voteAccount: vote,
      reserve,
      ...stakeAccounts(vote),
      clock: SYSVAR_CLOCK_PUBKEY,
      rent: SYSVAR_RENT_PUBKEY,
      stakeHistory: STAKE_HISTORY,
      stakeConfig: STAKE_CONFIG,
      stakeProgram: STAKE_PROGRAM,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
}

async function settle(vote: PublicKey): Promise<void> {
  await program.methods
    .settleStake()
    .accountsStrict({
      pool,
      validatorEntry: pda(Buffer.from('validator'), pool, vote),
      voteAccount: vote,
      reserve,
      ...stakeAccounts(vote),
      clock: SYSVAR_CLOCK_PUBKEY,
      stakeHistory: STAKE_HISTORY,
      stakeProgram: STAKE_PROGRAM,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
}

async function main(): Promise<void> {
  console.log(`Surfnet ${rpcUrl}, admin ${admin.publicKey.toBase58()}`);

  await program.methods
    .initializePool(POOL_ID, sol(0.001), new BN(SEASON_EPOCHS))
    .accountsStrict({
      authority: admin.publicKey,
      pool,
      reserve,
      prizeVault,
      feeVault,
      firstSeason: seasonPda(0),
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  log('pool initialized');

  const alice = await funded(400 * LAMPORTS_PER_SOL);
  const bob = await funded(200 * LAMPORTS_PER_SOL);
  await openAndDeposit(alice, sol(300));
  await openAndDeposit(bob, sol(100));
  log('alice deposited 300 SOL, bob 100 SOL');

  const validators: Validator[] = [];
  for (let i = 0; i < 5; i++) {
    const v = await createVoteAccount();
    await program.methods
      .addValidator()
      .accountsStrict({
        authority: admin.publicKey,
        pool,
        voteAccount: v.vote,
        validatorEntry: pda(Buffer.from('validator'), pool, v.vote),
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    validators.push(v);
  }
  log('5 vote accounts created by the Vote program and listed');

  // Season 0.
  await sponsorAll(validators, 0, sol(1));
  let season0 = await program.account.season.fetch(seasonPda(0));
  await travelToEpoch(season0.endEpoch.toNumber());
  await closeSeason(0, validators);
  season0 = await program.account.season.fetch(seasonPda(0));
  log(`season 0 closed: weights ${season0.weights.map((w) => w.weightBps).join('/')} bps, reserve ${season0.reserveBps} bps`);

  for (const v of validators) await rebalance(v.vote, 0);
  log(`rebalanced: ${await stakeState(stakeAccounts(validators[0].vote).stakeAccount)}`);
  log(`reserve holds ${((await connection.getBalance(reserve)) / LAMPORTS_PER_SOL).toFixed(3)} SOL`);

  // Season 1: bob withdraws mid-season, the fifth validator stops paying.
  const quitter = validators[4];
  await sponsorAll(validators.slice(0, 4), 1, sol(1));
  const bobAccount = pda(Buffer.from('user'), pool, bob.publicKey);
  await program.methods
    .withdraw(sol(40))
    .accountsStrict({ owner: bob.publicKey, pool, reserve, userAccount: bobAccount, systemProgram: SystemProgram.programId })
    .signers([bob])
    .rpc();
  log('bob withdrew 40 SOL mid-season');
  const season1Open = await program.account.season.fetch(seasonPda(1));
  await travelToEpoch(season1Open.endEpoch.toNumber());
  await closeSeason(1, validators.slice(0, 4));
  log('season 1 closed with 4 sponsors');

  await rebalance(quitter.vote, 1);
  log(`quitter after rebalance: ${await stakeState(stakeAccounts(quitter.vote).stakeAccount)}`);
  const epoch = (await connection.getEpochInfo()).epoch;
  await travelToEpoch(epoch + 1);
  const reserveBefore = await connection.getBalance(reserve);
  await settle(quitter.vote);
  const back = (await connection.getBalance(reserve)) - reserveBefore;
  log(`settled one epoch later: ${(back / LAMPORTS_PER_SOL).toFixed(3)} SOL back in the reserve`);
  assert.equal(await stakeState(stakeAccounts(quitter.vote).stakeAccount), 'none');

  // Round for season 1, rebuilt from events.
  const { json, tree } = await buildRound(program, 1);
  log(`season 1 list: ${json.leaves.map((l) => `${l.owner.slice(0, 4)}=${l.weight}`).join(', ')}`);
  json.leaves.forEach((leaf, i) => assert.ok(verify(tree.proof(i), tree.root, leafHash(tree.leaves[i]))));
  const sig = await commitRound(program, json);
  const round = await program.account.round.fetch(roundPda(1));
  assert.equal(Buffer.from(round.merkleRoot).toString('hex'), json.merkleRoot);
  log(`round 1 committed (${sig.slice(0, 16)}...): prize ${round.prize.toNumber() / LAMPORTS_PER_SOL} SOL`);
  log(`fee vault ${feeVault.toBase58().slice(0, 8)}..., prize vault ${((await connection.getBalance(prizeVault)) / LAMPORTS_PER_SOL).toFixed(3)} SOL`);
  console.log('E2E OK (VRF draw is covered by the devnet spike and the LiteSVM tests)');
}

main().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
