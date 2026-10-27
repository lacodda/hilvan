//! What hilvan decides the same way wherever it is decided.
//!
//! The server grades an answer; the phone checks each typed turn the moment
//! it is typed, with or without a connection. Both read the same code: this
//! crate is linked into the server and built to WebAssembly for the tutor
//! app (`crates/check`), so a turn the phone called right on a train is a
//! turn the server calls right when the answer arrives. The reader counts a
//! text the same way: how much of it is the learner's is one piece of
//! arithmetic, run by the server over a library and by the phone over the
//! page after every tap. Nothing here touches a database, a clock or the
//! network.

pub mod answer;
pub mod check;
pub mod cover;
pub mod material;
pub mod rating;
pub mod say;
