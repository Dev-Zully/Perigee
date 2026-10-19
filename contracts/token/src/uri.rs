use crate::storage_types::{CachedUri, DataKey, UriInfo};
use soroban_sdk::{Env, String};

/// Maximum accepted metadata URI length in bytes.
pub const MAX_URI_BYTES: u32 = 512;
/// Default cache lifetime in ledgers (~12 hours at a 5s ledger).
pub const DEFAULT_URI_TTL: u32 = 8_640;
/// Upper bound for an overridable cache TTL (matches Soroban's max entry TTL).
pub const MAX_URI_TTL: u32 = 3_115_200;

const URI_PREFIXES: &[&[u8]] = &[b"http://", b"https://", b"ipfs://"];

/// Validates a metadata URI for format and size before it enters the cache.
///
/// On-chain validation is structural (scheme, printable ASCII, size).
/// Liveness probing of the external resource is performed by the caller and
/// attested by committing the URI while it is known to be reachable.
fn validate_uri(uri: &String) {
    if uri.is_empty() {
        panic!("uri is empty");
    }
    if uri.len() > MAX_URI_BYTES {
        panic!("uri exceeds MAX_URI_BYTES");
    }

    let mut buf = [0u8; MAX_URI_BYTES as usize];
    let n = uri.len() as usize;
    uri.copy_into_slice(&mut buf[..n]);

    let has_scheme = URI_PREFIXES.iter().any(|p| buf[..n].starts_with(p));
    if !has_scheme {
        panic!("uri must start with http://, https://, or ipfs://");
    }

    for &b in &buf[..n] {
        if !(0x21..=0x7e).contains(&b) {
            panic!("uri contains whitespace, non-printable, or non-ASCII bytes");
        }
    }
}

fn read_cache(e: &Env) -> Option<CachedUri> {
    e.storage().instance().get(&DataKey::UriCache)
}

fn write_cache(e: &Env, uri: String, ledger: u32) {
    e.storage().instance().set(
        &DataKey::UriCache,
        &CachedUri {
            uri,
            cached_ledger: ledger,
        },
    );
}

fn read_ttl(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::UriTtl)
        .unwrap_or(DEFAULT_URI_TTL)
}

fn write_ttl(e: &Env, ttl: u32) {
    e.storage().instance().set(&DataKey::UriTtl, &ttl);
}

fn is_expired(e: &Env, cache: &CachedUri) -> bool {
    let ttl = read_ttl(e);
    let now = e.ledger().sequence();
    now.saturating_sub(cache.cached_ledger) > ttl
}

/// Validates and stores a metadata URI with the current ledger as its anchor.
pub fn set_token_uri(e: &Env, uri: &String) {
    validate_uri(uri);
    write_cache(e, uri.clone(), e.ledger().sequence());
}

/// Returns the cached URI. A stale entry is freshened on read so the cache
/// stays warm while in active use; `token_uri_info` reports whether a read
/// hit a stale entry.
pub fn token_uri(e: &Env) -> String {
    let Some(cache) = read_cache(e) else {
        return String::from_str(e, "");
    };
    if is_expired(e, &cache) {
        write_cache(e, cache.uri.clone(), e.ledger().sequence());
    }
    cache.uri
}

/// Returns the full cache view including staleness so callers can observe
/// whether the active prompt/model entry has expired.
pub fn token_uri_info(e: &Env) -> UriInfo {
    let ttl = read_ttl(e);
    match read_cache(e) {
        Some(cache) => UriInfo {
            expired: is_expired(e, &cache),
            uri: cache.uri,
            cached_ledger: cache.cached_ledger,
            ttl,
        },
        None => UriInfo {
            uri: String::from_str(e, ""),
            cached_ledger: e.ledger().sequence(),
            ttl,
            expired: true,
        },
    }
}

/// Overrides the cache TTL. Throws on zero or over-long values.
pub fn set_token_uri_ttl(e: &Env, ttl: u32) {
    if ttl == 0 || ttl > MAX_URI_TTL {
        panic!("uri ttl must be between 1 and MAX_URI_TTL");
    }
    write_ttl(e, ttl);
}

/// Drops the cached URI, forcing the next `set_token_uri` to repopulate it.
pub fn invalidate_token_uri(e: &Env) {
    e.storage().instance().remove(&DataKey::UriCache);
}
