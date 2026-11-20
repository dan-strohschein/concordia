//! Decoding: a bounds-checked reader and a serde `Deserializer` guided by the type's shape.
//! Decoding also validates: tags, arena bounds, UTF-8, scalars and declared ranges.

use std::cell::Cell;

use serde::de::{self, DeserializeSeed, IntoDeserializer, Visitor};

use super::WireError;
use super::layout::{REF, TAG, Ty, of, offsets, peel};
use crate::info::{FieldInfo, Primitive, Range, Shape, TypeInfo, TypeRef, VariantShape};

/// Deepest nesting a reader follows (guards against self-referencing arena data).
const MAX_DEPTH: usize = 128;
/// Most elements a list of zero-sized values may claim.
const MAX_ZERO_SIZED: usize = 1 << 20;

/// Bounds-checked access to wire bytes: fixed data, then the arena from `base`.
pub(crate) struct Reader<'a> {
    pub(crate) buf: &'a [u8],
    base: usize,
    /// Strict mode: the next arena offset a reference must start at (canonical order).
    cursor: Option<Cell<usize>>,
}

impl<'a> Reader<'a> {
    /// A reader whose arena starts at `base`; `strict` also checks canonical arena order.
    pub(crate) fn new(buf: &'a [u8], base: usize, strict: bool) -> Result<Self, WireError> {
        if buf.len() < base {
            return Err(WireError::Truncated {
                at: 0,
                needed: base,
                available: buf.len(),
            });
        }
        Ok(Self {
            buf,
            base,
            cursor: strict.then(|| Cell::new(0)),
        })
    }

    /// Strict mode: the arena bytes no reference covered.
    pub(crate) fn unread_arena(&self) -> usize {
        let used = self.cursor.as_ref().map_or(0, Cell::get);
        (self.buf.len() - self.base).saturating_sub(used)
    }

    pub(crate) fn bytes(&self, at: usize, n: usize) -> Result<&'a [u8], WireError> {
        self.buf
            .get(at..at.saturating_add(n))
            .ok_or(WireError::Truncated {
                at,
                needed: n,
                available: self.buf.len().saturating_sub(at),
            })
    }

    pub(crate) fn array<const N: usize>(&self, at: usize) -> Result<[u8; N], WireError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.bytes(at, N)?);
        Ok(out)
    }

    pub(crate) fn u32(&self, at: usize) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(self.array(at)?))
    }

    /// Follows the reference at `at` to `count * size` arena bytes; returns their position.
    pub(crate) fn region(&self, at: usize, size: usize) -> Result<(usize, usize), WireError> {
        let offset = self.u32(at)? as usize;
        let count = self.u32(at + 4)? as usize;
        if size == 0 && count > MAX_ZERO_SIZED {
            return Err(WireError::Mismatch(format!(
                "at byte {at}: {count} zero-sized elements"
            )));
        }
        let len = count.saturating_mul(size);
        let arena = self.buf.len() - self.base;
        if offset.saturating_add(len) > arena {
            return Err(WireError::ArenaOutOfBounds {
                at,
                offset,
                len,
                arena,
            });
        }
        if let Some(cursor) = &self.cursor {
            if offset != cursor.get() && len > 0 {
                return Err(WireError::Mismatch(format!(
                    "at byte {at}: arena data at {offset} is out of canonical order (expected {})",
                    cursor.get()
                )));
            }
            cursor.set(cursor.get() + len);
        }
        Ok((self.base + offset, count))
    }

    /// The string referenced at `at`.
    pub(crate) fn str(&self, at: usize) -> Result<&'a str, WireError> {
        let (pos, len) = self.region(at, 1)?;
        std::str::from_utf8(&self.buf[pos..pos + len]).map_err(|_| WireError::InvalidUtf8 { at })
    }

    /// Checks that `n` bytes at `at` are zero (strict mode only: padding is canonical zero).
    pub(crate) fn zeros(&self, at: usize, n: usize) -> Result<(), WireError> {
        if self.cursor.is_some() && self.bytes(at, n)?.iter().any(|&b| b != 0) {
            return Err(WireError::Mismatch(format!(
                "at byte {at}: padding is not zero"
            )));
        }
        Ok(())
    }
}

/// A decoded scalar.
#[derive(Clone, Copy)]
pub(crate) enum Scalar<'a> {
    Bool(bool),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    Char(char),
    Str(&'a str),
    Uuid([u8; 16]),
}

/// Reads and checks the scalar `p` at `at`.
pub(crate) fn scalar<'a>(
    r: &Reader<'a>,
    at: usize,
    p: Primitive,
    field: Option<&FieldInfo>,
    type_name: &str,
) -> Result<Scalar<'a>, WireError> {
    let pointer = || format!("/{}", field.map_or(type_name, |f| f.name));
    let invalid = |expected: &str| WireError::InvalidScalar {
        pointer: pointer(),
        expected: expected.to_owned(),
    };
    let value = match p {
        Primitive::Bool => match r.array::<1>(at)?[0] {
            0 => Scalar::Bool(false),
            1 => Scalar::Bool(true),
            tag => {
                return Err(WireError::InvalidTag {
                    at,
                    tag: tag.into(),
                    type_name: "bool".into(),
                });
            }
        },
        Primitive::I8 => Scalar::I64(i8::from_le_bytes(r.array(at)?).into()),
        Primitive::I16 => Scalar::I64(i16::from_le_bytes(r.array(at)?).into()),
        Primitive::I32 => Scalar::I64(i32::from_le_bytes(r.array(at)?).into()),
        Primitive::I64 => Scalar::I64(i64::from_le_bytes(r.array(at)?)),
        Primitive::U8 => Scalar::U64(u8::from_le_bytes(r.array(at)?).into()),
        Primitive::U16 => Scalar::U64(u16::from_le_bytes(r.array(at)?).into()),
        Primitive::U32 => Scalar::U64(u32::from_le_bytes(r.array(at)?).into()),
        Primitive::U64 => Scalar::U64(u64::from_le_bytes(r.array(at)?)),
        Primitive::NonZeroU32 => match u32::from_le_bytes(r.array(at)?) {
            0 => return Err(invalid("non-zero u32")),
            v => Scalar::U64(v.into()),
        },
        Primitive::NonZeroU64 => match u64::from_le_bytes(r.array(at)?) {
            0 => return Err(invalid("non-zero u64")),
            v => Scalar::U64(v),
        },
        Primitive::F32 => Scalar::F32(f32::from_le_bytes(r.array(at)?)),
        Primitive::F64 => Scalar::F64(f64::from_le_bytes(r.array(at)?)),
        Primitive::Char => char::from_u32(r.u32(at)?)
            .map_or_else(|| Err(invalid("char")), |c| Ok(Scalar::Char(c)))?,
        Primitive::String | Primitive::Pattern(_) => Scalar::Str(r.str(at)?),
        Primitive::Uuid => Scalar::Uuid(r.array(at)?),
    };
    if let Some(range) = field.and_then(|f| f.range) {
        check_range(value, range, pointer)?;
    }
    Ok(value)
}

fn check_range(
    value: Scalar<'_>,
    range: Range,
    pointer: impl Fn() -> String,
) -> Result<(), WireError> {
    let v = match value {
        Scalar::I64(v) => v as f64,
        Scalar::U64(v) => v as f64,
        Scalar::F32(v) => f64::from(v),
        Scalar::F64(v) => v,
        _ => return Ok(()),
    };
    if v < range.min || v > range.max {
        return Err(WireError::OutOfRange {
            pointer: pointer(),
            value: v,
            min: range.min,
            max: range.max,
        });
    }
    Ok(())
}

/// The hyphenated form of a UUID.
pub(crate) fn uuid_text(bytes: [u8; 16]) -> String {
    concordia_core::id::Uuid::from_bytes(bytes)
        .hyphenated()
        .to_string()
}

/// Deserializes the value of `ty` at `at`.
pub(crate) struct De<'r, 'a> {
    pub(crate) r: &'r Reader<'a>,
    pub(crate) at: usize,
    pub(crate) ty: Ty,
    pub(crate) field: Option<&'static FieldInfo>,
    pub(crate) depth: usize,
}

impl<'r, 'a> De<'r, 'a> {
    fn child(
        &self,
        at: usize,
        ty: Ty,
        field: Option<&'static FieldInfo>,
    ) -> Result<Self, WireError> {
        if self.depth >= MAX_DEPTH {
            return Err(WireError::Mismatch(format!(
                "nesting deeper than {MAX_DEPTH}"
            )));
        }
        Ok(De {
            r: self.r,
            at,
            ty,
            field,
            depth: self.depth + 1,
        })
    }

    fn of_ref(
        &self,
        at: usize,
        r: TypeRef,
        field: Option<&'static FieldInfo>,
    ) -> Result<Self, WireError> {
        self.child(at, Ty::Info(r.info()), field)
    }

    fn visit_scalar<V: Visitor<'a>>(
        self,
        p: Primitive,
        info: &TypeInfo,
        v: V,
    ) -> Result<V::Value, WireError> {
        match scalar(self.r, self.at, p, self.field, info.name)? {
            Scalar::Bool(b) => v.visit_bool(b),
            Scalar::I64(i) => v.visit_i64(i),
            Scalar::U64(u) => v.visit_u64(u),
            Scalar::F32(f) => v.visit_f32(f),
            Scalar::F64(f) => v.visit_f64(f),
            Scalar::Char(c) => v.visit_char(c),
            Scalar::Str(s) => v.visit_borrowed_str(s),
            Scalar::Uuid(bytes) => v.visit_string(uuid_text(bytes)),
        }
    }

    fn any<V: Visitor<'a>>(self, v: V) -> Result<V::Value, WireError> {
        let info = match self.ty {
            Ty::Fields(fields) => return v.visit_map(FieldsAccess::new(self, fields)?),
            Ty::Items(refs) => return v.visit_seq(ItemsAccess::new(self, Items::Refs(refs))),
            Ty::Info(info) => peel(info),
        };
        match &info.shape {
            Shape::Primitive(p) => self.visit_scalar(*p, info, v),
            Shape::Struct { fields, .. } => v.visit_map(FieldsAccess::new(self, fields)?),
            Shape::TupleStruct { fields } | Shape::Tuple(fields) => {
                v.visit_seq(ItemsAccess::new(self, Items::Refs(fields)))
            }
            Shape::Array { elem, len } => {
                v.visit_seq(ItemsAccess::new(self, Items::Repeat(*elem, *len)))
            }
            Shape::Newtype { inner, .. } => self.of_ref(self.at, *inner, self.field)?.any(v),
            Shape::Unit => v.visit_unit(),
            Shape::Enum { .. } => v.visit_enum(EnumAccess(self)),
            Shape::Option(inner) => match self.option_tag()? {
                false => v.visit_none(),
                true => v.visit_some(self.of_ref(self.at + 1, *inner, self.field)?),
            },
            Shape::List(elem) => {
                let stride = of(elem.info())?.stride();
                let (pos, count) = self.r.region(self.at, stride)?;
                let de = De { at: pos, ..self };
                v.visit_seq(ItemsAccess::new(de, Items::Repeat(*elem, count)))
            }
            Shape::Map { value } => {
                let stride = of(value.info())?.stride();
                let (pos, count) = self.r.region(self.at, REF + stride)?;
                v.visit_map(EntriesAccess {
                    de: De { at: pos, ..self },
                    value: *value,
                    stride,
                    count,
                    index: 0,
                    previous: None,
                })
            }
            Shape::Any => {
                let text = self.r.str(self.at)?;
                let json: serde_json::Value = serde_json::from_str(text)
                    .map_err(|e| WireError::Mismatch(format!("invalid JSON in Any: {e}")))?;
                de::Deserializer::deserialize_any(json, v)
                    .map_err(|e| WireError::Mismatch(e.to_string()))
            }
        }
    }

    /// The tag of the `Option` at this position (strict mode: `None` payloads are zero).
    fn option_tag(&self) -> Result<bool, WireError> {
        let Ty::Info(info) = self.ty else {
            return Err(WireError::Mismatch("expected an option".into()));
        };
        match self.r.array::<1>(self.at)?[0] {
            0 => {
                self.r.zeros(self.at + 1, of(peel(info))?.stride() - 1)?;
                Ok(false)
            }
            1 => Ok(true),
            tag => Err(WireError::InvalidTag {
                at: self.at,
                tag: tag.into(),
                type_name: info.name.to_owned(),
            }),
        }
    }
}

impl<'de> de::Deserializer<'de> for De<'_, 'de> {
    type Error = WireError;

    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value, WireError> {
        self.any(v)
    }

    fn deserialize_option<V: Visitor<'de>>(self, v: V) -> Result<V::Value, WireError> {
        match self.ty {
            Ty::Info(info) if matches!(peel(info).shape, Shape::Option(_)) => self.any(v),
            _ => v.visit_some(self),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        v: V,
    ) -> Result<V::Value, WireError> {
        match self.ty {
            Ty::Info(info) => match &info.shape {
                Shape::Newtype { inner, .. } => {
                    let inner = self.of_ref(self.at, *inner, self.field)?;
                    v.visit_newtype_struct(inner)
                }
                _ => v.visit_newtype_struct(self),
            },
            _ => v.visit_newtype_struct(self),
        }
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        v: V,
    ) -> Result<V::Value, WireError> {
        self.any(v)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value, WireError> {
        self.any(v)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct identifier
    }
}

/// Positional members.
#[derive(Clone, Copy)]
enum Items {
    Refs(&'static [TypeRef]),
    Repeat(TypeRef, usize),
}

impl Items {
    fn len(self) -> usize {
        match self {
            Items::Refs(refs) => refs.len(),
            Items::Repeat(_, n) => n,
        }
    }

    fn get(self, i: usize) -> TypeRef {
        match self {
            Items::Refs(refs) => refs[i],
            Items::Repeat(r, _) => r,
        }
    }
}

struct ItemsAccess<'r, 'a> {
    de: De<'r, 'a>,
    items: Items,
    index: usize,
    offset: usize,
}

impl<'r, 'a> ItemsAccess<'r, 'a> {
    fn new(de: De<'r, 'a>, items: Items) -> Self {
        Self {
            de,
            items,
            index: 0,
            offset: 0,
        }
    }
}

impl<'de> de::SeqAccess<'de> for ItemsAccess<'_, 'de> {
    type Error = WireError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, WireError> {
        if self.index >= self.items.len() {
            return Ok(None);
        }
        let r = self.items.get(self.index);
        let at = self.de.at + self.offset;
        self.offset += of(r.info())?.stride();
        self.index += 1;
        seed.deserialize(self.de.of_ref(at, r, None)?).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len() - self.index)
    }
}

struct FieldsAccess<'r, 'a> {
    de: De<'r, 'a>,
    fields: &'static [FieldInfo],
    offsets: Vec<usize>,
    index: usize,
}

impl<'r, 'a> FieldsAccess<'r, 'a> {
    fn new(de: De<'r, 'a>, fields: &'static [FieldInfo]) -> Result<Self, WireError> {
        Ok(Self {
            offsets: offsets(fields.iter().map(|f| f.ty))?,
            de,
            fields,
            index: 0,
        })
    }
}

impl<'de> de::MapAccess<'de> for FieldsAccess<'_, 'de> {
    type Error = WireError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, WireError> {
        let Some(field) = self.fields.get(self.index) else {
            return Ok(None);
        };
        seed.deserialize(field.name.into_deserializer()).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, WireError> {
        let field = &self.fields[self.index];
        let at = self.de.at + self.offsets[self.index];
        self.index += 1;
        seed.deserialize(self.de.of_ref(at, field.ty, Some(field))?)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.fields.len() - self.index)
    }
}

struct EntriesAccess<'r, 'a> {
    de: De<'r, 'a>,
    value: TypeRef,
    stride: usize,
    count: usize,
    index: usize,
    previous: Option<&'a str>,
}

impl<'de> de::MapAccess<'de> for EntriesAccess<'_, 'de> {
    type Error = WireError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, WireError> {
        if self.index >= self.count {
            return Ok(None);
        }
        let at = self.de.at + self.index * (REF + self.stride);
        let key = self.de.r.str(at)?;
        if self.previous.is_some_and(|p| p >= key) {
            return Err(WireError::Mismatch(format!(
                "at byte {at}: map keys are not strictly ascending"
            )));
        }
        self.previous = Some(key);
        seed.deserialize(de::value::BorrowedStrDeserializer::new(key))
            .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, WireError> {
        let at = self.de.at + self.index * (REF + self.stride) + REF;
        self.index += 1;
        seed.deserialize(self.de.of_ref(at, self.value, None)?)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.count - self.index)
    }
}

struct EnumAccess<'r, 'a>(De<'r, 'a>);

impl<'r, 'de> de::EnumAccess<'de> for EnumAccess<'r, 'de> {
    type Error = WireError;
    type Variant = VariantAccess<'r, 'de>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), WireError> {
        let de = self.0;
        let Ty::Info(info) = de.ty else {
            return Err(WireError::Mismatch("expected an enum".into()));
        };
        let info = peel(info);
        let Shape::Enum { variants } = &info.shape else {
            return Err(WireError::Mismatch(format!("{} is not an enum", info.name)));
        };
        let tag = de.r.u32(de.at)?;
        let variant = variants
            .get(tag as usize)
            .ok_or_else(|| WireError::InvalidTag {
                at: de.at,
                tag,
                type_name: info.name.to_owned(),
            })?;
        let used = match &variant.shape {
            VariantShape::Unit => 0,
            VariantShape::Newtype(r) => of(r.info())?.stride(),
            VariantShape::Tuple(refs) => Ty::Items(refs).stride()?,
            VariantShape::Struct(fields) => Ty::Fields(fields).stride()?,
        };
        let payload = of(info)?.stride() - TAG;
        de.r.zeros(de.at + TAG + used, payload - used)?;
        let name = seed.deserialize(variant.name.into_deserializer())?;
        Ok((
            name,
            VariantAccess {
                de: De {
                    at: de.at + TAG,
                    ..de
                },
                shape: &variant.shape,
            },
        ))
    }
}

struct VariantAccess<'r, 'a> {
    de: De<'r, 'a>,
    shape: &'static VariantShape,
}

impl<'de> de::VariantAccess<'de> for VariantAccess<'_, 'de> {
    type Error = WireError;

    fn unit_variant(self) -> Result<(), WireError> {
        match self.shape {
            VariantShape::Unit => Ok(()),
            _ => Err(WireError::Mismatch("expected a unit variant".into())),
        }
    }

    /// Any payload can be read as one value (serde's `IgnoredAny` reads every variant this way).
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, WireError> {
        let at = self.de.at;
        match self.shape {
            VariantShape::Newtype(r) => seed.deserialize(self.de.of_ref(at, *r, None)?),
            VariantShape::Tuple(refs) => {
                seed.deserialize(self.de.child(at, Ty::Items(refs), None)?)
            }
            VariantShape::Struct(fields) => {
                seed.deserialize(self.de.child(at, Ty::Fields(fields), None)?)
            }
            VariantShape::Unit => seed.deserialize(de::value::UnitDeserializer::new()),
        }
    }

    fn tuple_variant<V: Visitor<'de>>(self, _: usize, v: V) -> Result<V::Value, WireError> {
        match self.shape {
            VariantShape::Tuple(refs) => v.visit_seq(ItemsAccess::new(self.de, Items::Refs(refs))),
            _ => Err(WireError::Mismatch("expected a tuple variant".into())),
        }
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _: &'static [&'static str],
        v: V,
    ) -> Result<V::Value, WireError> {
        match self.shape {
            VariantShape::Struct(fields) => v.visit_map(FieldsAccess::new(self.de, fields)?),
            _ => Err(WireError::Mismatch("expected a struct variant".into())),
        }
    }
}

impl de::Error for WireError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        WireError::Mismatch(msg.to_string())
    }
}

impl serde::ser::Error for WireError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        WireError::Mismatch(msg.to_string())
    }
}
