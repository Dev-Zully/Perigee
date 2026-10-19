# Event Indexer

**CONTRACT-34 · Reusable on-chain event indexing helpers for every Perigee contract.**

A small Soroban contract that gives any other contract a standard way to build
an on-chain, queryable event log without coupling each contract to a bespoke
storage layout. Independent of any specific contract — feed it a topic, an
event name, a correlation key, and a payload.

## Why

Contracts emit transient events via `env.events().publish`, which are only
visible to the invocation that produced them and are not queryable by
off-chain indexers after the fact. `event_indexer` persists those records
on-chain in bounded per-topic logs so:

- off-chain consumers can paginate events with a stable cursor (`seq`),
- the most recent event per topic is readable in one call,
- retention is bounded (FIFO eviction) so storage cost stays predictable.

## API

| Function | Description |
|----------|-------------|
| `record(topic, event_name, key, payload) -> u32` | Append an event to `topic`; publishes a contract event and returns the assigned `seq` (1-based, monotonic per topic). |
| `query(topic, from_seq, limit) -> Vec<IndexedEvent>` | Return events `seq >= from_seq`, oldest-first, at most `limit`. |
| `count(topic) -> u32` | Number of retained events in `topic`. |
| `latest(topic) -> Option<IndexedEvent>` | Most recent event in `topic`. |
| `set_topic_limit(topic, limit) -> Result<(), Error>` | Set the per-topic retention limit (≥ 1). |
| `get_topic_limit(topic) -> u32` | Per-topic retention limit (default 256). |

### `IndexedEvent` payload

```rust
pub struct IndexedEvent {
    pub seq: u32,              // monotonic per topic
    pub timestamp: u64,        // ledger timestamp on record
    pub event_name: Symbol,    // e.g. pause_set, Payment Confirmed
    pub key: String,           // correlation key (contract id, claim id, ...)
    pub payload: Vec<(Symbol, Val)>, // (field, value) pairs
}
```

`payload` values use the host `Val` type, so any Soroban value (numbers,
symbols, addresses, `BytesN`, nested types) can be indexed without losing type
information.

## Errors

| Error | Meaning |
|-------|---------|
| `LimitTooSmall` | `set_topic_limit` called with `0`. |
| `InvalidQueryLimit` | `query` called with `limit == 0`. |

## Storage model

- Per-topic logs are stored under `DataKey::Log(topic)` in **persistent**
  storage with a bounded FIFO window (`DataKey::TopicLimit(topic)`).
- Records older than the limit are dropped by re-building a shifted log, so the
  retained window is always `≤ limit` entries.
- TTL is extended on every write (mirroring the `token` contract convention).

## Tests

```bash
cargo test -p event_indexer
```

Covers record/query/count/latest semantics, topic independence, FIFO eviction,
empty-topic behaviour, and limit validation.