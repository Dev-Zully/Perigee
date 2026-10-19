use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TriggerState {
    Below,
    Above,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TriggerEvent {
    Entered,
    Exited,
}

pub struct HysteresisGuard {
    enter_threshold: f64,
    exit_threshold: f64,
    staleness_threshold: Option<Duration>,
    state: TriggerState,
    last_update: Instant,
}

impl HysteresisGuard {
    pub fn new(enter_threshold: f64, exit_threshold: f64) -> Self {
        Self::with_staleness_threshold(enter_threshold, exit_threshold, None)
    }

    pub fn with_staleness_threshold(
        enter_threshold: f64,
        exit_threshold: f64,
        staleness_threshold: Option<Duration>,
    ) -> Self {
        Self {
            enter_threshold,
            exit_threshold,
            staleness_threshold,
            state: TriggerState::Below,
            last_update: Instant::now(),
        }
    }

    pub fn evaluate(&mut self, current_value: f64) -> Option<TriggerEvent> {
        self.evaluate_at(current_value, Instant::now())
    }

    pub fn evaluate_at(
        &mut self,
        current_value: f64,
        now: Instant,
    ) -> Option<TriggerEvent> {
        if self.is_stale(now) {
            self.state = TriggerState::Below;
        }
        self.last_update = now;

        match self.state {
            TriggerState::Below => {
                if current_value >= self.enter_threshold {
                    self.state = TriggerState::Above;
                    Some(TriggerEvent::Entered)
                } else {
                    None
                }
            }
            TriggerState::Above => {
                if current_value <= self.exit_threshold {
                    self.state = TriggerState::Below;
                    Some(TriggerEvent::Exited)
                } else {
                    None
                }
            }
        }
    }

    pub fn current_state(&self) -> TriggerState {
        self.state
    }

    fn is_stale(&self, now: Instant) -> bool {
        self.staleness_threshold.is_some_and(|threshold| {
            now.checked_duration_since(self.last_update)
                .is_some_and(|elapsed| elapsed >= threshold)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hysteresis_enter_exit_cycle() {
        let mut guard = HysteresisGuard::new(5.0, 3.0);
        assert_eq!(guard.current_state(), TriggerState::Below);

        assert!(guard.evaluate(4.0).is_none());
        assert_eq!(guard.current_state(), TriggerState::Below);

        assert_eq!(guard.evaluate(5.0), Some(TriggerEvent::Entered));
        assert_eq!(guard.current_state(), TriggerState::Above);

        assert!(guard.evaluate(4.0).is_none());

        assert_eq!(guard.evaluate(3.0), Some(TriggerEvent::Exited));
        assert_eq!(guard.current_state(), TriggerState::Below);
    }

    #[test]
    fn test_no_double_fire() {
        let mut guard = HysteresisGuard::new(5.0, 3.0);
        assert_eq!(guard.evaluate(5.0), Some(TriggerEvent::Entered));
        assert_eq!(guard.evaluate(6.0), None);
        assert_eq!(guard.evaluate(7.0), None);
    }

    #[test]
    fn test_stale_state_resets_after_extended_inactivity() {
        let start = Instant::now();
        let mut guard = HysteresisGuard::with_staleness_threshold(
            5.0,
            3.0,
            Some(Duration::from_secs(60)),
        );

        assert_eq!(guard.evaluate_at(5.0, start), Some(TriggerEvent::Entered));
        assert_eq!(guard.current_state(), TriggerState::Above);

        let stale_time = start + Duration::from_secs(61);
        assert_eq!(guard.evaluate_at(4.0, stale_time), None);
        assert_eq!(guard.current_state(), TriggerState::Below);
        assert_eq!(guard.evaluate_at(5.0, stale_time), Some(TriggerEvent::Entered));
    }

    #[test]
    fn test_state_does_not_reset_before_staleness_threshold() {
        let start = Instant::now();
        let mut guard = HysteresisGuard::with_staleness_threshold(
            5.0,
            3.0,
            Some(Duration::from_secs(60)),
        );

        assert_eq!(guard.evaluate_at(5.0, start), Some(TriggerEvent::Entered));
        assert_eq!(guard.evaluate_at(4.0, start + Duration::from_secs(59)), None);
        assert_eq!(guard.current_state(), TriggerState::Above);
    }
}
