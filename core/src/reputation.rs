#![allow(dead_code)]

//! Agent reputation scoring with exponential time-decay.
//!
//! ## Decay Formula
//!
//! The reputation score decays exponentially over time according to:
//!
//! ```text
//! S(t) = S₀ × e^(-λ × Δt)
//! ```
//!
//! Where:
//! - `S(t)` = score at time t
//! - `S₀` = base score (set when last updated)
//! - `λ` = decay rate (higher = faster decay)
//! - `Δt` = elapsed time in days since last update
//!
//! The score is floored at 0.0 and cannot go negative.
//!
//! ## Score Bounds
//!
//! - **Minimum:** 0.0 (fully decayed / penalized)
//! - **Maximum:** No cap (add_score accumulates freely)
//! - **Clamping:** Both `apply_decay` and `add_score` ensure score ≥ 0.0

use chrono::{DateTime, Duration, Utc};

pub struct ReputationRecord {
    pub agent_id: String,
    pub score: f64,
    pub last_updated: DateTime<Utc>,
    pub decay_rate: f64,
}

impl ReputationRecord {
    pub fn new(agent_id: String, initial_score: f64, decay_rate: f64) -> Self {
        Self {
            agent_id,
            score: initial_score,
            last_updated: Utc::now(),
            decay_rate,
        }
    }

    pub fn current_score(&self, now: DateTime<Utc>) -> f64 {
        let elapsed_days = (now - self.last_updated).num_milliseconds() as f64
            / (Duration::days(1).num_milliseconds() as f64);
        let decay_factor = (-self.decay_rate * elapsed_days).exp();
        (self.score * decay_factor).max(0.0)
    }

    pub fn apply_decay(&mut self, now: DateTime<Utc>) {
        self.score = self.current_score(now);
        self.last_updated = now;
    }

    pub fn add_score(&mut self, delta: f64, now: DateTime<Utc>) {
        self.apply_decay(now);
        self.score = (self.score + delta).max(0.0);
        self.last_updated = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_decay_over_time() {
        let mut record = ReputationRecord::new("agent1".to_string(), 100.0, 0.1);
        let later = Utc::now() + Duration::days(10);
        let score = record.current_score(later);
        assert!(score < 100.0);
        assert!(score > 0.0);
    }

    #[test]
    fn test_add_score_applies_decay_first() {
        let mut record = ReputationRecord::new("agent1".to_string(), 100.0, 0.1);
        let later = Utc::now() + Duration::days(5);
        record.add_score(50.0, later);
        assert!(record.score < 150.0);
        assert!(record.score > 50.0);
    }

    #[test]
    fn test_no_negative_score() {
        let mut record = ReputationRecord::new("agent1".to_string(), 10.0, 1.0);
        let far_future = Utc::now() + Duration::days(365);
        record.apply_decay(far_future);
        assert!(record.score >= 0.0);
    }

    #[test]
    fn test_initial_score_unchanged() {
        let record = ReputationRecord::new("agent1".to_string(), 100.0, 0.1);
        let score = record.current_score(record.last_updated);
        assert!((score - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_score_floor_at_zero() {
        let mut record = ReputationRecord::new("agent1".to_string(), 10.0, 10.0);
        let far_future = Utc::now() + Duration::days(365);
        record.apply_decay(far_future);
        assert_eq!(record.score, 0.0);
    }

    #[test]
    fn test_decay_rate_zero_no_decay() {
        let record = ReputationRecord::new("agent1".to_string(), 100.0, 0.0);
        let later = Utc::now() + Duration::days(365);
        let score = record.current_score(later);
        assert!((score - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_add_score_can_exceed_initial() {
        let mut record = ReputationRecord::new("agent1".to_string(), 100.0, 0.0);
        record.add_score(50.0, Utc::now());
        assert!((record.score - 150.0).abs() < 0.001);
    }

    #[test]
    fn test_boundary_max_inactivity() {
        let mut record = ReputationRecord::new("agent1".to_string(), 100.0, 0.1);
        // `Duration::days(i64::MAX / 2)` is outside chrono's representable
        // range, so it panicked inside `TimeDelta::days` before this test
        // reached the decay code at all. A thousand years is still far past
        // the point where `exp(-0.1 * days)` underflows to exactly 0.0, which
        // is the boundary this test is about.
        let far_future = Utc::now() + Duration::days(365_000);
        let score = record.current_score(far_future);
        assert_eq!(score, 0.0);
    }
}
