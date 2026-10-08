import { Cat } from "./ui";

export type Mode = "sim" | "devnet";

export function Header({ mode, onMode }: { mode: Mode; onMode: (m: Mode) => void }) {
  return (
    <header className="header">
      <div className="container header-inner">
        <a className="brand" href="./" aria-label="Stakitty home">
          <Cat pose="pixel" size={34} />
          <span className="wordmark">STAKITTY</span>
        </a>
        <div className="mode-switch" role="tablist" aria-label="Data source">
          <button role="tab" aria-selected={mode === "sim"} className={mode === "sim" ? "on" : ""} onClick={() => onMode("sim")}>
            Simulated
          </button>
          <button
            role="tab"
            aria-selected={mode === "devnet"}
            className={mode === "devnet" ? "on" : ""}
            onClick={() => onMode("devnet")}
          >
            Devnet live
          </button>
        </div>
      </div>
    </header>
  );
}
