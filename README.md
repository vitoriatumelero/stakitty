# Stakitty

Prize-linked SOL savings on Solana. Users deposit SOL and can always withdraw their principal.
Listed validators pay a per-season sponsorship to receive the pool's stake. Sponsorship and
staking yield fund the prizes.

## Build and test

```bash
anchor build --arch v1
cargo test
```

Off-chain scripts (Node 20):

```bash
npm install
npm run test:scripts   # Merkle parity with the Rust program, weight replay
npm run typecheck
```

`--arch v1` is required. Anchor 1.2.1 builds SBPF v3 by default, and LiteSVM 0.10 cannot load
that binary: every test fails in `add_program` with `InvalidAccountData`.

## Accounts and vaults

| PDA | Seeds | Holds |
|-----|-------|-------|
| `Pool` | `pool, pool_id (u16)` | Totals, season counter, open ticket total |
| Reserve | `reserve, pool` | Liquid principal; staker and withdrawer of every stake account |
| Prize vault | `prize, pool` | Sponsor SOL and the 80% yield share. No withdraw path reads it |
| Fee vault | `fee, pool` | 20% protocol share of yield. Admin-only withdrawals |
| `UserAccount` | `user, pool, owner` | Principal and time-weighted balance |
| `WithdrawTicket` | `ticket, pool, owner` | Principal owed after an unstake (one open per user) |
| `ValidatorEntry` | `validator, pool, vote` | Admin-listed validator (max 16) |
| `Season` | `season, pool, index` | Total paid, status, final weights |
| `SeasonSponsorship` | `sponsorship, season, vote` | One payment per validator per season |
| Stake / transient | `stake` or `transient, pool, vote` | Delegated stake and its pending change |
| `Round` | `round, pool, season` | Merkle root, VRF result and prize for one season |

`initialize_pool(pool_id, ...)` fixes the admin and parameters. A new `pool_id` starts a separate
pool, for example to change the season length on a test cluster.

## Season flow

1. `sponsor(amount)`: signed by the vote account's `authorized_withdrawer`. A separate
   `payer` funds it, because the withdrawer may be a cold wallet or the vote account itself,
   and those can't pay a transfer. SOL goes to the prize vault. Payments made in season `s` set the stake for season `s+1` and fund its prizes.
2. `close_season`: permissionless after the end epoch, with every sponsorship of the season.
   The weight is `paid / total` of the delegable 85% of principal, capped at 35%. It needs at
   least 4 paying validators.
3. `extend_season(epochs)`: admin only. Use it when a season can't reach 4 sponsors.
4. `rebalance`: permissionless, once per validator per season. Changes go through the
   transient account, so each one waits about one epoch.
5. `settle_stake`: permissionless, after the epoch boundary. Merges the transient into the
   main stake, or returns cooled-down stake to the reserve.

The reserve always keeps 15% of `total_principal` undelegated, plus any cap excess.

## Yield and prizes

1. `settle_stake` compares what comes back from stake with the validator's `stake_basis`
   (reserve SOL put in). Anything above the basis is staking reward, so it is recorded as
   `realized_yield`. That SOL stays in the reserve, but withdrawals and stake can't spend it.
2. `harvest_yield`: permissionless. Sends 20% to the fee vault and 80% to the prize vault.
3. `commit_round(root, total_weight, leaf_count)`: admin only, after the season closes.
   - Leaves are `(owner, range_start, range_end)` and tile `[0, total_weight)`.
   - Each user's weight is principal × seconds held during the season.
   - `total_weight` must equal the pool accumulator's growth over the season.
   - Prize = harvested yield and expired prizes + what sponsors paid in the previous season.
4. `request_draw`: permissionless. Asks MagicBlock VRF for randomness; a re-roll is rejected.
5. `consume_randomness`: called only by the VRF identity. Picks the winning ticket.
6. `claim_prize(range, proof)`: the owner of the leaf containing the ticket claims within
   14 epochs. After that, `expire_prize` returns the prize to the next round.

Run `harvest_yield` before `commit_round`, or that yield waits for the following round.

## Building a round list

```bash
RPC_URL=<rpc> WALLET=<admin keypair> npm run build-round -- --season 1 [--commit]
```

1. Reads every `Deposited`, `Withdrawn` and `WithdrawRequested` event. Failed transactions
   are skipped, because their logs can still contain events.
2. Replays each user's principal over the season's `[start_ts, end_ts]`.
3. Builds the tree, which `scripts/lib/merkle.test.ts` checks against the Rust hash.
4. Refuses to continue unless the total equals the on-chain accumulator.
5. Writes `rounds/season-<n>.json` with every leaf's range and proof. Publish that file so
   anyone can audit the list. With `--commit`, it also sends `commit_round`.

## Recorded demo (local validator, real epochs)

```bash
solana-test-validator --reset --slots-per-epoch 64 \
  --bpf-program 8dFfRCbNDeYt9y96ZC8uCcU2BbXt2LwWEH91ZUN3BRQg target/deploy/stakitty.so \
  --bpf-program Vrf1RNUjXmQGjmQrQLvJHs9SNkvDJEsRVFPkfSQUwGz mocks/vrf-oracle-mock/target/deploy/vrf_oracle_mock.so
RPC_URL=http://127.0.0.1:8899 WALLET=~/.config/solana/id.json LEDGER=test-ledger npm run demo:local
```

The demo runs deposit → sponsor → close_season → rebalance → stake activation → VRF draw
→ prize → withdraw, and takes about 3 minutes.
- **Real stake mechanics:** epochs last ~25 s, and the bank keeps `StakeHistory`, so the
  activation is the cluster's own bookkeeping.
- **Real yield:** the local validator votes, so the stake delegated to it earns rewards.
- **Mocked VRF:** the draw is a **local mock** of the MagicBlock oracle
  (`mocks/vrf-oracle-mock`, built with
  `cargo-build-sbf --arch v1 --tools-version v1.57`). It is loaded at the VRF program id and
  signs the callback with the same scoped identity PDA. It is never deployed. On devnet the
  real VRF answers.

## Live on devnet

- **Program:** [`8dFfRCbNDeYt9y96ZC8uCcU2BbXt2LwWEH91ZUN3BRQg`](https://explorer.solana.com/address/8dFfRCbNDeYt9y96ZC8uCcU2BbXt2LwWEH91ZUN3BRQg?cluster=devnet)
- **App:** https://vitoriatumelero.github.io/stakitty/ (toggle **Devnet live** to see every on-chain step)
- **Pool 1:** 4 test validators, 2 depositors (alice, bob), 4.95 SOL principal, 1-epoch seasons.

| Step | Transaction |
|---|---|
| Program upgrade | <link> |
| `initialize_pool` | <link> |
| Validators A–D listed | <links> |
| Deposits | <links> |
| Season 0 sponsorships | <links> |
| `close_season 0` + `rebalance` | <links> (epoch 1179) |
| First VRF draw + `claim_prize` | <links> (epoch 1180) |

### Running it yourself

    RPC_URL=<devnet rpc> WALLET=~/.config/solana/id.json POOL_ID=1 npm run cluster-run -- setup
    RPC_URL=<devnet rpc> WALLET=... POOL_ID=1 npm run cluster-run -- advance   # after each epoch boundary (~28 h)

- `advance` is idempotent: it closes seasons, pays sponsors, settles, rebalances, harvests,
  and commits, draws and claims round 1.
- Devnet's minimum stake delegation is **1 SOL**. Each validator's target, after the 15% reserve,
  must clear rent + 1 SOL or the rebalance leaves it liquid. Pool 1 deposits ~4.95 SOL for that reason.
- The public devnet RPC rate-limits uploads and cranks. Use a dedicated devnet RPC.
- Demo keypairs live in `.stakitty-run/` (gitignored, throwaway). The script refuses mainnet URLs.

## End-to-end on Surfpool

```bash
surfpool start --offline --ci --no-deploy
anchor deploy --provider.cluster http://127.0.0.1:8899
RPC_URL=http://127.0.0.1:8899 WALLET=~/.config/solana/id.json npm run e2e:surfpool
```

The run covers:
- real vote accounts and two seasons with time travel;
- rebalance, deactivation of a validator that stopped paying, and settle;
- the season 1 list rebuilt from events and committed.

The VRF draw is not included, because there is no oracle offline. It was validated on devnet
by the VRF spike.

## Withdrawals

- `withdraw`: instant, up to the reserve's liquid SOL minus what open tickets are owed.
- `request_withdraw`: for larger amounts. Principal and its prize weight leave the pool right
  away, so the next rebalance sizes stake without them.
- `claim_withdraw`: pays the ticket once the reserve holds enough SOL.

## Known risks and limits

- **Merkle list is trusted:** the program only checks the leaf total. A wrong individual
  weight that keeps the same sum would pass, so the published list must be audited off-chain
  against the deposit and withdraw events.
- **Oracle silence:** if MagicBlock never calls back, the round stays in
  `RandomnessRequested`. There is no retry path yet.
- **Two vote accounts per operator** can get around the 35% cap. The only defense is the
  admin-controlled list.
- **Ticket wait:** a ticket waits for the next season's rebalance and one epoch of cooldown.
  Claims are first come, first served.
- **Simplified epochs in tests:** the test helper `set_epoch` backfills `StakeHistory` so every
  activation or deactivation completes in exactly one epoch. Surfpool 1.6 time travel leaves
  `StakeHistory` empty, which the Stake program also treats as instant warmup and cooldown.
  Real timing (warmup capped by cluster stake) can only be checked on devnet, where an epoch
  takes ~2 days.
- **VRF test fixtures:** `tests/fixtures` holds MagicBlock's VRF program and default oracle
  queue, copied from devnet on 08/10/2026. Refresh them if the program is upgraded.
- **Program upgrades** on a cluster need a buffer as large as the program (~2.6 SOL
  temporarily, refunded after the upgrade).
- **Unaudited:** not reviewed for production use.
