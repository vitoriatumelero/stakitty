import { describe, expect, it } from "vitest";
import { initialState, proRataWeights, reduce, SimState } from "./engine";

const rnd = (first: number) => {
  const b = new Uint8Array(32);
  b[0] = first;
  return b;
};

describe("pro rata weights", () => {
  it("matches the pitch deck example (A–D)", () => {
    const w = proRataWeights({ A: 0.4, B: 0.33, C: 0.27, D: 0.2 });
    expect(w.A).toBeCloseTo(0.3333, 3);
    expect(w.D).toBeCloseTo(0.1667, 3);
  });

  it("caps a validator at 35% and redistributes the excess", () => {
    const w = proRataWeights({ A: 0.35, B: 0.3, C: 0.25, E: 1.0 });
    expect(w.E).toBeCloseTo(0.35, 9);
    const sum = Object.values(w).reduce((a, b) => a + b, 0);
    expect(sum).toBeCloseTo(1, 9);
    for (const v of Object.values(w)) expect(v).toBeLessThanOrEqual(0.35 + 1e-9);
  });

  it("refuses fewer than 4 sponsors", () => {
    expect(() => proRataWeights({ A: 1, B: 1, C: 1 })).toThrow(/at least 4/);
  });
});

describe("pool flow", () => {
  it("deposit, earn, draw, withdraw: principal is never touched", () => {
    let s: SimState = initialState();
    s = reduce(s, { type: "deposit", amount: 0.5 });
    expect(s.myDeposit).toBeCloseTo(0.5);
    const prizeBefore = s.prizeVault;
    s = reduce(s, { type: "advanceEpoch" });
    expect(s.prizeVault).toBeGreaterThan(prizeBefore);
    expect(s.protocolFees / (s.protocolFees + (s.prizeVault - prizeBefore))).toBeCloseTo(0.2, 6);
    s = reduce(s, { type: "draw", randomness: rnd(255) }); // high draw: someone else wins
    expect(s.rounds[0].winnerIsMe).toBe(false);
    expect(s.myDeposit).toBeCloseTo(0.5);
    s = reduce(s, { type: "withdraw", amount: 0.5 });
    expect(s.myDeposit).toBe(0);
    expect(s.myWallet).toBeCloseTo(10);
  });

  it("closing a season moves sponsorship into the prize and keeps a 15% reserve", () => {
    let s = initialState();
    s = reduce(s, { type: "closeSeason" }); // only 3 bids
    expect(s.season).toBe(1);
    s = reduce(s, { type: "sponsor", validatorId: "E", amount: 1 });
    const prize = s.prizeVault;
    s = reduce(s, { type: "closeSeason" });
    expect(s.season).toBe(2);
    expect(s.prizeVault).toBeCloseTo(prize + 1.9);
    expect(s.reserve / (s.reserve + s.staked)).toBeCloseTo(0.15);
    expect(s.validators.find((v) => v.id === "E")!.weight).toBeCloseTo(0.35);
  });
});
