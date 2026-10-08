import { Cat } from "./ui";

export function Footer() {
  return (
    <footer className="footer">
      <div className="container footer-inner">
        <Cat pose="napping" size={96} />
        <div>
          <p className="footer-line">
            Alone, it's pocket change. <span className="accent">Together, it's a prize.</span>
          </p>
          <p className="muted xsmall">
            Devnet MVP · not audited · no real funds. Open source, built for Colosseum's Crypto World's Fair (Solana track).
          </p>
        </div>
      </div>
    </footer>
  );
}
