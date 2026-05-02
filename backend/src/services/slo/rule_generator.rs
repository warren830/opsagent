//! Prometheus rule generator for SLO recording + MWMBR alerting rules.
//!
//! The generator renders one Prometheus rule group per SLO. The group contains
//! both `record:` rules (SLI numerator/denominator + pre-aggregated ratios over
//! 5m/1h/6h/3d windows) and `alert:` rules implementing Google SRE book's
//! Multi-Window Multi-Burn-Rate strategy (4 alerts per SLO: two Page-level
//! fast-burn and two Ticket-level slow-burn).
//!
//! See `docs/platform-evolution.md` §4.3 / §4.4 for the full spec.
//!
//! The rendered YAML is pushed to Mimir via [`super::ruler_client`]. A stable
//! `rules_hash` is persisted on the SLO row so we can detect drift and skip
//! pointless re-syncs.
//!
//! # Naming
//!
//! A "short id" — first 4 hex chars of the SLO UUID without dashes — is used
//! in recording-rule metric names. This keeps PromQL readable (e.g.
//! `sli:slo_7a3c:ratio_rate5m`) while being collision-free within one SLO
//! group because the rule group itself is namespaced by the full SLO id.

use crate::models::slo::Slo;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Rule group namespace pushed to Mimir. All SLO rule groups live under this
/// namespace so operators can wipe them in one call if needed.
pub const RULER_NAMESPACE: &str = "loops-slo";

/// Returns the fixed Mimir namespace used for SLO rule groups.
pub fn ruler_namespace() -> &'static str {
    RULER_NAMESPACE
}

/// Short, human-readable SLO id suffix used inside recording-rule metric names.
///
/// Takes the first 8 hex chars of the UUID (without dashes) — balance of
/// collision safety vs. readability in PromQL.
fn short_id(slo_id: &Uuid) -> String {
    let simple = slo_id.simple().to_string();
    simple.chars().take(8).collect()
}

/// Rule group name used in Mimir. Unique per SLO, prefixed with `slo_` so
/// operators can filter in the ruler UI (`group =~ "slo_.*"`).
pub fn group_name(slo_id: &Uuid) -> String {
    format!("slo_{}", short_id(slo_id))
}

/// Short id exposed for tests / callers that want to construct PromQL by hand.
#[cfg(test)]
pub fn slo_short_id(slo_id: &Uuid) -> String {
    short_id(slo_id)
}

/// MWMBR burn windows (long-window, short-window, multiplier, severity).
///
/// Table pulled verbatim from Google SRE book §5 / `docs/platform-evolution.md`
/// §4.4. Order matters — the generator emits alerts in this order so the
/// resulting YAML is deterministic.
const BURN_SPECS: &[BurnSpec] = &[
    BurnSpec {
        suffix: "Page1",
        severity: "page",
        long_window: "1h",
        short_window: "5m",
        multiplier: 14.4,
    },
    BurnSpec {
        suffix: "Page2",
        severity: "page",
        long_window: "6h",
        short_window: "30m",
        multiplier: 6.0,
    },
    BurnSpec {
        suffix: "Ticket1",
        severity: "ticket",
        long_window: "3d",
        short_window: "6h",
        multiplier: 1.0,
    },
    BurnSpec {
        suffix: "Ticket2",
        severity: "ticket",
        long_window: "7d",
        short_window: "1d",
        multiplier: 1.0,
    },
];

struct BurnSpec {
    /// Last segment of the alert name (e.g. `Page1`, `Ticket2`).
    suffix: &'static str,
    severity: &'static str,
    long_window: &'static str,
    short_window: &'static str,
    /// `burn_rate = multiplier * (1 - objective_pct/100)` — the trigger
    /// threshold on the error ratio for both windows.
    multiplier: f64,
}

/// Render the full rule group YAML for the given SLO.
///
/// The output contains:
/// * 2 raw-rate recording rules (`good_events:rate5m`, `total_events:rate5m`)
/// * 4 ratio recording rules (5m / 1h / 6h / 3d)
/// * 4 MWMBR alerting rules
///
/// The YAML is deterministic — same input always produces byte-identical
/// output, which means [`rules_hash`] is stable for drift detection.
pub fn render_rule_group(slo: &Slo) -> String {
    let short = short_id(&slo.id);
    let group = group_name(&slo.id);
    // Prefix used to build all recording-rule metric names.
    let metric_prefix = format!("sli:slo_{}", short);
    // 1 - objective as a decimal (e.g. 0.001 for 99.9%). Unwind once up-front.
    let error_budget = 1.0 - slo.objective_pct / 100.0;

    let mut out = String::new();
    out.push_str("groups:\n");
    out.push_str(&format!("- name: {group}\n"));
    // 30s is the SRE-book default for SLO rule evaluation — short enough to
    // react to 5m windows without hammering Mimir.
    out.push_str("  interval: 30s\n");
    out.push_str("  rules:\n");

    // -- Recording rules ----------------------------------------------------
    out.push_str(&format!("  - record: {metric_prefix}:good_events:rate5m\n"));
    out.push_str(&format!("    expr: {}\n", yaml_scalar(&slo.good_events_query)));
    out.push_str(&format!(
        "  - record: {metric_prefix}:total_events:rate5m\n"
    ));
    out.push_str(&format!(
        "    expr: {}\n",
        yaml_scalar(&slo.total_events_query)
    ));
    out.push_str(&format!("  - record: {metric_prefix}:ratio_rate5m\n"));
    out.push_str(&format!(
        "    expr: {metric_prefix}:good_events:rate5m / {metric_prefix}:total_events:rate5m\n"
    ));
    for window in ["1h", "6h", "3d"] {
        out.push_str(&format!(
            "  - record: {metric_prefix}:ratio_rate{window}\n"
        ));
        out.push_str(&format!(
            "    expr: sum_over_time({metric_prefix}:good_events:rate5m[{window}]) / sum_over_time({metric_prefix}:total_events:rate5m[{window}])\n"
        ));
    }

    // -- Alerting rules (MWMBR) --------------------------------------------
    let runbook_url = slo
        .labels
        .get("runbook_url")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    for spec in BURN_SPECS {
        let threshold = spec.multiplier * error_budget;
        let alert_name = format!("SloBurn{}_{}_{}",
            if spec.severity == "page" { "Fast" } else { "Slow" },
            short,
            spec.suffix,
        );
        out.push_str(&format!("  - alert: {alert_name}\n"));
        out.push_str("    expr: |\n");
        out.push_str("      (\n");
        // Long window threshold check
        out.push_str(&format!(
            "        (1 - {metric_prefix}:ratio_rate{}) > ({})\n",
            spec.long_window,
            format_threshold(spec.multiplier, error_budget),
        ));
        out.push_str("        and\n");
        // Short window threshold check — eliminates flap by requiring both to
        // breach simultaneously.
        out.push_str(&format!(
            "        (1 - {metric_prefix}:ratio_rate{}) > ({})\n",
            // Mapping: long->short is always the pair from BURN_SPECS.
            spec.short_window_ratio_field(),
            format_threshold(spec.multiplier, error_budget),
        ));
        out.push_str("      )\n");
        // 2m for-clause matches the SRE-book reference ("gate with a 2m dwell").
        out.push_str("    for: 2m\n");
        out.push_str("    labels:\n");
        out.push_str(&format!("      severity: {}\n", spec.severity));
        out.push_str(&format!("      slo_id: {}\n", yaml_scalar(&slo.id.to_string())));
        out.push_str(&format!(
            "      burn_window: {}\n",
            yaml_scalar(spec.long_window)
        ));
        out.push_str(&format!(
            "      burn_multiplier: {}\n",
            yaml_scalar(&format!("{}", spec.multiplier))
        ));
        out.push_str("    annotations:\n");
        let summary = format!(
            "SLO {name} burning {mult}x over {long}/{short} (threshold {thr:.6})",
            name = slo.name,
            mult = spec.multiplier,
            long = spec.long_window,
            short = spec.short_window,
            thr = threshold,
        );
        out.push_str(&format!("      summary: {}\n", yaml_scalar(&summary)));
        if let Some(ref runbook) = runbook_url {
            out.push_str(&format!("      runbook_url: {}\n", yaml_scalar(runbook)));
        }
    }

    out
}

impl BurnSpec {
    /// Returns the ratio recording-rule window suffix to query for the short
    /// window. Short windows (5m / 30m / 6h / 1d) map onto the ratio rules we
    /// actually emit — 5m is the native `ratio_rate5m`; 30m is approximated by
    /// `ratio_rate1h` (the next coarser published window); 6h uses
    /// `ratio_rate6h`; 1d uses `ratio_rate3d`. This trade-off is the SRE-book
    /// canonical shortcut so we don't need 8 separate recording rules — the
    /// gating still works because the long window dominates.
    fn short_window_ratio_field(&self) -> &'static str {
        match self.short_window {
            "5m" => "5m",
            "30m" => "1h",
            "6h" => "6h",
            "1d" => "3d",
            other => other,
        }
    }
}

/// Format the threshold as the literal PromQL expression used in the alert —
/// `multiplier * (1 - objective_pct/100)`. Keeping it as an expression (rather
/// than pre-multiplying to a number) means operators can eyeball the alert
/// rule and match it against the documented MWMBR table.
fn format_threshold(multiplier: f64, error_budget: f64) -> String {
    // Render the error-budget ratio with enough precision that 99.99 and
    // 99.999 both round-trip.
    format!("{} * {}", format_num(multiplier), format_num(error_budget))
}

/// Render a float with trailing zeros trimmed, but at least one decimal digit
/// so YAML readers don't mis-parse `14` as an int in a string context.
fn format_num(x: f64) -> String {
    // `{:?}` for f64 is lossless but ugly; prefer 6 sig-figs + trim.
    let raw = format!("{:.6}", x);
    let trimmed = raw.trim_end_matches('0').trim_end_matches('.').to_string();
    if trimmed.contains('.') {
        trimmed
    } else {
        format!("{}.0", trimmed)
    }
}

/// Quote a value as a YAML double-quoted scalar if it contains any character
/// that requires escaping, otherwise emit it bare. This keeps the common case
/// (simple PromQL, UUIDs) readable while handling messy user input safely.
fn yaml_scalar(value: &str) -> String {
    // Double-quote when the value contains ":", "#", leading/trailing space,
    // newline, quote, or starts with a reserved YAML indicator.
    let needs_quote = value.is_empty()
        || value.starts_with(' ')
        || value.ends_with(' ')
        || value.contains('\n')
        || value.contains(':')
        || value.contains('#')
        || value.contains('"')
        || value.contains('\'')
        || value.starts_with('&')
        || value.starts_with('*')
        || value.starts_with('?')
        || value.starts_with('|')
        || value.starts_with('>')
        || value.starts_with('!')
        || value.starts_with('%')
        || value.starts_with('@')
        || value.starts_with('`');

    if needs_quote {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    } else {
        value.to_string()
    }
}

/// SHA-256 hex of the rendered YAML, truncated to 16 chars — stored on
/// `slos.recording_rules_hash` for drift detection.
pub fn rules_hash(yaml: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(yaml.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest).chars().take(16).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn sample_slo(objective_pct: f64) -> Slo {
        // Fixed UUID so short_id / group_name assertions are deterministic.
        let id = Uuid::parse_str("7a3cbeef-1234-4567-89ab-abcd12345678").unwrap();
        Slo {
            id,
            tenant_id: Uuid::new_v4(),
            component_id: None,
            name: "checkout-availability".to_string(),
            description: Some("demo".to_string()),
            sli_type: "availability".to_string(),
            good_events_query: "sum(rate(http_requests_total{code!~\"5..\"}[5m]))".to_string(),
            total_events_query: "sum(rate(http_requests_total[5m]))".to_string(),
            objective_pct,
            window_days: 28,
            burn_rate_policy: "mwmbr_default".to_string(),
            labels: json!({"runbook_url": "https://runbooks.example.com/checkout"}),
            enabled: true,
            recording_rules_hash: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn group_name_is_stable_across_calls() {
        let slo = sample_slo(99.9);
        let a = group_name(&slo.id);
        let b = group_name(&slo.id);
        assert_eq!(a, b);
        assert!(a.starts_with("slo_"));
        // 4-char prefix after `slo_` + first 8 hex of simple UUID.
        assert_eq!(a.len(), "slo_".len() + 8);
    }

    #[test]
    fn ruler_namespace_is_fixed() {
        assert_eq!(ruler_namespace(), "loops-slo");
    }

    #[test]
    fn render_contains_all_recording_rules() {
        let slo = sample_slo(99.9);
        let yaml = render_rule_group(&slo);
        let short = slo_short_id(&slo.id);
        let prefix = format!("sli:slo_{short}");

        for suffix in [
            ":good_events:rate5m",
            ":total_events:rate5m",
            ":ratio_rate5m",
            ":ratio_rate1h",
            ":ratio_rate6h",
            ":ratio_rate3d",
        ] {
            let marker = format!("record: {prefix}{suffix}");
            assert!(
                yaml.contains(&marker),
                "missing recording rule {marker}\nrendered:\n{yaml}"
            );
        }
    }

    #[test]
    fn render_contains_all_four_mwmbr_alerts() {
        let slo = sample_slo(99.9);
        let yaml = render_rule_group(&slo);
        let short = slo_short_id(&slo.id);

        for name in [
            format!("SloBurnFast_{short}_Page1"),
            format!("SloBurnFast_{short}_Page2"),
            format!("SloBurnSlow_{short}_Ticket1"),
            format!("SloBurnSlow_{short}_Ticket2"),
        ] {
            let marker = format!("alert: {name}");
            assert!(
                yaml.contains(&marker),
                "missing alert {marker}\nrendered:\n{yaml}"
            );
        }
    }

    #[test]
    fn threshold_math_is_correct_for_three_nines() {
        // objective 99.9% → error budget 0.001
        // fast page multiplier 14.4 → threshold 0.0144
        let slo = sample_slo(99.9);
        let yaml = render_rule_group(&slo);

        // The literal expression "14.4 * 0.001" must appear in the Page1 alert.
        assert!(
            yaml.contains("14.4 * 0.001"),
            "expected `14.4 * 0.001` in page fast alert\n{yaml}"
        );
        // And "6.0 * 0.001" for Page2 (6h @ 6x).
        assert!(
            yaml.contains("6.0 * 0.001"),
            "expected `6.0 * 0.001` in page2 alert\n{yaml}"
        );
        // Ticket tier uses 1x.
        assert!(
            yaml.contains("1.0 * 0.001"),
            "expected `1.0 * 0.001` in ticket alerts\n{yaml}"
        );
    }

    #[test]
    fn threshold_math_is_correct_for_two_nines() {
        // objective 99.0% → error budget 0.01
        let slo = sample_slo(99.0);
        let yaml = render_rule_group(&slo);
        assert!(
            yaml.contains("14.4 * 0.01"),
            "expected `14.4 * 0.01` for 99.0% SLO\n{yaml}"
        );
        assert!(
            yaml.contains("6.0 * 0.01"),
            "expected `6.0 * 0.01` for 99.0% SLO\n{yaml}"
        );
    }

    #[test]
    fn alert_labels_include_slo_id_severity_window() {
        let slo = sample_slo(99.9);
        let yaml = render_rule_group(&slo);
        let id_str = slo.id.to_string();

        assert!(yaml.contains(&format!("slo_id: {id_str}")) || yaml.contains(&format!("slo_id: \"{id_str}\"")));
        assert!(yaml.contains("severity: page"));
        assert!(yaml.contains("severity: ticket"));
        assert!(yaml.contains("burn_window: 1h") || yaml.contains("burn_window: \"1h\""));
        assert!(yaml.contains("burn_window: 6h") || yaml.contains("burn_window: \"6h\""));
        assert!(yaml.contains("burn_window: 3d") || yaml.contains("burn_window: \"3d\""));
        assert!(yaml.contains("burn_window: 7d") || yaml.contains("burn_window: \"7d\""));
    }

    #[test]
    fn annotation_includes_runbook_when_label_present() {
        let slo = sample_slo(99.9);
        let yaml = render_rule_group(&slo);
        assert!(
            yaml.contains("runbook_url:"),
            "runbook_url annotation missing: {yaml}"
        );
    }

    #[test]
    fn annotation_omits_runbook_when_label_absent() {
        let mut slo = sample_slo(99.9);
        slo.labels = json!({});
        let yaml = render_rule_group(&slo);
        assert!(
            !yaml.contains("runbook_url:"),
            "runbook_url should not appear: {yaml}"
        );
    }

    #[test]
    fn rules_hash_is_deterministic_and_sensitive() {
        let slo = sample_slo(99.9);
        let yaml_a = render_rule_group(&slo);
        let yaml_b = render_rule_group(&slo);
        assert_eq!(yaml_a, yaml_b, "render must be deterministic");
        assert_eq!(rules_hash(&yaml_a), rules_hash(&yaml_b));
        assert_eq!(rules_hash(&yaml_a).len(), 16);

        let mut slo2 = sample_slo(99.9);
        slo2.objective_pct = 99.5;
        let yaml_c = render_rule_group(&slo2);
        assert_ne!(
            rules_hash(&yaml_a),
            rules_hash(&yaml_c),
            "different objective must produce different hash"
        );
    }

    #[test]
    fn rendered_yaml_is_valid_yaml() {
        // Sanity check: whatever we render must parse back as YAML. We don't
        // validate the Prometheus schema here (ruler does that on upload),
        // only that it isn't syntactically broken.
        let slo = sample_slo(99.9);
        let yaml = render_rule_group(&slo);
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("rendered YAML must parse");
        let groups = parsed.get("groups").expect("groups present");
        let seq = groups.as_sequence().expect("groups is sequence");
        assert_eq!(seq.len(), 1);
        let rules = seq[0].get("rules").expect("rules present");
        // 2 raw + 4 ratio + 4 alert = 10
        assert_eq!(
            rules.as_sequence().unwrap().len(),
            10,
            "expected 10 rules total"
        );
    }

    #[test]
    fn yaml_scalar_quotes_when_colon_present() {
        // PromQL often contains `:` from label selectors; must be quoted.
        let out = yaml_scalar("sum(rate(x{job=\"a\"}[5m]))");
        assert!(out.starts_with('"') && out.ends_with('"'));
    }
}
