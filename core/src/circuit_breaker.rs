use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

pub struct CircuitBreaker {
    drop_threshold_pct: f64,
    state: BreakerState,
    triggered_at: Option<DateTime<Utc>>,
}

impl CircuitBreaker {
    pub fn new(drop_threshold_pct: f64) -> Self {
        Self {
            drop_threshold_pct,
            state: BreakerState::Closed,
            triggered_at: None,
        }
    }

    pub fn evaluate_nav_drop(&mut self, previous_nav: f64, current_nav: f64) -> bool {
        if previous_nav == 0.0 || !previous_nav.is_finite() {
            return false;
        }

        let drop_pct = (previous_nav - current_nav) / previous_nav * 100.0;

        if drop_pct >= self.drop_threshold_pct {
            self.state = BreakerState::Open;
            self.triggered_at = Some(Utc::now());
            true
        } else {
            false
        }
    }

    pub fn is_open(&self) -> bool {
        self.state == BreakerState::Open
    }

    pub fn reset(&mut self) {
        self.state = BreakerState::Closed;
        self.triggered_at = None;
    }

    pub fn attempt_half_open(&mut self) {
        if self.state == BreakerState::Open {
            self.state = BreakerState::HalfOpen;
        }
    }

    pub fn is_manual_review_required(&self) -> bool {
        self.state == BreakerState::Open && self.triggered_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_trip_when_below_threshold() {
        let mut cb = CircuitBreaker::new(10.0);
        assert!(!cb.evaluate_nav_drop(100.0, 95.0));
        assert_eq!(cb.state, BreakerState::Closed);
    }

    #[test]
    fn test_trip_when_at_threshold() {
        let mut cb = CircuitBreaker::new(10.0);
        assert!(cb.evaluate_nav_drop(100.0, 90.0));
        assert!(cb.is_open());
    }

    #[test]
    fn test_trip_when_above_threshold() {
        let mut cb = CircuitBreaker::new(5.0);
        assert!(cb.evaluate_nav_drop(100.0, 80.0));
        assert!(cb.is_open());
        assert!(cb.is_manual_review_required());
    }

    #[test]
    fn test_half_open_transition() {
        let mut cb = CircuitBreaker::new(10.0);
        cb.evaluate_nav_drop(100.0, 85.0);
        assert!(cb.is_open());
        cb.attempt_half_open();
        assert_eq!(cb.state, BreakerState::HalfOpen);
        assert!(!cb.is_open());
    }

    #[test]
    fn test_reset() {
        let mut cb = CircuitBreaker::new(10.0);
        cb.evaluate_nav_drop(100.0, 80.0);
        assert!(cb.is_open());
        cb.reset();
        assert_eq!(cb.state, BreakerState::Closed);
        assert!(!cb.is_manual_review_required());
    }

    #[test]
    fn test_invalid_previous_nav() {
        let mut cb = CircuitBreaker::new(10.0);
        assert!(!cb.evaluate_nav_drop(0.0, 0.0));
        assert!(!cb.evaluate_nav_drop(f64::NAN, 100.0));
        assert!(!cb.evaluate_nav_drop(f64::INFINITY, 100.0));
    }

    #[test]
    fn test_half_open_to_closed_after_reset() {
        let mut cb = CircuitBreaker::new(10.0);
        cb.evaluate_nav_drop(100.0, 80.0);
        assert!(cb.is_open());
        cb.attempt_half_open();
        assert_eq!(cb.state, BreakerState::HalfOpen);
        cb.reset();
        assert_eq!(cb.state, BreakerState::Closed);
        assert!(!cb.is_open());
        assert!(!cb.is_manual_review_required());
    }

    #[test]
    fn test_cooldown_reset_clears_triggered_at() {
        let mut cb = CircuitBreaker::new(5.0);
        cb.evaluate_nav_drop(100.0, 50.0);
        assert!(cb.is_open());
        assert!(cb.triggered_at.is_some());
        cb.reset();
        assert!(cb.triggered_at.is_none());
    }

    #[test]
    fn test_consecutive_trips_stay_open() {
        let mut cb = CircuitBreaker::new(10.0);
        cb.evaluate_nav_drop(100.0, 85.0);
        assert!(cb.is_open());
        cb.attempt_half_open();
        assert_eq!(cb.state, BreakerState::HalfOpen);
        cb.evaluate_nav_drop(100.0, 80.0);
        assert!(cb.is_open());
    }

    #[test]
    fn test_half_open_rejects_on_failure() {
        let mut cb = CircuitBreaker::new(10.0);
        cb.evaluate_nav_drop(100.0, 80.0);
        cb.attempt_half_open();
        assert_eq!(cb.state, BreakerState::HalfOpen);
        cb.evaluate_nav_drop(100.0, 75.0);
        assert!(cb.is_open());
    }
}
