//! SLO engine services — error budget math, rule generation, ruler sync.
//!
//! See `docs/platform-evolution.md` §4 for the full design.

pub mod budget_calc;
pub mod mimir_client;
pub mod rule_generator;
pub mod ruler_client;
