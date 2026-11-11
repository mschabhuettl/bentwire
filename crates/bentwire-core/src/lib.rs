//! Deterministic behaviour core of bentwired (no I/O).
//!
//! Spec section 2 (B-CORE-1). Entry point: [`Core`]; inputs and outputs in [`api`];
//! the character playback model in [`model`].

pub mod api;
pub mod balloon;
mod core;
#[cfg(test)]
mod core_tests;
pub mod draws;
pub mod fallback;
pub mod geometry;
pub mod l10n;
pub mod lipsync;
pub mod model;
pub mod office;
pub mod player;
pub mod speak;

pub use crate::core::Core;
