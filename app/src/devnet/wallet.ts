// Tiny wallet connector for injected Solana wallets (Phantom, Solflare, Backpack).
// Enough to show the connected address and devnet balance. For signing program
// instructions, the dev can swap this for @solana/wallet-adapter-react.

interface InjectedProvider {
  isPhantom?: boolean;
  publicKey?: { toString(): string } | null;
  connect: (opts?: { onlyIfTrusted?: boolean }) => Promise<{ publicKey: { toString(): string } }>;
  disconnect: () => Promise<void>;
}

declare global {
  interface Window {
    phantom?: { solana?: InjectedProvider };
    solflare?: InjectedProvider;
    backpack?: InjectedProvider;
    solana?: InjectedProvider;
  }
}

export function findProvider(): { name: string; provider: InjectedProvider } | null {
  if (typeof window === "undefined") return null;
  if (window.phantom?.solana) return { name: "Phantom", provider: window.phantom.solana };
  if (window.solflare) return { name: "Solflare", provider: window.solflare };
  if (window.backpack) return { name: "Backpack", provider: window.backpack };
  if (window.solana) return { name: "Wallet", provider: window.solana };
  return null;
}

export async function connectWallet(): Promise<string> {
  const found = findProvider();
  if (!found) throw new Error("No Solana wallet found. Install Phantom or Solflare and switch it to Devnet.");
  const { publicKey } = await found.provider.connect();
  return publicKey.toString();
}

export async function disconnectWallet(): Promise<void> {
  await findProvider()?.provider.disconnect();
}
