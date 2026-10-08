import { short, SimState } from "../sim/engine";
import { Badge, Cat, Eyebrow, formatChance, sol } from "./ui";

export function LatestDraw({ state }: { state: SimState }) {
  const r = state.rounds[0];
  return (
    <section className="card">
      <div className="row-between">
        <Eyebrow>{r ? `Round #${r.id}` : "Draws"}</Eyebrow>
        <Badge tone="mint">Simulated VRF</Badge>
      </div>
      {!r ? (
        <div className="empty">
          <Cat pose="sleeping" size={110} />
          <p className="muted">
            No draw yet. Deposit, advance an epoch so your balance counts, then run a draw from the demo console.
          </p>
        </div>
      ) : (
        <>
          <div className={`winner-card ${r.winnerIsMe ? "me" : ""}`}>
            <div className="eyebrow dark">Winner</div>
            <div className="winner-amount">{sol(r.prize)} SOL</div>
            <div className="winner-to">to {r.winnerIsMe ? "you 🎉" : short(r.winner)}</div>
          </div>
          <div className="proof">
            <span className="check" aria-hidden>
              ✓
            </span>
            <div>
              <strong>Randomness proof</strong>
              <code title={r.randomness}>{r.randomness.slice(0, 32)}…</code>
            </div>
          </div>
          <p className="muted small">
            {r.winnerIsMe
              ? "You won! Claim the prize in Your position. Your deposit stays in the pool."
              : `Not this time. Your ${sol(state.myDeposit)} SOL is still yours. Your chance was ${formatChance(r.myChance)}.`}
          </p>
          <p className="muted xsmall">
            On devnet the randomness comes from MagicBlock VRF and the proof is checkable in the explorer. Here it is
            generated in your browser.
          </p>
          {state.rounds.length > 1 && (
            <ul className="history">
              {state.rounds.slice(1, 5).map((x) => (
                <li key={x.id}>
                  <span>#{x.id}</span>
                  <span>{sol(x.prize)} SOL</span>
                  <span>{x.winnerIsMe ? "you" : short(x.winner)}</span>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}
