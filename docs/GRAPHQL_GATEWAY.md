# GraphQL API gateway

`core/src/graphql_gateway.rs` sits in front of the vaults, users, transactions and analytics
subgraphs. A request is processed in this order:

1. **Authentication**: `Context::from_claims` builds the caller from JWT claims that the auth
   layer has already verified.
2. **Persisted queries**: in `AllowlistOnly` mode only queries registered with
   `PersistedQueries::register` (at build time) run. Clients may send just
   `extensions.persistedQuery.sha256Hash`. A hash that does not match the supplied query text
   is rejected, so an allowed hash cannot smuggle in a different query.
3. **Parse** the document.
4. **Complexity limits**: maximum depth (default 10) and maximum cost (default 1000). Cost is
   1 per field (overridable per field with `Limits::field_costs`), `__typename` is free, and a
   field's children are multiplied by its `first`/`last`/`limit`/`take` argument (a literal or a
   variable), so `vaults(first: 100) { owner { id } }` costs more than `vaults(first: 5) {...}`.
   Aliases are counted per use, and fragments are expanded (cycles are rejected).
5. **Per-operation rate limit**: fixed window per user and operation name, with overrides
   (for example a stricter limit for a mutation).
6. **Routing**: `SubgraphRegistry::standard()` maps root fields to subgraphs; the plan is
   passed to an `Executor`.

## Batching and the DataLoader

A JSON array body is a batch (default maximum 10); each operation succeeds or fails on its
own. Resolvers use `DataLoader::load_many` to turn N lookups into one batched call per
request and to cache the results.

## Errors

Errors follow the GraphQL shape with a stable `extensions.code`: `GRAPHQL_PARSE_FAILED`,
`GRAPHQL_VALIDATION_FAILED`, `PERSISTED_QUERY_NOT_FOUND`, `PERSISTED_QUERY_NOT_ALLOWED`,
`QUERY_TOO_DEEP`, `QUERY_TOO_COMPLEX`, `RATE_LIMITED`, `UNAUTHENTICATED`, `BAD_USER_INPUT`,
`BAD_REQUEST`.

## Schema versions

`SchemaTags::publish(tag, sdl)` records which schema each tag (`production`, `staging`) points
at. Publishing to Apollo Studio's registry is a CI step (`rover subgraph publish`) outside this
crate.
