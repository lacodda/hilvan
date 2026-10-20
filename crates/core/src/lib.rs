//! What hilvan decides the same way wherever it is decided.
//!
//! The server grades an answer; the phone checks each typed turn the moment
//! it is typed, with or without a connection. Both read the same code: this
//! crate is linked into the server and built to WebAssembly for the tutor
//! app (`crates/check`), so a turn the phone called right on a train is a
//! turn the server calls right when the answer arrives. Nothing here touches
//! a database, a clock or the network.

pub mod answer;
pub mod check;
pub mod material;
pub mod rating;
pub mod say;
