//! utterance backend library. The binary (`src/main.rs`) is a thin wrapper;
//! integration tests in `tests/` exercise this public surface.
//!
//! This crate is the shell: HTTP, storage, wiring. The reasoning lives in
//! `utterance-analysis` (objective), `utterance-mapping` (aesthetic) and
//! `utterance-realisation` (mechanical) — see `docs/architecture.md`. It is the
//! composition root, and the only crate that may depend on all three.

pub mod config;
pub mod error;
pub mod routes;
pub mod state;
pub mod store;
pub mod voice;
