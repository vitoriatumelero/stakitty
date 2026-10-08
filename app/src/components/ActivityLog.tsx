import { SimState } from "../sim/engine";
import { Eyebrow } from "./ui";

export function ActivityLog({ state }: { state: SimState }) {
  return (
    <section className="card">
      <Eyebrow>Activity</Eyebrow>
      <ol className="log">
        {state.log.map((e) => (
          <li key={e.id} className={`lk-${e.kind}`}>
            <span className="log-epoch">E{e.epoch}</span>
            <span>{e.text}</span>
          </li>
        ))}
      </ol>
    </section>
  );
}
