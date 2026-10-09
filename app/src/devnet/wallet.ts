// Tiny wallet connector for injected Solana wallets (Phantom, Solflare, Backpack).
// Enough to show the connected address and devnet balance. For signing program
// instructions, the dev can swap this for @solana/wallet-adapter-react.

interface Key {
  toString(): string;
}

interface InjectedProvider {
  isPhantom?: boolean;
  publicKey?: Key | null;
  // Phantom resolves `{ publicKey }`; Solflare resolves `true`; others resolve nothing
  // and only set `provider.publicKey`.
  connect: (opts?: { onlyIfTrusted?: boolean }) => Promise<unknown>;
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

/** The connected address, whichever way the wallet reports it. */
export function connectedAddress(result: unknown, provider: Pick<InjectedProvider, "publicKey">): string | null {
  const fromResult = (result as { publicKey?: Key | null } | null | undefined)?.publicKey;
  const key = fromResult ?? provider.publicKey;
  return key ? key.toString() : null;
}

export async function connectWallet(): Promise<string> {
  const found = findProvider();
  if (!found) throw new Error("No Solana wallet found. Install Phantom or Solflare and switch it to Devnet.");
  const address = connectedAddress(await found.provider.connect(), found.provider);
  if (!address) throw new Error(`${found.name} connected but did not share an address. Unlock it and try again.`);
  return address;
}

export async function disconnectWallet(): Promise<void> {
  await findProvider()?.provider.disconnect();
}
