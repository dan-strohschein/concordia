//! Lane traffic (M2 phase 2.5d, ADR-0079): lanes and junctions as scene entities (N6), vehicles
//! following them with the Intelligent Driver Model, yielding at junctions by a fixed rule and
//! braking for people in the way (N7).

pub(crate) mod components;
pub(crate) mod debug;
pub(crate) mod drive;
pub(crate) mod graph;
pub(crate) mod idm;
pub(crate) mod junction;
pub(crate) mod route;
pub(crate) mod snapshot;
pub(crate) mod steps;
pub(crate) mod traits;
