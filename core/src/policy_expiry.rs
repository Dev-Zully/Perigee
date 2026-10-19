//! Vault policy expiry (BE-023).
//!
//! A vault's policy is stored as JSON in `VaultRecord.config_json`. It may
//! carry an `expires_at` timestamp, after which the policy no longer
//! authorises anything. Nothing checked it: an expired policy still permitted
//! every mutating vault operation, so a policy with a deliberate end date was
//! documentation rather than a control.
//!
//! # Where the timestamp lives
//!
//! Both of these are accepted:
//!
//! ```json
//! { "expires_at": "2026-01-01T00:00:00Z" }
//! { "policy": { "expires_at": "2026-01-01T00:00:00Z" } }
//! ```
//!
//! The nested form is checked first, since a policy block is the more specific
//! statement when both are present.
//!
//! # A policy with no `expires_at` never expires
//!
//! Existing vaults default to `config_json = "{}"`, and the field is not
//! required. Treating an absent timestamp as "expired" would lock every
//! existing vault the moment this shipped; treating it as "no expiry" keeps
//! current behaviour and makes expiry opt-in. That is a deliberate choice, not
//! an oversight — see [`PolicyExpiry::has_expiry`] for distinguishing the two
//! at a call site that cares.
//!
//! # Malformed input fails closed
//!
//! Unparseable JSON, or an `expires_at` that is not a timestamp, is rejected
//! rather than ignored. An operation authorised by a policy nobody can read is
//! worse than a refused operation.

use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyExpiryError {
    /// The policy's expiry has passed.
    #[error("policy expired at {expires_at} (now {now})")]
    PolicyExpired {
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    },

    /// `config_json` is not valid JSON.
    #[error("vault policy is not valid JSON: {0}")]
    MalformedPolicy(String),

    /// `expires_at` is present but is not an RFC 3339 timestamp.
    #[error("policy expires_at is not a valid RFC 3339 timestamp: {0}")]
    InvalidExpiry(String),
}

/// The expiry component of a vault policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyExpiry {
    expires_at: Option<DateTime<Utc>>,
}

impl PolicyExpiry {
    /// A policy that never expires.
    pub fn never() -> Self {
        Self { expires_at: None }
    }

    /// A policy expiring at `expires_at`.
    pub fn at(expires_at: DateTime<Utc>) -> Self {
        Self {
            expires_at: Some(expires_at),
        }
    }

    /// Parse the expiry out of a vault's `config_json`.
    pub fn from_config_json(config_json: &str) -> Result<Self, PolicyExpiryError> {
        let trimmed = config_json.trim();

        // An empty config is a policy with nothing in it, not a parse failure.
        if trimmed.is_empty() {
            return Ok(Self::never());
        }

        let value: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|e| PolicyExpiryError::MalformedPolicy(e.to_string()))?;

        let raw = value
            .get("policy")
            .and_then(|policy| policy.get("expires_at"))
            .or_else(|| value.get("expires_at"));

        let raw = match raw {
            None | Some(serde_json::Value::Null) => return Ok(Self::never()),
            Some(v) => v,
        };

        let text = raw.as_str().ok_or_else(|| {
            PolicyExpiryError::InvalidExpiry(format!("expected a string, got {raw}"))
        })?;

        let parsed = DateTime::parse_from_rfc3339(text)
            .map_err(|e| PolicyExpiryError::InvalidExpiry(format!("{text}: {e}")))?;

        Ok(Self::at(parsed.with_timezone(&Utc)))
    }

    /// Whether an expiry is set at all.
    pub fn has_expiry(&self) -> bool {
        self.expires_at.is_some()
    }

    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
    }

    /// Whether the policy has expired as of `now`.
    ///
    /// Expiry is inclusive of the boundary: a policy expiring at `T` is still
    /// valid *at* `T` and expired after it. Picking the other convention would
    /// make a policy expire a moment before its stated time.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            Some(expires_at) => now > expires_at,
            None => false,
        }
    }

    /// Fail if the policy has expired.
    pub fn ensure_active(&self, now: DateTime<Utc>) -> Result<(), PolicyExpiryError> {
        match self.expires_at {
            Some(expires_at) if now > expires_at => {
                Err(PolicyExpiryError::PolicyExpired { expires_at, now })
            }
            _ => Ok(()),
        }
    }
}

/// Parse `config_json` and fail if the policy it describes has expired.
///
/// This is the single call every mutating vault operation makes.
pub fn ensure_policy_active(
    config_json: &str,
    now: DateTime<Utc>,
) -> Result<(), PolicyExpiryError> {
    PolicyExpiry::from_config_json(config_json)?.ensure_active(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .expect("test timestamp")
            .with_timezone(&Utc)
    }

    const EXPIRY: &str = "2026-01-01T00:00:00Z";

    #[test]
    fn an_empty_config_never_expires() {
        let policy = PolicyExpiry::from_config_json("{}").unwrap();

        assert!(!policy.has_expiry());
        assert!(!policy.is_expired(ts("2099-01-01T00:00:00Z")));
        assert_eq!(policy.ensure_active(ts("2099-01-01T00:00:00Z")), Ok(()));
    }

    #[test]
    fn a_blank_config_string_never_expires() {
        assert_eq!(
            PolicyExpiry::from_config_json("   ").unwrap(),
            PolicyExpiry::never()
        );
    }

    #[test]
    fn a_top_level_expires_at_is_read() {
        let policy =
            PolicyExpiry::from_config_json(&format!(r#"{{"expires_at":"{EXPIRY}"}}"#)).unwrap();

        assert_eq!(policy.expires_at(), Some(ts(EXPIRY)));
    }

    #[test]
    fn a_nested_policy_expires_at_is_read() {
        let policy =
            PolicyExpiry::from_config_json(&format!(r#"{{"policy":{{"expires_at":"{EXPIRY}"}}}}"#))
                .unwrap();

        assert_eq!(policy.expires_at(), Some(ts(EXPIRY)));
    }

    #[test]
    fn the_nested_form_wins_when_both_are_present() {
        let json = format!(
            r#"{{"expires_at":"2030-01-01T00:00:00Z","policy":{{"expires_at":"{EXPIRY}"}}}}"#
        );

        assert_eq!(
            PolicyExpiry::from_config_json(&json).unwrap().expires_at(),
            Some(ts(EXPIRY))
        );
    }

    #[test]
    fn an_explicit_null_expiry_never_expires() {
        let policy = PolicyExpiry::from_config_json(r#"{"expires_at":null}"#).unwrap();

        assert!(!policy.has_expiry());
    }

    #[test]
    fn a_policy_before_its_expiry_is_active() {
        let policy = PolicyExpiry::at(ts(EXPIRY));

        assert!(!policy.is_expired(ts("2025-12-31T23:59:59Z")));
        assert_eq!(policy.ensure_active(ts("2025-12-31T23:59:59Z")), Ok(()));
    }

    /// The boundary is inclusive — a policy is still valid at its stated
    /// expiry instant, and expires the moment after.
    #[test]
    fn expiry_is_inclusive_of_the_boundary_instant() {
        let policy = PolicyExpiry::at(ts(EXPIRY));

        assert!(!policy.is_expired(ts(EXPIRY)));
        assert_eq!(policy.ensure_active(ts(EXPIRY)), Ok(()));

        assert!(policy.is_expired(ts("2026-01-01T00:00:01Z")));
        assert!(policy.ensure_active(ts("2026-01-01T00:00:01Z")).is_err());
    }

    #[test]
    fn an_expired_policy_reports_both_timestamps() {
        let policy = PolicyExpiry::at(ts(EXPIRY));
        let now = ts("2026-06-01T00:00:00Z");

        assert_eq!(
            policy.ensure_active(now),
            Err(PolicyExpiryError::PolicyExpired {
                expires_at: ts(EXPIRY),
                now,
            })
        );
    }

    #[test]
    fn malformed_json_is_rejected_rather_than_ignored() {
        let err = PolicyExpiry::from_config_json("{not json").unwrap_err();

        assert!(matches!(err, PolicyExpiryError::MalformedPolicy(_)));
    }

    #[test]
    fn a_non_timestamp_expiry_is_rejected() {
        let err = PolicyExpiry::from_config_json(r#"{"expires_at":"whenever"}"#).unwrap_err();

        assert!(matches!(err, PolicyExpiryError::InvalidExpiry(_)));
    }

    #[test]
    fn a_non_string_expiry_is_rejected() {
        let err = PolicyExpiry::from_config_json(r#"{"expires_at":12345}"#).unwrap_err();

        assert!(matches!(err, PolicyExpiryError::InvalidExpiry(_)));
    }

    #[test]
    fn a_non_utc_offset_is_normalised() {
        // 2026-01-01T05:00:00+05:00 is 2026-01-01T00:00:00Z.
        let policy =
            PolicyExpiry::from_config_json(r#"{"expires_at":"2026-01-01T05:00:00+05:00"}"#)
                .unwrap();

        assert_eq!(policy.expires_at(), Some(ts(EXPIRY)));
    }

    #[test]
    fn the_convenience_helper_matches_the_two_step_form() {
        let json = format!(r#"{{"expires_at":"{EXPIRY}"}}"#);
        let now = ts("2026-06-01T00:00:00Z");

        assert_eq!(
            ensure_policy_active(&json, now),
            PolicyExpiry::from_config_json(&json)
                .unwrap()
                .ensure_active(now)
        );
    }
}
