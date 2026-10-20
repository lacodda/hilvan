//! A self-hosted language tutor: grammar formulas, spaced repetition and
//! audio lessons compiled from what you already know.
//!
//! The crate is a Rust library so the integration tests can build the router
//! the way `main` does; the binary in `main.rs` is the command line over it.

pub mod app;
pub mod auth;
pub mod breaks;
pub mod config;
pub mod db;
pub mod drift;
pub mod inventory;
pub mod lexicon;
pub mod model;
pub mod own;
pub mod pack;
pub mod scheduling;
pub mod study;
pub mod voice;
pub mod words;

// What the server shares with the tutor app, under the names the rest of the
// crate has always used for them.
pub use hilvan_core::{answer, say};
