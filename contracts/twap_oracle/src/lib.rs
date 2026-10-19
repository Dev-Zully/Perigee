#![no_std]
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

#[cfg(test)]
mod test;

/// Errors returned by the `TwapOracle` contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidPrice = 3,
    InsufficientTimeElapsed = 4,
    StalePrice = 5,
}

/// Storage keys used by the TWAP oracle contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    TokenA,
    TokenB,
    MinUpdateIntervalSeconds,
    MaxPriceAgeSeconds,
    /// Single instance key holding the whole TWAP accumulator state.
    ///
    /// Replaces the four separate `CumulativePrice` / `TotalTime` /
    /// `LastUpdateTimestamp` / `LastPrice` keys: `update_price` now performs
    /// one read + one write of instance storage instead of four.
    State,
}

/// The TWAP accumulator, packed into a single instance-storage entry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TwapStorage {
    pub cumulative_price: i128,
    pub total_time: u64,
    pub last_update_timestamp: u64,
    pub last_price: i128,
}

impl Default for TwapStorage {
    fn default() -> Self {
        Self {
            cumulative_price: 0,
            total_time: 0,
            last_update_timestamp: 0,
            last_price: 0,
        }
    }
}

pub trait PriceOracle {
    fn latest_price(e: Env) -> i128;
}

#[contract]
/// TWAP Price Oracle for token pairs.
pub struct TwapOracle;

#[contractimpl]
impl TwapOracle {
    /// Initializes the TWAP oracle with token pair addresses and minimum update interval.
    ///
    /// # Parameters
    /// - `e`: Soroban environment.
    /// - `token_a`: Contract address of token A.
    /// - `token_b`: Contract address of token B.
    /// - `min_update_interval_seconds`: Minimum time in seconds between updates.
    ///
    /// # Returns
    /// - `Ok(())` when initialization succeeds.
    /// - `Err(Error::AlreadyInitialized)` if already initialized.
    pub fn initialize(
        e: Env,
        token_a: Address,
        token_b: Address,
        min_update_interval_seconds: u64,
    ) -> Result<(), Error> {
        if e.storage().instance().has(&DataKey::TokenA) {
            return Err(Error::AlreadyInitialized);
        }
        e.storage().instance().set(&DataKey::TokenA, &token_a);
        e.storage().instance().set(&DataKey::TokenB, &token_b);
        e.storage().instance().set(&DataKey::MinUpdateIntervalSeconds, &min_update_interval_seconds);
        // Staleness checks are disabled by default (0 = unlimited age).
        e.storage().instance().set(&DataKey::MaxPriceAgeSeconds, &0u64);
        // One write for the whole accumulator state.
        e.storage().instance().set(&DataKey::State, &TwapStorage::default());
        Ok(())
    }

    /// Updates the TWAP accumulator with a new price.
    /// Only allows updates after the minimum interval has elapsed.
    ///
    /// # Parameters
    /// - `e`: Soroban environment.
    /// - `current_price`: The current price of token_b in terms of token_a (scaled appropriately).
    ///
    /// # Returns
    /// - `Ok(())` on success.
    /// - `Err(Error::NotInitialized)` if not initialized.
    /// - `Err(Error::InvalidPrice)` if price <= 0.
    /// - `Err(Error::InsufficientTimeElapsed)` if not enough time has passed since last update.
    pub fn update_price(e: Env, current_price: i128) -> Result<(), Error> {
        if !e.storage().instance().has(&DataKey::TokenA) {
            return Err(Error::NotInitialized);
        }
        if current_price <= 0 {
            return Err(Error::InvalidPrice);
        }

        let now = e.ledger().timestamp();
        let mut state = Self::read_state(&e);
        let min_interval: u64 = e
            .storage()
            .instance()
            .get(&DataKey::MinUpdateIntervalSeconds)
            .unwrap_or(0);

        if state.last_update_timestamp > 0
            && now - state.last_update_timestamp < min_interval
        {
            return Err(Error::InsufficientTimeElapsed);
        }

        let elapsed = if state.last_update_timestamp == 0 {
            0
        } else {
            now - state.last_update_timestamp
        };
        state.cumulative_price += state.last_price * elapsed as i128;
        state.total_time += elapsed;
        state.last_update_timestamp = now;
        state.last_price = current_price;

        // One write for the whole accumulator state.
        Self::write_state(&e, &state);

        Ok(())
    }

    /// Returns the Time-Weighted Average Price since initialization.
    ///
    /// # Parameters
    /// - `e`: Soroban environment.
    ///
    /// # Returns
    /// - The TWAP as i128, or 0 if no updates.
    pub fn get_twap(e: Env) -> i128 {
        let state = Self::read_state(&e);
        if state.total_time == 0 {
            0
        } else {
            state.cumulative_price / state.total_time as i128
        }
    }

    /// Sets the maximum allowed age (in seconds) of the last price update before
    /// the feed is considered stale. A value of 0 disables the staleness check.
    ///
    /// # Returns
    /// - `Err(Error::NotInitialized)` if not initialized.
    pub fn set_max_price_age(e: Env, max_age_seconds: u64) -> Result<(), Error> {
        if !e.storage().instance().has(&DataKey::TokenA) {
            return Err(Error::NotInitialized);
        }
        e.storage().instance().set(&DataKey::MaxPriceAgeSeconds, &max_age_seconds);
        Ok(())
    }

    /// Returns the configured maximum price age in seconds (0 = disabled).
    pub fn get_max_price_age(e: Env) -> u64 {
        e.storage().instance().get(&DataKey::MaxPriceAgeSeconds).unwrap_or(0)
    }

    /// Returns `true` when the last recorded price is older than the configured
    /// maximum age, or when no price has been recorded yet, so consumers never
    /// trust an empty feed.
    ///
    /// Returns `false` when staleness checks are disabled (`max_age_seconds == 0`).
    pub fn is_price_stale(e: Env) -> bool {
        let max_age: u64 = e.storage().instance().get(&DataKey::MaxPriceAgeSeconds).unwrap_or(0);
        if max_age == 0 {
            return false;
        }
        let last_update: u64 = e.storage().instance().get(&DataKey::LastUpdateTimestamp).unwrap_or(0);
        if last_update == 0 {
            // No price update has ever been recorded.
            return true;
        }
        let elapsed = e.ledger().timestamp().saturating_sub(last_update);
        elapsed > max_age
    }

    /// Returns the token pair addresses.
    pub fn get_tokens(e: Env) -> (Address, Address) {
        let token_a: Address = e.storage().instance().get(&DataKey::TokenA).unwrap();
        let token_b: Address = e.storage().instance().get(&DataKey::TokenB).unwrap();
        (token_a, token_b)
    }

    /// Returns the packed TWAP accumulator state.
    pub fn get_state(e: Env) -> TwapStorage {
        Self::read_state(&e)
    }

    /// Reads the packed accumulator state, defaulting to all zeros when the
    /// contract is not initialized or no update has happened yet.
    fn read_state(e: &Env) -> TwapStorage {
        e.storage()
            .instance()
            .get(&DataKey::State)
            .unwrap_or_default()
    }

    /// Writes the packed accumulator state in a single instance-storage write.
    fn write_state(e: &Env, state: &TwapStorage) {
        e.storage().instance().set(&DataKey::State, state);
    }
}

#[contractimpl]
impl PriceOracle for TwapOracle {
    /// Returns the latest TWAP price.
    ///
    /// Returns `0` when the feed is stale so downstream aggregators (which
    /// treat `0` as "no reliable quote") reject it instead of consuming an
    /// out-of-date value. The `i128` ABI is preserved.
    fn latest_price(e: Env) -> i128 {
        if Self::is_price_stale(e.clone()) {
            return 0;
        }
        Self::get_twap(e)
    }
}