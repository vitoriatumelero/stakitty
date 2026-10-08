import { useState } from "react";
import { CAP_BPS, MIN_VALIDATORS, SimState } from "../sim/engine";
import { Cat, Dispatch, Eyebrow, parseAmount, sol } from "./ui";

export function Validators({ state, dispatch }: { state: SimState; dispatch: Dispatch }) {
  const free = state.validators.filter((v) => !state.bids[v.id]);
  const [vid, setVid] = useState<string>(free[0]?.id ?? "");
  const [amount, setAmount] = useState("0.50");
  const value = parseAmount(amount);
  const selected = free.some((v) => v.id === vid) ? vid : free[0]?.id ?? "";
  const bidsCount = Object.keys(state.bids).length;
  const bidsTotal = Object.values(state.bids).reduce((a, b) => a + b, 0);

  return (
    <section className="validators-wrap">
      <div className="sponsor-copy">
        <Eyebrow>For validators</Eyebrow>
        <h2>
          Validators pay to join. <span className="accent-purple">Stake follows the payment.</span>
        </h2>
        <dl className="rules">
          <div>
            <dt>Share</dt>
            <dd>paid ÷ total, capped at {CAP_BPS / 100}%</dd>
          </div>
          <div>
            <dt>Prizes</dt>
            <dd>100% of sponsorship goes to the prize vault</dd>
          </div>
          <div>
            <dt>Minimum</dt>
            <dd>{MIN_VALIDATORS} sponsoring validators per season</dd>
          </div>
        </dl>
        <Cat pose="waving" size={170} />
      </div>

      <div className="window">
        <div className="window-bar">
          <span>VALIDATORS.EXE</span>
          <span>SEASON {state.season}</span>
        </div>
        <div className="window-body">
          <table className="vtable">
            <thead>
              <tr>
                <th>Val.</th>
                <th>Comm.</th>
                <th>Share</th>
                <th>Stake</th>
                <th>Next bid</th>
              </tr>
            </thead>
            <tbody>
              {state.validators.map((v) => (
                <tr key={v.id} className={v.weight === 0 ? "dim" : ""}>
                  <td>{v.name}</td>
                  <td>{v.commission}%</td>
                  <td>
                    {v.weight > 0 ? `${Math.round(v.weight * 100)}%` : "—"}
                    {v.weight > 0 && (
                      <span className="bar">
                        <span style={{ width: `${(v.weight / (CAP_BPS / 10_000)) * 100}%` }} />
                      </span>
                    )}
                  </td>
                  <td>{v.stake > 0 ? sol(v.stake, 0) : "—"}</td>
                  <td>{state.bids[v.id] ? sol(state.bids[v.id]) : "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="window-note">
            Season {state.season} funded by {sol(state.seasonPaid)} SOL. Bids for season {state.season + 1}: {bidsCount}/
            {MIN_VALIDATORS}+ validators, {sol(bidsTotal)} SOL.
          </p>

          <div className="sponsor-form">
            <select value={selected} onChange={(e) => setVid(e.target.value)} disabled={!free.length} aria-label="Validator">
              {free.length ? (
                free.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.name}
                  </option>
                ))
              ) : (
                <option>All validators bid</option>
              )}
            </select>
            <div className="amount-box">
              <input inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} aria-label="Sponsorship in SOL" />
              <span className="unit">SOL</span>
            </div>
            <button
              className="btn btn-primary"
              disabled={!selected || !(value > 0) || value > state.myWallet}
              onClick={() => dispatch({ type: "sponsor", validatorId: selected, amount: value })}
            >
              Sponsor season {state.season + 1}
            </button>
          </div>
          <p className="window-note">In the demo you sign as the validator. On-chain, only the vote account's withdraw authority can sponsor.</p>
        </div>
      </div>
    </section>
  );
}
