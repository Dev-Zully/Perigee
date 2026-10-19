# Dynamic Fees (#667)

Utilization-based, volatility-adjusted, competitor-aware fee quotes.

- `quote(signals)` → `FeeQuote` clamped to `[min_fee_bps, max_fee_bps]`
- Competitor clamp respects `max_undercut_bps` / `max_premium_bps`
