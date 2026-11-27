//! Rust functions graphs call (`#[graph_node]`, M4 4.9): the vector math core.

use concordia_core::math::Vec3;
use concordia_reflect::{GraphFnInfo, graph_node};

/// The vector's length.
#[graph_node(category = "Vector", pure)]
pub fn length(v: Vec3) -> f32 {
    v.length()
}

/// The vector scaled to length 1 (zero stays zero).
#[graph_node(category = "Vector", pure)]
pub fn normalize(v: Vec3) -> Vec3 {
    v.normalize_or_zero()
}

/// `a` · `b`.
#[graph_node(category = "Vector", pure)]
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a.dot(b)
}

/// The distance between two points.
#[graph_node(category = "Vector", pure)]
pub fn distance(a: Vec3, b: Vec3) -> f32 {
    a.distance(b)
}

/// Every function this crate offers graphs.
pub fn graph_fns() -> Vec<GraphFnInfo> {
    vec![
        length_graph_node(),
        normalize_graph_node(),
        dot_graph_node(),
        distance_graph_node(),
    ]
}
