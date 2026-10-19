# Performance notes

## Re-renders (#692)

- Purely presentational components that receive stable props are wrapped in
  `React.memo`: `Badge`, `Spinner`, `NutritionLabel`, `GasUsageChart`,
  `GasGolfingSuggestionsTable`, `ResourceHeatmap` and `StrategyPhaseTable`.
- `react-hooks/exhaustive-deps` is enforced as an **error** in
  `.eslintrc.json` (via `next/core-web-vitals`), so memoised callbacks and
  values cannot silently go stale.
- Wrap suspicious subtrees in `<RenderProfiler id="...">` to report renders
  slower than the 16ms budget (console in development, anonymous
  `slow_render` telemetry in production). Use the React DevTools Profiler to
  find the cause.
- When passing handlers or derived objects to a memoised child, stabilise them
  with `useCallback` / `useMemo`, otherwise `React.memo` is defeated.

## Web vitals (#693)

See `lib/webVitals.ts` and `pages/api/vitals.ts`. Metrics are sent only when
the user consented to performance telemetry. `GET /api/vitals` returns
p50/p75/p95 per metric, per page, device class and connection type, and the
alert state against the thresholds in `VITAL_THRESHOLDS`. The dashboard lives
at `/admin/vitals`.

## Hydration (#694)

Header/navigation, command palette, wallet connection and primary CTAs stay in
the main bundle and hydrate first. The analytics consent banner (`components/Analytics.tsx`) and other
non-critical widgets are loaded with `next/dynamic` and mounted inside
`Suspense` after the first paint. `lib/hydrationMetrics.ts` records
`perigee:hydration` (navigation start to hydration complete) as a User Timing
measure and a `hydration_time` telemetry event.

## Compression (#695)

See `docs/compression.md`.
