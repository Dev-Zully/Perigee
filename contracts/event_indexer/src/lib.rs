#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Env, String, Symbol, Val, Vec,
};

/// Default number of events retained per topic before the oldest are evicted.
pub const DEFAULT_MAX_EVENTS_PER_TOPIC: u32 = 256;

/// TTL extension applied to the stored log entries (mirrors the token
/// contract's `extend_ttl(100, 100)` convention).
const LOG_TTL_THRESHOLD: u32 = 100;
const LOG_TTL_EXTEND_TO: u32 = 100;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    LimitTooSmall = 1,
    InvalidQueryLimit = 2,
}

/// Storage keys used by the event indexer contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// All indexed events for a topic, ordered oldest-first.
    Log(Symbol),
    /// Per-topic maximum retained event count.
    TopicLimit(Symbol),
}

/// A single indexed event.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedEvent {
    /// Monotonic sequence number within the topic (starts at 1).
    pub seq: u32,
    /// Ledger timestamp when the event was recorded.
    pub timestamp: u64,
    /// Event name (e.g. `pause_set`, `Payment Confirmed`).
    pub event_name: Symbol,
    /// Correlation key for the event (e.g. contract id, claim id).
    pub key: String,
    /// Event payload as `(field, value)` pairs.
    pub payload: Vec<(Symbol, Val)>,
}

fn read_log(env: &Env, topic: &Symbol) -> Vec<IndexedEvent> {
    env.storage()
        .persistent()
        .get(&DataKey::Log(topic.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

fn write_log(env: &Env, topic: &Symbol, log: &Vec<IndexedEvent>) {
    let key = DataKey::Log(topic.clone());
    env.storage().persistent().set(&key, log);
    env.storage()
        .persistent()
        .extend_ttl(&key, LOG_TTL_THRESHOLD, LOG_TTL_EXTEND_TO);
}

#[contract]
pub struct EventIndexer;

#[contractimpl]
impl EventIndexer {
    /// Records an indexed event under `topic` and publishes it as a contract
    /// event.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `topic`: Index bucket (e.g. `payment`, `emergency_guard`).
    /// - `event_name`: Name of the recorded event.
    /// - `key`: Correlation key for the event.
    /// - `payload`: `(field, value)` pairs describing the event.
    ///
    /// # Returns
    /// - The sequence number assigned to the event within its topic.
    pub fn record(
        env: Env,
        topic: Symbol,
        event_name: Symbol,
        key: String,
        payload: Vec<(Symbol, Val)>,
    ) -> u32 {
        let mut log = read_log(&env, &topic);
        let seq = match log.iter().last() {
            Some(last) => last.seq + 1,
            None => 1,
        };

        log.push_back(IndexedEvent {
            seq,
            timestamp: env.ledger().timestamp(),
            event_name: event_name.clone(),
            key: key.clone(),
            payload,
        });

        let limit = Self::get_topic_limit(env.clone(), topic.clone());
        while log.len() > limit {
            let mut retained = Vec::new(&env);
            for i in 1..log.len() {
                if let Some(event) = log.get(i) {
                    retained.push_back(event);
                }
            }
            log = retained;
        }

        write_log(&env, &topic, &log);
        env.events().publish((topic, event_name), key);
        seq
    }

    /// Returns events for `topic` with `seq >= from_seq`, at most `limit`
    /// entries, ordered oldest-first.
    pub fn query(
        env: Env,
        topic: Symbol,
        from_seq: u32,
        limit: u32,
    ) -> Result<Vec<IndexedEvent>, Error> {
        if limit == 0 {
            return Err(Error::InvalidQueryLimit);
        }

        let log = read_log(&env, &topic);
        let mut result = Vec::new(&env);
        for event in log.iter() {
            if event.seq >= from_seq {
                result.push_back(event);
                if result.len() >= limit {
                    break;
                }
            }
        }
        Ok(result)
    }

    /// Returns the number of retained events for `topic`.
    pub fn count(env: Env, topic: Symbol) -> u32 {
        read_log(&env, &topic).len()
    }

    /// Returns the most recently recorded event for `topic`, if any.
    pub fn latest(env: Env, topic: Symbol) -> Option<IndexedEvent> {
        let log = read_log(&env, &topic);
        log.iter().last()
    }

    /// Sets the maximum number of events retained for `topic`. Older events
    /// are evicted (FIFO) once the limit is exceeded.
    pub fn set_topic_limit(env: Env, topic: Symbol, limit: u32) -> Result<(), Error> {
        if limit == 0 {
            return Err(Error::LimitTooSmall);
        }
        env.storage()
            .persistent()
            .set(&DataKey::TopicLimit(topic), &limit);
        Ok(())
    }

    /// Returns the retention limit for `topic`, defaulting to
    /// `DEFAULT_MAX_EVENTS_PER_TOPIC`.
    pub fn get_topic_limit(env: Env, topic: Symbol) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::TopicLimit(topic))
            .unwrap_or(DEFAULT_MAX_EVENTS_PER_TOPIC)
    }
}

#[cfg(test)]
mod test;