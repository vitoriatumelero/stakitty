import { useState } from "react";
import { MIN_DEPOSIT, SimState } from "../sim/engine";
import { Badge, Cat, Dispatch, Eyebrow, parseAmount, sol } from "./ui";

export function Hero({ state, dispatch }: { state: SimState; dispatch: Dispatch }) {
  const [amount, setAmount] = useState("0.10");
  const value = parseAmount(amount);
  const valid = value >= MIN_DEPOSIT && value <= state.myWallet;
  const won = state.rounds[0]?.winnerIsMe;

  return (
    <section className="hero">
      <div className="hero-copy">
        <Eyebrow>Prize-linked staking · Solana</Eyebrow>
        <h1>
          Any amount of SOL. <span className="accent">The yield becomes the prize.</span>
        </h1>
        <p className="lead">
          Stakitty pools everyone's SOL, stakes it with small validators and turns the pool's yield into verifiable prizes.
          Your deposit always comes back.
        </p>
        <ul className="steps">
          <li>
            <span className="pill pill-mint">Deposit</span>
            <span>From ~0.01 SOL. Withdraw anytime.</span>
          </li>
          <li>
            <span className="pill pill-ice">Stake</span>
            <span>Across 4–5 small validators.</span>
          </li>
          <li>
            <span className="pill pill-orange">Win</span>
            <span>Verifiable on-chain draw.</span>
          </li>
        </ul>
      </div>

      <div className="phone">
        <div className="phone-screen">
          <div className="phone-top">
            <span className="wordmark small">STAKITTY</span>
            <Badge tone="mint">Simulated</Badge>
          </div>
          <div className="prize-card">
            <div className="eyebrow light">Next prize</div>
            <div className="prize-amount">{sol(state.prizeVault)} SOL</div>
            <Cat pose={won ? "happy" : "sleeping"} size={118} />
          </div>
          <div className="mini-row">
            <span>Your deposit</span>
            <strong>{sol(state.myDeposit)}</strong>
          </div>
          <label className="field">
            <span className="field-label">Amount</span>
            <div className="amount-box">
              <input
                inputMode="decimal"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                aria-label="Deposit amount in SOL"
              />
              <span className="unit">SOL</span>
              <button className="max" onClick={() => setAmount(String(Math.max(0, Math.floor(state.myWallet * 100) / 100)))}>
                MAX
              </button>
            </div>
          </label>
          <p className="fineprint">Your deposit always comes back. Only the yield becomes the prize.</p>
          <button
            className="btn btn-primary btn-block"
            disabled={!valid}
            onClick={() => dispatch({ type: "deposit", amount: value })}
          >
            {valid ? `Deposit ${sol(value)} SOL` : value > state.myWallet ? "Not enough SOL" : `Min ${MIN_DEPOSIT} SOL`}
          </button>
          <div className="wallet-line">Demo wallet: {sol(state.myWallet)} SOL</div>
        </div>
      </div>
    </section>
  );
}
