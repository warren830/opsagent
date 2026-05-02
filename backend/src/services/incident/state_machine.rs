//! Incident status state machine. See docs/platform-evolution.md §5.3.
//!
//! Valid transitions:
//!
//! - `triggered` → `acknowledged` | `investigating`
//! - `acknowledged` → `investigating`
//! - `investigating` → `identified` | `mitigated`
//! - `identified` → `mitigated` | `investigating`  (rollback when wrong root cause)
//! - `mitigated` → `resolved` | `investigating`    (rollback when the issue recurs)
//! - `resolved` → `postmortem_draft` | `closed` | `investigating`
//!   - Sev3/Sev4 may skip postmortem and jump straight to `closed` (enforced
//!     in the handler, not here).
//!   - `resolved` → `investigating` is the "reopen within 30 min" path;
//!     the 30-minute window is enforced in the handler.
//! - `postmortem_draft` → `postmortem_published`
//! - `postmortem_published` → `closed`
//!
//! Terminal: `closed` (no outgoing transitions).
//!
//! Transitions that re-enter `investigating` from a more advanced stage are
//! regressions and require a non-empty reason (see
//! [`transition_requires_reason`]).

use crate::models::incident::{
    STATUS_ACKNOWLEDGED, STATUS_CLOSED, STATUS_IDENTIFIED, STATUS_INVESTIGATING,
    STATUS_MITIGATED, STATUS_POSTMORTEM_DRAFT, STATUS_POSTMORTEM_PUBLISHED, STATUS_RESOLVED,
    STATUS_TRIGGERED,
};

/// Returns `true` if an incident may transition from `from` to `to`.
///
/// Unknown status values always return `false`. A self-transition
/// (`from == to`) is rejected — status changes should be no-ops at a
/// higher level.
pub fn can_transition(from: &str, to: &str) -> bool {
    if from == to {
        return false;
    }
    valid_next_statuses(from).contains(&to)
}

/// Returns the set of valid next statuses from the given current status.
///
/// Returns an empty slice if `from` is unknown or terminal.
pub fn valid_next_statuses(from: &str) -> Vec<&'static str> {
    match from {
        STATUS_TRIGGERED => vec![STATUS_ACKNOWLEDGED, STATUS_INVESTIGATING],
        STATUS_ACKNOWLEDGED => vec![STATUS_INVESTIGATING],
        STATUS_INVESTIGATING => vec![STATUS_IDENTIFIED, STATUS_MITIGATED],
        STATUS_IDENTIFIED => vec![STATUS_MITIGATED, STATUS_INVESTIGATING],
        STATUS_MITIGATED => vec![STATUS_RESOLVED, STATUS_INVESTIGATING],
        STATUS_RESOLVED => vec![
            STATUS_POSTMORTEM_DRAFT,
            STATUS_CLOSED,
            STATUS_INVESTIGATING,
        ],
        STATUS_POSTMORTEM_DRAFT => vec![STATUS_POSTMORTEM_PUBLISHED],
        STATUS_POSTMORTEM_PUBLISHED => vec![STATUS_CLOSED],
        STATUS_CLOSED => vec![],
        _ => vec![],
    }
}

/// Returns `true` if a legal transition from `from` to `to` requires a
/// non-empty `reason` (the three regression paths back to
/// `investigating`). Returns `false` for transitions that are not legal
/// in the first place — callers should check [`can_transition`] before
/// using this.
pub fn transition_requires_reason(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        (STATUS_IDENTIFIED, STATUS_INVESTIGATING)
            | (STATUS_MITIGATED, STATUS_INVESTIGATING)
            | (STATUS_RESOLVED, STATUS_INVESTIGATING)
    )
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_transitions_are_allowed() {
        // triggered → acknowledged → investigating → identified → mitigated →
        //   resolved → postmortem_draft → postmortem_published → closed
        assert!(can_transition(STATUS_TRIGGERED, STATUS_ACKNOWLEDGED));
        assert!(can_transition(STATUS_ACKNOWLEDGED, STATUS_INVESTIGATING));
        assert!(can_transition(STATUS_INVESTIGATING, STATUS_IDENTIFIED));
        assert!(can_transition(STATUS_IDENTIFIED, STATUS_MITIGATED));
        assert!(can_transition(STATUS_MITIGATED, STATUS_RESOLVED));
        assert!(can_transition(STATUS_RESOLVED, STATUS_POSTMORTEM_DRAFT));
        assert!(can_transition(
            STATUS_POSTMORTEM_DRAFT,
            STATUS_POSTMORTEM_PUBLISHED
        ));
        assert!(can_transition(STATUS_POSTMORTEM_PUBLISHED, STATUS_CLOSED));
    }

    #[test]
    fn shortcut_transitions_are_allowed() {
        // triggered directly into investigating (ack+investigate in one step).
        assert!(can_transition(STATUS_TRIGGERED, STATUS_INVESTIGATING));
        // investigating → mitigated without going through identified
        // (stop the bleeding before finding root cause).
        assert!(can_transition(STATUS_INVESTIGATING, STATUS_MITIGATED));
        // resolved directly to closed for low-severity incidents that skip
        // the postmortem gate.
        assert!(can_transition(STATUS_RESOLVED, STATUS_CLOSED));
    }

    #[test]
    fn illegal_skipping_transitions_are_rejected() {
        // Cannot jump from triggered to resolved or closed.
        assert!(!can_transition(STATUS_TRIGGERED, STATUS_RESOLVED));
        assert!(!can_transition(STATUS_TRIGGERED, STATUS_CLOSED));
        // Cannot skip from acknowledged straight to mitigated.
        assert!(!can_transition(STATUS_ACKNOWLEDGED, STATUS_MITIGATED));
        // Cannot jump over the postmortem gate.
        assert!(!can_transition(STATUS_POSTMORTEM_DRAFT, STATUS_CLOSED));
    }

    #[test]
    fn closed_is_terminal() {
        for s in [
            STATUS_TRIGGERED,
            STATUS_ACKNOWLEDGED,
            STATUS_INVESTIGATING,
            STATUS_IDENTIFIED,
            STATUS_MITIGATED,
            STATUS_RESOLVED,
            STATUS_POSTMORTEM_DRAFT,
            STATUS_POSTMORTEM_PUBLISHED,
            STATUS_CLOSED,
        ] {
            assert!(
                !can_transition(STATUS_CLOSED, s),
                "closed must not transition to {s}"
            );
        }
        assert!(valid_next_statuses(STATUS_CLOSED).is_empty());
    }

    #[test]
    fn self_transitions_are_rejected() {
        for s in [
            STATUS_TRIGGERED,
            STATUS_INVESTIGATING,
            STATUS_MITIGATED,
            STATUS_RESOLVED,
            STATUS_CLOSED,
        ] {
            assert!(!can_transition(s, s), "self-transition {s}->{s} rejected");
        }
    }

    #[test]
    fn unknown_statuses_are_rejected() {
        assert!(!can_transition("bogus", STATUS_INVESTIGATING));
        assert!(!can_transition(STATUS_TRIGGERED, "bogus"));
        assert!(valid_next_statuses("bogus").is_empty());
    }

    #[test]
    fn bidirectional_regressions_are_allowed() {
        // mitigated can progress OR regress — both legal.
        assert!(can_transition(STATUS_MITIGATED, STATUS_RESOLVED));
        assert!(can_transition(STATUS_MITIGATED, STATUS_INVESTIGATING));

        // identified can progress to mitigated OR rewind to investigating.
        assert!(can_transition(STATUS_IDENTIFIED, STATUS_MITIGATED));
        assert!(can_transition(STATUS_IDENTIFIED, STATUS_INVESTIGATING));

        // resolved: forward to postmortem_draft or closed, or reopen back to
        // investigating (the 30-minute reopen path).
        assert!(can_transition(STATUS_RESOLVED, STATUS_POSTMORTEM_DRAFT));
        assert!(can_transition(STATUS_RESOLVED, STATUS_CLOSED));
        assert!(can_transition(STATUS_RESOLVED, STATUS_INVESTIGATING));
    }

    #[test]
    fn valid_next_statuses_match_expected_sets() {
        assert_eq!(
            valid_next_statuses(STATUS_TRIGGERED),
            vec![STATUS_ACKNOWLEDGED, STATUS_INVESTIGATING]
        );
        assert_eq!(
            valid_next_statuses(STATUS_ACKNOWLEDGED),
            vec![STATUS_INVESTIGATING]
        );
        assert_eq!(
            valid_next_statuses(STATUS_INVESTIGATING),
            vec![STATUS_IDENTIFIED, STATUS_MITIGATED]
        );
        assert_eq!(
            valid_next_statuses(STATUS_MITIGATED),
            vec![STATUS_RESOLVED, STATUS_INVESTIGATING]
        );
        assert_eq!(
            valid_next_statuses(STATUS_RESOLVED),
            vec![
                STATUS_POSTMORTEM_DRAFT,
                STATUS_CLOSED,
                STATUS_INVESTIGATING,
            ]
        );
        assert_eq!(
            valid_next_statuses(STATUS_POSTMORTEM_DRAFT),
            vec![STATUS_POSTMORTEM_PUBLISHED]
        );
        assert_eq!(
            valid_next_statuses(STATUS_POSTMORTEM_PUBLISHED),
            vec![STATUS_CLOSED]
        );
    }

    #[test]
    fn transition_requires_reason_only_for_regressions() {
        // The three regression paths back to investigating.
        assert!(transition_requires_reason(
            STATUS_IDENTIFIED,
            STATUS_INVESTIGATING
        ));
        assert!(transition_requires_reason(
            STATUS_MITIGATED,
            STATUS_INVESTIGATING
        ));
        assert!(transition_requires_reason(
            STATUS_RESOLVED,
            STATUS_INVESTIGATING
        ));

        // Forward transitions do not require a reason.
        assert!(!transition_requires_reason(
            STATUS_TRIGGERED,
            STATUS_INVESTIGATING
        ));
        assert!(!transition_requires_reason(
            STATUS_INVESTIGATING,
            STATUS_MITIGATED
        ));
        assert!(!transition_requires_reason(
            STATUS_MITIGATED,
            STATUS_RESOLVED
        ));
        assert!(!transition_requires_reason(STATUS_RESOLVED, STATUS_CLOSED));
    }
}
