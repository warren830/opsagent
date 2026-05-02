//! SLO engine services — error budget math, rule generation, ruler sync.
//!
//! See `docs/platform-evolution.md` §4 for the full design. This module
//! currently only exposes [`budget_calc`]; ruler client and rule generator
//! land in later units of the SLO MVP (W3).

pub mod budget_calc;
pub mod mimir_client;
