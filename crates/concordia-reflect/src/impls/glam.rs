//! glam vectors, quaternions and matrices, serialized by glam as flat float tuples.

use std::sync::OnceLock;

use concordia_core::math::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::info::{Shape, TypeInfo, TypeRef};
use crate::reflect::Reflect;

macro_rules! float_array {
    ($($ty:ty => $name:literal, $len:literal, $widget:literal, $docs:literal;)+) => {$(
        impl Reflect for $ty {
            fn type_info() -> &'static TypeInfo {
                static INFO: OnceLock<TypeInfo> = OnceLock::new();
                INFO.get_or_init(|| {
                    let shape = Shape::Array { elem: TypeRef::of::<f32>(), len: $len };
                    TypeInfo::named($name, stringify!($ty), $docs, shape).with_widget($widget)
                })
            }
        }
    )+};
}

float_array! {
    Vec2 => "Vec2", 2, "vec2", "2D vector as [x, y].";
    Vec3 => "Vec3", 3, "vec3", "3D vector as [x, y, z].";
    Vec4 => "Vec4", 4, "vec4", "4D vector as [x, y, z, w].";
    Quat => "Quat", 4, "quat", "Rotation quaternion as [x, y, z, w]; identity is [0, 0, 0, 1].";
    Mat4 => "Mat4", 16, "mat4", "4x4 matrix as 16 floats in column-major order.";
}
