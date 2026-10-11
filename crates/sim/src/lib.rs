//! Pure simulation library: no Godot, no I/O, no wall clock, no threads (TS3-TS5, TM9).

#![forbid(unsafe_code)]

pub mod codec;
mod economy;
pub mod event;
pub mod fixed;
pub mod hash;
pub mod id;
pub mod log;
pub mod order;
pub mod queue;
pub mod rng;
pub mod time;
pub mod world;
