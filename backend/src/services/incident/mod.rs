//! Incident Command Center service layer.
//!
//! See `docs/platform-evolution.md` §5. W1 shipped the state machine,
//! W2 the CRUD + timeline helpers, W3 adds promote-from-issue plus the
//! Slack war-room + Jira ticket automation.

pub mod lifecycle;
pub mod slack_helper;
pub mod state_machine;
pub mod timeline;
pub mod war_room;
