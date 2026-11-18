//! Checking `#[repr(C)]` Rust structs against Slang's reflected layout.

use crate::error::ShaderError;
use crate::reflect::{FieldLayout, StructLayout};

/// A Rust struct's layout (fields in declaration order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Rust type name.
    pub name: &'static str,
    /// `size_of`.
    pub size: usize,
    /// Fields shared with the shader (padding fields are not listed).
    pub fields: Vec<FieldLayout>,
}

/// A `#[repr(C)]` struct mirrored in Slang; implement it with [`shader_struct!`](crate::shader_struct).
pub trait ShaderStruct {
    /// The Rust layout.
    fn layout() -> Layout;
}

/// Implements [`ShaderStruct`] from `offset_of!`: `shader_struct!(Push { a, b, c });` (list shared fields only).
#[macro_export]
macro_rules! shader_struct {
    ($ty:ident { $($field:ident),+ $(,)? }) => {
        impl $crate::ShaderStruct for $ty {
            fn layout() -> $crate::Layout {
                fn size_of_field<T, F>(_: fn(&T) -> &F) -> usize {
                    ::std::mem::size_of::<F>()
                }
                $crate::Layout {
                    name: stringify!($ty),
                    size: ::std::mem::size_of::<$ty>(),
                    fields: vec![$($crate::FieldLayout {
                        name: stringify!($field).to_owned(),
                        offset: ::std::mem::offset_of!($ty, $field),
                        size: size_of_field::<$ty, _>(|s| &s.$field),
                    }),+],
                }
            }
        }
    };
}

/// Fails if any Slang field is missing in Rust or differs in offset or size, or if total sizes differ.
pub fn check_layout<T: ShaderStruct>(slang: &StructLayout) -> Result<(), ShaderError> {
    let rust = T::layout();
    let fail = |reason: String| {
        Err(ShaderError::Layout {
            name: rust.name.to_owned(),
            reason,
        })
    };
    for field in &slang.fields {
        let Some(mine) = rust.fields.iter().find(|f| f.name == field.name) else {
            return fail(format!("field {} is missing on the Rust side", field.name));
        };
        if (mine.offset, mine.size) != (field.offset, field.size) {
            return fail(format!(
                "field {}: Rust offset {} size {}, Slang offset {} size {}",
                field.name, mine.offset, mine.size, field.offset, field.size
            ));
        }
    }
    if let Some(extra) = rust
        .fields
        .iter()
        .find(|f| !slang.fields.iter().any(|s| s.name == f.name))
    {
        return fail(format!(
            "field {} does not exist in Slang struct {}",
            extra.name, slang.name
        ));
    }
    match rust.size == slang.size {
        true => Ok(()),
        false => fail(format!(
            "Rust size {} vs Slang size {}",
            rust.size, slang.size
        )),
    }
}
