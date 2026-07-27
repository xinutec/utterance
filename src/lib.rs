//! music backend library. The binary (`src/main.rs`) is a thin wrapper;
//! integration tests in `tests/` exercise this public surface.
//!
//! This crate is the shell: HTTP, storage, wiring. The reasoning lives in
//! `music-analysis` (objective) and, as it arrives, the mapping layer
//! (aesthetic) — see `docs/architecture.md`.

pub mod config;
pub mod error;
pub mod routes;
pub mod state;
pub mod store;
