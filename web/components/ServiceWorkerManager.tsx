"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import {
  applyServiceWorkerUpdate,
  registerServiceWorker,
} from "../lib/serviceWorker";

/**
 * Registers the service worker on first visit and shows a prompt when a new
 * version is waiting. Offline status itself is surfaced by
 * `NetworkStatusBanner`; this component additionally tells the user when
 * mutations were queued for background sync (issue #688).
 */
export function ServiceWorkerManager() {
  const t = useTranslations();
  const [registration, setRegistration] = useState<ServiceWorkerRegistration | null>(null);
  const [queued, setQueued] = useState(false);

  useEffect(() => {
    let cancelled = false;
    registerServiceWorker({
      onUpdateAvailable: (reg) => {
        if (!cancelled) setRegistration(reg);
      },
      onMutationQueued: () => setQueued(true),
      onMutationsReplayed: () => setQueued(false),
    }).catch(() => {
      // Registration failure must never break the app.
    });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!registration && !queued) return null;

  return (
    <div
      role="status"
      aria-live="polite"
      className="fixed bottom-4 left-4 z-50 max-w-sm rounded-xl border border-slate-800 bg-slate-900/95 p-4 text-xs text-slate-200 shadow-xl backdrop-blur"
    >
      {queued && <p className="mb-2 text-slate-400">{t("serviceWorker.queued")}</p>}
      {registration && (
        <div className="flex items-center justify-between gap-3">
          <span>{t("serviceWorker.updateAvailable")}</span>
          <button
            type="button"
            onClick={() => applyServiceWorkerUpdate(registration)}
            className="rounded bg-cyan-600 px-3 py-1.5 font-medium text-white transition-colors hover:bg-cyan-500"
          >
            {t("serviceWorker.reload")}
          </button>
        </div>
      )}
    </div>
  );
}
