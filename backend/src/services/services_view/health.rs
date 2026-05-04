//! Health computation for the services v2 overview.
//!
//! This is deliberately a **pure function** — the aggregator passes in the
//! SLO summary, the list of active incidents for this Component, and the
//! runtime probe result, and we return a `(HealthStatus, reason)` pair.
//! Any DB / IO side-effects live in the caller so this module stays cheap
//! to unit-test across the decision matrix (design §2 D9).
//!
//! Algorithm (locked to design doc §2 D9):
//!
//! ```text
//! Critical:
//!   - any active sev1/sev2 incident
//!   - OR any SLO burn_rate_1h > 14.4 (Page-class burn)
//!   - OR runtime probe Unavailable
//!
//! Warning (not Critical):
//!   - any active incident (including sev3/sev4)
//!   - OR any SLO burn_rate_1h > 1.0
//!   - OR budget_remaining_min_pct < 20
//!
//! Healthy (not Critical/Warning):
//!   - has SLOs AND all burn_rate_1h < 1.0
//!   - AND budget_remaining_min_pct >= 50
//!   - AND no active incidents
//!   - AND runtime probe OK
//!
//! Unknown:
//!   - everything else (no SLOs + no runtime signal)
//! ```

use crate::models::incident::{Incident, SEVERITY_SEV1, SEVERITY_SEV2};
use crate::models::services_view::{HealthStatus, RuntimeDetail, SloSummary};

/// Page-class burn threshold. Anything above this is the classic MWMBR
/// fast-burn alert and should page — so we flag it Critical regardless of
/// whether an incident has been raised yet (design §2 D9).
pub const BURN_RATE_CRITICAL: f64 = 14.4;

/// Slow-burn threshold. Anything above this is still abnormal and worth
/// surfacing as a Warning even if no budget shortfall has landed yet.
pub const BURN_RATE_WARNING: f64 = 1.0;

/// Budget-remaining floor for Warning. Below 20 % means we're burning
/// through the window and should prompt attention.
pub const BUDGET_WARNING_PCT: f64 = 20.0;

/// Budget-remaining floor for Healthy. We require at least 50 % remaining
/// to call a service unambiguously healthy — anything between 20 and 50
/// lands in Warning via the implicit fall-through.
pub const BUDGET_HEALTHY_PCT: f64 = 50.0;

/// Compute the health status + a short human-readable reason from the
/// three inputs the aggregator has already gathered. Pure — no DB, no IO.
pub fn compute_health(
    slo_summary: &SloSummary,
    incidents: &[Incident],
    runtime: &RuntimeDetail,
) -> (HealthStatus, String) {
    let active_count = incidents.len();
    let high_sev_count = incidents
        .iter()
        .filter(|i| matches!(i.severity.as_str(), SEVERITY_SEV1 | SEVERITY_SEV2))
        .count();

    let burn_1h = slo_summary.burn_rate_1h_max.unwrap_or(0.0);
    let budget_pct = slo_summary.budget_remaining_min_pct;

    // ---- Critical ---------------------------------------------------------
    if high_sev_count > 0 {
        return (
            HealthStatus::Critical,
            format!("{high_sev_count} high-severity incident(s) active"),
        );
    }
    if burn_1h > BURN_RATE_CRITICAL {
        return (
            HealthStatus::Critical,
            format!("SLO burn rate 1h = {burn_1h:.2} (> {BURN_RATE_CRITICAL})"),
        );
    }
    if runtime.is_unavailable() {
        let reason = if let RuntimeDetail::Unavailable { reason } = runtime {
            reason.clone()
        } else {
            "runtime probe unavailable".to_string()
        };
        return (
            HealthStatus::Critical,
            format!("runtime unavailable: {reason}"),
        );
    }

    // ---- Warning ----------------------------------------------------------
    if active_count > 0 {
        return (
            HealthStatus::Warning,
            format!("{active_count} active incident(s)"),
        );
    }
    if burn_1h > BURN_RATE_WARNING {
        return (
            HealthStatus::Warning,
            format!("SLO burn rate 1h = {burn_1h:.2} (> {BURN_RATE_WARNING})"),
        );
    }
    if let Some(pct) = budget_pct
        && pct < BUDGET_WARNING_PCT
    {
        return (
            HealthStatus::Warning,
            format!("budget remaining {pct:.1}% (< {BUDGET_WARNING_PCT}%)"),
        );
    }

    // ---- Healthy ----------------------------------------------------------
    //
    // Requires at least one SLO signal to confidently call it healthy.
    // Otherwise fall through to Unknown so the UI can show a `?` badge
    // instead of a green dot on a service with zero measurement.
    let has_slo_signal = slo_summary.total > 0 && budget_pct.is_some();
    let budget_ok = budget_pct.map(|p| p >= BUDGET_HEALTHY_PCT).unwrap_or(false);
    let burn_ok = burn_1h < BURN_RATE_WARNING;
    let runtime_ok = !matches!(runtime, RuntimeDetail::Unavailable { .. });

    if has_slo_signal && burn_ok && budget_ok && runtime_ok {
        return (
            HealthStatus::Healthy,
            "SLOs in budget, no incidents, runtime OK".to_string(),
        );
    }

    // ---- Unknown ----------------------------------------------------------
    //
    // Examples: Component has no SLO rows AND runtime is Generic/Null, or
    // budget hasn't been snapshotted yet. We still emit a readable reason
    // so the user understands *why* it's grey, not red.
    let reason = if slo_summary.total == 0 {
        "no SLOs defined".to_string()
    } else if budget_pct.is_none() {
        "no budget snapshot yet".to_string()
    } else {
        "insufficient signal".to_string()
    };
    (HealthStatus::Unknown, reason)
}

// ---------------------------------------------------------------------------
// Tests — 8+ combinations of (SLO burn × incident × runtime probe).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn mk_incident(sev: &str) -> Incident {
        // Hand-construct an Incident row. None of the columns we don't
        // touch in the health calculator matter — we're only reading
        // `severity`.
        Incident {
            id: Uuid::new_v4(),
            tenant_id: None,
            number: 1,
            title: "test".into(),
            severity: sev.to_string(),
            status: "triggered".into(),
            commander_user_id: None,
            scribe_user_id: None,
            impact_summary: None,
            affected_component_ids: vec![],
            affected_customer_tier: None,
            detection_source: "manual".into(),
            source_issue_id: None,
            started_at: Utc::now(),
            detected_at: Utc::now(),
            acknowledged_at: None,
            mitigated_at: None,
            resolved_at: None,
            closed_at: None,
            war_room_channel_ref: None,
            bridge_url: None,
            jira_key: None,
            postmortem_doc_ref: None,
            root_cause: None,
            root_cause_category: None,
            labels: serde_json::json!({}),
            slo_budget_burn: None,
            merged_into_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn runtime_ok() -> RuntimeDetail {
        RuntimeDetail::Eks {
            pod_ready: Some(2),
            pod_desired: Some(2),
            image: Some("img:v1".into()),
            last_updated: None,
        }
    }

    fn runtime_unavailable() -> RuntimeDetail {
        RuntimeDetail::Unavailable {
            reason: "no deployment_events".into(),
        }
    }

    fn slo_ok() -> SloSummary {
        SloSummary {
            total: 2,
            budget_remaining_min_pct: Some(85.0),
            burn_rate_1h_max: Some(0.3),
        }
    }

    fn slo_burning_fast() -> SloSummary {
        SloSummary {
            total: 2,
            budget_remaining_min_pct: Some(60.0),
            burn_rate_1h_max: Some(20.0), // > 14.4 -> Critical
        }
    }

    fn slo_burning_slow() -> SloSummary {
        SloSummary {
            total: 2,
            budget_remaining_min_pct: Some(70.0),
            burn_rate_1h_max: Some(3.0), // > 1.0 but < 14.4 -> Warning
        }
    }

    fn slo_low_budget() -> SloSummary {
        SloSummary {
            total: 2,
            budget_remaining_min_pct: Some(10.0), // < 20 -> Warning
            burn_rate_1h_max: Some(0.2),
        }
    }

    fn slo_empty() -> SloSummary {
        SloSummary::default()
    }

    // --- Critical combinations -------------------------------------------

    #[test]
    fn case_1_sev1_incident_is_critical() {
        let (status, reason) = compute_health(
            &slo_ok(),
            &[mk_incident(SEVERITY_SEV1)],
            &runtime_ok(),
        );
        assert_eq!(status, HealthStatus::Critical);
        assert!(reason.contains("high-severity"), "reason: {reason}");
    }

    #[test]
    fn case_2_sev2_incident_is_critical() {
        let (status, _) = compute_health(
            &slo_ok(),
            &[mk_incident(SEVERITY_SEV2)],
            &runtime_ok(),
        );
        assert_eq!(status, HealthStatus::Critical);
    }

    #[test]
    fn case_3_fast_burn_without_incident_is_critical() {
        let (status, reason) = compute_health(&slo_burning_fast(), &[], &runtime_ok());
        assert_eq!(status, HealthStatus::Critical);
        assert!(reason.contains("burn rate"), "reason: {reason}");
    }

    #[test]
    fn case_4_runtime_unavailable_is_critical() {
        let (status, reason) = compute_health(&slo_ok(), &[], &runtime_unavailable());
        assert_eq!(status, HealthStatus::Critical);
        assert!(reason.contains("runtime unavailable"), "reason: {reason}");
    }

    // --- Warning combinations --------------------------------------------

    #[test]
    fn case_5_sev3_incident_is_warning() {
        let (status, reason) =
            compute_health(&slo_ok(), &[mk_incident("sev3")], &runtime_ok());
        assert_eq!(status, HealthStatus::Warning);
        assert!(reason.contains("active incident"), "reason: {reason}");
    }

    #[test]
    fn case_6_slow_burn_is_warning() {
        let (status, _) = compute_health(&slo_burning_slow(), &[], &runtime_ok());
        assert_eq!(status, HealthStatus::Warning);
    }

    #[test]
    fn case_7_low_budget_is_warning() {
        let (status, reason) = compute_health(&slo_low_budget(), &[], &runtime_ok());
        assert_eq!(status, HealthStatus::Warning);
        assert!(reason.contains("budget remaining"), "reason: {reason}");
    }

    // --- Healthy / Unknown -----------------------------------------------

    #[test]
    fn case_8_all_ok_is_healthy() {
        let (status, _) = compute_health(&slo_ok(), &[], &runtime_ok());
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn case_9_no_slo_no_runtime_is_unknown() {
        let generic = RuntimeDetail::Generic {
            info: serde_json::Value::Null,
        };
        let (status, reason) = compute_health(&slo_empty(), &[], &generic);
        assert_eq!(status, HealthStatus::Unknown);
        assert!(reason.contains("no SLOs"), "reason: {reason}");
    }

    #[test]
    fn case_10_has_slo_but_no_snapshot_is_unknown() {
        // Edge case: total>0 but no snapshots yet. budget_remaining=None
        // so has_slo_signal is false -> Unknown.
        let s = SloSummary {
            total: 1,
            budget_remaining_min_pct: None,
            burn_rate_1h_max: None,
        };
        let (status, reason) = compute_health(&s, &[], &runtime_ok());
        assert_eq!(status, HealthStatus::Unknown);
        assert!(reason.contains("budget snapshot"), "reason: {reason}");
    }

    #[test]
    fn case_11_critical_wins_over_warning_when_both_apply() {
        // Low budget (Warning signal) + sev1 incident (Critical). Critical
        // must win — otherwise the UI would under-alarm while a sev1 burns.
        let (status, _) = compute_health(
            &slo_low_budget(),
            &[mk_incident(SEVERITY_SEV1)],
            &runtime_ok(),
        );
        assert_eq!(status, HealthStatus::Critical);
    }

    #[test]
    fn case_12_mid_budget_between_warning_and_healthy_is_unknown() {
        // budget=35% falls between WARNING (<20) and HEALTHY (>=50). No
        // incidents, no burn, runtime ok. We land in Unknown with
        // "insufficient signal".
        let s = SloSummary {
            total: 2,
            budget_remaining_min_pct: Some(35.0),
            burn_rate_1h_max: Some(0.2),
        };
        let (status, reason) = compute_health(&s, &[], &runtime_ok());
        assert_eq!(status, HealthStatus::Unknown);
        assert!(reason.contains("insufficient"), "reason: {reason}");
    }
}
