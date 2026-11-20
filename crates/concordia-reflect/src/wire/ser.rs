//! Encoding: a serde `Serializer` guided by the target type's shape, so Rust values and JSON
//! encode to the same bytes.

use serde::ser::{self, Impossible, Serialize};

use super::WireError;
use super::layout::{REF, TAG, Ty, of, offsets, peel};
use crate::info::{FieldInfo, Primitive, Shape, TypeInfo, TypeRef, VariantInfo, VariantShape};

/// A growing buffer: fixed rows first, the arena after `base`.
pub(crate) struct Writer {
    pub(crate) buf: Vec<u8>,
    base: usize,
    /// Arena data may be out of canonical order (fields or keys arrived unsorted, defaults
    /// filled late, rows written out of order); the caller re-encodes before finishing.
    pub(crate) unsorted: bool,
}

impl Writer {
    /// A writer with `fixed` zeroed bytes before the arena.
    pub(crate) fn new(fixed: usize) -> Self {
        Self {
            buf: vec![0; fixed],
            base: fixed,
            unsorted: false,
        }
    }

    pub(crate) fn put(&mut self, at: usize, bytes: &[u8]) {
        debug_assert!(
            at + bytes.len() <= self.buf.len(),
            "write inside the buffer"
        );
        self.buf[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// Reserves `len` zeroed arena bytes; returns their absolute position and arena offset.
    pub(crate) fn alloc(&mut self, len: usize) -> Result<(usize, u32), WireError> {
        let at = self.buf.len();
        let offset = u32::try_from(at - self.base).map_err(|_| too_big())?;
        u32::try_from(len).map_err(|_| too_big())?;
        self.buf.resize(at + len, 0);
        Ok((at, offset))
    }

    pub(crate) fn put_ref(&mut self, at: usize, offset: u32, len: u32) {
        self.put(at, &offset.to_le_bytes());
        self.put(at + 4, &len.to_le_bytes());
    }

    /// Stores `bytes` in the arena and a reference to them at `at`.
    pub(crate) fn put_bytes(&mut self, at: usize, bytes: &[u8]) -> Result<(), WireError> {
        let (pos, offset) = self.alloc(bytes.len())?;
        self.put(pos, bytes);
        self.put_ref(at, offset, bytes.len() as u32);
        Ok(())
    }
}

fn too_big() -> WireError {
    WireError::Mismatch("the arena exceeds 4 GiB".into())
}

fn mismatch(expected: &str, ty: Ty) -> WireError {
    WireError::Mismatch(format!("expected {expected} for {}", ty.name()))
}

/// Encodes `value` as type `info` at `at` (JSON for `Any`).
pub(crate) fn put_value<T: Serialize + ?Sized>(
    w: &mut Writer,
    at: usize,
    info: &'static TypeInfo,
    value: &T,
) -> Result<(), WireError> {
    if matches!(peel(info).shape, Shape::Any) {
        let json = serde_json::to_string(value).map_err(|e| WireError::Mismatch(e.to_string()))?;
        return w.put_bytes(at, json.as_bytes());
    }
    value.serialize(Ser {
        w,
        at,
        ty: Ty::Info(info),
    })
}

/// Serializes one value into a writer position.
pub(crate) struct Ser<'w> {
    pub(crate) w: &'w mut Writer,
    pub(crate) at: usize,
    pub(crate) ty: Ty,
}

/// A scalar from serde, before it is matched to the target primitive.
#[derive(Clone, Copy)]
enum Scalar<'s> {
    Bool(bool),
    Int(i128),
    F32(f32),
    F64(f64),
    Char(char),
    Str(&'s str),
}

impl<'w> Ser<'w> {
    /// The target type, or a mismatch if this position is a variant payload.
    fn info(&self) -> Result<&'static TypeInfo, WireError> {
        match self.ty {
            Ty::Info(info) => Ok(peel(info)),
            other => Err(mismatch("a struct or tuple", other)),
        }
    }

    /// A non-null value for an `Option` position is `Some(value)` (JSON has no `Some`).
    fn implicit_some(self) -> Self {
        if let Ty::Info(info) = self.ty
            && let Shape::Option(inner) = &peel(info).shape
        {
            self.w.put(self.at, &[1]);
            return Ser {
                w: self.w,
                at: self.at + 1,
                ty: Ty::Info(inner.info()),
            }
            .implicit_some();
        }
        self
    }

    fn scalar(self, value: Scalar<'_>) -> Result<(), WireError> {
        let this = self.implicit_some();
        this.scalar_inner(value)
    }

    fn scalar_inner(self, value: Scalar<'_>) -> Result<(), WireError> {
        let info = self.info()?;
        match (&info.shape, value) {
            (Shape::Primitive(p), v) => self.primitive(*p, v),
            (Shape::Enum { variants }, Scalar::Str(name)) => {
                let (tag, variant) = find_variant(variants, name, self.ty)?;
                if !matches!(variant.shape, VariantShape::Unit) {
                    return Err(mismatch("a payload", self.ty));
                }
                self.w.put(self.at, &tag.to_le_bytes());
                Ok(())
            }
            _ => Err(mismatch("a scalar shape", self.ty)),
        }
    }

    fn primitive(self, p: Primitive, value: Scalar<'_>) -> Result<(), WireError> {
        let (w, at, ty) = (self.w, self.at, self.ty);
        match (p, value) {
            (Primitive::Bool, Scalar::Bool(b)) => w.put(at, &[u8::from(b)]),
            (Primitive::F32, Scalar::F32(f)) => w.put(at, &f.to_le_bytes()),
            (Primitive::F32, Scalar::F64(f)) => w.put(at, &(f as f32).to_le_bytes()),
            (Primitive::F32, Scalar::Int(i)) => w.put(at, &(i as f32).to_le_bytes()),
            (Primitive::F64, Scalar::F64(f)) => w.put(at, &f.to_le_bytes()),
            (Primitive::F64, Scalar::F32(f)) => w.put(at, &f64::from(f).to_le_bytes()),
            (Primitive::F64, Scalar::Int(i)) => w.put(at, &(i as f64).to_le_bytes()),
            (Primitive::Char, Scalar::Char(c)) => w.put(at, &u32::from(c).to_le_bytes()),
            (Primitive::Char, Scalar::Str(s)) => {
                let mut chars = s.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => w.put(at, &u32::from(c).to_le_bytes()),
                    _ => return Err(mismatch("one character", ty)),
                }
            }
            (Primitive::String | Primitive::Pattern(_), Scalar::Str(s)) => {
                w.put_bytes(at, s.as_bytes())?;
            }
            (Primitive::String | Primitive::Pattern(_), Scalar::Char(c)) => {
                w.put_bytes(at, c.to_string().as_bytes())?;
            }
            (Primitive::Uuid, Scalar::Str(s)) => {
                let uuid = concordia_core::id::Uuid::parse_str(s)
                    .map_err(|e| WireError::Mismatch(format!("invalid UUID {s:?}: {e}")))?;
                w.put(at, uuid.as_bytes());
            }
            (int, Scalar::Int(i)) => put_int(w, at, int, i, ty)?,
            _ => return Err(mismatch(&format!("{p:?}"), ty)),
        }
        Ok(())
    }

    fn unit(self) -> Result<(), WireError> {
        match &self.info()?.shape {
            Shape::Unit => Ok(()),
            // JSON `null` for an absent option: the tag and payload stay zero.
            Shape::Option(_) => Ok(()),
            _ => Err(mismatch("unit", self.ty)),
        }
    }

    fn variant(&mut self, name: &str) -> Result<&'static VariantInfo, WireError> {
        if let Ty::Info(info) = self.ty
            && let Shape::Option(inner) = &peel(info).shape
        {
            self.w.put(self.at, &[1]);
            self.at += 1;
            self.ty = Ty::Info(inner.info());
            return self.variant(name);
        }
        let Shape::Enum { variants } = &self.info()?.shape else {
            return Err(mismatch("an enum", self.ty));
        };
        let (tag, variant) = find_variant(variants, name, self.ty)?;
        self.w.put(self.at, &tag.to_le_bytes());
        Ok(variant)
    }

    /// A struct-like writer over `fields` at `at`.
    fn fields(
        self,
        fields: &'static [FieldInfo],
        deny_unknown: bool,
    ) -> Result<StructSer<'w>, WireError> {
        Ok(StructSer {
            offsets: offsets(fields.iter().map(|f| f.ty))?,
            seen: vec![false; fields.len()],
            last: None,
            w: self.w,
            at: self.at,
            fields,
            deny_unknown,
            pending_key: None,
        })
    }

    /// A sequence writer for this position, given an element count when known.
    fn seq(self, len: Option<usize>) -> Result<SeqSer<'w>, WireError> {
        let this = self.implicit_some();
        this.seq_inner(len)
    }

    fn seq_inner(self, len: Option<usize>) -> Result<SeqSer<'w>, WireError> {
        let items = |refs: &'static [TypeRef]| Items::Refs(refs);
        let (at, w) = (self.at, self.w);
        let inline = |items: Items, w: &'w mut Writer| SeqSer {
            w,
            at,
            items,
            index: 0,
            offset: 0,
        };
        match self.ty {
            Ty::Items(refs) => Ok(inline(items(refs), w)),
            Ty::Fields(_) => Err(mismatch("a struct", self.ty)),
            Ty::Info(info) => match &peel(info).shape {
                Shape::TupleStruct { fields } | Shape::Tuple(fields) => {
                    Ok(inline(items(fields), w))
                }
                Shape::Array { elem, len } => Ok(inline(Items::Repeat(*elem, *len), w)),
                Shape::List(elem) => {
                    let n = len.ok_or_else(|| mismatch("a sequence of known length", self.ty))?;
                    let stride = of(elem.info())?.stride;
                    let bytes = n.checked_mul(stride).ok_or_else(too_big)?;
                    let (pos, offset) = w.alloc(bytes)?;
                    w.put_ref(at, offset, u32::try_from(n).map_err(|_| too_big())?);
                    Ok(SeqSer {
                        w,
                        at: pos,
                        items: Items::Repeat(*elem, n),
                        index: 0,
                        offset: 0,
                    })
                }
                _ => Err(mismatch("a sequence shape", Ty::Info(info))),
            },
        }
    }
}

impl<'w> Ser<'w> {
    fn strukt(self) -> Result<StructSer<'w>, WireError> {
        if let Ty::Fields(fields) = self.ty {
            return self.fields(fields, false);
        }
        match &self.info()?.shape {
            Shape::Struct {
                fields,
                deny_unknown,
            } => self.fields(fields, *deny_unknown),
            _ => Err(mismatch("a struct", self.ty)),
        }
    }

    fn map(self, len: Option<usize>) -> Result<MapSer<'w>, WireError> {
        if let Ty::Fields(fields) = self.ty {
            return Ok(MapSer::Fields(self.fields(fields, false)?));
        }
        let info = self.info()?;
        match &info.shape {
            Shape::Struct {
                fields,
                deny_unknown,
            } => Ok(MapSer::Fields(self.fields(fields, *deny_unknown)?)),
            Shape::Enum { .. } => Ok(MapSer::Variant {
                ser: Some(self),
                variant: None,
            }),
            Shape::Map { value } => {
                let count = len.ok_or_else(|| mismatch("a map of known length", self.ty))?;
                let stride = of(value.info())?.stride();
                let bytes = count.checked_mul(REF + stride).ok_or_else(too_big)?;
                let (pos, offset) = self.w.alloc(bytes)?;
                self.w.put_ref(
                    self.at,
                    offset,
                    u32::try_from(count).map_err(|_| too_big())?,
                );
                Ok(MapSer::Entries {
                    w: self.w,
                    at: pos,
                    value: *value,
                    stride,
                    count,
                    index: 0,
                    key: None,
                    previous: None,
                })
            }
            _ => Err(mismatch("a map shape", self.ty)),
        }
    }
}

fn find_variant(
    variants: &'static [VariantInfo],
    name: &str,
    ty: Ty,
) -> Result<(u32, &'static VariantInfo), WireError> {
    variants
        .iter()
        .enumerate()
        .find(|(_, v)| v.name == name)
        .map(|(i, v)| (i as u32, v))
        .ok_or_else(|| WireError::Mismatch(format!("{} has no variant {name:?}", ty.name())))
}

fn put_int(w: &mut Writer, at: usize, p: Primitive, i: i128, ty: Ty) -> Result<(), WireError> {
    let Some((min, max)) = p.int_bounds() else {
        return Err(mismatch(&format!("{p:?}"), ty));
    };
    if i < min || i > max {
        return Err(WireError::Mismatch(format!(
            "{i} does not fit {p:?} in {}",
            ty.name()
        )));
    }
    let bytes = i.to_le_bytes();
    let width = super::layout::primitive(p).stride();
    w.put(at, &bytes[..width]);
    Ok(())
}

/// Positional members of a sequence position.
#[derive(Clone, Copy)]
enum Items {
    /// Different types, in order (tuples, tuple structs, tuple variants).
    Refs(&'static [TypeRef]),
    /// One type `n` times (arrays, list elements).
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

/// Writes sequence members one after another.
pub(crate) struct SeqSer<'w> {
    w: &'w mut Writer,
    at: usize,
    items: Items,
    index: usize,
    offset: usize,
}

impl SeqSer<'_> {
    fn push<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), WireError> {
        if self.index >= self.items.len() {
            return Err(WireError::Mismatch(format!(
                "too many elements: expected {}",
                self.items.len()
            )));
        }
        let ty = self.items.get(self.index).info();
        put_value(self.w, self.at + self.offset, ty, value)?;
        self.offset += of(ty)?.stride();
        self.index += 1;
        Ok(())
    }

    fn finish(self) -> Result<(), WireError> {
        if self.index == self.items.len() {
            Ok(())
        } else {
            Err(WireError::Mismatch(format!(
                "expected {} elements, found {}",
                self.items.len(),
                self.index
            )))
        }
    }
}

/// Writes named fields at their fixed offsets, in any order.
pub(crate) struct StructSer<'w> {
    w: &'w mut Writer,
    at: usize,
    fields: &'static [FieldInfo],
    offsets: Vec<usize>,
    seen: Vec<bool>,
    last: Option<usize>,
    deny_unknown: bool,
    /// A map key waiting for its value (JSON objects arrive as maps).
    pending_key: Option<String>,
}

impl StructSer<'_> {
    fn field<T: Serialize + ?Sized>(&mut self, key: &str, value: &T) -> Result<(), WireError> {
        let Some(index) = self.fields.iter().position(|f| f.name == key) else {
            if self.deny_unknown {
                return Err(WireError::Mismatch(format!("unknown field {key:?}")));
            }
            return Ok(());
        };
        if std::mem::replace(&mut self.seen[index], true) {
            return Err(WireError::Mismatch(format!("duplicate field {key:?}")));
        }
        if self.last.is_some_and(|last| index < last) {
            self.w.unsorted = true;
        }
        self.last = Some(index);
        put_value(
            self.w,
            self.at + self.offsets[index],
            self.fields[index].ty.info(),
            value,
        )
    }

    fn finish(self) -> Result<(), WireError> {
        for (index, field) in self.fields.iter().enumerate() {
            if self.seen[index] {
                continue;
            }
            let at = self.at + self.offsets[index];
            match field.default {
                Some(default) => {
                    self.w.unsorted = true;
                    put_value(self.w, at, field.ty.info(), &default.get())?;
                }
                // Absent optional fields are zero: `None`, `0`, `false`, empty.
                None if field.optional => {}
                None => {
                    return Err(WireError::Mismatch(format!(
                        "missing field {:?}",
                        field.name
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Writes a map position: a string-keyed map, a struct given as a JSON object, or an
/// externally tagged enum given as `{"Variant": payload}`.
pub(crate) enum MapSer<'w> {
    /// A `Map` shape: entries in an arena region.
    Entries {
        w: &'w mut Writer,
        at: usize,
        value: TypeRef,
        stride: usize,
        count: usize,
        index: usize,
        key: Option<String>,
        previous: Option<String>,
    },
    /// A struct (or struct variant) given as a map.
    Fields(StructSer<'w>),
    /// An enum given as a one-entry map.
    Variant {
        ser: Option<Ser<'w>>,
        variant: Option<&'static VariantInfo>,
    },
}

impl ser::SerializeMap for MapSer<'_> {
    type Ok = ();
    type Error = WireError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), WireError> {
        let key = key.serialize(KeyCapture)?;
        match self {
            MapSer::Entries { key: slot, .. } => *slot = Some(key),
            MapSer::Fields(s) => s.pending_key = Some(key),
            MapSer::Variant { ser, variant } => {
                let ser = ser
                    .as_mut()
                    .ok_or_else(|| WireError::Mismatch("an enum map has one entry".into()))?;
                *variant = Some(ser.variant(&key)?);
            }
        }
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), WireError> {
        match self {
            MapSer::Entries {
                w,
                at,
                value: ty,
                stride,
                count,
                index,
                key,
                previous,
            } => {
                let key = key
                    .take()
                    .ok_or_else(|| WireError::Mismatch("map value without a key".into()))?;
                if *index >= *count {
                    return Err(WireError::Mismatch("too many map entries".into()));
                }
                if let Some(prev) = previous.as_deref() {
                    if prev == key {
                        return Err(WireError::Mismatch(format!("duplicate key {key:?}")));
                    }
                    if prev > key.as_str() {
                        w.unsorted = true;
                    }
                }
                let entry = *at + *index * (REF + *stride);
                w.put_bytes(entry, key.as_bytes())?;
                put_value(w, entry + REF, ty.info(), value)?;
                *index += 1;
                *previous = Some(key);
                Ok(())
            }
            MapSer::Fields(s) => {
                let key = s
                    .pending_key
                    .take()
                    .ok_or_else(|| WireError::Mismatch("struct value without a key".into()))?;
                s.field(&key, value)
            }
            MapSer::Variant { ser, variant } => {
                let (ser, variant) = match (ser.take(), variant.take()) {
                    (Some(ser), Some(variant)) => (ser, variant),
                    _ => return Err(WireError::Mismatch("enum payload without a variant".into())),
                };
                payload(ser.w, ser.at + TAG, variant, value)
            }
        }
    }

    fn end(self) -> Result<(), WireError> {
        match self {
            MapSer::Entries { count, index, .. } if count != index => Err(WireError::Mismatch(
                format!("expected {count} map entries, found {index}"),
            )),
            MapSer::Entries { .. } => Ok(()),
            MapSer::Fields(s) => s.finish(),
            MapSer::Variant { ser: None, .. } => Ok(()),
            MapSer::Variant { .. } => {
                Err(WireError::Mismatch("an enum map needs one entry".into()))
            }
        }
    }
}

/// Writes an enum payload given as one serde value (from a JSON `{"Variant": payload}`).
fn payload<T: Serialize + ?Sized>(
    w: &mut Writer,
    at: usize,
    variant: &'static VariantInfo,
    value: &T,
) -> Result<(), WireError> {
    let ty = match &variant.shape {
        VariantShape::Unit => {
            return Err(WireError::Mismatch(format!(
                "{} has no payload",
                variant.name
            )));
        }
        VariantShape::Newtype(r) => return put_value(w, at, r.info(), value),
        VariantShape::Tuple(refs) => Ty::Items(refs),
        VariantShape::Struct(fields) => Ty::Fields(fields),
    };
    value.serialize(Ser { w, at, ty })
}

impl<'w> ser::Serializer for Ser<'w> {
    type Ok = ();
    type Error = WireError;
    type SerializeSeq = SeqSer<'w>;
    type SerializeTuple = SeqSer<'w>;
    type SerializeTupleStruct = SeqSer<'w>;
    type SerializeTupleVariant = SeqSer<'w>;
    type SerializeMap = MapSer<'w>;
    type SerializeStruct = StructSer<'w>;
    type SerializeStructVariant = StructSer<'w>;

    fn serialize_bool(self, v: bool) -> Result<(), WireError> {
        self.scalar(Scalar::Bool(v))
    }
    fn serialize_i8(self, v: i8) -> Result<(), WireError> {
        self.scalar(Scalar::Int(v.into()))
    }
    fn serialize_i16(self, v: i16) -> Result<(), WireError> {
        self.scalar(Scalar::Int(v.into()))
    }
    fn serialize_i32(self, v: i32) -> Result<(), WireError> {
        self.scalar(Scalar::Int(v.into()))
    }
    fn serialize_i64(self, v: i64) -> Result<(), WireError> {
        self.scalar(Scalar::Int(v.into()))
    }
    fn serialize_i128(self, v: i128) -> Result<(), WireError> {
        self.scalar(Scalar::Int(v))
    }
    fn serialize_u8(self, v: u8) -> Result<(), WireError> {
        self.scalar(Scalar::Int(v.into()))
    }
    fn serialize_u16(self, v: u16) -> Result<(), WireError> {
        self.scalar(Scalar::Int(v.into()))
    }
    fn serialize_u32(self, v: u32) -> Result<(), WireError> {
        self.scalar(Scalar::Int(v.into()))
    }
    fn serialize_u64(self, v: u64) -> Result<(), WireError> {
        self.scalar(Scalar::Int(v.into()))
    }
    fn serialize_u128(self, v: u128) -> Result<(), WireError> {
        let v = i128::try_from(v).map_err(|_| WireError::Mismatch(format!("{v} is too large")))?;
        self.scalar(Scalar::Int(v))
    }
    fn serialize_f32(self, v: f32) -> Result<(), WireError> {
        self.scalar(Scalar::F32(v))
    }
    fn serialize_f64(self, v: f64) -> Result<(), WireError> {
        self.scalar(Scalar::F64(v))
    }
    fn serialize_char(self, v: char) -> Result<(), WireError> {
        self.scalar(Scalar::Char(v))
    }
    fn serialize_str(self, v: &str) -> Result<(), WireError> {
        self.scalar(Scalar::Str(v))
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), WireError> {
        Err(mismatch("no raw bytes", self.ty))
    }
    fn serialize_none(self) -> Result<(), WireError> {
        match &self.info()?.shape {
            Shape::Option(_) => Ok(()),
            _ => Err(mismatch("an option", self.ty)),
        }
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), WireError> {
        let Shape::Option(inner) = &self.info()?.shape else {
            return Err(mismatch("an option", self.ty));
        };
        self.w.put(self.at, &[1]);
        put_value(self.w, self.at + 1, inner.info(), value)
    }
    fn serialize_unit(self) -> Result<(), WireError> {
        self.unit()
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), WireError> {
        self.unit()
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), WireError> {
        self.scalar(Scalar::Str(variant))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), WireError> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        mut self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), WireError> {
        let info = self.variant(variant)?;
        payload(self.w, self.at + TAG, info, value)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<SeqSer<'w>, WireError> {
        self.seq(len)
    }
    fn serialize_tuple(self, len: usize) -> Result<SeqSer<'w>, WireError> {
        self.seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<SeqSer<'w>, WireError> {
        self.seq(Some(len))
    }
    fn serialize_tuple_variant(
        mut self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<SeqSer<'w>, WireError> {
        let info = self.variant(variant)?;
        let VariantShape::Tuple(refs) = &info.shape else {
            return Err(mismatch("a tuple variant", self.ty));
        };
        Ser {
            w: self.w,
            at: self.at + TAG,
            ty: Ty::Items(refs),
        }
        .seq(Some(len))
    }
    fn serialize_map(self, len: Option<usize>) -> Result<MapSer<'w>, WireError> {
        let self_ = self.implicit_some();
        self_.map(len)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<StructSer<'w>, WireError> {
        self.implicit_some().strukt()
    }

    fn serialize_struct_variant(
        mut self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<StructSer<'w>, WireError> {
        let info = self.variant(variant)?;
        let VariantShape::Struct(fields) = &info.shape else {
            return Err(mismatch("a struct variant", self.ty));
        };
        Ser {
            w: self.w,
            at: self.at + TAG,
            ty: Ty::Fields(fields),
        }
        .fields(fields, false)
    }
}

impl ser::SerializeSeq for SeqSer<'_> {
    type Ok = ();
    type Error = WireError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), WireError> {
        self.push(value)
    }
    fn end(self) -> Result<(), WireError> {
        self.finish()
    }
}

impl ser::SerializeTuple for SeqSer<'_> {
    type Ok = ();
    type Error = WireError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), WireError> {
        self.push(value)
    }
    fn end(self) -> Result<(), WireError> {
        self.finish()
    }
}

impl ser::SerializeTupleStruct for SeqSer<'_> {
    type Ok = ();
    type Error = WireError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), WireError> {
        self.push(value)
    }
    fn end(self) -> Result<(), WireError> {
        self.finish()
    }
}

impl ser::SerializeTupleVariant for SeqSer<'_> {
    type Ok = ();
    type Error = WireError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), WireError> {
        self.push(value)
    }
    fn end(self) -> Result<(), WireError> {
        self.finish()
    }
}

impl ser::SerializeStruct for StructSer<'_> {
    type Ok = ();
    type Error = WireError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), WireError> {
        self.field(key, value)
    }
    fn skip_field(&mut self, _: &'static str) -> Result<(), WireError> {
        Ok(())
    }
    fn end(self) -> Result<(), WireError> {
        self.finish()
    }
}

impl ser::SerializeStructVariant for StructSer<'_> {
    type Ok = ();
    type Error = WireError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), WireError> {
        self.field(key, value)
    }
    fn end(self) -> Result<(), WireError> {
        self.finish()
    }
}

/// Captures a map key, which must be a string.
struct KeyCapture;

fn not_a_key() -> WireError {
    WireError::Mismatch("map keys must be strings".into())
}

impl ser::Serializer for KeyCapture {
    type Ok = String;
    type Error = WireError;
    type SerializeSeq = Impossible<String, WireError>;
    type SerializeTuple = Impossible<String, WireError>;
    type SerializeTupleStruct = Impossible<String, WireError>;
    type SerializeTupleVariant = Impossible<String, WireError>;
    type SerializeMap = Impossible<String, WireError>;
    type SerializeStruct = Impossible<String, WireError>;
    type SerializeStructVariant = Impossible<String, WireError>;

    fn serialize_str(self, v: &str) -> Result<String, WireError> {
        Ok(v.to_owned())
    }
    fn serialize_char(self, v: char) -> Result<String, WireError> {
        Ok(v.to_string())
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        v: &'static str,
    ) -> Result<String, WireError> {
        Ok(v.to_owned())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<String, WireError> {
        v.serialize(self)
    }
    fn serialize_bool(self, _: bool) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_i8(self, _: i8) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_i16(self, _: i16) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_i32(self, _: i32) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_i64(self, _: i64) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_u8(self, _: u8) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_u16(self, _: u16) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_u32(self, _: u32) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_u64(self, _: u64) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_f32(self, _: f32) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_f64(self, _: f64) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_none(self) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_unit(self) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<String, WireError> {
        Err(not_a_key())
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, WireError> {
        Err(not_a_key())
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, WireError> {
        Err(not_a_key())
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, WireError> {
        Err(not_a_key())
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, WireError> {
        Err(not_a_key())
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, WireError> {
        Err(not_a_key())
    }
    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, WireError> {
        Err(not_a_key())
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, WireError> {
        Err(not_a_key())
    }
}
