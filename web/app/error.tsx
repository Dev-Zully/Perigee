'use client';

import { useEffect } from 'react';
import { reportError } from '../lib/error-reporting';

export default function AppRouteError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    reportError(error, {
      boundary: 'route',
      route: typeof window !== 'undefined' ? window.location.pathname : '/',
      component: 'AppRouteError',
    });
  }, [error]);

  return (
    <div className="flex min-h-screen items-center justify-center bg-slate-950 px-4 text-slate-50">
      <div className="w-full max-w-lg rounded-xl border border-red-800/70 bg-slate-900 p-6 shadow-2xl shadow-black/30">
        <h2 className="text-xl font-semibold text-red-100">This page failed to load</h2>
        <p className="mt-3 text-sm leading-6 text-slate-300">
          The route hit an unexpected error during render. Try again to recover without reloading the app.
        </p>
        <div className="mt-5 flex gap-3">
          <button
            type="button"
            onClick={reset}
            className="rounded-md border border-red-700 bg-red-900/40 px-4 py-2 text-sm font-medium text-red-50 hover:bg-red-900/60"
          >
            Try again
          </button>
          <button
            type="button"
            onClick={() => {
              if (typeof window !== 'undefined') {
                window.location.reload();
              }
            }}
            className="rounded-md border border-slate-700 px-4 py-2 text-sm font-medium text-slate-100 hover:bg-slate-800"
          >
            Reload page
          </button>
        </div>
      </div>
    </div>
  );
}
