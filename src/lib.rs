//! A self-hosted language tutor: grammar formulas, spaced repetition and
//! audio lessons compiled from what you already know.
//!
//! The crate is a Rust library so the integration tests can build the router
//! the way `main` does; the binary in `main.rs` is the command line over it.

pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod pack;
pub mod say;
pub mod scheduling;
pub mod study;
