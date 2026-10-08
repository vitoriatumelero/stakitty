import { useState } from "react";
import { myChance, SimState } from "../sim/engine";
import { Dispatch, Eyebrow, formatChance, parseAmount, sol } from "./ui";

export function Position({ state, dispatch }: { state: SimState; dispatch: Dispatch }) {
  const [amount, setAmount] = useState("");
  const value = parseAmount(amount);
  const valid = value > 0 && value <= state.myDeposit + 1e-9;
  const instant = value <= state.reserve;
  const chance = myChance(state);

  return (
    <section className="card">
      <Eyebrow>Your position</Eyebrow>
      <div className="kv">
        <div>
          <span>Deposited</span>
          <strong>{sol(state.myDeposit)} SOL</strong>
        </div>
        <div>
          <span>Chance next round</span>
          <strong>{state.myDeposit > 0 || state.myTwab > 0 ? formatChance(chance) : "—"}</strong>
        </div>
        <div>
          <span>Prize to claim</span>
          <strong className={state.myWinnings > 0 ? "tone-orange" : ""}>{sol(state.myWinnings)} SOL</strong>
        </div>
      </div>
      <p className="muted small">
        Chance is your average balance over time divided by everyone's, so depositing a minute before the draw doesn't help.
      </p>

      {state.myWinnings > 0 && (
        <button className="btn btn-orange btn-block" onClick={() => dispatch({ type: "claim" })}>
          Claim {sol(state.myWinnings)} SOL prize
        </button>
      )}

      <div className="divider" />
      <label className="field">
        <span className="field-label">Withdraw</span>
        <div className="amount-box dark">
          <input
            inputMode="decimal"
            placeholder="0.00"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            aria-label="Withdraw amount in SOL"
          />
          <span className="unit">SOL</span>
          <button className="max" onClick={() => setAmount(String(state.myDeposit))}>
            MAX
          </button>
        </div>
      </label>
      <button
        className="btn btn-outline btn-block"
        disabled={!valid}
        onClick={() => {
          dispatch({ type: "withdraw", amount: value });
          setAmount("");
        }}
      >
        {valid ? (instant ? `Withdraw ${sol(value)} SOL now` : `Request ${sol(value)} SOL (≈2 days)`) : "Withdraw"}
      </button>
      <p className="muted small">Instant from the 15% reserve. Larger amounts wait one epoch (~2 days) for the stake to cool down.</p>

      {state.tickets.length > 0 && (
        <ul className="tickets">
          {state.tickets.map((t) => (
            <li key={t.id}>
              Ticket #{t.id} · {sol(t.amount)} SOL · ready at epoch {t.readyEpoch}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
