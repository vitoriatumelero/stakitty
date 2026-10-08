# Stakitty web app

Front-end for **Stakitty**, prize-linked staking on Solana: deposit any amount of SOL, the pool stakes it across 4–5 small validators, and the pool's yield becomes a verifiable prize. Your deposit always comes back.

> Devnet MVP. Not audited. No real funds.

## Run it

```bash
cd app
npm install
npm run dev        # http://localhost:5173
npm test           # simulation engine tests
npm run build      # static site in app/dist
```

Optional: `cp .env.example .env.local` and set `VITE_RPC_URL` to a dedicated devnet RPC (Helius, Triton…). Never commit `.env*` files.

## Two modes

| Mode | What it does |
|---|---|
| **Simulated** | Full product flow in the browser: deposit, sponsor as a validator, close the season (pro rata stake, 35% cap, min 4 validators), advance epochs (yield → 80% prize / 20% fee), run a draw weighted by time-weighted balance, claim, withdraw (instant from the 15% reserve, or a ticket after cooldown). Every screen is labelled *Simulated*. Used for the demo video. |
| **Devnet live** | Read-only view of the deployed program `8dFfRCbNDeYt9y96ZC8uCcU2BbXt2LwWEH91ZUN3BRQg`: deploy status, current epoch, recent program transactions with explorer links, and the connected wallet's devnet balance. |

The rules in `src/sim/engine.ts` mirror the on-chain program parameters (`RESERVE_BPS`, `FEE_BPS`, `CAP_BPS`, `MIN_VALIDATORS`). If the program changes, update them there.

## Layout

```
app/
  public/cats/          mascot art (from the pitch deck)
  src/sim/engine.ts     pure simulation of the pool rules (+ engine.test.ts)
  src/devnet/rpc.ts     minimal JSON-RPC reads (no SDK)
  src/devnet/wallet.ts  injected wallet connect (Phantom / Solflare / Backpack)
  src/components/       UI
```

## Next step: devnet writes

Wire the *Simulated* buttons to the real program for devnet mode:

1. Copy the IDL and types from `target/idl/stakitty.json` and `target/types/stakitty.ts` into `app/src/idl/`.
2. Add the Anchor TS client matching the program's Anchor version, plus `@solana/web3.js` and `@solana/wallet-adapter-react` (+ `-react-ui`).
3. Implement `deposit`, `withdraw`, `sponsor` and `claim` with the connected wallet; read `Pool`, `Season`, `Round` accounts for the stats instead of the simulation.
4. Keep `close_season`, `rebalance`, `settle_stake` and the VRF request in the existing `cluster-run` crank script.

## Visual identity

Navy `#1B2040`, orange `#F7A35C`, lilac `#A98BF5`, mint `#CFEDE3`, ice `#D6E9F2`, lavender `#EEEBFA`. Type: Figtree (text), Unbounded (numbers), Silkscreen (labels), Tiny5 (wordmark).
