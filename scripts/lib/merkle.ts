import { createHash } from 'node:crypto';
import type { PublicKey } from '@solana/web3.js';

// Mirrors programs/stakitty/src/merkle.rs byte for byte.
// Leaf: sha256(0x00 || owner || range_start_le_u128 || range_end_le_u128).
// Node: sha256(0x01 || min(a, b) || max(a, b)).

export interface Leaf {
  owner: PublicKey;
  rangeStart: bigint;
  rangeEnd: bigint;
}

function u128Le(value: bigint): Buffer {
  if (value < 0n || value >= 1n << 128n) {
    throw new RangeError(`u128 out of range: ${value}`);
  }
  const buf = Buffer.alloc(16);
  buf.writeBigUInt64LE(value & 0xffff_ffff_ffff_ffffn, 0);
  buf.writeBigUInt64LE(value >> 64n, 8);
  return buf;
}

function sha256(...parts: Uint8Array[]): Buffer {
  const hash = createHash('sha256');
  for (const part of parts) hash.update(part);
  return hash.digest();
}

export function leafHash(leaf: Leaf): Buffer {
  return sha256(Buffer.from([0]), leaf.owner.toBuffer(), u128Le(leaf.rangeStart), u128Le(leaf.rangeEnd));
}

export function nodeHash(a: Buffer, b: Buffer): Buffer {
  const [lo, hi] = Buffer.compare(a, b) <= 0 ? [a, b] : [b, a];
  return sha256(Buffer.from([1]), lo, hi);
}

export class MerkleTree {
  readonly leaves: Leaf[];
  private readonly layers: Buffer[][];

  constructor(leaves: Leaf[]) {
    if (leaves.length === 0) throw new Error('Merkle tree needs at least one leaf');
    this.leaves = leaves;
    this.layers = [leaves.map(leafHash)];
    while (this.layers[this.layers.length - 1].length > 1) {
      const level = this.layers[this.layers.length - 1];
      const next: Buffer[] = [];
      for (let i = 0; i < level.length; i += 2) {
        // An odd node is promoted unchanged, so its proof skips that level.
        next.push(i + 1 < level.length ? nodeHash(level[i], level[i + 1]) : level[i]);
      }
      this.layers.push(next);
    }
  }

  get root(): Buffer {
    return this.layers[this.layers.length - 1][0];
  }

  proof(index: number): Buffer[] {
    const proof: Buffer[] = [];
    let i = index;
    for (const level of this.layers.slice(0, -1)) {
      const sibling = i ^ 1;
      if (sibling < level.length) proof.push(level[sibling]);
      i = Math.floor(i / 2);
    }
    return proof;
  }
}

export function verify(proof: Buffer[], root: Buffer, leaf: Buffer): boolean {
  return proof.reduce((acc, sibling) => nodeHash(acc, sibling), leaf).equals(root);
}
