"use client";

/**
 * React binding for the settings domain model (issue #682).
 *
 * Mirrors the `createStore`/`createUseStore` pattern already used by
 * `context/WalletContext.tsx` instead of pulling in a state-management
 * dependency for a single page's worth of state.
 */

import { useEffect } from "react";
import { createStore, createUseStore } from "./createStore";
import {
  AppSettings,
  DEFAULT_SETTINGS,
  clampSlippage,
  clampDeadlineMinutes,
  clampGasPrice,
  loadSettings,
  persistSettings,
  type AdvancedSettings,
  type DisplaySettings,
  type NetworkSettings,
  type SecuritySettings,
  type TradingSettings,
} from "./settings";

interface SettingsState {
  settings: AppSettings;
  /** False until the client has read localStorage once (avoids SSR/client mismatch). */
  hydrated: boolean;
  hydrate: () => void;
  updateNetwork: (patch: Partial<NetworkSettings>) => void;
  updateTrading: (patch: Partial<TradingSettings>) => void;
  updateDisplay: (patch: Partial<DisplaySettings>) => void;
  updateSecurity: (patch: Partial<SecuritySettings>) => void;
  updateAdvanced: (patch: Partial<AdvancedSettings>) => void;
  resetSettings: () => void;
}

const settingsStore = createStore<SettingsState>((set, get) => ({
  settings: DEFAULT_SETTINGS,
  hydrated: false,

  hydrate: () => {
    if (get().hydrated) return;
    set({ settings: loadSettings(), hydrated: true });
  },

  updateNetwork: (patch) => {
    const next: AppSettings = {
      ...get().settings,
      network: { ...get().settings.network, ...patch },
    };
    set({ settings: next });
    persistSettings(next);
  },

  updateTrading: (patch) => {
    const merged: TradingSettings = { ...get().settings.trading, ...patch };
    if (patch.slippageTolerance !== undefined) {
      merged.slippageTolerance = clampSlippage(patch.slippageTolerance);
    }
    if (patch.transactionDeadlineMinutes !== undefined) {
      merged.transactionDeadlineMinutes = clampDeadlineMinutes(patch.transactionDeadlineMinutes);
    }
    if (patch.customGasPrice !== undefined) {
      merged.customGasPrice = clampGasPrice(patch.customGasPrice);
    }
    const next: AppSettings = { ...get().settings, trading: merged };
    set({ settings: next });
    persistSettings(next);
  },

  updateDisplay: (patch) => {
    const next: AppSettings = {
      ...get().settings,
      display: { ...get().settings.display, ...patch },
    };
    set({ settings: next });
    persistSettings(next);
  },

  updateSecurity: (patch) => {
    const next: AppSettings = {
      ...get().settings,
      security: { ...get().settings.security, ...patch },
    };
    set({ settings: next });
    persistSettings(next);
  },

  updateAdvanced: (patch) => {
    const next: AppSettings = {
      ...get().settings,
      advanced: {
        ...get().settings.advanced,
        ...patch,
        featureFlagOverrides: {
          ...get().settings.advanced.featureFlagOverrides,
          ...(patch.featureFlagOverrides ?? {}),
        },
      },
    };
    set({ settings: next });
    persistSettings(next);
  },

  resetSettings: () => {
    set({ settings: DEFAULT_SETTINGS });
    persistSettings(DEFAULT_SETTINGS);
  },
}));

const useSettingsStoreImpl = createUseStore(settingsStore);

export function useSettingsStore<U>(selector: (state: SettingsState) => U): U {
  return useSettingsStoreImpl(selector);
}

/**
 * Hydrates the store from localStorage on mount. Call once near the root of
 * the settings page — every field reads `DEFAULT_SETTINGS` until then, which
 * keeps the server-rendered and first client render identical.
 */
export function useHydrateSettings(): boolean {
  const hydrated = useSettingsStore((s) => s.hydrated);
  const hydrate = useSettingsStore((s) => s.hydrate);

  useEffect(() => {
    hydrate();
  }, [hydrate]);

  return hydrated;
}

export { settingsStore };
