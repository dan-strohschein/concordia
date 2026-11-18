//! Scalars and `String`.

use crate::info::{Primitive, Shape, TypeInfo};
use crate::reflect::Reflect;

macro_rules! primitive {
    ($($ty:ty => $name:literal, $prim:ident;)+) => {$(
        impl Reflect for $ty {
            fn type_info() -> &'static TypeInfo {
                static INFO: std::sync::OnceLock<TypeInfo> = std::sync::OnceLock::new();
                INFO.get_or_init(|| {
                    TypeInfo::named($name, $name, "", Shape::Primitive(Primitive::$prim))
                })
            }
        }
    )+};
}

primitive! {
    bool => "bool", Bool;
    i8 => "i8", I8;
    i16 => "i16", I16;
    i32 => "i32", I32;
    i64 => "i64", I64;
    u8 => "u8", U8;
    u16 => "u16", U16;
    u32 => "u32", U32;
    u64 => "u64", U64;
    std::num::NonZeroU32 => "NonZeroU32", NonZeroU32;
    std::num::NonZeroU64 => "NonZeroU64", NonZeroU64;
    f32 => "f32", F32;
    f64 => "f64", F64;
    char => "char", Char;
    String => "String", String;
}

impl Reflect for serde_json::Value {
    fn type_info() -> &'static TypeInfo {
        static INFO: std::sync::OnceLock<TypeInfo> = std::sync::OnceLock::new();
        INFO.get_or_init(|| {
            TypeInfo::named("Any", "serde_json::Value", "Any JSON value.", Shape::Any)
        })
    }
}
