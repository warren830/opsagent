//! Timeline Bus — in-process fan-out channel for `IncidentTimelineEvent`.
//!
//! Any writer that inserts a row into `incident_timeline_events` also pushes
//! the event onto this bus, so SSE subscribers (the War Room page) can render
//! it in real time without polling.
//!
//! Why `tokio::broadcast`?
//!
//! - We need 1-to-many fan-out (multiple War Room tabs / operators watching
//!   the same incident).
//! - Back-pressure is "drop-oldest" — that's exactly what we want for
//!   operator-facing timelines: slow subscribers lose the oldest events but
//!   never block producers. The SSE handler detects lag and emits a
//!   synthetic `stream_lagged` notice.
//! - Producers are strictly fire-and-forget. A missing subscriber or a
//!   saturated channel MUST NOT abort the DB insert.
//!
//! The channel capacity (`1024`) is deliberately generous — a single
//! incident rarely crosses ~50 events, and we multiplex all tenants on one
//! bus. If you see `Lagged` errors in production, raise this or split per
//! tenant, but that's a V2 concern.

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::models::incident::IncidentTimelineEvent;

/// Payload broadcast to every SSE subscriber. We carry the `incident_id`
/// outside the struct so subscribers can filter without deserializing the
/// full event.
#[derive(Clone, Debug)]
pub struct TimelineBroadcast {
    pub incident_id: Uuid,
    pub event: IncidentTimelineEvent,
}

/// Thin wrapper over `broadcast::Sender`. Cloneable (the inner sender is
/// cheap to clone), so `AppState` can hold an `Arc<TimelineBus>` and every
/// handler gets a shared reference.
#[derive(Clone)]
pub struct TimelineBus {
    tx: broadcast::Sender<TimelineBroadcast>,
}

impl TimelineBus {
    /// Creates a new bus with a 1024-slot ring buffer.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }

    /// Fire-and-forget publish. Returns silently when there are no
    /// subscribers (the normal case — most events fire without anyone
    /// listening).
    pub fn publish(&self, broadcast: TimelineBroadcast) {
        // `send` errors iff the channel has zero active receivers. That is
        // the steady state — don't pollute logs.
        let _ = self.tx.send(broadcast);
    }

    /// Subscribe to the bus. Callers are responsible for filtering by
    /// `incident_id`.
    pub fn subscribe(&self) -> broadcast::Receiver<TimelineBroadcast> {
        self.tx.subscribe()
    }

    /// Current number of active subscribers (testing / diagnostics).
    #[allow(dead_code)]
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for TimelineBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_event(incident_id: Uuid, kind: &str) -> IncidentTimelineEvent {
        IncidentTimelineEvent {
            id: Uuid::new_v4(),
            incident_id,
            kind: kind.to_string(),
            actor: serde_json::json!({"kind":"system","source":"test"}),
            occurred_at: Utc::now(),
            service_id: None,
            summary: "hello".to_string(),
            payload: serde_json::json!({}),
            correlation_id: None,
        }
    }

    #[tokio::test]
    async fn publish_without_subscribers_is_noop() {
        let bus = TimelineBus::new();
        assert_eq!(bus.receiver_count(), 0);
        bus.publish(TimelineBroadcast {
            incident_id: Uuid::new_v4(),
            event: sample_event(Uuid::new_v4(), "noop"),
        });
        // Must not panic / block / error. Reaching this line is the assertion.
    }

    #[tokio::test]
    async fn subscriber_receives_published_event() {
        let bus = TimelineBus::new();
        let mut rx = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);

        let incident = Uuid::new_v4();
        let ev = sample_event(incident, "incident_status_changed");
        bus.publish(TimelineBroadcast {
            incident_id: incident,
            event: ev.clone(),
        });

        let got = rx.recv().await.expect("broadcast delivered");
        assert_eq!(got.incident_id, incident);
        assert_eq!(got.event.id, ev.id);
        assert_eq!(got.event.kind, "incident_status_changed");
    }

    #[tokio::test]
    async fn multiple_subscribers_all_receive() {
        let bus = TimelineBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 2);

        let incident = Uuid::new_v4();
        bus.publish(TimelineBroadcast {
            incident_id: incident,
            event: sample_event(incident, "deploy_started"),
        });

        let a = rx1.recv().await.expect("rx1 got event");
        let b = rx2.recv().await.expect("rx2 got event");
        assert_eq!(a.incident_id, incident);
        assert_eq!(b.incident_id, incident);
        assert_eq!(a.event.id, b.event.id);
    }

    #[tokio::test]
    async fn receivers_filter_by_incident_id_client_side() {
        // The bus is intentionally NOT filtered — callers filter their own
        // Receiver. This test documents that contract.
        let bus = TimelineBus::new();
        let mut rx = bus.subscribe();

        let target = Uuid::new_v4();
        let other = Uuid::new_v4();

        bus.publish(TimelineBroadcast {
            incident_id: other,
            event: sample_event(other, "noise"),
        });
        bus.publish(TimelineBroadcast {
            incident_id: target,
            event: sample_event(target, "signal"),
        });

        // Client-side filter loop.
        let mut got_target = None;
        for _ in 0..2 {
            let b = rx.recv().await.expect("broadcast");
            if b.incident_id == target {
                got_target = Some(b);
                break;
            }
        }
        let b = got_target.expect("target event delivered");
        assert_eq!(b.event.kind, "signal");
    }
}
