//! Shared test fixtures and helpers for Cherenkov.
//!
//! Anything intended to be reused between integration tests across crates
//! lives here. The crate is gated behind the `test-support` feature so its
//! contents are not pulled into a release build.
//!
//! The crate is currently empty; helpers will be added as integration tests
//! across crates start sharing scaffolding.
#![cfg(any(test, feature = "test-support"))]
