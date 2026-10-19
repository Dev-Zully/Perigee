#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotEnoughSources = 1,
    NotEnoughValidPrices = 2,
    NotEnoughReliableSources = 3,
    InvalidPrice = 4,
    StalePrice = 5,
    NotInitialized = 6,
    AlreadyInitialized = 7,
}

/// Storage keys used by the oracle aggregator contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Whether the contract has been initialized.
    Initialized,
    /// Maximum age (in seconds) before a price is considered stale.
    MaxStaleness,
    /// Stores the latest price entry for a given source address.
    SourcePrice(Address),
}

/// A stored price entry with its timestamp.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceEntry {
    /// The price value.
    pub price: i128,
    /// The ledger timestamp when the price was stored.
    pub timestamp: u64,
}

#[soroban_sdk::contractclient(name = "PriceOracleClient")]
pub trait PriceOracle {
    fn latest_price(env: Env) -> i128;
}

#[contract]
pub struct OracleAggregator;

#[contractimpl]
impl OracleAggregator {
    /// Initializes the oracle aggregator with a maximum staleness threshold.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `max_staleness`: Maximum age in seconds before a price is considered stale.
    ///   Use 0 to disable staleness checks.
    ///
    /// # Returns
    /// - `Ok(())` when initialization succeeds.
    /// - `Err(Error::AlreadyInitialized)` if already initialized.
    pub fn initialize(env: Env, max_staleness: u64) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage()
            .instance()
            .set(&DataKey::Initialized, &true);
        env.storage()
            .instance()
            .set(&DataKey::MaxStaleness, &max_staleness);
        Ok(())
    }

    /// Returns the current maximum staleness threshold.
    pub fn get_max_staleness(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MaxStaleness)
            .unwrap_or(0)
    }

    /// Updates the maximum staleness threshold.
    pub fn set_max_staleness(env: Env, max_staleness: u64) {
        env.storage()
            .instance()
            .set(&DataKey::MaxStaleness, &max_staleness);
    }

    /// Stores a price from a specific oracle source with the current timestamp.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `source`: The address of the oracle source.
    /// - `price`: The price to store (must be > 0).
    ///
    /// # Returns
    /// - `Ok(())` on success.
    /// - `Err(Error::InvalidPrice)` if price <= 0.
    pub fn submit_price(env: Env, source: Address, price: i128) -> Result<(), Error> {
        if price <= 0 {
            return Err(Error::InvalidPrice);
        }
        let entry = PriceEntry {
            price,
            timestamp: env.ledger().timestamp(),
        };
        env.storage()
            .instance()
            .set(&DataKey::SourcePrice(source), &entry);
        Ok(())
    }

    /// Returns the stored price entry for a given source, if it exists.
    pub fn get_price_entry(env: Env, source: Address) -> Option<PriceEntry> {
        env.storage()
            .instance()
            .get(&DataKey::SourcePrice(source))
    }

    /// Aggregates prices from multiple oracle sources, using stored prices and
    /// staleness checks.
    ///
    /// This method combines two strategies:
    /// 1. Fetches live prices from oracle sources that support the PriceOracle trait.
    /// 2. Falls back to stored prices from previous `submit_price` calls.
    ///
    /// Filters out prices that exceed the staleness threshold and returns the
    /// median of the reliable sources.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `sources`: A list of oracle source addresses.
    ///
    /// # Returns
    /// - `Ok(i128)`: The median price from reliable sources.
    /// - `Err(Error::NotEnoughSources)` if fewer than 3 sources provided.
    /// - `Err(Error::NotEnoughValidPrices)` if fewer than 3 valid prices after filtering.
    /// - `Err(Error::NotEnoughReliableSources)` if fewer than 3 prices remain after outlier filtering.
    pub fn aggregate_price(env: Env, sources: Vec<Address>) -> Result<i128, Error> {
        if sources.len() < 3 {
            return Err(Error::NotEnoughSources);
        }

        let max_staleness: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MaxStaleness)
            .unwrap_or(0);
        let now = env.ledger().timestamp();

        let mut prices = Vec::new(&env);
        for idx in 0..sources.len() {
            let source = sources.get(idx).unwrap();

            // Try live price first via cross-contract call
            let client = PriceOracleClient::new(&env, &source);
            let live_price = client.latest_price();
            if live_price > 0 && Self::is_price_fresh(&env, &source, now, max_staleness) {
                // Update stored entry with live price
                let entry = PriceEntry {
                    price: live_price,
                    timestamp: now,
                };
                env.storage()
                    .instance()
                    .set(&DataKey::SourcePrice(source), &entry);
                prices.push_back(live_price);
                continue;
            }

            // Fall back to stored price
            if let Some(entry) = Self::get_stored_price(&env, &source) {
                if max_staleness == 0 || now.saturating_sub(entry.timestamp) <= max_staleness {
                    prices.push_back(entry.price);
                }
            }
        }

        if prices.len() < 3 {
            return Err(Error::NotEnoughValidPrices);
        }

        let sorted = Self::sort_prices(prices);
        let median = Self::median(&sorted);
        let filtered = Self::filter_outliers(&env, &sorted, median);

        if filtered.len() < 3 {
            return Err(Error::NotEnoughReliableSources);
        }

        Ok(Self::median(&filtered))
    }

    /// Checks if a stored price for the given source is still fresh.
    fn is_price_fresh(env: &Env, source: &Address, now: u64, max_staleness: u64) -> bool {
        if max_staleness == 0 {
            return true;
        }
        if let Some(entry) = Self::get_stored_price(env, source) {
            now.saturating_sub(entry.timestamp) <= max_staleness
        } else {
            // No stored price yet, so the live price is fresh
            true
        }
    }

    /// Retrieves a stored price entry for a given source.
    fn get_stored_price(env: &Env, source: &Address) -> Option<PriceEntry> {
        env.storage()
            .instance()
            .get(&DataKey::SourcePrice(source.clone()))
    }

    fn sort_prices(mut prices: Vec<i128>) -> Vec<i128> {
        let n = prices.len();
        for i in 0..n {
            for j in 0..n - i - 1 {
                let current = prices.get(j).unwrap();
                let next = prices.get(j + 1).unwrap();
                if current > next {
                    prices.set(j, next);
                    prices.set(j + 1, current);
                }
            }
        }
        prices
    }

    fn median(prices: &Vec<i128>) -> i128 {
        let len = prices.len();
        let mid = len / 2;
        if len % 2 == 1 {
            prices.get(mid).unwrap()
        } else {
            let low = prices.get(mid - 1).unwrap();
            let high = prices.get(mid).unwrap();
            (low + high) / 2
        }
    }

    fn filter_outliers(env: &Env, prices: &Vec<i128>, median: i128) -> Vec<i128> {
        let mut filtered = Vec::new(env);
        let threshold = median.saturating_mul(5);

        for idx in 0..prices.len() {
            let price = prices.get(idx).unwrap();
            if Self::abs_diff(price, median).saturating_mul(100) <= threshold {
                filtered.push_back(price);
            }
        }

        filtered
    }

    fn abs_diff(left: i128, right: i128) -> i128 {
        if left > right {
            left - right
        } else {
            right - left
        }
    }
}

#[cfg(test)]
mod test;
