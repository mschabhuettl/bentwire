//! Deterministic behaviour core of bentwired (no I/O).
//!
//! Spec section 2 (B-CORE-1). Entry point: [`Core`]; inputs and outputs in [`api`];
//! the character playback model in [`model`].

pub mod api;
mod core;
pub mod draws;
pub mod model;

pub use crate::core::Core;
