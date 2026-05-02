//! Error budget arithmetic for the SLO engine.
//!
//! All functions are pure and free of I/O — they implement the canonical
//! Google SRE formulas for error budget accounting and Multi-Window
//! Multi-Burn-Rate alerting.
//!
//! See `docs/platform-evolution.md` §4.5 for the design spec.
//!
//! ## Units (read carefully before editing)
//!
//! * `objective_pct` and `sli_achieved_pct` are **human-readable percent**
//!   values (e.g. `99.9`, NOT `0.999`).
//! * `sli_ratio` in [`burn_rate`] is a **ratio in `[0.0, 1.0]`**
//!   (e.g. `0.999`, NOT `99.9`).
//! * Budget totals are expressed in **minutes of allowed downtime** over
//!   the SLO rolling window.

/// Total error budget in minutes over the SLO window.
///
/// ```text
/// total = (1 - SLO/100) * window_days * 1440
/// ```
///
/// If `objective_pct >= 100`, the budget is zero (no errors are allowed).
/// Negative or NaN inputs collapse to `0.0` rather than panic, so callers
/// can treat this as infallible arithmetic.
pub fn total_minutes(objective_pct: f64, window_days: i32) -> f64 {
    if !objective_pct.is_finite() || objective_pct >= 100.0 || window_days <= 0 {
        return 0.0;
    }
    let budget_ratio = (1.0 - objective_pct / 100.0).max(0.0);
    budget_ratio * (window_days as f64) * 1440.0
}

/// Consumed error budget in minutes, given the observed SLI achievement
/// over the window.
///
/// `sli_achieved_pct` of `99.73` means 0.27 % of the traffic missed the
/// target, so the consumed budget is `0.0027 * window_days * 1440`.
///
/// Returns `0.0` for non-finite or out-of-range inputs (defensive: the
/// upstream Mimir query can produce NaN on empty series).
pub fn consumed_minutes(sli_achieved_pct: f64, window_days: i32) -> f64 {
    if !sli_achieved_pct.is_finite() || window_days <= 0 {
        return 0.0;
    }
    // Clamp achievement to [0, 100] so over-shoots (e.g. 100.0001 from
    // floating-point arithmetic) don't produce negative consumption.
    let achieved = sli_achieved_pct.clamp(0.0, 100.0);
    let miss_ratio = 1.0 - achieved / 100.0;
    miss_ratio * (window_days as f64) * 1440.0
}

/// Remaining budget as a percentage of the total budget. Values outside
/// `[-100, 100]` are clamped:
///
/// * Positive values mean budget remains.
/// * Zero means the budget is fully spent.
/// * Negative values mean the SLO has been breached by that percentage of
///   the total budget (e.g. `-50.0` means the team has spent 1.5× the
///   allowed budget).
///
/// If `total` is zero (which happens when `objective_pct == 100`), we
/// return `0.0` — there is no budget to remain.
pub fn remaining_pct(total: f64, consumed: f64) -> f64 {
    if !total.is_finite() || !consumed.is_finite() || total <= 0.0 {
        return 0.0;
    }
    let pct = (total - consumed) / total * 100.0;
    pct.clamp(-100.0, 100.0)
}

/// Burn rate as defined by Google SRE: the ratio of the observed error
/// rate to the allowed error rate (budget burn rate).
///
/// ```text
/// burn_rate = (1 - sli_ratio) / (1 - objective_pct / 100)
/// ```
///
/// A burn rate of `1.0` means the service is consuming budget at exactly
/// the long-run allowed rate; `> 1.0` means it is burning faster than
/// sustainable. The MWMBR policy at `objective_pct = 99.9` uses a typical
/// Page threshold of ~14.4 over a 1-hour window (which consumes 2 % of a
/// 30-day budget in 1 hour).
///
/// `sli_ratio` must be a ratio (e.g. `0.999`), not a percentage.
pub fn burn_rate(sli_ratio: f64, objective_pct: f64) -> f64 {
    if !sli_ratio.is_finite() || !objective_pct.is_finite() {
        return 0.0;
    }
    let budget_rate = 1.0 - objective_pct / 100.0;
    if budget_rate <= 0.0 {
        // SLO is 100 % (or ill-formed): no budget to burn, signal
        // "infinite burn" as 0.0 to keep downstream arithmetic safe.
        // Alerting policy should never rely on this path because the
        // schema CHECK rejects `objective_pct >= 100`.
        return 0.0;
    }
    let error_rate = (1.0 - sli_ratio.clamp(0.0, 1.0)).max(0.0);
    error_rate / budget_rate
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Allow for floating-point noise when comparing derived quantities.
    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    // ---- total_minutes -----------------------------------------------------

    #[test]
    fn total_minutes_slo_99_9_over_28_days_is_40_32() {
        // 0.001 * 28 * 1440 = 40.32
        let total = total_minutes(99.9, 28);
        assert!(
            approx(total, 40.32),
            "expected 40.32, got {total}"
        );
    }

    #[test]
    fn total_minutes_slo_99_0_over_7_days_is_100_8() {
        // 0.01 * 7 * 1440 = 100.8
        let total = total_minutes(99.0, 7);
        assert!(
            approx(total, 100.8),
            "expected 100.8, got {total}"
        );
    }

    #[test]
    fn total_minutes_objective_100_is_zero() {
        // Not schema-valid (CHECK constraint rejects this), but defensive.
        assert_eq!(total_minutes(100.0, 30), 0.0);
    }

    #[test]
    fn total_minutes_non_positive_window_is_zero() {
        assert_eq!(total_minutes(99.9, 0), 0.0);
        assert_eq!(total_minutes(99.9, -7), 0.0);
    }

    // ---- consumed_minutes --------------------------------------------------

    #[test]
    fn consumed_minutes_matches_half_the_budget_when_achievement_is_halfway() {
        // Achieved halfway means consumed_minutes == total / 2 across any
        // window_days (proportionality check).
        let achieved = 99.95; // halfway between 100 and 99.9
        let consumed = consumed_minutes(achieved, 28);
        let total = total_minutes(99.9, 28);
        assert!(
            approx(consumed, total / 2.0),
            "expected {} (total/2), got {consumed}",
            total / 2.0
        );
    }

    #[test]
    fn consumed_minutes_clamps_over_one_hundred_to_zero() {
        // Achieved > 100 (floating-point noise) should not produce
        // negative consumption.
        assert_eq!(consumed_minutes(100.5, 28), 0.0);
    }

    // ---- remaining_pct -----------------------------------------------------

    #[test]
    fn remaining_pct_half_spent_is_fifty() {
        assert!(approx(remaining_pct(40.32, 20.16), 50.0));
    }

    #[test]
    fn remaining_pct_fully_spent_is_zero() {
        assert!(approx(remaining_pct(40.32, 40.32), 0.0));
    }

    #[test]
    fn remaining_pct_over_spent_goes_negative() {
        // Consumed 1.5× total → -50 %.
        assert!(approx(remaining_pct(40.32, 60.48), -50.0));
    }

    #[test]
    fn remaining_pct_total_zero_returns_zero() {
        // No total (objective == 100) → no budget to remain.
        assert_eq!(remaining_pct(0.0, 0.0), 0.0);
        assert_eq!(remaining_pct(0.0, 10.0), 0.0);
    }

    #[test]
    fn remaining_pct_clamps_extreme_overspend_to_minus_one_hundred() {
        // Consumed 10× total: raw value is -900 %, clamped to -100.
        assert_eq!(remaining_pct(40.32, 403.2), -100.0);
    }

    // ---- burn_rate ---------------------------------------------------------

    #[test]
    fn burn_rate_equals_one_when_error_matches_budget() {
        // sli = 99.9 %, objective = 99.9 % → error_rate == budget_rate → 1.0
        assert!(approx(burn_rate(0.999, 99.9), 1.0));
    }

    #[test]
    fn burn_rate_14_4_for_page_threshold_typical() {
        // Classic MWMBR Page threshold: burning 2 % of a 30-day budget in
        // 1 hour = burn_rate 14.4 at SLO 99.9. Check the arithmetic:
        // sli_ratio = 0.9856 → error_rate = 0.0144; budget_rate = 0.001 →
        // burn = 14.4.
        assert!(approx(burn_rate(0.9856, 99.9), 14.4));
    }

    #[test]
    fn burn_rate_zero_error_is_zero() {
        assert_eq!(burn_rate(1.0, 99.9), 0.0);
    }

    #[test]
    fn burn_rate_total_outage_scales_with_slo() {
        // sli_ratio = 0.0 (total outage), objective = 99.9:
        // error_rate = 1.0, budget_rate = 0.001 → burn = 1000.
        assert!(approx(burn_rate(0.0, 99.9), 1000.0));
    }

    #[test]
    fn burn_rate_objective_100_returns_zero_safely() {
        // Defensive: schema rejects this, but the function must not divide
        // by zero.
        assert_eq!(burn_rate(0.5, 100.0), 0.0);
    }

    #[test]
    fn burn_rate_non_finite_inputs_return_zero() {
        assert_eq!(burn_rate(f64::NAN, 99.9), 0.0);
        assert_eq!(burn_rate(0.999, f64::NAN), 0.0);
        assert_eq!(burn_rate(f64::INFINITY, 99.9), 0.0);
    }
}
