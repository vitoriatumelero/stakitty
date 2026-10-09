// Minimal Solana JSON-RPC helpers (no SDK needed for read-only views).
// Writes (deposit, sponsor, draw...) go through the Anchor client: see README.

export const RPC_URL: string = import.meta.env.VITE_RPC_URL || "https://api.devnet.solana.com";
export const PROGRAM_ID: string = import.meta.env.VITE_PROGRAM_ID || "8dFfRCbNDeYt9y96ZC8uCcU2BbXt2LwWEH91ZUN3BRQg";
export const LAMPORTS_PER_SOL = 1_000_000_000;

export const explorer = (kind: "address" | "tx", id: string) =>
  `https://explorer.solana.com/${kind}/${id}?cluster=devnet`;

async function rpc<T>(method: string, params: unknown[]): Promise<T> {
  let res: Response;
  try {
    res = await fetch(RPC_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    });
  } catch {
    throw new Error("Could not reach the devnet RPC. Check your connection or set VITE_RPC_URL.");
  }
  if (!res.ok) throw new Error(`RPC ${res.status}${res.status === 429 ? " (rate limited: set VITE_RPC_URL)" : ""}`);
  const json = await res.json();
  if (json.error) throw new Error(json.error.message ?? "RPC error");
  return json.result as T;
}

export interface SigInfo {
  signature: string;
  slot: number;
  blockTime: number | null;
  err: unknown;
}

export async function getBalance(address: string): Promise<number> {
  const r = await rpc<{ value: number }>("getBalance", [address, { commitment: "confirmed" }]);
  return r.value / LAMPORTS_PER_SOL;
}

export async function getProgramInfo(address: string): Promise<{ exists: boolean; executable: boolean }> {
  const r = await rpc<{ value: { executable: boolean } | null }>("getAccountInfo", [
    address,
    { encoding: "base64", dataSlice: { offset: 0, length: 0 } },
  ]);
  return { exists: !!r.value, executable: !!r.value?.executable };
}

export async function getRecentSignatures(address: string, limit = 12): Promise<SigInfo[]> {
  return rpc<SigInfo[]>("getSignaturesForAddress", [address, { limit }]);
}

export interface EpochInfo {
  epoch: number;
  slotIndex: number;
  slotsInEpoch: number;
  secondsPerSlot: number;
}

// Nominal slot time, used only when the cluster returns no performance samples.
const NOMINAL_SECONDS_PER_SLOT = 0.4;

interface PerfSample {
  numSlots: number;
  samplePeriodSecs: number;
}

/** Average seconds per slot over the cluster's recent performance samples (one per minute). */
export function secondsPerSlot(samples: PerfSample[]): number {
  const slots = samples.reduce((sum, s) => sum + s.numSlots, 0);
  const secs = samples.reduce((sum, s) => sum + s.samplePeriodSecs, 0);
  return slots > 0 && secs > 0 ? secs / slots : NOMINAL_SECONDS_PER_SLOT;
}

export async function getEpoch(): Promise<EpochInfo> {
  const [info, samples] = await Promise.all([
    rpc<Omit<EpochInfo, "secondsPerSlot">>("getEpochInfo", [{ commitment: "confirmed" }]),
    rpc<PerfSample[]>("getRecentPerformanceSamples", [30]).catch(() => []),
  ]);
  return { ...info, secondsPerSlot: secondsPerSlot(samples) };
}
