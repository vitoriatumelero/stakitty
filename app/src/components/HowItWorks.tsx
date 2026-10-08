import { Eyebrow } from "./ui";

const ITEMS = [
  {
    n: "01",
    tone: "orange",
    title: "Deposit any amount",
    text: "Below 1 SOL is fine: the pool stakes together. 15% stays liquid so most withdrawals are instant.",
  },
  {
    n: "02",
    tone: "purple",
    title: "Small validators sponsor",
    text: "Each season, validators pay SOL to join. Stake is split pro rata (max 35% each) and every lamport of sponsorship goes to prizes.",
  },
  {
    n: "03",
    tone: "mint",
    title: "Yield becomes the prize",
    text: "80% of the staking yield plus sponsorship fills the prize vault (20% is the protocol fee). A verifiable random draw picks the winner.",
  },
];

export function HowItWorks() {
  return (
    <section className="card">
      <Eyebrow>How it works</Eyebrow>
      <ol className="how">
        {ITEMS.map((i) => (
          <li key={i.n}>
            <span className={`how-n tone-${i.tone}`}>{i.n}</span>
            <div>
              <strong>{i.title}</strong>
              <p className="muted small">{i.text}</p>
            </div>
          </li>
        ))}
      </ol>
      <p className="muted xsmall">
        Honest math: on average depositors get ~3.6–4.1%/yr back as prizes vs ~5.2% native staking. You trade a tiny, certain
        yield for a chance at a prize that matters.
      </p>
    </section>
  );
}
