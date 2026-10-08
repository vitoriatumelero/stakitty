import { useEffect, useReducer, useState } from "react";
import { initialState, reduce } from "./sim/engine";
import { Header, Mode } from "./components/Header";
import { Hero } from "./components/Hero";
import { Stats } from "./components/Stats";
import { Position } from "./components/Position";
import { LatestDraw } from "./components/LatestDraw";
import { Validators } from "./components/Validators";
import { DemoConsole } from "./components/DemoConsole";
import { ActivityLog } from "./components/ActivityLog";
import { HowItWorks } from "./components/HowItWorks";
import { DevnetView } from "./components/DevnetView";
import { Footer } from "./components/Footer";

const MODE_KEY = "stakitty:mode";

function loadMode(): Mode {
  try {
    const m = localStorage.getItem(MODE_KEY);
    return m === "devnet" ? "devnet" : "sim";
  } catch {
    return "sim";
  }
}

export default function App() {
  const [mode, setMode] = useState<Mode>(loadMode);
  const [state, dispatch] = useReducer(reduce, "You (demo wallet)", initialState);

  useEffect(() => {
    try {
      localStorage.setItem(MODE_KEY, mode);
    } catch {
      /* storage unavailable: fine */
    }
  }, [mode]);

  return (
    <div className="page">
      <Header mode={mode} onMode={setMode} />
      <main className="container">
        {mode === "sim" ? (
          <>
            <Hero state={state} dispatch={dispatch} />
            <DemoConsole state={state} dispatch={dispatch} />
            <Stats state={state} />
            <div className="grid-2">
              <Position state={state} dispatch={dispatch} />
              <LatestDraw state={state} />
            </div>
            <Validators state={state} dispatch={dispatch} />
            <div className="grid-2">
              <HowItWorks />
              <ActivityLog state={state} />
            </div>
          </>
        ) : (
          <DevnetView />
        )}
      </main>
      <Footer />
    </div>
  );
}
