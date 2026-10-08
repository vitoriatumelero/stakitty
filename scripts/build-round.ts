/**
 * Rebuilds a closed season's Merkle list from program events, verifies it against the
 * on-chain accumulator, writes rounds/season-<n>.json (root, ranges, proofs) and, with
 * --commit, publishes the root as the pool admin.
 *
 *   RPC_URL=<url> WALLET=<keypair.json> npm run build-round -- --season 1 [--commit]
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { buildRound, commitRound, connect, loadKeypair } from './lib/stakitty.ts';

async function main(): Promise<void> {
  const { values } = parseArgs({
    options: {
      season: { type: 'string' },
      commit: { type: 'boolean', default: false },
      out: { type: 'string', default: 'rounds' },
    },
  });
  const rpcUrl = process.env.RPC_URL;
  const wallet = process.env.WALLET;
  if (!rpcUrl || !wallet || values.season === undefined) {
    throw new Error('usage: RPC_URL=... WALLET=... build-round --season <n> [--commit]');
  }
  const season = Number.parseInt(values.season, 10);
  const program = connect(rpcUrl, loadKeypair(wallet));

  const { json } = await buildRound(program, season);
  mkdirSync(values.out, { recursive: true });
  const path = `${values.out}/season-${season}.json`;
  writeFileSync(path, `${JSON.stringify(json, null, 2)}\n`);
  console.log(`season ${season}: ${json.leaves.length} leaves, total ${json.totalWeight}, root ${json.merkleRoot}`);
  console.log(`wrote ${path}`);

  if (values.commit) {
    console.log(`commit_round: ${await commitRound(program, json)}`);
  }
}

main().catch((err: unknown) => {
  console.error(err instanceof Error ? err.message : err);
  process.exit(1);
});
