// Stakitty simulation engine.
//
// A pure, in-browser model of the on-chain rules, used by the "Simulated" mode
// of the app (and for recording the demo video). It mirrors the program's
// parameters: 15% withdrawal reserve, 20% protocol fee on yield, sponsorship
// goes 100% to prizes, stake split pro rata to sponsorship with a 35% cap and
// at least 4 sponsoring validators, chance weighted by time-weighted balance.
//
// Nothing here touches a blockchain. The devnet program is the source of truth.

export const RESERVE_BPS = 1500; // 15% of principal stays liquid for withdrawals
export const FEE_BPS = 2000; // 20% of yield goes to the protocol
export const CAP_BPS = 3500; // max 35% of stake per validator
export const MIN_VALIDATORS = 4;
export const MIN_DEPOSIT = 0.01; // SOL
export const NET_APY = 0.052; // ~5.2%/yr native staking, after validator commission
export const EPOCHS_PER_YEAR = 182; // ~2 days per epoch
export const EPOCHS_PER_SEASON = 3; // short seasons for the demo (~15 epochs in production)

export interface Validator {
  id: string;
  name: string;
  vote: string; // vote account (display only in simulation)
  commission: number; // %
  weight: number; // current season share of stake, 0..1
  stake: number; // SOL delegated this season
}

export interface Round {
  id: number;
  epoch: number;
  prize: number;
  winner: string;
  winnerIsMe: boolean;
  randomness: string; // hex
  myChance: number; // 0..1
}

export interface Ticket {
  id: number;
  amount: number;
  readyEpoch: number;
}

export type LogKind = "deposit" | "withdraw" | "sponsor" | "epoch" | "season" | "draw" | "claim" | "error";

export interface LogEntry {
  id: number;
  epoch: number;
  kind: LogKind;
  text: string;
}

export interface SimState {
  me: string;
  epoch: number;
  season: number;
  seasonStartEpoch: number;
  myWallet: number;
  myDeposit: number;
  othersDeposit: number;
  staked: number;
  reserve: number;
  prizeVault: number;
  protocolFees: number;
  lastYield: number;
  myTwab: number; // SOL·epochs accumulated since the last draw
  othersTwab: number;
  validators: Validator[];
  bids: Record<string, number>; // sponsorship paid for the NEXT season, by validator id
  seasonPaid: number; // sponsorship that funded the current season
  rounds: Round[];
  tickets: Ticket[];
  myWinnings: number; // claimable prize
  log: LogEntry[];
  nextId: number;
}

const OTHER_WINNERS = [
  "7xQ4mB2kVfR9tLwP3nZc8sYhD5gJ1aEo6uKqT4f9Pa",
  "3rTn8WqLk5ZbH2vXc9MdF7sJp4GaY6eRu1oNi8Lm2Q",
  "9BvK2pLm4XcN7tRw5QzH8sDf3JgA6yEu1oTi9Kd4Wn",
  "5HzQ8mTk2LwR7vNc4XpB9sJd3FgY6aEu1oKi5Pr8Vx",
  "2MkT9qWn5LbR8vHc3XzP7sJd4GfA6yEu1oNi2Tq7Bz",
];

export const short = (addr: string) => (addr.length > 10 ? `${addr.slice(0, 4)}…${addr.slice(-4)}` : addr);

export const totalDeposits = (s: SimState) => s.myDeposit + s.othersDeposit;

export function myChance(s: SimState): number {
  const total = s.myTwab + s.othersTwab;
  if (total <= 0) {
    const dep = totalDeposits(s);
    return dep > 0 ? s.myDeposit / dep : 0;
  }
  return s.myTwab / total;
}

/**
 * Split stake pro rata to sponsorship, capping any validator at CAP_BPS and
 * redistributing the excess among the rest (iteratively).
 * Requires at least MIN_VALIDATORS sponsors, which also makes the cap feasible.
 */
export function proRataWeights(paid: Record<string, number>): Record<string, number> {
  const ids = Object.keys(paid).filter((id) => paid[id] > 0);
  if (ids.length < MIN_VALIDATORS) {
    throw new Error(`Need at least ${MIN_VALIDATORS} sponsoring validators (have ${ids.length}).`);
  }
  const cap = CAP_BPS / 10_000;
  const weights: Record<string, number> = {};
  const capped = new Set<string>();
  for (;;) {
    const free = ids.filter((id) => !capped.has(id));
    const freeShare = 1 - capped.size * cap;
    const freePaid = free.reduce((a, id) => a + paid[id], 0);
    let newlyCapped = false;
    for (const id of free) {
      const w = (paid[id] / freePaid) * freeShare;
      if (w > cap + 1e-12) {
        capped.add(id);
        newlyCapped = true;
      }
    }
    if (!newlyCapped) {
      for (const id of capped) weights[id] = cap;
      for (const id of free) weights[id] = (paid[id] / freePaid) * freeShare;
      return weights;
    }
  }
}

function log(s: SimState, kind: LogKind, text: string): SimState {
  const entry: LogEntry = { id: s.nextId, epoch: s.epoch, kind, text };
  return { ...s, nextId: s.nextId + 1, log: [entry, ...s.log].slice(0, 60) };
}

function fmt(n: number, d = 2) {
  return n.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
}

export function initialState(me = "You (demo wallet)"): SimState {
  // Season 1 mirrors the pitch deck example: 10,000 SOL pool, 8,500 in stake,
  // validators A–D paid 0.40 / 0.33 / 0.27 / 0.20 SOL.
  const validators: Validator[] = [
    { id: "A", name: "Validator A", vote: "Vote1111AAAAaaaa", commission: 5, weight: 0, stake: 0 },
    { id: "B", name: "Validator B", vote: "Vote1111BBBBbbbb", commission: 5, weight: 0, stake: 0 },
    { id: "C", name: "Validator C", vote: "Vote1111CCCCcccc", commission: 4, weight: 0, stake: 0 },
    { id: "D", name: "Validator D", vote: "Vote1111DDDDdddd", commission: 6, weight: 0, stake: 0 },
    { id: "E", name: "Validator E", vote: "Vote1111EEEEeeee", commission: 5, weight: 0, stake: 0 },
  ];
  const season1 = { A: 0.4, B: 0.33, C: 0.27, D: 0.2 };
  const w = proRataWeights(season1);
  const staked = 8500;
  for (const v of validators) {
    v.weight = w[v.id] ?? 0;
    v.stake = staked * v.weight;
  }
  const s: SimState = {
    me,
    epoch: 1000,
    season: 1,
    seasonStartEpoch: 1000,
    myWallet: 10,
    myDeposit: 0,
    othersDeposit: 10_000,
    staked,
    reserve: 1500,
    prizeVault: 6.5,
    protocolFees: 0,
    lastYield: 0,
    myTwab: 0,
    othersTwab: 0,
    validators,
    // Bids already placed for season 2. Validator E has not joined yet.
    bids: { A: 0.35, B: 0.3, C: 0.25 },
    seasonPaid: 1.2,
    rounds: [],
    tickets: [],
    myWinnings: 0,
    log: [],
    nextId: 1,
  };
  return log(s, "season", "Season 1 running: 4 validators sponsored 1.20 SOL, stake split pro rata.");
}

export type Action =
  | { type: "deposit"; amount: number }
  | { type: "withdraw"; amount: number }
  | { type: "sponsor"; validatorId: string; amount: number }
  | { type: "advanceEpoch" }
  | { type: "closeSeason" }
  | { type: "draw"; randomness: Uint8Array }
  | { type: "claim" }
  | { type: "reset" };

export function reduce(s: SimState, a: Action): SimState {
  switch (a.type) {
    case "reset":
      return initialState(s.me);

    case "deposit": {
      const amt = a.amount;
      if (!(amt >= MIN_DEPOSIT)) return log(s, "error", `Minimum deposit is ${MIN_DEPOSIT} SOL.`);
      if (amt > s.myWallet + 1e-9) return log(s, "error", "Not enough SOL in wallet.");
      // New SOL waits in the reserve; the next rebalance moves the excess into stake.
      return log(
        { ...s, myWallet: s.myWallet - amt, myDeposit: s.myDeposit + amt, reserve: s.reserve + amt },
        "deposit",
        `Deposited ${fmt(amt)} SOL. It counts for draws from the next epoch.`,
      );
    }

    case "withdraw": {
      const amt = a.amount;
      if (!(amt > 0) || amt > s.myDeposit + 1e-9) return log(s, "error", "Amount exceeds your deposit.");
      const base = { ...s, myDeposit: s.myDeposit - amt };
      if (amt <= s.reserve) {
        return log(
          { ...base, reserve: s.reserve - amt, myWallet: s.myWallet + amt },
          "withdraw",
          `Withdrew ${fmt(amt)} SOL instantly from the reserve.`,
        );
      }
      const ticket: Ticket = { id: s.nextId, amount: amt, readyEpoch: s.epoch + 1 };
      return log(
        { ...base, staked: s.staked - amt, tickets: [...s.tickets, ticket] },
        "withdraw",
        `Reserve too small: ticket #${ticket.id} for ${fmt(amt)} SOL, paid after the stake cools down (epoch ${ticket.readyEpoch}).`,
      );
    }

    case "sponsor": {
      const amt = a.amount;
      if (!(amt > 0)) return log(s, "error", "Sponsorship must be positive.");
      if (amt > s.myWallet + 1e-9) return log(s, "error", "Not enough SOL in wallet.");
      if (s.bids[a.validatorId]) return log(s, "error", "This validator already sponsored next season.");
      const v = s.validators.find((x) => x.id === a.validatorId);
      return log(
        { ...s, myWallet: s.myWallet - amt, bids: { ...s.bids, [a.validatorId]: amt } },
        "sponsor",
        `${v?.name ?? a.validatorId} sponsored season ${s.season + 1} with ${fmt(amt)} SOL (100% goes to prizes).`,
      );
    }

    case "advanceEpoch": {
      const gross = (s.staked * NET_APY) / EPOCHS_PER_YEAR;
      const fee = (gross * FEE_BPS) / 10_000;
      const toPrize = gross - fee;
      let next: SimState = {
        ...s,
        epoch: s.epoch + 1,
        prizeVault: s.prizeVault + toPrize,
        protocolFees: s.protocolFees + fee,
        lastYield: gross,
        myTwab: s.myTwab + s.myDeposit,
        othersTwab: s.othersTwab + s.othersDeposit,
      };
      next = log(
        next,
        "epoch",
        `Epoch ${next.epoch}: stake earned ${fmt(gross, 4)} SOL → ${fmt(toPrize, 4)} to the prize, ${fmt(fee, 4)} protocol fee.`,
      );
      const ready = next.tickets.filter((t) => t.readyEpoch <= next.epoch);
      if (ready.length) {
        const paid = ready.reduce((acc, t) => acc + t.amount, 0);
        next = log(
          { ...next, tickets: next.tickets.filter((t) => t.readyEpoch > next.epoch), myWallet: next.myWallet + paid },
          "withdraw",
          `Withdrawal ticket paid: ${fmt(paid)} SOL back to your wallet.`,
        );
      }
      return next;
    }

    case "closeSeason": {
      let weights: Record<string, number>;
      try {
        weights = proRataWeights(s.bids);
      } catch (e) {
        return log(s, "error", (e as Error).message);
      }
      const principal = totalDeposits(s);
      const reserve = (principal * RESERVE_BPS) / 10_000;
      const staked = principal - reserve;
      const paid = Object.values(s.bids).reduce((a, b) => a + b, 0);
      const validators = s.validators.map((v) => ({ ...v, weight: weights[v.id] ?? 0, stake: staked * (weights[v.id] ?? 0) }));
      const capped = validators.filter((v) => Math.abs(v.weight - CAP_BPS / 10_000) < 1e-9).map((v) => v.name);
      return log(
        {
          ...s,
          season: s.season + 1,
          seasonStartEpoch: s.epoch,
          validators,
          staked,
          reserve,
          prizeVault: s.prizeVault + paid,
          seasonPaid: paid,
          bids: {},
        },
        "season",
        `Season ${s.season + 1} started: ${Object.keys(weights).length} validators paid ${fmt(paid)} SOL → added to the prize. ` +
          `Stake rebalanced to ${fmt(staked, 0)} SOL, reserve ${fmt(reserve, 0)} SOL` +
          (capped.length ? ` (${capped.join(", ")} capped at 35%).` : "."),
      );
    }

    case "draw": {
      if (s.prizeVault <= 0) return log(s, "error", "Prize vault is empty.");
      const twabTotal = s.myTwab + s.othersTwab;
      if (twabTotal <= 0) return log(s, "error", "No eligible balance yet: advance at least one epoch after depositing.");
      const r = randomUnit(a.randomness);
      const chance = s.myTwab / twabTotal;
      const meWins = r < chance;
      const winner = meWins ? s.me : OTHER_WINNERS[a.randomness[9] % OTHER_WINNERS.length];
      const round: Round = {
        id: s.rounds.length + 1,
        epoch: s.epoch,
        prize: s.prizeVault,
        winner,
        winnerIsMe: meWins,
        randomness: toHex(a.randomness),
        myChance: chance,
      };
      return log(
        {
          ...s,
          rounds: [round, ...s.rounds],
          prizeVault: 0,
          myTwab: 0,
          othersTwab: 0,
          myWinnings: s.myWinnings + (meWins ? round.prize : 0),
        },
        "draw",
        `Round #${round.id}: ${fmt(round.prize)} SOL to ${meWins ? "you" : short(winner)}. Deposits untouched.`,
      );
    }

    case "claim": {
      if (s.myWinnings <= 0) return s;
      return log(
        { ...s, myWallet: s.myWallet + s.myWinnings, myWinnings: 0 },
        "claim",
        `Claimed ${fmt(s.myWinnings)} SOL prize.`,
      );
    }
  }
}

/** First 6 bytes of the randomness as a number in [0, 1). */
export function randomUnit(bytes: Uint8Array): number {
  let x = 0;
  for (let i = 0; i < 6; i++) x = x * 256 + bytes[i];
  return x / 2 ** 48;
}

export function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
