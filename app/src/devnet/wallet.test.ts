import { describe, expect, it } from "vitest";
import { connectedAddress } from "./wallet";

const key = (s: string) => ({ toString: () => s });

describe("connectedAddress", () => {
  it("reads the key Phantom returns from connect()", () => {
    expect(connectedAddress({ publicKey: key("Phan") }, { publicKey: null })).toBe("Phan");
  });

  it("falls back to provider.publicKey when connect() resolves true (Solflare)", () => {
    expect(connectedAddress(true, { publicKey: key("Sol") })).toBe("Sol");
  });

  it("falls back to provider.publicKey when connect() resolves nothing", () => {
    expect(connectedAddress(undefined, { publicKey: key("Bp") })).toBe("Bp");
  });

  it("returns null when no address is available", () => {
    expect(connectedAddress(true, { publicKey: null })).toBeNull();
  });
});
