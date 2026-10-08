/**
 * Recorded demo on a local validator with fast, real epochs (StakeHistory is maintained by the
 * bank, so warmup is real). The VRF draw uses a LOCAL MOCK of the MagicBlock oracle loaded at
 * the VRF program id; the real VRF runs on devnet.
 *
 *   solana-test-validator --reset --slots-per-epoch 64 \
 *     --bpf-program 8dFfRCbNDeYt9y96ZC8uCcU2BbXt2LwWEH91ZUN3BRQg target/deploy/stakitty.so \
 *     --bpf-program Vrf1RNUjXmQGjmQrQLvJHs9SNkvDJEsRVFPkfSQUwGz mocks/vrf-oracle-mock/target/deploy/vrf_oracle_mock.so
 *   RPC_URL=http://127.0.0.1:8899 WALLET=~/.config/solana/id.json LEDGER=test-ledger npm run demo:local
 *
 * deposit -> sponsor -> close_season -> rebalance -> real stake activation -> VRF draw (mock)
 * -> prize payout -> withdraw.
 */
import { createHash } from 'node:crypto';
import { LAMPORTS_PER_SOL } from '@solana/web3.js';
import { Flow, fmtSol, mockVrfFulfill, sol, type Validator } from './lib/flow.ts';
import { buildRound, commitRound, connect, loadKeypair, roundPda, seasonPda } from './lib/stakitty.ts';

const home = (p: string): string => p.replace(/^~/, process.env.HOME ?? '~');
const { RPC_URL, WALLET, LEDGER } = process.env;
if (!RPC_URL || !WALLET || !LEDGER) throw new Error('set RPC_URL, WALLET and LEDGER (test-validator ledger dir)');
if (!/127\.0\.0\.1|localhost/.test(RPC_URL)) throw new Error('demo-local only runs against a local validator');

const ledger = home(LEDGER);
const admin = loadKeypair(home(WALLET));
const flow = new Flow(connect(RPC_URL, admin), admin);
const program = flow.program;

let step = 0;
const say = (title: string): void => console.log(`\n${++step}. ${title}`);
const note = (msg: string): void => console.log(`   ${msg}`);

async function waitEpoch(target: number): Promise<void> {
  let last = -1;
  await flow.waitForEpoch(target, (current) => {
    if (current !== last) note(`... waiting for epoch ${target} (now ${current})`);
    last = current;
  });
}

async function main(): Promise<void> {
  await flow.airdropTo(admin.publicKey, 100 * LAMPORTS_PER_SOL);

  say('Admin creates the pool (season = 1 epoch, 15% reserve, 35% cap)');
  await flow.initializePool(sol(0.001), 1);
  note(`pool ${flow.pool.toBase58()} at epoch ${await flow.epoch()}`);

  say('Users deposit SOL: principal is always withdrawable');
  const alice = await flow.funded(50 * LAMPORTS_PER_SOL);
  const bob = await flow.funded(20 * LAMPORTS_PER_SOL);
  await flow.openAndDeposit(alice, sol(30));
  await flow.openAndDeposit(bob, sol(10));
  note('alice 30 SOL, bob 10 SOL -> reserve');

  say('Admin lists 4 validators (3 new vote accounts + the local validator, which earns rewards)');
  const validators: Validator[] = [];
  for (const label of ['Validator A', 'Validator B', 'Validator C']) {
    validators.push(await flow.createVoteAccount(await flow.funded(10 * LAMPORTS_PER_SOL), label));
  }
  // Its withdrawer is the vote account itself, which can only sign: a funder pays for it.
  const bootstrap = loadKeypair(`${ledger}/vote-account-keypair.json`);
  const bootstrapFunder = await flow.funded(10 * LAMPORTS_PER_SOL);
  validators.push({ vote: bootstrap.publicKey, withdrawer: bootstrap, label: 'Local validator' });
  for (const v of validators) await flow.addValidator(v.vote);

  say('Season 0: validators sponsor (SOL goes to the prize vault, buys next season\'s stake)');
  const payments = [2, 1, 1, 1];
  for (const [i, v] of validators.entries()) {
    await flow.sponsor(v, 0, sol(payments[i]), v.withdrawer === bootstrap ? bootstrapFunder : v.withdrawer);
    note(`${v.label} pays ${payments[i]} SOL`);
  }
  note(`prize vault: ${fmtSol(await flow.balance(flow.prizeVault))} SOL`);

  let season = await program.account.season.fetch(seasonPda(0));
  await waitEpoch(season.endEpoch.toNumber());
  say('close_season: weight = paid / total of the delegable 85%, capped at 35%');
  await flow.closeSeason(0, validators.map((v) => v.vote));
  season = await program.account.season.fetch(seasonPda(0));
  for (const v of validators) {
    const w = season.weights.find((x) => x.voteAccount.equals(v.vote))?.weightBps ?? 0;
    note(`${v.label}: ${(w / 100).toFixed(2)}%`);
  }
  note(`kept liquid in the reserve: ${(season.reserveBps / 100).toFixed(2)}%`);

  say('rebalance: the pool delegates real stake to each validator');
  for (const v of validators) await flow.rebalance(v.vote, 0);
  const activationEpoch = await flow.epoch();
  for (const v of validators) note(`${v.label}: ${await flow.describeStake(v.vote)}`);
  note(`reserve liquid: ${fmtSol(await flow.balance(flow.reserve))} SOL`);

  say('Season 1 sponsors pay; bob withdraws 2 SOL instantly from the 15% reserve');
  for (const v of validators) {
    await flow.sponsor(v, 1, sol(1), v.withdrawer === bootstrap ? bootstrapFunder : v.withdrawer);
  }
  await flow.withdraw(bob, sol(2));
  note('bob: 10 -> 8 SOL principal');

  await waitEpoch(activationEpoch + 1);
  say('Epoch boundary: the stake activation is recorded in the cluster StakeHistory');
  const history = await flow.stakeHistory(activationEpoch);
  if (history) {
    note(`epoch ${activationEpoch}: activating ${fmtSol(history.activating)} SOL, effective ${fmtSol(history.effective)} SOL`);
  }
  for (const v of validators) note(`${v.label}: ${await flow.describeStake(v.vote)}`);

  season = await program.account.season.fetch(seasonPda(1));
  await waitEpoch(season.endEpoch.toNumber());
  say('close_season 1, then the round list is rebuilt from on-chain events');
  await flow.closeSeason(1, validators.map((v) => v.vote));
  const { json } = await buildRound(program, 1);
  for (const leaf of json.leaves) {
    const who = leaf.owner === alice.publicKey.toBase58() ? 'alice' : leaf.owner === bob.publicKey.toBase58() ? 'bob' : leaf.owner;
    note(`${who}: tickets [${leaf.rangeStart}, ${leaf.rangeEnd})`);
  }
  note(`total ${json.totalWeight} = on-chain accumulator (checked), root ${json.merkleRoot.slice(0, 16)}...`);

  say('commit_round: admin publishes the Merkle root; prize = season 0 sponsorship');
  await commitRound(program, json);
  let round = await program.account.round.fetch(roundPda(1));
  note(`prize ${fmtSol(round.prize.toString())} SOL`);

  say('request_draw -> VRF callback picks the winning ticket (LOCAL MOCK oracle; real VRF on devnet)');
  await flow.requestDraw(1);
  const { blockhash } = await flow.connection.getLatestBlockhash('confirmed');
  await mockVrfFulfill(flow, 1, createHash('sha256').update(blockhash).digest());
  round = await program.account.round.fetch(roundPda(1));
  const ticket = BigInt(round.winningTicket.toString());
  const index = json.leaves.findIndex((l) => BigInt(l.rangeStart) <= ticket && ticket < BigInt(l.rangeEnd));
  const winner = json.leaves[index].owner === alice.publicKey.toBase58() ? alice : bob;
  note(`winning ticket ${ticket} -> ${winner === alice ? 'alice' : 'bob'}`);

  say('claim_prize: the winner proves the leaf with the Merkle proof and gets paid');
  const before = await flow.balance(winner.publicKey);
  await flow.claimPrize(winner, json, index);
  note(`${winner === alice ? 'alice' : 'bob'} +${fmtSol((await flow.balance(winner.publicKey)) - before)} SOL`);

  say('Withdraw: principal comes back; nobody lost a lamport');
  await flow.withdraw(alice, sol(3));
  const alicePrincipal = (await program.account.userAccount.fetch(flow.userAccount(alice.publicKey))).principal;
  note(`alice withdrew 3 SOL instantly (principal now ${fmtSol(alicePrincipal.toString())} SOL)`);
  await flow.requestWithdraw(alice, sol(20));
  note('alice queued 20 SOL more as a withdraw ticket: paid once next season unstakes it');

  await waitEpoch((await flow.epoch()) + 1);
  say('Staking rewards: the local validator votes, so its delegation earns real yield');
  for (const v of validators) note(`${v.label}: ${await flow.describeStake(v.vote)}`);
  console.log('\nDemo complete.');
}

main().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
