import { BN, type Program } from '@anchor-lang/core';
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_CLOCK_PUBKEY,
  SYSVAR_RENT_PUBKEY,
  SYSVAR_SLOT_HASHES_PUBKEY,
  sendAndConfirmTransaction,
  Transaction,
  TransactionInstruction,
  VoteInit,
  VoteProgram,
} from '@solana/web3.js';
import type { Stakitty } from '../../target/types/stakitty.ts';
import type { RoundJson } from './stakitty.ts';
import { POOL_ID, poolPda, PROGRAM_ID, roundPda, seasonPda } from './stakitty.ts';

export const STAKE_PROGRAM = new PublicKey('Stake11111111111111111111111111111111111111');
export const STAKE_CONFIG = new PublicKey('StakeConfig11111111111111111111111111111111');
export const STAKE_HISTORY = new PublicKey('SysvarStakeHistory1111111111111111111111111');
export const VRF_PROGRAM = new PublicKey('Vrf1RNUjXmQGjmQrQLvJHs9SNkvDJEsRVFPkfSQUwGz');
export const VRF_QUEUE = new PublicKey('Cuj97ggrhhidhbu39TijNVqE74xvKJ69gDervRUXAxGh');
const U64_MAX = 0xffff_ffff_ffff_ffffn;

export const sol = (n: number): BN => new BN(Math.round(n * LAMPORTS_PER_SOL));
export const fmtSol = (lamports: number | bigint): string => (Number(lamports) / LAMPORTS_PER_SOL).toFixed(4);

export interface Validator {
  vote: PublicKey;
  withdrawer: Keypair;
  label: string;
}

/** Instruction builders and reads for one pool; every method sends and confirms. */
export class Flow {
  readonly pool = poolPda();
  readonly reserve: PublicKey;
  readonly prizeVault: PublicKey;
  readonly feeVault: PublicKey;

  constructor(
    readonly program: Program<Stakitty>,
    readonly admin: Keypair,
  ) {
    this.reserve = this.pda(Buffer.from('reserve'), this.pool);
    this.prizeVault = this.pda(Buffer.from('prize'), this.pool);
    this.feeVault = this.pda(Buffer.from('fee'), this.pool);
  }

  get connection() {
    return this.program.provider.connection;
  }

  pda(...seeds: (Buffer | PublicKey)[]): PublicKey {
    return PublicKey.findProgramAddressSync(
      seeds.map((s) => (s instanceof PublicKey ? s.toBuffer() : s)),
      PROGRAM_ID,
    )[0];
  }

  userAccount(owner: PublicKey): PublicKey {
    return this.pda(Buffer.from('user'), this.pool, owner);
  }

  entry(vote: PublicKey): PublicKey {
    return this.pda(Buffer.from('validator'), this.pool, vote);
  }

  stakeAccounts(vote: PublicKey): { stakeAccount: PublicKey; transientStake: PublicKey } {
    return {
      stakeAccount: this.pda(Buffer.from('stake'), this.pool, vote),
      transientStake: this.pda(Buffer.from('transient'), this.pool, vote),
    };
  }

  async epoch(): Promise<number> {
    return (await this.connection.getEpochInfo('confirmed')).epoch;
  }

  /** Polls until the cluster reaches `epoch` (real epoch boundaries, no time travel). */
  async waitForEpoch(epoch: number, onTick?: (current: number) => void): Promise<void> {
    for (;;) {
      const current = await this.epoch();
      if (current >= epoch) return;
      onTick?.(current);
      await new Promise((r) => setTimeout(r, 2_000));
    }
  }

  async balance(key: PublicKey): Promise<number> {
    return this.connection.getBalance(key, 'confirmed');
  }

  async airdropTo(key: PublicKey, lamports: number): Promise<void> {
    const sig = await this.connection.requestAirdrop(key, lamports);
    const latest = await this.connection.getLatestBlockhash('confirmed');
    await this.connection.confirmTransaction({ signature: sig, ...latest }, 'confirmed');
  }

  /** Transfer from the admin: devnet airdrops are rate-limited. */
  async fundFromAdmin(key: PublicKey, lamports: number): Promise<void> {
    const tx = new Transaction().add(
      SystemProgram.transfer({ fromPubkey: this.admin.publicKey, toPubkey: key, lamports: Math.round(lamports) }),
    );
    await sendAndConfirmTransaction(this.connection, tx, [this.admin], { commitment: 'confirmed' });
  }

  async funded(lamports: number): Promise<Keypair> {
    const kp = Keypair.generate();
    await this.airdropTo(kp.publicKey, lamports);
    return kp;
  }

  async initializePool(minDeposit: BN, seasonEpochs: number): Promise<string> {
    return this.program.methods
      .initializePool(POOL_ID, minDeposit, new BN(seasonEpochs))
      .accountsStrict({
        authority: this.admin.publicKey,
        pool: this.pool,
        reserve: this.reserve,
        prizeVault: this.prizeVault,
        feeVault: this.feeVault,
        firstSeason: seasonPda(0),
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  /** A real vote account created by the Vote program; `withdrawer` funds it. */
  async createVoteAccount(withdrawer: Keypair, label: string): Promise<Validator> {
    const node = Keypair.generate();
    const vote = Keypair.generate();
    const lamports = await this.connection.getMinimumBalanceForRentExemption(VoteProgram.space);
    const tx = VoteProgram.createAccount({
      fromPubkey: withdrawer.publicKey,
      votePubkey: vote.publicKey,
      voteInit: new VoteInit(node.publicKey, node.publicKey, withdrawer.publicKey, 0),
      lamports,
    });
    await sendAndConfirmTransaction(this.connection, tx, [withdrawer, vote, node], { commitment: 'confirmed' });
    return { vote: vote.publicKey, withdrawer, label };
  }

  async addValidator(vote: PublicKey): Promise<string> {
    return this.program.methods
      .addValidator()
      .accountsStrict({
        authority: this.admin.publicKey,
        pool: this.pool,
        voteAccount: vote,
        validatorEntry: this.entry(vote),
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  async openAndDeposit(user: Keypair, amount: BN): Promise<string> {
    const userAccount = this.userAccount(user.publicKey);
    if (!(await this.connection.getAccountInfo(userAccount, 'confirmed'))) {
      await this.program.methods
        .openAccount()
        .accountsStrict({ owner: user.publicKey, pool: this.pool, userAccount, systemProgram: SystemProgram.programId })
        .signers([user])
        .rpc();
    }
    return this.program.methods
      .deposit(amount)
      .accountsStrict({
        owner: user.publicKey,
        pool: this.pool,
        reserve: this.reserve,
        userAccount,
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();
  }

  async withdraw(user: Keypair, amount: BN): Promise<string> {
    return this.program.methods
      .withdraw(amount)
      .accountsStrict({
        owner: user.publicKey,
        pool: this.pool,
        reserve: this.reserve,
        userAccount: this.userAccount(user.publicKey),
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();
  }

  async requestWithdraw(user: Keypair, amount: BN): Promise<string> {
    return this.program.methods
      .requestWithdraw(amount)
      .accountsStrict({
        owner: user.publicKey,
        pool: this.pool,
        userAccount: this.userAccount(user.publicKey),
        ticket: this.pda(Buffer.from('ticket'), this.pool, user.publicKey),
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();
  }

  /** `payer` defaults to the withdrawer; pass another wallet when the withdrawer can't fund. */
  async sponsor(v: Validator, season: number, amount: BN, payer: Keypair = v.withdrawer): Promise<string> {
    const signers = payer.publicKey.equals(v.withdrawer.publicKey) ? [v.withdrawer] : [v.withdrawer, payer];
    return this.program.methods
      .sponsor(amount)
      .accountsStrict({
        withdrawer: v.withdrawer.publicKey,
        payer: payer.publicKey,
        pool: this.pool,
        prizeVault: this.prizeVault,
        voteAccount: v.vote,
        validatorEntry: this.entry(v.vote),
        season: seasonPda(season),
        sponsorship: this.pda(Buffer.from('sponsorship'), seasonPda(season), v.vote),
        systemProgram: SystemProgram.programId,
      })
      .signers(signers)
      .rpc();
  }

  async closeSeason(season: number, paying: PublicKey[]): Promise<string> {
    const votes = [...paying].sort((a, b) => Buffer.compare(a.toBuffer(), b.toBuffer()));
    return this.program.methods
      .closeSeason()
      .accountsStrict({
        payer: this.admin.publicKey,
        pool: this.pool,
        season: seasonPda(season),
        nextSeason: seasonPda(season + 1),
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(
        votes.map((vote) => ({
          pubkey: this.pda(Buffer.from('sponsorship'), seasonPda(season), vote),
          isSigner: false,
          isWritable: false,
        })),
      )
      .rpc();
  }

  async rebalance(vote: PublicKey, season: number): Promise<string> {
    return this.program.methods
      .rebalance()
      .accountsStrict({
        pool: this.pool,
        season: seasonPda(season),
        validatorEntry: this.entry(vote),
        voteAccount: vote,
        reserve: this.reserve,
        ...this.stakeAccounts(vote),
        clock: SYSVAR_CLOCK_PUBKEY,
        rent: SYSVAR_RENT_PUBKEY,
        stakeHistory: STAKE_HISTORY,
        stakeConfig: STAKE_CONFIG,
        stakeProgram: STAKE_PROGRAM,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  async settle(vote: PublicKey): Promise<string> {
    return this.program.methods
      .settleStake()
      .accountsStrict({
        pool: this.pool,
        validatorEntry: this.entry(vote),
        voteAccount: vote,
        reserve: this.reserve,
        ...this.stakeAccounts(vote),
        clock: SYSVAR_CLOCK_PUBKEY,
        stakeHistory: STAKE_HISTORY,
        stakeProgram: STAKE_PROGRAM,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  async harvestYield(): Promise<string> {
    return this.program.methods
      .harvestYield()
      .accountsStrict({
        pool: this.pool,
        reserve: this.reserve,
        prizeVault: this.prizeVault,
        feeVault: this.feeVault,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  async requestDraw(season: number): Promise<string> {
    return this.program.methods
      .requestDraw()
      .accountsStrict({
        payer: this.admin.publicKey,
        pool: this.pool,
        round: roundPda(season),
        oracleQueue: VRF_QUEUE,
        programIdentity: PublicKey.findProgramAddressSync([Buffer.from('identity')], PROGRAM_ID)[0],
        vrfProgram: VRF_PROGRAM,
        slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  async claimPrize(winner: Keypair, round: RoundJson, index: number): Promise<string> {
    const leaf = round.leaves[index];
    return this.program.methods
      .claimPrize(
        new BN(leaf.rangeStart),
        new BN(leaf.rangeEnd),
        leaf.proof.map((p) => [...Buffer.from(p, 'hex')]),
      )
      .accountsStrict({
        winner: winner.publicKey,
        pool: this.pool,
        prizeVault: this.prizeVault,
        round: roundPda(round.season),
        systemProgram: SystemProgram.programId,
      })
      .signers([winner])
      .rpc();
  }

  /** `none`, or delegated lamports with activation / deactivation epochs. */
  async stakeInfo(account: PublicKey): Promise<{ lamports: number; activation: bigint; deactivation: bigint } | null> {
    const info = await this.connection.getAccountInfo(account, 'confirmed');
    if (!info || !info.owner.equals(STAKE_PROGRAM)) return null;
    return {
      lamports: info.lamports,
      activation: info.data.readBigUInt64LE(164),
      deactivation: info.data.readBigUInt64LE(172),
    };
  }

  async describeStake(vote: PublicKey): Promise<string> {
    const info = await this.stakeInfo(this.stakeAccounts(vote).stakeAccount);
    if (!info) return 'no stake';
    return info.deactivation === U64_MAX
      ? `${fmtSol(info.lamports)} SOL, activation epoch ${info.activation}`
      : `${fmtSol(info.lamports)} SOL, deactivating since epoch ${info.deactivation}`;
  }

  /** Cluster-wide StakeHistory entry for `epoch`: real warmup / cooldown bookkeeping. */
  async stakeHistory(epoch: number): Promise<{ effective: bigint; activating: bigint; deactivating: bigint } | null> {
    const info = await this.connection.getAccountInfo(STAKE_HISTORY, 'confirmed');
    if (!info) return null;
    const data = info.data;
    const len = Number(data.readBigUInt64LE(0));
    for (let i = 0; i < len; i++) {
      const o = 8 + i * 32;
      if (Number(data.readBigUInt64LE(o)) === epoch) {
        return {
          effective: data.readBigUInt64LE(o + 8),
          activating: data.readBigUInt64LE(o + 16),
          deactivating: data.readBigUInt64LE(o + 24),
        };
      }
    }
    return null;
  }
}

/**
 * LOCAL DEMO ONLY: asks the VRF mock (loaded at the VRF program id on a local validator) to
 * deliver `randomness` through `consume_randomness`, signed by the scoped VRF identity.
 */
export async function mockVrfFulfill(flow: Flow, season: number, randomness: Buffer): Promise<string> {
  const identity = PublicKey.findProgramAddressSync([Buffer.from('identity'), PROGRAM_ID.toBuffer()], VRF_PROGRAM)[0];
  const discriminator = Buffer.from([190, 217, 49, 162, 99, 26, 73, 234]);
  const ix = new TransactionInstruction({
    programId: VRF_PROGRAM,
    keys: [
      { pubkey: PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: identity, isSigner: false, isWritable: false },
      { pubkey: roundPda(season), isSigner: false, isWritable: true },
    ],
    data: Buffer.concat([Buffer.alloc(8, 0xff), discriminator, randomness]),
  });
  return sendAndConfirmTransaction(flow.connection, new Transaction().add(ix), [flow.admin], { commitment: 'confirmed' });
}
