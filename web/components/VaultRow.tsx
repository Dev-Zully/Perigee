import type { Vault } from "@/types/vault";
import { PrefetchLink } from "./PrefetchLink";

export interface VaultRowProps {
  vault: Vault;
  style?: React.CSSProperties;
  /** When set, the vault name links to this page and prefetches it on hover (#691). */
  href?: string;
}

export function VaultRow({ vault, style, href }: VaultRowProps) {
  return (
    <div
      style={style}
      className="flex items-center justify-between rounded-lg border border-slate-800 bg-slate-900/60 px-4 py-3"
    >
      <div className="flex flex-col">
        {href ? (
          <PrefetchLink href={href} className="text-sm font-medium text-white hover:text-cyan-300">
            {vault.name}
          </PrefetchLink>
        ) : (
          <span className="text-sm font-medium text-white">{vault.name}</span>
        )}
        <span className="text-xs text-slate-400">{vault.owner}</span>
      </div>
      <div className="flex items-center gap-4 text-xs text-slate-400">
        <span className="font-mono text-cyan-400">{vault.balance.toLocaleString()} {vault.asset}</span>
        <span className="text-emerald-400">{vault.apy !== undefined ? `${vault.apy}%` : "0.0%"}</span>
        <span
          className={`rounded-full px-2 py-0.5 text-[10px] font-semibold ${
            vault.status === "ACTIVE"
              ? "bg-emerald-950 text-emerald-400"
              : vault.status === "LOCKED"
                ? "bg-red-950 text-red-400"
                : vault.status === "PENDING"
                  ? "bg-amber-950 text-amber-400"
                  : "bg-slate-800 text-slate-400"
          }`}
        >
          {vault.status}
        </span>
      </div>
    </div>
  );
}
