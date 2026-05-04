//! Services v2 view — aggregator + probe + health calculator for
//! `GET /api/services/overview`.
//!
//! See design doc `aidlc-docs/2026-05-03-services-v2-multi-runtime/design.md`
//! §3.3, §4.1, §4.7. This module is the back end side of build unit U2.
//!
//! Submodules:
//! - [`aggregator`] fuses catalog / slo / incident / probe into the DTO.
//! - [`runtime_probe`] dispatches per-runtime shape collection (DB-only v1).
//! - [`health`] pure-function health classifier (unit-tested combinatorially).

pub mod aggregator;
pub mod health;
pub mod runtime_probe;
