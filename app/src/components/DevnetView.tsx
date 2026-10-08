import { useCallback, useEffect, useState } from "react";
import { explorer, getBalance, getEpoch, getProgramInfo, getRecentSignatures, PROGRAM_ID, RPC_URL, SigInfo } from "../devnet/rpc";
import { connectWallet, disconnectWallet, findProvider } from "../devnet/wallet";
import { short } from "../sim/engine";
import { Badge, Cat, Eyebrow, sol } from "./ui";

type Load<T> = { status: "idle" | "loading" | "ok" | "error"; data?: T; error?: string };

export function DevnetView() {
  const [program, setProgram] = useState<Load<{ exists: boolean; executable: boolean }>>({ status: "idle" });
  const [sigs, setSigs] = useState<Load<SigInfo[]>>({ status: "idle" });
  const [epoch, setEpoch] = useState<Load<{ epoch: number; slotIndex: number; slotsInEpoch: number }>>({ status: "idle" });
  const [wallet, setWallet] = useState<string | null>(null);
  const [balance, setBalance] = useState<number | null>(null);
  const [walletError, setWalletError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setProgram({ status: "loading" });
    setSigs({ status: "loading" });
    setEpoch({ status: "loading" });
    getProgramInfo(PROGRAM_ID).then(
      (data) => setProgram({ status: "ok", data }),
      (e) => setProgram({ status: "error", error: String(e.message ?? e) }),
    );
    getRecentSignatures(PROGRAM_ID).then(
      (data) => setSigs({ status: "ok", data }),
      (e) => setSigs({ status: "error", error: String(e.message ?? e) }),
    );
    getEpoch().then(
      (data) => setEpoch({ status: "ok", data }),
      (e) => setEpoch({ status: "error", error: String(e.message ?? e) }),
    );
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (!wallet) return;
    getBalance(wallet).then(setBalance, () => setBalance(null));
  }, [wallet]);

  const onConnect = async () => {
    setWalletError(null);
    try {
      setWallet(await connectWallet());
    } catch (e) {
      setWalletError((e as Error).message);
    }
  };

  const progress = epoch.data ? epoch.data.slotIndex / epoch.data.slotsInEpoch : 0;
  // ~400ms per slot
  const hoursLeft = epoch.data ? ((epoch.data.slotsInEpoch - epoch.data.slotIndex) * 0.4) / 3600 : 0;

  return (
    <div className="devnet">
      <section className="hero hero-devnet">
        <div className="hero-copy">
          <Eyebrow>Devnet live</Eyebrow>
          <h1>
            The real program, <span className="accent">on-chain.</span>
          </h1>
          <p className="lead">
            Everything on this page is read straight from Solana devnet. Every transaction links to the explorer so anyone can
            check deposits, sponsorships and the VRF draw.
          </p>
          <div className="btn-row">
            <a className="btn btn-primary" href={explorer("address", PROGRAM_ID)} target="_blank" rel="noreferrer">
              Open program in explorer ↗
            </a>
            <button className="btn btn-outline" onClick={refresh}>
              Refresh
            </button>
          </div>
        </div>
        <Cat pose="idle" size={300} alt="Stakitty mascot" />
      </section>

      <div className="grid-3">
        <section className="card">
          <Eyebrow>Program</Eyebrow>
          <code className="addr">{short(PROGRAM_ID)}</code>
          <div className="status-line">
            {program.status === "loading" && <Badge tone="ice">Checking…</Badge>}
            {program.status === "ok" &&
              (program.data?.executable ? <Badge tone="mint">Deployed · executable</Badge> : <Badge tone="orange">Not found</Badge>)}
            {program.status === "error" && <Badge tone="orange">RPC error</Badge>}
          </div>
          {program.error && <p className="muted xsmall">{program.error}</p>}
        </section>

        <section className="card">
          <Eyebrow>Epoch</Eyebrow>
          {epoch.data ? (
            <>
              <div className="stat-big tone-purple">{epoch.data.epoch}</div>
              <div className="bar wide">
                <span style={{ width: `${progress * 100}%` }} />
              </div>
              <p className="muted xsmall">
                {Math.round(progress * 100)}% done · next turn in ~{hoursLeft.toFixed(1)} h. Stake activates and draws settle on
                epoch turns.
              </p>
            </>
          ) : (
            <p className="muted small">{epoch.status === "error" ? epoch.error : "Loading…"}</p>
          )}
        </section>

        <section className="card">
          <Eyebrow>Your wallet</Eyebrow>
          {wallet ? (
            <>
              <code className="addr">{short(wallet)}</code>
              <div className="stat-big tone-orange">{balance === null ? "…" : sol(balance, 3)} <span className="stat-unit">SOL</span></div>
              <button
                className="btn btn-outline btn-small"
                onClick={async () => {
                  await disconnectWallet().catch(() => undefined);
                  setWallet(null);
                  setBalance(null);
                }}
              >
                Disconnect
              </button>
            </>
          ) : (
            <>
              <p className="muted small">
                {findProvider() ? "Switch your wallet to Devnet, then connect." : "No wallet detected. Install Phantom or Solflare."}
              </p>
              <button className="btn btn-primary btn-small" onClick={onConnect}>
                Connect wallet
              </button>
              {walletError && <p className="muted xsmall">{walletError}</p>}
            </>
          )}
        </section>
      </div>

      <section className="card">
        <div className="row-between">
          <Eyebrow>Recent program transactions</Eyebrow>
          <span className="muted xsmall">RPC: {new URL(RPC_URL).host}</span>
        </div>
        {sigs.status === "loading" && <p className="muted small">Loading…</p>}
        {sigs.status === "error" && <p className="muted small">Could not load: {sigs.error}</p>}
        {sigs.status === "ok" && sigs.data && sigs.data.length === 0 && <p className="muted small">No transactions yet.</p>}
        {sigs.data && sigs.data.length > 0 && (
          <ol className="txs">
            {sigs.data.map((s) => (
              <li key={s.signature}>
                <a href={explorer("tx", s.signature)} target="_blank" rel="noreferrer">
                  {s.signature.slice(0, 10)}…{s.signature.slice(-6)}
                </a>
                <span className="muted">{s.blockTime ? new Date(s.blockTime * 1000).toLocaleString() : `slot ${s.slot}`}</span>
                {s.err ? <Badge tone="orange">failed</Badge> : <Badge tone="mint">ok</Badge>}
              </li>
            ))}
          </ol>
        )}
        <p className="muted xsmall">
          Deposit, sponsor and draw buttons for devnet are wired through the Anchor client (see README). Until then, the
          cluster-run script drives the pool and every step shows up here.
        </p>
      </section>
    </div>
  );
}
