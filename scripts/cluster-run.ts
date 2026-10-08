/**
 * Idempotent crank for a slow, real cluster (devnet): each run reads on-chain state and does
 * whatever is due, then exits. Run it once per epoch boundary (~28 h on devnet).
 *
 *   RPC_URL=https://api.devnet.solana.com WALLET=~/.config/solana/id.json POOL_ID=1 \
 *     npm run cluster-run -- setup      # once: pool, 4 vote accounts, 2 users, season 0 sponsors
 *   ... npm run cluster-run -- advance   # after each epoch boundary
 *
 * Demo keypairs (users, vote accounts, withdrawers) are throwaway and saved under
 * `.stakitty-run/` (gitignored). Refuses mainnet.
 *
 * Plan: season 0 all pay 0.01 -> rebalance (stake activates over the next boundary) ->
 * season 1: Validator A pays less -> its stake is split off and deactivated -> settle after the
 * real cooldown. Round 1 (season 0 sponsorship as prize) is drawn by MagicBlock's VRF.
 */
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from '@solana/web3.js';
import { Flow, fmtSol, mockVrfFulfill, sol, STAKE_PROGRAM, type Validator } from './lib/flow.ts';
import { buildRound, commitRound, connect, loadKeypair, POOL_ID, roundPda, seasonPda, type RoundJson } from './lib/stakitty.ts';

const { RPC_URL, WALLET } = process.env;
if (!RPC_URL || !WALLET) throw new Error('set RPC_URL and WALLET');
if (/mainnet/.test(RPC_URL)) throw new Error('cluster-run is for test clusters only');
const cluster = /devnet/.test(RPC_URL) ? 'devnet' : `custom&customUrl=${encodeURIComponent(RPC_URL)}`;
// LOCAL TESTING ONLY: deliver randomness through the VRF mock instead of waiting for an oracle.
const MOCK_VRF = process.env.MOCK_VRF === '1' && /127\.0\.0\.1|localhost/.test(RPC_URL);
const explorer = (sig: string): string => `https://explorer.solana.com/tx/${sig}?cluster=${cluster}`;

const admin = loadKeypair(WALLET.replace(/^~/, process.env.HOME ?? '~'));
const flow = new Flow(connect(RPC_URL, admin), admin);
const program = flow.program;
const statePath = `.stakitty-run/pool-${POOL_ID}.json`;

// Deposits scale up on clusters where the minimum delegation is 1 SOL (local validator).
const SCALE = Number.parseFloat(process.env.AMOUNT_SCALE ?? '1');
const SPONSOR = 0.01;
const SPONSOR_LOW = 0.002;
const SEASON_EPOCHS = Number.parseInt(process.env.SEASON_EPOCHS ?? '1', 10);

interface RunState {
  validators: { label: string; vote: string; withdrawer: number[] }[];
  users: { label: string; secret: number[] }[];
  withdrawDone: boolean;
}

const log = (msg: string): void => console.log(`- ${msg}`);
const tx = (label: string, sig: string): void => console.log(`- ${label}\n    ${explorer(sig)}`);

function loadState(): RunState {
  if (!existsSync(statePath)) throw new Error(`no ${statePath}: run "setup" first`);
  return JSON.parse(readFileSync(statePath, 'utf8')) as RunState;
}

function saveState(state: RunState): void {
  mkdirSync('.stakitty-run', { recursive: true });
  writeFileSync(statePath, JSON.stringify(state, null, 2), { mode: 0o600 });
}

const validatorsOf = (state: RunState): Validator[] =>
  state.validators.map((v) => ({
    label: v.label,
    vote: new PublicKey(v.vote),
    withdrawer: Keypair.fromSecretKey(Uint8Array.from(v.withdrawer)),
  }));
const usersOf = (state: RunState): { label: string; kp: Keypair }[] =>
  state.users.map((u) => ({ label: u.label, kp: Keypair.fromSecretKey(Uint8Array.from(u.secret)) }));

/** Anchor error name of a failed transaction, if any. */
function anchorError(err: unknown): string | undefined {
  const logs = (err as { logs?: string[]; transactionLogs?: string[] }).logs ?? (err as { transactionLogs?: string[] }).transactionLogs;
  return logs?.join('\n').match(/Error Code: (\w+)/)?.[1];
}

function sponsorAmount(season: number, label: string): number {
  return season === 1 && label === 'Validator A' ? SPONSOR_LOW : SPONSOR;
}

/** Resumable: skips whatever already exists, so a failed run can simply be repeated. */
async function setup(): Promise<void> {
  log(`admin ${admin.publicKey.toBase58()}: ${fmtSol(await flow.balance(admin.publicKey))} SOL`);
  log(`stake minimum delegation on this cluster: ${fmtSol(await minimumDelegation())} SOL`);
  const state: RunState = existsSync(statePath) ? loadState() : { validators: [], users: [], withdrawDone: false };
  if (!(await flow.connection.getAccountInfo(flow.pool, 'confirmed'))) {
    tx(`initialize_pool id=${POOL_ID}, season = ${SEASON_EPOCHS} epoch`, await flow.initializePool(sol(0.001), SEASON_EPOCHS));
  }

  for (const label of ['Validator A', 'Validator B', 'Validator C', 'Validator D']) {
    if (state.validators.some((v) => v.label === label)) continue;
    const withdrawer = Keypair.generate();
    // Vote account rent + ~5 seasons of sponsorship and sponsorship-account rent.
    await flow.fundFromAdmin(withdrawer.publicKey, 0.1 * LAMPORTS_PER_SOL);
    const v = await flow.createVoteAccount(withdrawer, label);
    state.validators.push({ label, vote: v.vote.toBase58(), withdrawer: [...withdrawer.secretKey] });
    saveState(state);
  }
  for (const v of validatorsOf(state)) {
    if (await flow.connection.getAccountInfo(flow.entry(v.vote), 'confirmed')) continue;
    tx(`${v.label} listed (vote ${v.vote.toBase58()})`, await flow.addValidator(v.vote));
  }

  for (const [label, base] of [['alice', 0.3], ['bob', 0.2]] as const) {
    const amount = base * SCALE;
    let user = state.users.find((u) => u.label === label);
    if (!user) {
      const kp = Keypair.generate();
      await flow.fundFromAdmin(kp.publicKey, (amount + 0.01) * LAMPORTS_PER_SOL);
      user = { label, secret: [...kp.secretKey] };
      state.users.push(user);
      saveState(state);
    }
    const kp = Keypair.fromSecretKey(Uint8Array.from(user.secret));
    if (await flow.connection.getAccountInfo(flow.userAccount(kp.publicKey), 'confirmed')) continue;
    tx(`${label} deposits ${amount} SOL`, await flow.openAndDeposit(kp, sol(amount)));
  }

  await payMissingSponsors(validatorsOf(state), 0);
  const season = await program.account.season.fetch(seasonPda(0));
  log(`season 0 ends at epoch ${season.endEpoch} (now ${await flow.epoch()}): run "advance" after that boundary`);
}

/** Read through the Stake program's GetMinimumDelegation return data (simulation only). */
async function minimumDelegation(): Promise<number> {
  const ix = new TransactionInstruction({ programId: STAKE_PROGRAM, keys: [], data: Buffer.from([13, 0, 0, 0]) });
  const message = new TransactionMessage({
    payerKey: admin.publicKey,
    recentBlockhash: (await flow.connection.getLatestBlockhash('confirmed')).blockhash,
    instructions: [ix],
  }).compileToV0Message();
  const sim = await flow.connection.simulateTransaction(new VersionedTransaction(message), { sigVerify: false });
  const data = sim.value.returnData?.data?.[0];
  if (!data) throw new Error('GetMinimumDelegation returned no data');
  return Number(Buffer.from(data, 'base64').readBigUInt64LE(0));
}

async function payMissingSponsors(validators: Validator[], season: number): Promise<void> {
  for (const v of validators) {
    const sponsorship = flow.pda(Buffer.from('sponsorship'), seasonPda(season), v.vote);
    if (await flow.connection.getAccountInfo(sponsorship, 'confirmed')) continue;
    const amount = sponsorAmount(season, v.label);
    tx(`${v.label} sponsors season ${season} (${amount} SOL)`, await flow.sponsor(v, season, sol(amount)));
  }
}

async function advance(): Promise<void> {
  const state = loadState();
  const validators = validatorsOf(state);
  const users = usersOf(state);
  const epoch = await flow.epoch();
  let pool = await program.account.pool.fetch(flow.pool);
  log(`epoch ${epoch}, pool ${POOL_ID} season ${pool.currentSeason}`);

  // 1. Close the open season once its end epoch is reached, then fund the next one.
  const open = await program.account.season.fetch(seasonPda(pool.currentSeason));
  if (epoch >= open.endEpoch.toNumber()) {
    const paid: PublicKey[] = [];
    for (const v of validators) {
      const sponsorship = flow.pda(Buffer.from('sponsorship'), seasonPda(pool.currentSeason), v.vote);
      if (await flow.connection.getAccountInfo(sponsorship, 'confirmed')) paid.push(v.vote);
    }
    tx(`close_season ${pool.currentSeason}`, await flow.closeSeason(pool.currentSeason, paid));
    pool = await program.account.pool.fetch(flow.pool);
  }
  await payMissingSponsors(validators, pool.currentSeason);

  // 2. Settle anything whose warmup or cooldown finished; this also unblocks rebalances.
  for (const v of validators) {
    try {
      tx(`settle_stake ${v.label}`, await flow.settle(v.vote));
    } catch (err) {
      if (anchorError(err) !== 'NothingToSettle') throw err;
    }
  }

  // 3. Apply the latest closed season's weights.
  const closed = pool.currentSeason - 1;
  if (closed >= 0) {
    for (const v of validators) {
      const entry = await program.account.validatorEntry.fetch(flow.entry(v.vote));
      if (entry.rebalancedThrough > closed) continue;
      try {
        tx(`rebalance ${v.label} (season ${closed})`, await flow.rebalance(v.vote, closed));
      } catch (err) {
        const name = anchorError(err);
        if (name !== 'StakeTransitionPending' && name !== 'StakeNotSettled') throw err;
        log(`${v.label}: ${name}, retry after the next boundary`);
      }
    }
  }

  // 4. Yield, then the round of season 1 (funded by season 0 sponsorship).
  pool = await program.account.pool.fetch(flow.pool);
  if (pool.realizedYield.toNumber() > 0) tx('harvest_yield', await flow.harvestYield());
  if (pool.currentSeason > 1) await runRound(1, users);

  // 5. One instant withdrawal once the round is paid: principal is always there.
  const round = await program.account.round.fetchNullable(roundPda(1));
  if (round && 'paid' in round.status && !state.withdrawDone) {
    tx(`bob withdraws ${0.05 * SCALE} SOL`, await flow.withdraw(users[1].kp, sol(0.05 * SCALE)));
    state.withdrawDone = true;
    saveState(state);
  }

  for (const v of validators) log(`${v.label}: ${await flow.describeStake(v.vote)}`);
  log(`reserve ${fmtSol(await flow.balance(flow.reserve))} SOL, prize vault ${fmtSol(await flow.balance(flow.prizeVault))} SOL`);
}

async function runRound(season: number, users: { label: string; kp: Keypair }[]): Promise<void> {
  const roundPath = `.stakitty-run/pool-${POOL_ID}-round-${season}.json`;
  let round = await program.account.round.fetchNullable(roundPda(season));
  if (!round) {
    const { json } = await buildRound(program, season);
    writeFileSync(roundPath, `${JSON.stringify(json, null, 2)}\n`);
    tx(`commit_round ${season}: ${json.leaves.length} leaves, root ${json.merkleRoot.slice(0, 16)}...`, await commitRound(program, json));
    round = await program.account.round.fetch(roundPda(season));
  }
  if ('committed' in round.status) {
    tx(`request_draw ${season} (MagicBlock VRF)`, await flow.requestDraw(season));
    if (MOCK_VRF) {
      const { blockhash } = await flow.connection.getLatestBlockhash('confirmed');
      tx('LOCAL MOCK oracle callback', await mockVrfFulfill(flow, season, createHash('sha256').update(blockhash).digest()));
    }
  }
  for (let i = 0; i < 30 && !('settled' in round.status) && !('paid' in round.status); i++) {
    await new Promise((r) => setTimeout(r, 2_000));
    round = await program.account.round.fetch(roundPda(season));
  }
  if ('randomnessRequested' in round.status) {
    log('oracle has not called back yet: run "advance" again later');
    return;
  }
  if ('settled' in round.status) {
    const json = JSON.parse(readFileSync(roundPath, 'utf8')) as RoundJson;
    const ticket = BigInt(round.winningTicket.toString());
    const index = json.leaves.findIndex((l) => BigInt(l.rangeStart) <= ticket && ticket < BigInt(l.rangeEnd));
    const winner = users.find((u) => u.kp.publicKey.toBase58() === json.leaves[index].owner);
    if (!winner) throw new Error(`winning leaf ${index} belongs to an unknown owner`);
    log(`VRF ticket ${ticket} -> ${winner.label}`);
    tx(`claim_prize by ${winner.label} (${fmtSol(round.prize.toString())} SOL)`, await flow.claimPrize(winner.kp, json, index));
  }
}

const command = process.argv[2];
(command === 'setup' ? setup() : command === 'advance' ? advance() : Promise.reject(new Error('usage: cluster-run setup|advance')))
  .catch((err: unknown) => {
    console.error(err);
    process.exit(1);
  });
