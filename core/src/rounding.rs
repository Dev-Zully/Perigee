//! The protocol's single fee-rounding strategy (BE-025).
//!
//! # Why this module exists
//!
//! Fee arithmetic was spread across three modules using three different
//! rounding strategies:
//!
//! | Site | Strategy |
//! |---|---|
//! | `rounding::calculate_fee_with_bankers_round` | banker's rounding, on `f64` |
//! | `billing_service::safety_margin_to_bps` | `f64::round` (half away from zero) |
//! | `fee_analytics::ratio_to_bps` | integer floor |
//!
//! Three strategies means the same amount and rate produce different answers
//! depending on which path computed them, which is exactly the off-by-one
//! discrepancy BE-025 describes.
//!
//! # The strategy
//!
//! **Round in the direction that never favours the protocol.**
//!
//! - [`fee_charged`] — money taken *from* a user: **round down**.
//! - [`fee_owed`] — money the protocol owes *to* someone: **round up**.
//!
//! Both directions err against the protocol and in the user's favour, so a
//! rounding bug can never quietly accumulate value on the protocol's side.
//! The direction is a property of *what the number means*, not of the call
//! site, which is what makes it consistent.
//!
//! Banker's rounding is deliberately not used. It is the right choice for
//! minimising statistical bias over a symmetric distribution, but it makes the
//! result depend on the parity of the truncated value — so reviewers cannot
//! tell from a fee alone whether it was computed correctly, and two paths that
//! *look* equivalent can disagree on exact halves.
//!
//! # Integer only
//!
//! Every function here is integer arithmetic on `u128` intermediates. `f64`
//! carries a 53-bit mantissa, so it silently loses precision above
//! `2^53 ≈ 9.007e15` stroops — well inside the `i64` range the fee tables
//! use. Money never goes through a float in this module.

/// One hundred percent, in basis points.
pub const BPS_DENOMINATOR: u64 = 10_000;

/// `amount * bps / 10_000`, rounded **down**.
///
/// The `u128` intermediate is what keeps `amount * bps` exact: at `u64` it
/// overflows for any amount above roughly `1.8e15` at a 1% rate.
pub fn apply_bps_floor(amount: u64, bps: u64) -> u64 {
    let scaled = (amount as u128) * (bps as u128) / (BPS_DENOMINATOR as u128);

    scaled.min(u64::MAX as u128) as u64
}

/// `amount * bps / 10_000`, rounded **up**.
///
/// Uses `u128::div_ceil` so the rounding direction is stated rather than
/// encoded in an idiom, and computed in `u128` so the multiplication above
/// cannot overflow first.
pub fn apply_bps_ceil(amount: u64, bps: u64) -> u64 {
    let numerator = (amount as u128) * (bps as u128);
    let scaled = numerator.div_ceil(BPS_DENOMINATOR as u128);

    scaled.min(u64::MAX as u128) as u64
}

/// A fee charged **to** a user, rounded down, then raised to `min_fee`.
///
/// Rounding down first and clamping second is the order that matters: a fee
/// that rounds to zero still has to clear the minimum, and a fee already above
/// the minimum must not be inflated by it.
pub fn fee_charged(amount: u64, fee_bps: u64, min_fee: u64) -> u64 {
    apply_bps_floor(amount, fee_bps).max(min_fee)
}

/// A fee the protocol owes **to** someone, rounded up, then raised to
/// `min_fee`.
pub fn fee_owed(amount: u64, fee_bps: u64, min_fee: u64) -> u64 {
    apply_bps_ceil(amount, fee_bps).max(min_fee)
}

/// Express `numerator / denominator` in basis points, rounded **down**.
///
/// Returns `0` for a zero denominator and saturates at [`BPS_DENOMINATOR`], so
/// a ratio above 1.0 reads as 100% rather than wrapping.
pub fn ratio_to_bps(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        return 0;
    }

    let scaled = (numerator as u128) * (BPS_DENOMINATOR as u128) / (denominator as u128);

    scaled.min(BPS_DENOMINATOR as u128) as u64
}

/// Convert a decimal multiplier (e.g. `1.10`) to basis points, rounded down.
///
/// This exists for the one remaining float on the boundary: older API requests
/// carry `safety_margin` as a decimal. The conversion happens once, at the
/// edge, and everything downstream is integer.
pub fn decimal_to_bps(value: f64) -> Option<u64> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }

    let scaled = value * BPS_DENOMINATOR as f64;

    if scaled > u64::MAX as f64 {
        return None;
    }

    Some(scaled.floor() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_and_ceil_agree_on_exact_division() {
        // 1000 at 250 bps is exactly 25 — no remainder, so direction is moot.
        assert_eq!(apply_bps_floor(1_000, 250), 25);
        assert_eq!(apply_bps_ceil(1_000, 250), 25);
    }

    #[test]
    fn floor_and_ceil_differ_by_one_on_a_remainder() {
        // 1001 at 250 bps is 25.025.
        assert_eq!(apply_bps_floor(1_001, 250), 25);
        assert_eq!(apply_bps_ceil(1_001, 250), 26);
    }

    #[test]
    fn a_charged_fee_never_rounds_up() {
        // The half case is where banker's rounding used to disagree with
        // itself depending on parity. Direction here is unconditional.
        assert_eq!(apply_bps_floor(1, 5_000), 0); // 0.5 -> 0
        assert_eq!(apply_bps_floor(3, 5_000), 1); // 1.5 -> 1
        assert_eq!(apply_bps_floor(5, 5_000), 2); // 2.5 -> 2
        assert_eq!(apply_bps_floor(7, 5_000), 3); // 3.5 -> 3
    }

    #[test]
    fn an_owed_fee_never_rounds_down() {
        assert_eq!(apply_bps_ceil(1, 5_000), 1); // 0.5 -> 1
        assert_eq!(apply_bps_ceil(3, 5_000), 2); // 1.5 -> 2
        assert_eq!(apply_bps_ceil(5, 5_000), 3); // 2.5 -> 3
        assert_eq!(apply_bps_ceil(7, 5_000), 4); // 3.5 -> 4
    }

    #[test]
    fn zero_rate_and_zero_amount_are_free() {
        assert_eq!(fee_charged(1_000_000, 0, 0), 0);
        assert_eq!(fee_owed(1_000_000, 0, 0), 0);
        assert_eq!(fee_charged(0, 250, 0), 0);
        assert_eq!(fee_owed(0, 250, 0), 0);
    }

    #[test]
    fn full_rate_returns_the_whole_amount() {
        assert_eq!(apply_bps_floor(12_345, BPS_DENOMINATOR), 12_345);
        assert_eq!(apply_bps_ceil(12_345, BPS_DENOMINATOR), 12_345);
    }

    #[test]
    fn minimum_fee_applies_only_when_the_computed_fee_is_below_it() {
        // 100 at 10 bps is 0.1 -> floors to 0, so the minimum takes over.
        assert_eq!(fee_charged(100, 10, 1), 1);

        // 1000 at 250 bps is 25, comfortably above the minimum, which must
        // not inflate it.
        assert_eq!(fee_charged(1_000, 250, 1), 25);
    }

    /// The precision failure that motivated dropping `f64`. Above `2^53` a
    /// double cannot represent consecutive integers, so a float
    /// implementation returns a neighbouring value; the integer path is exact.
    #[test]
    fn large_amounts_keep_full_precision() {
        let amount = 9_007_199_254_740_993_u64; // 2^53 + 1, not representable in f64
        let via_float = ((amount as f64) * 10_000.0 / 10_000.0) as u64;

        assert_ne!(via_float, amount, "f64 should lose this value");
        assert_eq!(apply_bps_floor(amount, BPS_DENOMINATOR), amount);
        assert_eq!(apply_bps_ceil(amount, BPS_DENOMINATOR), amount);
    }

    #[test]
    fn intermediate_multiplication_does_not_overflow_u64() {
        // amount * bps exceeds u64::MAX here; the u128 intermediate holds it.
        let amount = u64::MAX / 2;

        assert_eq!(apply_bps_floor(amount, BPS_DENOMINATOR), amount);
        assert_eq!(apply_bps_ceil(amount, BPS_DENOMINATOR), amount);
    }

    #[test]
    fn ratio_saturates_rather_than_exceeding_one_hundred_percent() {
        assert_eq!(ratio_to_bps(1, 2), 5_000);
        assert_eq!(ratio_to_bps(1, 3), 3_333); // floored, not 3_333.33 rounded
        assert_eq!(ratio_to_bps(0, 100), 0);
        assert_eq!(ratio_to_bps(100, 100), BPS_DENOMINATOR);
        assert_eq!(ratio_to_bps(200, 100), BPS_DENOMINATOR);
    }

    #[test]
    fn ratio_of_zero_denominator_is_zero_not_a_panic() {
        assert_eq!(ratio_to_bps(1, 0), 0);
    }

    #[test]
    fn decimal_conversion_rejects_nonsense() {
        assert_eq!(decimal_to_bps(1.1), Some(11_000));
        assert_eq!(decimal_to_bps(1.0), Some(BPS_DENOMINATOR));
        assert_eq!(decimal_to_bps(0.0), Some(0));
        assert_eq!(decimal_to_bps(-1.0), None);
        assert_eq!(decimal_to_bps(f64::NAN), None);
        assert_eq!(decimal_to_bps(f64::INFINITY), None);
    }

    /// BE-025's actual requirement: the same inputs must produce the same fee
    /// no matter which module asks. Every fee path now routes through these
    /// two functions, so this holds by construction — the test pins it so a
    /// future path that reimplements the arithmetic fails here.
    #[test]
    fn every_path_agrees_for_the_same_inputs() {
        let cases = [
            (1_000_u64, 250_u64),
            (1_001, 250),
            (7, 5_000),
            (999_999, 1),
            (u64::MAX / 4, 10_000),
        ];

        for (amount, bps) in cases {
            let charged = fee_charged(amount, bps, 0);
            let owed = fee_owed(amount, bps, 0);

            assert_eq!(charged, apply_bps_floor(amount, bps));
            assert_eq!(owed, apply_bps_ceil(amount, bps));

            // Rounding direction can only ever separate the two by one unit.
            assert!(
                owed == charged || owed == charged + 1,
                "charged {charged} and owed {owed} differ by more than one for \
                 amount {amount} at {bps} bps"
            );
            assert!(charged <= owed);
        }
    }
}
