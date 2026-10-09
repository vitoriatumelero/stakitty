import { describe, expect, it } from "vitest";
import { secondsPerSlot } from "./rpc";

describe("secondsPerSlot", () => {
  it("averages the measured slot time across samples", () => {
    const samples = [
      { numSlots: 250, samplePeriodSecs: 60 },
      { numSlots: 254, samplePeriodSecs: 60 },
    ];
    expect(secondsPerSlot(samples)).toBeCloseTo(120 / 504);
  });

  it("falls back to the nominal 0.4 s when there are no samples", () => {
    expect(secondsPerSlot([])).toBe(0.4);
  });
});
