import { PublicKey } from '@solana/web3.js';
import type { Leaf } from './merkle.ts';

/** A user's principal right after a Deposited / Withdrawn / WithdrawRequested event. */
export interface PrincipalChange {
  user: string;
  principal: bigint;
  timestamp: bigint;
}

/**
 * Principal x seconds each user held inside [startTs, endTs], replaying the same step
 * function the on-chain accumulators integrate. Changes must be in execution order.
 */
export function seasonWeights(
  changes: PrincipalChange[],
  startTs: bigint,
  endTs: bigint,
): Map<string, bigint> {
  if (endTs < startTs) throw new Error(`season ends (${endTs}) before it starts (${startTs})`);
  const byUser = new Map<string, PrincipalChange[]>();
  for (const change of changes) {
    const list = byUser.get(change.user) ?? [];
    list.push(change);
    byUser.set(change.user, list);
  }

  const weights = new Map<string, bigint>();
  for (const [user, list] of byUser) {
    let weight = 0n;
    let principal = 0n;
    let since = startTs;
    for (const change of list) {
      const at = change.timestamp < startTs ? startTs : change.timestamp > endTs ? endTs : change.timestamp;
      if (at > since) {
        weight += principal * (at - since);
        since = at;
      }
      principal = change.principal;
    }
    if (endTs > since) weight += principal * (endTs - since);
    if (weight > 0n) weights.set(user, weight);
  }
  return weights;
}

/** Sorted by owner so every builder produces the same tree for the same weights. */
export function toLeaves(weights: Map<string, bigint>): Leaf[] {
  let cursor = 0n;
  return [...weights.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([owner, weight]) => {
      const leaf = { owner: new PublicKey(owner), rangeStart: cursor, rangeEnd: cursor + weight };
      cursor += weight;
      return leaf;
    });
}
