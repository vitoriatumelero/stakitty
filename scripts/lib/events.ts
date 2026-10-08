import { BorshCoder, EventParser, type Idl } from '@anchor-lang/core';
import type { Connection, PublicKey } from '@solana/web3.js';
import type { PrincipalChange } from './weights.ts';

const PRINCIPAL_EVENTS = new Set(['Deposited', 'Withdrawn', 'WithdrawRequested']);
const PAGE = 1_000;

interface DecodedPrincipalEvent {
  pool: PublicKey;
  user: PublicKey;
  principal: { toString(): string };
  timestamp: { toString(): string };
}

/**
 * Every principal change of `pool`, oldest first. Several pools share the program, so events
 * are filtered by pool. Failed transactions are skipped: their logs can still contain
 * `emit!` output from before the failure.
 */
export async function fetchPrincipalChanges(
  connection: Connection,
  programId: PublicKey,
  idl: Idl,
  pool: PublicKey,
): Promise<PrincipalChange[]> {
  const signatures: { signature: string; slot: number }[] = [];
  let before: string | undefined;
  for (;;) {
    const page = await connection.getSignaturesForAddress(programId, { before, limit: PAGE }, 'confirmed');
    signatures.push(...page.filter((s) => s.err === null).map((s) => ({ signature: s.signature, slot: s.slot })));
    if (page.length < PAGE) break;
    before = page[page.length - 1].signature;
  }
  // RPC returns newest first; replay oldest first.
  signatures.reverse();

  const parser = new EventParser(programId, new BorshCoder(idl));
  const changes: PrincipalChange[] = [];
  for (const { signature } of signatures) {
    const tx = await connection.getTransaction(signature, {
      commitment: 'confirmed',
      maxSupportedTransactionVersion: 0,
    });
    if (!tx || tx.meta?.err) continue;
    for (const event of parser.parseLogs(tx.meta?.logMessages ?? [])) {
      if (!PRINCIPAL_EVENTS.has(event.name)) continue;
      const data = event.data as unknown as DecodedPrincipalEvent;
      if (!data.pool.equals(pool)) continue;
      changes.push({
        user: data.user.toBase58(),
        principal: BigInt(data.principal.toString()),
        timestamp: BigInt(data.timestamp.toString()),
      });
    }
  }
  return changes;
}
