//! Incident Command Center service layer.
//!
//! See `docs/platform-evolution.md` §5. W1 ships only the state machine;
//! later units add `war_room.rs`, `timeline_bus.rs`, and
//! `postmortem_drafter.rs`.

pub mod state_machine;
pub mod timeline;
