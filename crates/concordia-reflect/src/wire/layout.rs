//! Fixed layouts: one stride per type, cached by type (type infos live for the whole process).

use std::collections::HashMap;
use std::sync::{OnceLock, PoisonError, RwLock};

use super::{Layout, WireError};
use crate::info::{FieldInfo, Primitive, Shape, TypeInfo, TypeRef, VariantShape};

/// Bytes of an arena reference (`u32` offset, `u32` length).
pub(crate) const REF: usize = 8;
/// Bytes of an enum tag.
pub(crate) const TAG: usize = 4;
/// Largest stride a type may have (arena references are `u32` offsets).
const MAX_STRIDE: u32 = u32::MAX;

/// What a value position holds: a type, or an enum variant's payload.
#[derive(Clone, Copy)]
pub(crate) enum Ty {
    /// A whole type.
    Info(&'static TypeInfo),
    /// Named fields (a struct variant's payload).
    Fields(&'static [FieldInfo]),
    /// Positional values (a tuple variant's payload).
    Items(&'static [TypeRef]),
}

impl Ty {
    /// The stride of this position.
    pub(crate) fn stride(self) -> Result<usize, WireError> {
        match self {
            Ty::Info(info) => Ok(of(info)?.stride),
            Ty::Fields(fields) => sum(fields.iter().map(|f| f.ty)),
            Ty::Items(refs) => sum(refs.iter().copied()),
        }
    }

    /// A short name for errors.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Ty::Info(info) => info.name,
            Ty::Fields(_) => "a struct variant",
            Ty::Items(_) => "a tuple variant",
        }
    }
}

/// The type behind any newtype wrappers (they encode as their inner value).
pub(crate) fn peel(mut info: &'static TypeInfo) -> &'static TypeInfo {
    while let Shape::Newtype { inner, .. } = &info.shape {
        info = inner.info();
    }
    info
}

/// The layout of `info` (cached).
pub(crate) fn of(info: &'static TypeInfo) -> Result<Layout, WireError> {
    static CACHE: OnceLock<RwLock<HashMap<usize, Layout>>> = OnceLock::new();
    let cache = CACHE.get_or_init(RwLock::default);
    let key = std::ptr::from_ref(info) as usize;
    if let Some(found) = cache
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return Ok(*found);
    }
    let computed = compute(info, &mut Vec::new())?;
    cache
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, computed);
    Ok(computed)
}

/// Offsets of each field, in declaration order.
pub(crate) fn offsets(refs: impl Iterator<Item = TypeRef>) -> Result<Vec<usize>, WireError> {
    let mut at = 0;
    let mut out = Vec::new();
    for r in refs {
        out.push(at);
        at += of(r.info())?.stride;
    }
    Ok(out)
}

fn sum(refs: impl Iterator<Item = TypeRef>) -> Result<usize, WireError> {
    let mut total = 0usize;
    for r in refs {
        total = total.saturating_add(of(r.info())?.stride);
    }
    Ok(total)
}

fn compute(info: &'static TypeInfo, stack: &mut Vec<usize>) -> Result<Layout, WireError> {
    let key = std::ptr::from_ref(info) as usize;
    if stack.contains(&key) {
        return Err(WireError::Unsupported {
            type_name: info.name.to_owned(),
            reason: "it contains itself inline; put the recursion behind a list".into(),
        });
    }
    stack.push(key);
    let result = shape(&info.shape, stack);
    stack.pop();
    let layout = result?;
    if u32::try_from(layout.stride).is_err() {
        return Err(WireError::Unsupported {
            type_name: info.name.to_owned(),
            reason: format!("its stride {} exceeds {MAX_STRIDE} bytes", layout.stride),
        });
    }
    Ok(layout)
}

fn shape(shape: &Shape, stack: &mut Vec<usize>) -> Result<Layout, WireError> {
    let refs = |refs: &[TypeRef], stack: &mut Vec<usize>| all(refs.iter().copied(), stack);
    Ok(match shape {
        Shape::Primitive(p) => primitive(*p),
        Shape::Struct { fields, .. } => all(fields.iter().map(|f| f.ty), stack)?,
        Shape::TupleStruct { fields } | Shape::Tuple(fields) => refs(fields, stack)?,
        Shape::Newtype { inner, .. } => compute(inner.info(), stack)?,
        Shape::Unit => fixed(0),
        Shape::Enum { variants } => {
            let mut largest = fixed(0);
            for v in variants {
                let payload = match &v.shape {
                    VariantShape::Unit => fixed(0),
                    VariantShape::Newtype(r) => compute(r.info(), stack)?,
                    VariantShape::Tuple(rs) => refs(rs, stack)?,
                    VariantShape::Struct(fields) => all(fields.iter().map(|f| f.ty), stack)?,
                };
                largest = Layout {
                    stride: largest.stride.max(payload.stride),
                    has_arena: largest.has_arena || payload.has_arena,
                };
            }
            Layout {
                stride: TAG + largest.stride,
                has_arena: largest.has_arena,
            }
        }
        Shape::Option(r) => {
            let inner = compute(r.info(), stack)?;
            Layout {
                stride: 1 + inner.stride,
                has_arena: inner.has_arena,
            }
        }
        Shape::List(_) | Shape::Map { .. } | Shape::Any => arena_ref(),
        Shape::Array { elem, len } => {
            let inner = compute(elem.info(), stack)?;
            Layout {
                stride: inner.stride.saturating_mul(*len),
                has_arena: inner.has_arena,
            }
        }
    })
}

fn all(refs: impl Iterator<Item = TypeRef>, stack: &mut Vec<usize>) -> Result<Layout, WireError> {
    let mut total = fixed(0);
    for r in refs {
        let l = compute(r.info(), stack)?;
        total = Layout {
            stride: total.stride.saturating_add(l.stride),
            has_arena: total.has_arena || l.has_arena,
        };
    }
    Ok(total)
}

const fn fixed(stride: usize) -> Layout {
    Layout {
        stride,
        has_arena: false,
    }
}

const fn arena_ref() -> Layout {
    Layout {
        stride: REF,
        has_arena: true,
    }
}

/// Size of a scalar.
pub(crate) fn primitive(p: Primitive) -> Layout {
    match p {
        Primitive::Bool | Primitive::I8 | Primitive::U8 => fixed(1),
        Primitive::I16 | Primitive::U16 => fixed(2),
        Primitive::I32
        | Primitive::U32
        | Primitive::F32
        | Primitive::NonZeroU32
        | Primitive::Char => fixed(4),
        Primitive::I64 | Primitive::U64 | Primitive::F64 | Primitive::NonZeroU64 => fixed(8),
        Primitive::Uuid => fixed(16),
        Primitive::String | Primitive::Pattern(_) => arena_ref(),
    }
}
