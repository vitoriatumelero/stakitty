import assert from 'node:assert/strict';
import { test } from 'node:test';
import { seasonWeights, toLeaves } from './weights.ts';

const A = '11111111111111111111111111111112';
const B = 'So11111111111111111111111111111111111111112';

test('holding for the whole season weighs principal x duration', () => {
  const w = seasonWeights([{ user: A, principal: 3n, timestamp: 50n }], 100n, 200n);
  assert.equal(w.get(A), 300n);
});

test('mid-season deposits and withdrawals are integrated piecewise', () => {
  const w = seasonWeights(
    [
      { user: A, principal: 10n, timestamp: 120n },
      { user: A, principal: 4n, timestamp: 150n },
      { user: A, principal: 0n, timestamp: 180n },
    ],
    100n,
    200n,
  );
  assert.equal(w.get(A), 10n * 30n + 4n * 30n);
});

test('changes after the season end do not count, users with no weight are dropped', () => {
  const w = seasonWeights(
    [
      { user: A, principal: 5n, timestamp: 190n },
      { user: A, principal: 1_000n, timestamp: 250n },
      { user: B, principal: 9n, timestamp: 300n },
    ],
    100n,
    200n,
  );
  assert.equal(w.get(A), 50n);
  assert.equal(w.has(B), false);
});

test('same-second changes keep only the last principal', () => {
  const w = seasonWeights(
    [
      { user: A, principal: 7n, timestamp: 100n },
      { user: A, principal: 2n, timestamp: 100n },
    ],
    100n,
    110n,
  );
  assert.equal(w.get(A), 20n);
});

test('leaves tile [0, total) in owner order', () => {
  const leaves = toLeaves(new Map([[B, 5n], [A, 3n]]));
  assert.deepEqual(
    leaves.map((l) => [l.owner.toBase58(), l.rangeStart, l.rangeEnd]),
    [
      [A, 0n, 3n],
      [B, 3n, 8n],
    ],
  );
});
