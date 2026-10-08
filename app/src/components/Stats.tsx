import { EPOCHS_PER_SEASON, SimState, totalDeposits } from "../sim/engine";
import { sol } from "./ui";

export function Stats({ state }: { state: SimState }) {
  const sponsors = state.validators.filter((v) => v.weight > 0).length;
  const epochInSeason = state.epoch - state.seasonStartEpoch;
  return (
    <section className="stats" aria-label="Pool stats">
      <Stat big={sol(totalDeposits(state), 0)} unit="SOL" label="Total deposited" tone="orange" />
      <Stat big={sol(state.staked, 0)} unit="SOL" label={`In stake across ${sponsors} validators`} tone="purple" />
      <Stat big={sol(state.reserve, 0)} unit="SOL" label="Withdrawal reserve (15%)" tone="mint" />
      <Stat
        big={`S${state.season}`}
        unit={`· E${state.epoch}`}
        label={`Season · epoch (${epochInSeason}/${EPOCHS_PER_SEASON} into the season)`}
        tone="light"
      />
    </section>
  );
}

function Stat({ big, unit, label, tone }: { big: string; unit: string; label: string; tone: string }) {
  return (
    <div className="card stat">
      <div className={`stat-big tone-${tone}`}>
        {big} <span className="stat-unit">{unit}</span>
      </div>
      <div className="stat-label">{label}</div>
    </div>
  );
}
