import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PublicKey } from '@solana/web3.js';
import { leafHash, MerkleTree, nodeHash, verify } from './merkle.ts';

// Same vector as `cross_language_vector` in programs/stakitty/src/merkle.rs.
const LEAF_A = '44f8f9b9ab53ca607c21a460616c4b0ab54c06658f5a5cde06a93c3af187deb0';
const ROOT = '01f395e5fe84cc61f884f544ba6b5c84114430d34aa555c04fd5e725e5e46f77';

const key = (byte: number): PublicKey => new PublicKey(Buffer.alloc(32, byte));
const leaves = [
  { owner: key(1), rangeStart: 0n, rangeEnd: 3_000n },
  { owner: key(2), rangeStart: 3_000n, rangeEnd: 4_000n },
  { owner: key(3), rangeStart: 4_000n, rangeEnd: 4_500n },
];

test('hashes match the Rust program', () => {
  assert.equal(leafHash(leaves[0]).toString('hex'), LEAF_A);
  const [a, b, c] = leaves.map(leafHash);
  assert.equal(nodeHash(nodeHash(a, b), c).toString('hex'), ROOT);
  assert.equal(new MerkleTree(leaves).root.toString('hex'), ROOT);
});

test('every leaf proves against the root, including the promoted odd leaf', () => {
  const tree = new MerkleTree(leaves);
  leaves.forEach((leaf, i) => assert.ok(verify(tree.proof(i), tree.root, leafHash(leaf))));
  assert.equal(tree.proof(2).length, 1);
});

test('a stretched range does not verify', () => {
  const tree = new MerkleTree(leaves);
  const forged = leafHash({ ...leaves[0], rangeEnd: 4_500n });
  assert.equal(verify(tree.proof(0), tree.root, forged), false);
});

test('large u128 weights encode without loss', () => {
  const big = { owner: key(9), rangeStart: (1n << 100n) + 5n, rangeEnd: (1n << 127n) - 1n };
  const tree = new MerkleTree([big]);
  assert.ok(verify([], tree.root, leafHash(big)));
  assert.throws(() => leafHash({ ...big, rangeEnd: 1n << 128n }), RangeError);
});
