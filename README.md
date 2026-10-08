# Stakitty

Prize-linked SOL savings on Solana. Users deposit SOL and can always withdraw their principal.
Listed validators pay a per-season sponsorship to receive the pool's stake. Sponsorship and
staking yield fund the prizes.

## Build and test

```bash
anchor build --arch v1
cargo test
```

`--arch v1` is required. Anchor 1.2.1 builds SBPF v3 by default, and LiteSVM 0.10 cannot load
that binary: every test fails in `add_program` with `InvalidAccountData`.

## Accounts and vaults

| PDA | Seeds | Holds |
|-----|-------|-------|
| `Pool` | `pool` | Totals, season counter, open ticket total |
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

## Season flow

1. `sponsor(amount)`: signed by the vote account's `authorized_withdrawer`. SOL goes to the
   prize vault. Payments made in season `s` set the stake for season `s+1` and fund its prizes.
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
  activation or deactivation completes in exactly one epoch. Real warmup and cooldown still
  need validation on devnet or Surfpool.
- **VRF test fixtures:** `tests/fixtures` holds MagicBlock's VRF program and default oracle
  queue, copied from devnet on 08/10/2026. Refresh them if the program is upgraded.
- **Unaudited:** not reviewed for production use.
