"use client";

import { useSettingsStore } from "@/lib/settingsStore";
import { getStage } from "@/lib/contracts.config";
import type { PreferredNetwork } from "@/lib/settings";

const stageLabels: Record<ReturnType<typeof getStage>, string> = {
  mainnet: "Mainnet",
  testnet: "Testnet",
  local: "Local",
};

/**
 * Network tab — issue #682.
 *
 * The *active* Soroban network is resolved at build time from
 * `NEXT_PUBLIC_STELLAR_NETWORK` (see `lib/contracts.config.ts`), which is
 * exactly what `NetworkStatusBanner` already reads. This tab does not
 * duplicate that logic or pretend to switch it at runtime — it shows the
 * build-time value read-only, and persists a UI-only "preferred network"
 * label plus a custom RPC endpoint / chain id override for forward
 * compatibility with a future runtime network switcher.
 */
export function NetworkSettingsTab() {
  const network = useSettingsStore((s) => s.settings.network);
  const updateNetwork = useSettingsStore((s) => s.updateNetwork);
  const activeStage = getStage();

  return (
    <div className="flex flex-col gap-6">
      <section>
        <h3 className="text-sm font-semibold text-slate-200">Active network</h3>
        <p className="mt-1 text-xs text-slate-400">
          Resolved at deploy time from <code className="text-cyan-400">NEXT_PUBLIC_STELLAR_NETWORK</code>.
          Changing it here does not switch contracts — see the status banner at the top of the app.
        </p>
        <div className="mt-2 flex items-center gap-2">
          <span
            className={`h-2 w-2 rounded-full ${
              activeStage === "mainnet" ? "bg-emerald-400" : activeStage === "testnet" ? "bg-amber-400" : "bg-slate-400"
            }`}
            aria-hidden="true"
          />
          <span className="text-sm text-slate-200">{stageLabels[activeStage]}</span>
        </div>
      </section>

      <section>
        <label htmlFor="preferred-network" className="text-sm font-semibold text-slate-200">
          Preferred network (display only)
        </label>
        <p className="mt-1 text-xs text-slate-400">
          Used for UI defaults such as which explorer links are shown. Does not change the deployed
          contract network.
        </p>
        <select
          id="preferred-network"
          value={network.preferredNetwork}
          onChange={(e) => updateNetwork({ preferredNetwork: e.target.value as PreferredNetwork })}
          className="mt-2 w-full max-w-xs rounded border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-cyan-500"
        >
          <option value="mainnet">Mainnet</option>
          <option value="testnet">Testnet</option>
        </select>
      </section>

      <section>
        <label htmlFor="custom-rpc" className="text-sm font-semibold text-slate-200">
          Custom RPC endpoint
        </label>
        <input
          id="custom-rpc"
          type="url"
          inputMode="url"
          placeholder="https://soroban-rpc.example.com"
          value={network.customRpcEndpoint}
          onChange={(e) => updateNetwork({ customRpcEndpoint: e.target.value })}
          className="mt-2 w-full max-w-md rounded border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-200 placeholder:text-slate-600 focus:outline-none focus:ring-2 focus:ring-cyan-500"
        />
      </section>

      <section>
        <label htmlFor="chain-id" className="text-sm font-semibold text-slate-200">
          Chain ID
        </label>
        <input
          id="chain-id"
          type="text"
          placeholder="Testnet SDF Network ; September 2015"
          value={network.chainId}
          onChange={(e) => updateNetwork({ chainId: e.target.value })}
          className="mt-2 w-full max-w-md rounded border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-200 placeholder:text-slate-600 focus:outline-none focus:ring-2 focus:ring-cyan-500"
        />
      </section>
    </div>
  );
}
