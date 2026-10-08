import { MIN_VALIDATORS, SimState } from "../sim/engine";
import { Dispatch } from "./ui";

/** Buttons that play the role of time passing and of the crank/admin, for the demo. */
export function DemoConsole({ state, dispatch }: { state: SimState; dispatch: Dispatch }) {
  const canClose = Object.keys(state.bids).length >= MIN_VALIDATORS;
  const canDraw = state.prizeVault > 0 && state.myTwab + state.othersTwab > 0;

  const draw = () => {
    const bytes = new Uint8Array(32);
    crypto.getRandomValues(bytes);
    dispatch({ type: "draw", randomness: bytes });
  };

  return (
    <section className="console" aria-label="Demo console">
      <div className="console-title">
        <span className="dot" />
        DEMO CONSOLE <span className="muted">· time and crank, simulated</span>
      </div>
      <div className="console-buttons">
        <button className="btn btn-console" onClick={() => dispatch({ type: "advanceEpoch" })}>
          ⏭ Advance epoch <small>~2 days</small>
        </button>
        <button
          className="btn btn-console"
          disabled={!canClose}
          title={canClose ? "" : `Needs ${MIN_VALIDATORS} validator bids`}
          onClick={() => dispatch({ type: "closeSeason" })}
        >
          ⟳ Close season &amp; rebalance
        </button>
        <button className="btn btn-console btn-console-hot" disabled={!canDraw} onClick={draw}>
          ✦ Run draw
        </button>
        <button className="btn btn-console ghost" onClick={() => dispatch({ type: "reset" })}>
          Reset
        </button>
      </div>
    </section>
  );
}
