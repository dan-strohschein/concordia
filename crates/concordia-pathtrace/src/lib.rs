//! The reference path tracer (M4 4.1b, ADR-0101): a test oracle for the renderer, built so that
//! a GPU backend (4.1b3) and CPU-only rendering (M14) reuse its backend-neutral core. It never
//! depends on `concordia-render`.

pub mod accel;
pub mod bvh;
pub mod geometry;
pub mod rng;

pub use accel::{Accel, Instance, InstanceHit};
pub use bvh::Bvh;
pub use geometry::{Hit, Ray, Triangle};
pub use rng::SampleStream;
