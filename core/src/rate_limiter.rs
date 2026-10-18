//! Rate limiter — Issue #597
//!
//! Implements a token-bucket rate limiter per client identity with:
//! - Tiered policies (Free / Pro / Enterprise)
//! - Redis-backed sliding window for multi-instance deployments
//! - Per-endpoint limit overrides
//! - Standard `X-RateLimit-*` response headers
//! - Admin API for dynamic rule updates

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// ── Header name constants ────────────────────────────────────────────────────

pub const HEADER_RATELIMIT_LIMIT: &str = "X-RateLimit-Limit";
pub const HEADER_RATELIMIT_REMAINING: &str = "X-RateLimit-Remaining";
pub const HEADER_RATELIMIT_RESET: &str = "X-RateLimit-Reset";
pub const HEADER_RATELIMIT_RETRY_AFTER: &str = "Retry-After";

// ── Tier definitions ─────────────────────────────────────────────────────────

/// Service tier that determines which default policy applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitTier {
    Free,
    Pro,
    Enterprise,
}

impl Default for RateLimitTier {
    fn default() -> Self {
        Self::Free
    }
}

// ── Policy ───────────────────────────────────────────────────────────────────

/// Rate-limit policy for a client tier or a specific endpoint override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitPolicy {
    /// Maximum requests allowed per window.
    pub requests_per_window: u64,
    /// Window length in seconds.
    pub window_secs: u64,
    /// Additional burst tokens above `requests_per_window`.
    pub burst_allowance: u64,
}

impl RateLimitPolicy {
    pub fn new(requests_per_window: u64, window_secs: u64, burst_allowance: u64) -> Self {
        Self {
            requests_per_window,
            window_secs,
            burst_allowance,
        }
    }

    /// Total capacity including burst.
    pub fn capacity(&self) -> u64 {
        self.requests_per_window + self.burst_allowance
    }
}

impl Default for RateLimitPolicy {
    fn default() -> Self {
        Self::new(60, 60, 0)
    }
}

/// Built-in default policies per tier.
pub fn default_policy(tier: RateLimitTier) -> RateLimitPolicy {
    match tier {
        RateLimitTier::Free => RateLimitPolicy::new(60, 60, 10),
        RateLimitTier::Pro => RateLimitPolicy::new(1_000, 60, 100),
        RateLimitTier::Enterprise => RateLimitPolicy::new(10_000, 60, 1_000),
    }
}

// ── Client key ───────────────────────────────────────────────────────────────

/// The identity of the client making a request.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum RateLimitKey {
    ApiKey(String),
    IpAddress(String),
    UserId(String),
}

impl RateLimitKey {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ApiKey(s) | Self::IpAddress(s) | Self::UserId(s) => s.as_str(),
        }
    }

    pub fn cache_key(&self) -> String {
        match self {
            Self::ApiKey(k) => format!("rl:api:{}", k),
            Self::IpAddress(ip) => format!("rl:ip:{}", ip),
            Self::UserId(uid) => format!("rl:user:{}", uid),
        }
    }
}

// ── Result ───────────────────────────────────────────────────────────────────

/// Outcome of a rate-limit check.
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    /// Total capacity for this client/policy.
    pub limit: u64,
    /// Tokens remaining after this request.
    pub remaining: u64,
    /// Unix timestamp (seconds) when the window resets.
    pub reset_at: u64,
}

impl RateLimitResult {
    /// Convert to HTTP response headers as `(name, value)` pairs.
    pub fn to_headers(&self) -> Vec<(String, String)> {
        vec![
            (HEADER_RATELIMIT_LIMIT.to_string(), self.limit.to_string()),
            (
                HEADER_RATELIMIT_REMAINING.to_string(),
                self.remaining.to_string(),
            ),
            (HEADER_RATELIMIT_RESET.to_string(), self.reset_at.to_string()),
        ]
    }
}

// ── Trait ────────────────────────────────────────────────────────────────────

/// Core rate-limiter interface.
pub trait RateLimiter: Send + Sync {
    /// Check (and consume) one token for `key` using `policy`.
    /// Returns a [`RateLimitResult`] indicating whether the request is allowed.
    fn check(&self, key: &RateLimitKey, policy: &RateLimitPolicy) -> RateLimitResult;

    /// Dynamically replace the policy for a given tier.
    fn update_rule(&self, tier: RateLimitTier, policy: RateLimitPolicy);
}

// ── In-memory token-bucket implementation ───────────────────────────────────

struct BucketState {
    tokens: f64,
    last_refill: Instant,
    window_start: u64, // Unix epoch seconds
}

/// In-process token-bucket rate limiter (single-instance safe).
///
/// For multi-instance deployments, replace with [`RedisRateLimiter`].
pub struct TokenBucketRateLimiter {
    /// Per-key bucket state.
    buckets: Arc<RwLock<HashMap<String, BucketState>>>,
    /// Per-tier policy overrides (admin-writable).
    tier_policies: Arc<RwLock<HashMap<RateLimitTier, RateLimitPolicy>>>,
}

impl TokenBucketRateLimiter {
    pub fn new() -> Self {
        let mut policies = HashMap::new();
        for tier in [
            RateLimitTier::Free,
            RateLimitTier::Pro,
            RateLimitTier::Enterprise,
        ] {
            policies.insert(tier, default_policy(tier));
        }
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            tier_policies: Arc::new(RwLock::new(policies)),
        }
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

impl Default for TokenBucketRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter for TokenBucketRateLimiter {
    fn check(&self, key: &RateLimitKey, policy: &RateLimitPolicy) -> RateLimitResult {
        // Use blocking read on the async RwLock via try_read; fall back to a
        // new bucket when lock is contended (simple non-blocking path).
        let cache_key = key.cache_key();
        let capacity = policy.capacity() as f64;
        let refill_rate = policy.requests_per_window as f64 / policy.window_secs as f64;

        let mut buckets = self.buckets.blocking_write();
        let now_secs = Self::now_secs();

        let bucket = buckets.entry(cache_key).or_insert_with(|| BucketState {
            tokens: capacity,
            last_refill: Instant::now(),
            window_start: now_secs,
        });

        // Refill tokens based on elapsed time.
        let elapsed = bucket.last_refill.elapsed().as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * refill_rate).min(capacity);
        bucket.last_refill = Instant::now();

        let reset_at = bucket.window_start + policy.window_secs;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            let remaining = bucket.tokens.floor() as u64;
            RateLimitResult {
                allowed: true,
                limit: policy.capacity(),
                remaining,
                reset_at,
            }
        } else {
            RateLimitResult {
                allowed: false,
                limit: policy.capacity(),
                remaining: 0,
                reset_at,
            }
        }
    }

    fn update_rule(&self, tier: RateLimitTier, policy: RateLimitPolicy) {
        let mut policies = self.tier_policies.blocking_write();
        policies.insert(tier, policy);
    }
}

// ── Per-endpoint policy registry ─────────────────────────────────────────────

/// Registry mapping `(endpoint_pattern, tier)` to an override policy.
/// Falls back to the tier's default policy when no override is present.
#[derive(Default)]
pub struct EndpointPolicyRegistry {
    /// key: (endpoint_pattern, tier)
    overrides: RwLock<HashMap<(String, RateLimitTier), RateLimitPolicy>>,
    tier_defaults: RwLock<HashMap<RateLimitTier, RateLimitPolicy>>,
}

impl EndpointPolicyRegistry {
    pub fn new() -> Self {
        let mut defaults = HashMap::new();
        for tier in [
            RateLimitTier::Free,
            RateLimitTier::Pro,
            RateLimitTier::Enterprise,
        ] {
            defaults.insert(tier, default_policy(tier));
        }
        Self {
            overrides: RwLock::new(HashMap::new()),
            tier_defaults: RwLock::new(defaults),
        }
    }

    /// Resolve the effective policy for a request.
    pub fn resolve(&self, endpoint: &str, tier: RateLimitTier) -> RateLimitPolicy {
        let overrides = self.overrides.blocking_read();
        if let Some(p) = overrides.get(&(endpoint.to_string(), tier)) {
            return p.clone();
        }
        let defaults = self.tier_defaults.blocking_read();
        defaults
            .get(&tier)
            .cloned()
            .unwrap_or_else(|| default_policy(tier))
    }

    /// Admin: set a per-endpoint override for a tier.
    pub fn set_endpoint_policy(
        &self,
        endpoint: impl Into<String>,
        tier: RateLimitTier,
        policy: RateLimitPolicy,
    ) {
        let mut overrides = self.overrides.blocking_write();
        overrides.insert((endpoint.into(), tier), policy);
    }

    /// Admin: update the tier-wide default.
    pub fn set_tier_policy(&self, tier: RateLimitTier, policy: RateLimitPolicy) {
        let mut defaults = self.tier_defaults.blocking_write();
        defaults.insert(tier, policy);
    }
}

// ── Admin API DTOs ────────────────────────────────────────────────────────────

/// Request body for `POST /admin/rate-limits/rules`.
#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateRuleRequest {
    pub tier: RateLimitTier,
    /// Optional endpoint pattern (e.g. `/v1/analyze`).
    /// When `None`, the update applies to the tier-wide default.
    pub endpoint: Option<String>,
    pub policy: RateLimitPolicy,
}

/// Response body for rule-update operations.
#[derive(Debug, Serialize)]
pub struct UpdateRuleResponse {
    pub success: bool,
    pub message: String,
}

/// Axum handler for `POST /admin/rate-limits/rules`.
///
/// # Example
///
/// ```ignore
/// Router::new()
///     .route("/admin/rate-limits/rules", post(admin_update_rate_limit_rule))
///     .with_state(Arc::new(registry))
/// ```
pub async fn admin_update_rate_limit_rule(
    axum::extract::State(registry): axum::extract::State<Arc<EndpointPolicyRegistry>>,
    axum::Json(body): axum::Json<UpdateRuleRequest>,
) -> axum::Json<UpdateRuleResponse> {
    match body.endpoint {
        Some(endpoint) => {
            registry.set_endpoint_policy(&endpoint, body.tier, body.policy);
            axum::Json(UpdateRuleResponse {
                success: true,
                message: format!(
                    "Updated endpoint policy for {:?} on {}",
                    body.tier, endpoint
                ),
            })
        }
        None => {
            registry.set_tier_policy(body.tier, body.policy);
            axum::Json(UpdateRuleResponse {
                success: true,
                message: format!("Updated tier-default policy for {:?}", body.tier),
            })
        }
    }
}

// ── Redis-backed sliding-window rate limiter ─────────────────────────────────
//
// Uses a sorted-set per key where each member is a request timestamp.
// Members older than the window are pruned on every check (sliding window).

/// Redis-backed rate limiter for multi-instance deployments.
///
/// Requires a `redis::aio::ConnectionManager` (async multiplexed connection).
pub struct RedisRateLimiter {
    conn: redis::aio::ConnectionManager,
}

impl RedisRateLimiter {
    pub fn new(conn: redis::aio::ConnectionManager) -> Self {
        Self { conn }
    }

    /// Async check — preferred over the blocking `RateLimiter` trait method.
    pub async fn check_async(
        &self,
        key: &RateLimitKey,
        policy: &RateLimitPolicy,
    ) -> RateLimitResult {
        use redis::AsyncCommands;

        let cache_key = key.cache_key();
        let now_micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as i64;
        let window_micros = (policy.window_secs as i64) * 1_000_000;
        let cutoff = now_micros - window_micros;
        let capacity = policy.capacity();
        let reset_at = (now_micros / 1_000_000) as u64 + policy.window_secs;

        let mut conn = self.conn.clone();

        // Remove expired entries.
        let _: () = conn
            .zrembyscore(&cache_key, "-inf", cutoff)
            .await
            .unwrap_or(());

        // Count remaining entries in window.
        let count: u64 = conn.zcard(&cache_key).await.unwrap_or(capacity);

        if count < capacity {
            // Add this request.
            let member = format!("{}-{}", now_micros, uuid::Uuid::new_v4());
            let _: () = conn
                .zadd(&cache_key, &member, now_micros)
                .await
                .unwrap_or(());
            // Set TTL so stale keys are cleaned up automatically.
            let _: () = conn
                .expire(&cache_key, policy.window_secs as i64 * 2)
                .await
                .unwrap_or(());

            RateLimitResult {
                allowed: true,
                limit: capacity,
                remaining: capacity.saturating_sub(count + 1),
                reset_at,
            }
        } else {
            RateLimitResult {
                allowed: false,
                limit: capacity,
                remaining: 0,
                reset_at,
            }
        }
    }
}

// ── Blanket blocking impl for RedisRateLimiter (wraps async in block_in_place)

impl RateLimiter for RedisRateLimiter {
    fn check(&self, key: &RateLimitKey, policy: &RateLimitPolicy) -> RateLimitResult {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.check_async(key, policy))
        })
    }

    fn update_rule(&self, _tier: RateLimitTier, _policy: RateLimitPolicy) {
        // Redis implementation: policies are stored externally (e.g. config
        // service or database); this method is a no-op for the Redis backend.
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn free_policy() -> RateLimitPolicy {
        RateLimitPolicy::new(3, 60, 1) // capacity = 4
    }

    #[test]
    fn test_token_bucket_allows_within_limit() {
        let limiter = TokenBucketRateLimiter::new();
        let key = RateLimitKey::ApiKey("test-key".to_string());
        let policy = free_policy();

        let r1 = limiter.check(&key, &policy);
        assert!(r1.allowed);
        assert_eq!(r1.limit, 4);

        let r2 = limiter.check(&key, &policy);
        assert!(r2.allowed);
        assert!(r2.remaining < r1.remaining);
    }

    #[test]
    fn test_token_bucket_exhausts_capacity() {
        let limiter = TokenBucketRateLimiter::new();
        let key = RateLimitKey::IpAddress("127.0.0.1".to_string());
        let policy = RateLimitPolicy::new(2, 60, 0); // capacity = 2

        assert!(limiter.check(&key, &policy).allowed);
        assert!(limiter.check(&key, &policy).allowed);
        assert!(!limiter.check(&key, &policy).allowed);
    }

    #[test]
    fn test_different_keys_independent() {
        let limiter = TokenBucketRateLimiter::new();
        let policy = RateLimitPolicy::new(1, 60, 0);

        let k1 = RateLimitKey::UserId("user-1".to_string());
        let k2 = RateLimitKey::UserId("user-2".to_string());

        assert!(limiter.check(&k1, &policy).allowed);
        assert!(!limiter.check(&k1, &policy).allowed);
        // k2 is unaffected by k1
        assert!(limiter.check(&k2, &policy).allowed);
    }

    #[test]
    fn test_update_rule() {
        let limiter = TokenBucketRateLimiter::new();
        let new_policy = RateLimitPolicy::new(500, 60, 50);
        limiter.update_rule(RateLimitTier::Free, new_policy.clone());

        let policies = limiter.tier_policies.blocking_read();
        let stored = policies.get(&RateLimitTier::Free).unwrap();
        assert_eq!(stored.requests_per_window, 500);
    }

    #[test]
    fn test_result_to_headers() {
        let result = RateLimitResult {
            allowed: true,
            limit: 100,
            remaining: 42,
            reset_at: 9999,
        };
        let headers = result.to_headers();
        assert_eq!(headers.len(), 3);
        assert!(headers.iter().any(|(k, v)| k == HEADER_RATELIMIT_LIMIT && v == "100"));
        assert!(headers.iter().any(|(k, v)| k == HEADER_RATELIMIT_REMAINING && v == "42"));
        assert!(headers.iter().any(|(k, v)| k == HEADER_RATELIMIT_RESET && v == "9999"));
    }

    #[test]
    fn test_endpoint_registry_fallback() {
        let registry = EndpointPolicyRegistry::new();
        let policy = registry.resolve("/v1/simulate", RateLimitTier::Free);
        assert_eq!(policy.requests_per_window, 60);
    }

    #[test]
    fn test_endpoint_registry_override() {
        let registry = EndpointPolicyRegistry::new();
        let custom = RateLimitPolicy::new(5, 10, 2);
        registry.set_endpoint_policy("/v1/admin", RateLimitTier::Free, custom);

        let resolved = registry.resolve("/v1/admin", RateLimitTier::Free);
        assert_eq!(resolved.requests_per_window, 5);
        // Other endpoints still fall back to default
        let fallback = registry.resolve("/v1/simulate", RateLimitTier::Free);
        assert_eq!(fallback.requests_per_window, 60);
    }

    #[test]
    fn test_cache_key_prefixes() {
        assert!(RateLimitKey::ApiKey("x".into()).cache_key().starts_with("rl:api:"));
        assert!(RateLimitKey::IpAddress("x".into()).cache_key().starts_with("rl:ip:"));
        assert!(RateLimitKey::UserId("x".into()).cache_key().starts_with("rl:user:"));
    }

    #[test]
    fn test_default_tier_policies() {
        let free = default_policy(RateLimitTier::Free);
        let pro = default_policy(RateLimitTier::Pro);
        let ent = default_policy(RateLimitTier::Enterprise);

        assert!(pro.requests_per_window > free.requests_per_window);
        assert!(ent.requests_per_window > pro.requests_per_window);
        assert!(ent.burst_allowance > pro.burst_allowance);
    }
}
