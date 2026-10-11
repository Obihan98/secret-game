//! Pure simulation library: no Godot, no I/O, no wall clock, no threads (TS3-TS5, TM9).

#![forbid(unsafe_code)]

pub mod fixed;
pub mod id;
pub mod queue;
pub mod rng;
pub mod time;
