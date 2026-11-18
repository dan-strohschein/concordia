//! Structural generic types: `Option`, `Vec`, `BTreeMap<String, _>`, arrays and tuples.

use std::any::type_name;
use std::collections::BTreeMap;

use crate::info::{Shape, TypeInfo, TypeRef};
use crate::reflect::{Reflect, generic_type_info, leak_name};

fn structural<T: 'static>(name: String, shape: impl FnOnce() -> Shape) -> &'static TypeInfo {
    generic_type_info::<T>(|| TypeInfo::structural(leak_name(name), type_name::<T>(), shape()))
}

impl<T: Reflect> Reflect for Option<T> {
    fn type_info() -> &'static TypeInfo {
        structural::<Self>(format!("Option<{}>", T::type_info().name), || {
            Shape::Option(TypeRef::of::<T>())
        })
    }
}

impl<T: Reflect> Reflect for Vec<T> {
    fn type_info() -> &'static TypeInfo {
        structural::<Self>(format!("Vec<{}>", T::type_info().name), || {
            Shape::List(TypeRef::of::<T>())
        })
    }
}

impl<T: Reflect> Reflect for BTreeMap<String, T> {
    fn type_info() -> &'static TypeInfo {
        structural::<Self>(format!("Map<{}>", T::type_info().name), || Shape::Map {
            value: TypeRef::of::<T>(),
        })
    }
}

impl<T: Reflect, const N: usize> Reflect for [T; N]
where
    [T; N]: serde::Serialize + serde::de::DeserializeOwned,
{
    fn type_info() -> &'static TypeInfo {
        structural::<Self>(format!("[{}; {N}]", T::type_info().name), || Shape::Array {
            elem: TypeRef::of::<T>(),
            len: N,
        })
    }
}

macro_rules! tuple {
    ($($t:ident),+) => {
        impl<$($t: Reflect),+> Reflect for ($($t,)+) {
            fn type_info() -> &'static TypeInfo {
                let names = [$($t::type_info().name),+];
                structural::<Self>(format!("({})", names.join(", ")), || {
                    Shape::Tuple(vec![$(TypeRef::of::<$t>()),+])
                })
            }
        }
    };
}

tuple!(A);
tuple!(A, B);
tuple!(A, B, C);
tuple!(A, B, C, D);
