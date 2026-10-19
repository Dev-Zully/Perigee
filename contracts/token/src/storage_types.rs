use soroban_sdk::{contracttype, Address, String};

#[derive(Clone)]
#[contracttype]
pub struct AllowanceDataKey {
    pub from: Address,
    pub spender: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct AllowanceValue {
    pub amount: i128,
    pub expiration_ledger: u32,
}

/// Token metadata grouped into a single instance-storage entry.
/// Replaces 3 separate DataKey variants: Name, Symbol, Decimals.
#[derive(Clone)]
#[contracttype]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
}

/// A cached token metadata URI and the ledger at which it was written.
#[derive(Clone)]
#[contracttype]
pub struct CachedUri {
    pub uri: String,
    pub cached_ledger: u32,
}

/// Observability view of the metadata URI cache for callers.
#[derive(Clone)]
#[contracttype]
pub struct UriInfo {
    pub uri: String,
    pub cached_ledger: u32,
    pub ttl: u32,
    pub expired: bool,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Allowance(AllowanceDataKey),
    Balance(Address),
    Admin,
    State(Address),
    /// Single instance key replacing Name + Symbol + Decimals.
    Metadata,
    /// Cached token metadata URI validated before write.
    UriCache,
    /// Lifetime (in ledgers) before the cached URI is considered stale.
    UriTtl,
}
