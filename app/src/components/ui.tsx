import type { ReactNode } from "react";
import type { Action } from "../sim/engine";

export type Dispatch = (a: Action) => void;

export const sol = (n: number, d = 2) =>
  n.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });

export function Eyebrow({ children }: { children: ReactNode }) {
  return <div className="eyebrow">{children}</div>;
}

export function Badge({ tone = "mint", children }: { tone?: "mint" | "orange" | "purple" | "ice"; children: ReactNode }) {
  return <span className={`badge badge-${tone}`}>{children}</span>;
}

export function Cat({ pose, size, alt = "" }: { pose: CatPose; size: number; alt?: string }) {
  return <img className="cat" src={`./cats/cat-${pose}.png`} alt={alt} style={{ height: size }} draggable={false} />;
}

export type CatPose = "standing" | "pixel" | "sleeping" | "waving" | "happy" | "point";

export function formatChance(p: number): string {
  if (p <= 0) return "—";
  if (p >= 0.01) return `${(p * 100).toFixed(1)}%`;
  return `1 in ${Math.round(1 / p).toLocaleString("en-US")}`;
}

/** Number input that accepts "0,5" (comma) as well as "0.5". */
export function parseAmount(v: string): number {
  return Number(v.replace(",", ".").trim());
}
