# Pagination standard

Every list endpoint uses the same cursor-based contract. Implemented in
`core/src/pagination.rs`.

## Request

| Parameter | Meaning |
| --- | --- |
| `limit` | Page size, 1-100 (default 25). Larger values are rejected with `400 limit_too_large`. |
| `cursor` | Opaque token from the previous response's `next_cursor`. |
| `sort` | A field from the endpoint's allowlist; prefix `-` for descending (`sort=-created_at`). |
| `filter[name]` | Structured filter, only for the endpoint's allowlisted names. |
| `include_total` | `true` to also compute `total_count` (off by default: counting is expensive). |

Cursors are signed. A cursor that was edited, or that was issued for a different
`sort` or `filter`, is rejected (`invalid_cursor` / `cursor_mismatch`). Treat it as
opaque; do not parse or construct one.

## Response

```json
{
  "data": [ ... ],
  "pagination": { "next_cursor": "eyJ...", "has_more": true, "total_count": 120 }
}
```

`next_cursor` is `null` and `has_more` is `false` on the last page. `total_count` is
present only when `include_total=true`.

Rows are ordered by the sort field and then `id`, so rows that tie on the sort field
are never skipped or repeated between pages.

## Errors

`invalid_limit`, `limit_too_large`, `invalid_sort`, `invalid_filter`, `invalid_cursor`
and `cursor_mismatch`, all `400`.

## Endpoints

| Endpoint | Sortable | Default sort | Filters |
| --- | --- | --- | --- |
| `GET /vaults` | `created_at`, `name` | `-created_at` | `status` |
| `GET /managers` | `created_at` | `-created_at` | `status` |
| `GET /fees/history` | `ledger` | `-ledger` | none |

These are declared as `VAULTS_LIST`, `MANAGERS_LIST` and `FEE_HISTORY` in
`pagination.rs`. The handlers still answer with the older `page`/`page_size` shape;
moving a handler over means parsing its query with `PageRequest::parse`, fetching
`request.fetch_limit()` rows with `order_by()` / `keyset_predicate()`, and returning
`build_page(...)`.
