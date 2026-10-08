import { readFileSync } from 'node:fs';
import { AnchorProvider, BN, Program, Wallet } from '@anchor-lang/core';
import { Connection, Keypair, PublicKey, SystemProgram } from '@solana/web3.js';
import idlJson from '../../target/idl/stakitty.json' with { type: 'json' };
import type { Stakitty } from '../../target/types/stakitty.ts';
import { MerkleTree } from './merkle.ts';
import { fetchPrincipalChanges } from './events.ts';
import { seasonWeights, toLeaves } from './weights.ts';

export const idl = idlJson as Stakitty;
export const PROGRAM_ID = new PublicKey(idl.address);

const seed = (s: string): Buffer => Buffer.from(s);
const u32Le = (n: number): Buffer => {
  const b = Buffer.alloc(4);
  b.writeUInt32LE(n);
  return b;
};
const pda = (seeds: Buffer[]): PublicKey => PublicKey.findProgramAddressSync(seeds, PROGRAM_ID)[0];

export const poolPda = (): PublicKey => pda([seed('pool')]);
export const seasonPda = (index: number): PublicKey => pda([seed('season'), poolPda().toBuffer(), u32Le(index)]);
export const roundPda = (index: number): PublicKey => pda([seed('round'), poolPda().toBuffer(), u32Le(index)]);

export function loadKeypair(path: string): Keypair {
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(path, 'utf8')) as number[]));
}

export function connect(rpcUrl: string, payer: Keypair): Program<Stakitty> {
  const connection = new Connection(rpcUrl, 'confirmed');
  const provider = new AnchorProvider(connection, new Wallet(payer), { commitment: 'confirmed' });
  return new Program<Stakitty>(idl, provider);
}

export interface RoundLeafJson {
  owner: string;
  weight: string;
  rangeStart: string;
  rangeEnd: string;
  proof: string[];
}

export interface RoundJson {
  season: number;
  merkleRoot: string;
  totalWeight: string;
  startTs: string;
  endTs: string;
  leaves: RoundLeafJson[];
}

/**
 * Rebuilds a closed season's weight list from program events and checks it against the
 * on-chain accumulator. Throws if they disagree: never publish a list that won't commit.
 */
export async function buildRound(program: Program<Stakitty>, season: number): Promise<{ json: RoundJson; tree: MerkleTree }> {
  const state = await program.account.season.fetch(seasonPda(season));
  if (!('closed' in state.status)) throw new Error(`season ${season} is not closed yet`);
  const startTs = BigInt(state.startTs.toString());
  const endTs = BigInt(state.endTs.toString());
  const expected = BigInt(state.endPoolWeight.toString()) - BigInt(state.startPoolWeight.toString());

  const changes = await fetchPrincipalChanges(program.provider.connection, PROGRAM_ID, idl);
  const weights = seasonWeights(changes, startTs, endTs);
  const leaves = toLeaves(weights);
  const total = leaves.length ? leaves[leaves.length - 1].rangeEnd : 0n;
  if (total !== expected) {
    throw new Error(`rebuilt weight ${total} != on-chain ${expected} (diff ${total - expected})`);
  }
  if (leaves.length === 0) throw new Error(`season ${season} has no weight to draw from`);

  const tree = new MerkleTree(leaves);
  const json: RoundJson = {
    season,
    merkleRoot: tree.root.toString('hex'),
    totalWeight: total.toString(),
    startTs: startTs.toString(),
    endTs: endTs.toString(),
    leaves: leaves.map((leaf, i) => ({
      owner: leaf.owner.toBase58(),
      weight: (leaf.rangeEnd - leaf.rangeStart).toString(),
      rangeStart: leaf.rangeStart.toString(),
      rangeEnd: leaf.rangeEnd.toString(),
      proof: tree.proof(i).map((p) => p.toString('hex')),
    })),
  };
  return { json, tree };
}

export async function commitRound(program: Program<Stakitty>, round: RoundJson): Promise<string> {
  const authority = program.provider.publicKey;
  if (!authority) throw new Error('provider has no wallet');
  return program.methods
    .commitRound(
      [...Buffer.from(round.merkleRoot, 'hex')],
      new BN(round.totalWeight),
      round.leaves.length,
    )
    .accountsStrict({
      authority,
      pool: poolPda(),
      season: seasonPda(round.season),
      previousSeason: round.season > 0 ? seasonPda(round.season - 1) : null,
      round: roundPda(round.season),
      systemProgram: SystemProgram.programId,
    })
    .rpc();
}
