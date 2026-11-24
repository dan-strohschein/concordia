//! The reference path tracer (M4 4.1b, ADR-0101): a test oracle for the renderer, built so that
//! a GPU backend (4.1b3) and CPU-only rendering (M14) reuse its backend-neutral core. It never
//! depends on `concordia-render`.

pub mod accel;
mod backend;
pub mod bvh;
pub mod cpu;
pub mod env;
mod error;
pub mod geometry;
pub mod gpu;
pub mod module;
pub mod rng;
pub mod scene;
pub mod world;

pub use accel::{Accel, Instance, InstanceHit};
pub use backend::{Backend, render};
pub use bvh::Bvh;
pub use cpu::{HdrImage, Settings, render_cpu};
pub use env::EnvMap;
pub use error::PathTraceError;
pub use geometry::{Hit, Ray, Triangle};
pub use gpu::GpuTracer;
pub use module::{CaptureReference, PathTraceModule};
pub use rng::SampleStream;
pub use scene::{Light, MaterialId, MeshId, Scene, SceneBuilder};
pub use world::{AssetKind, LoadedAssets, View, asset_refs, scene_from_world};
