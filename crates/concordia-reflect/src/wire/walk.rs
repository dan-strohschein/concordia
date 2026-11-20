//! Walking encoded values: serializing them like the Rust type of the same shape would, and
//! copying them canonically (into columns, out of columns, or to re-order arena data).

use serde::ser::{self, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeTuple};

use super::WireError;
use super::de::{Reader, Scalar, scalar, uuid_text};
use super::layout::{REF, TAG, Ty, of, offsets, peel};
use super::ser::Writer;
use crate::info::{FieldInfo, Shape, TypeInfo, TypeRef, VariantShape};

const MAX_DEPTH: usize = 128;

/// An encoded value, serializable as the type it encodes.
pub(crate) struct Show<'r, 'a> {
    pub(crate) r: &'r Reader<'a>,
    pub(crate) at: usize,
    pub(crate) info: &'static TypeInfo,
    pub(crate) depth: usize,
}

impl<'r, 'a> Show<'r, 'a> {
    fn child(&self, at: usize, r: TypeRef) -> Show<'r, 'a> {
        Show {
            r: self.r,
            at,
            info: r.info(),
            depth: self.depth + 1,
        }
    }
}

fn err<E: ser::Error>(e: WireError) -> E {
    E::custom(e)
}

impl Serialize for Show<'_, '_> {
    fn serialize<S: ser::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.depth >= MAX_DEPTH {
            return Err(err(WireError::Mismatch("nesting too deep".into())));
        }
        let info = self.info;
        match &info.shape {
            Shape::Primitive(p) => {
                match scalar(self.r, self.at, *p, None, info.name).map_err(err)? {
                    Scalar::Bool(b) => s.serialize_bool(b),
                    Scalar::I64(i) => match p {
                        crate::Primitive::I8 => s.serialize_i8(i as i8),
                        crate::Primitive::I16 => s.serialize_i16(i as i16),
                        crate::Primitive::I32 => s.serialize_i32(i as i32),
                        _ => s.serialize_i64(i),
                    },
                    Scalar::U64(u) => match p {
                        crate::Primitive::U8 => s.serialize_u8(u as u8),
                        crate::Primitive::U16 => s.serialize_u16(u as u16),
                        crate::Primitive::U32 | crate::Primitive::NonZeroU32 => {
                            s.serialize_u32(u as u32)
                        }
                        _ => s.serialize_u64(u),
                    },
                    Scalar::F32(f) => s.serialize_f32(f),
                    Scalar::F64(f) => s.serialize_f64(f),
                    Scalar::Char(c) => s.serialize_char(c),
                    Scalar::Str(text) => s.serialize_str(text),
                    Scalar::Uuid(bytes) => s.serialize_str(&uuid_text(bytes)),
                }
            }
            Shape::Struct { fields, .. } => {
                let mut st = s.serialize_struct(info.name, fields.len())?;
                let offs = offsets(fields.iter().map(|f| f.ty)).map_err(err)?;
                for (f, off) in fields.iter().zip(offs) {
                    st.serialize_field(f.name, &self.child(self.at + off, f.ty))?;
                }
                st.end()
            }
            Shape::TupleStruct { fields } => {
                use ser::SerializeTupleStruct;
                let mut st = s.serialize_tuple_struct(info.name, fields.len())?;
                for (r, off) in refs_at(fields, self.at).map_err(err)? {
                    st.serialize_field(&self.child(off, r))?;
                }
                st.end()
            }
            Shape::Tuple(fields) => {
                let mut st = s.serialize_tuple(fields.len())?;
                for (r, off) in refs_at(fields, self.at).map_err(err)? {
                    st.serialize_element(&self.child(off, r))?;
                }
                st.end()
            }
            Shape::Array { elem, len } => {
                let stride = of(elem.info()).map_err(err)?.stride();
                let mut st = s.serialize_tuple(*len)?;
                for i in 0..*len {
                    st.serialize_element(&self.child(self.at + i * stride, *elem))?;
                }
                st.end()
            }
            Shape::Newtype {
                inner,
                transparent: true,
            } => self.child(self.at, *inner).serialize(s),
            Shape::Newtype { inner, .. } => {
                s.serialize_newtype_struct(info.name, &self.child(self.at, *inner))
            }
            Shape::Unit => s.serialize_unit_struct(info.name),
            Shape::Enum { variants } => {
                let tag = self.r.u32(self.at).map_err(err)?;
                let v = variants.get(tag as usize).ok_or_else(|| {
                    err(WireError::InvalidTag {
                        at: self.at,
                        tag,
                        type_name: info.name.to_owned(),
                    })
                })?;
                let at = self.at + TAG;
                match &v.shape {
                    VariantShape::Unit => s.serialize_unit_variant(info.name, tag, v.name),
                    VariantShape::Newtype(r) => {
                        s.serialize_newtype_variant(info.name, tag, v.name, &self.child(at, *r))
                    }
                    VariantShape::Tuple(refs) => {
                        use ser::SerializeTupleVariant;
                        let mut st =
                            s.serialize_tuple_variant(info.name, tag, v.name, refs.len())?;
                        for (r, off) in refs_at(refs, at).map_err(err)? {
                            st.serialize_field(&self.child(off, r))?;
                        }
                        st.end()
                    }
                    VariantShape::Struct(fields) => {
                        use ser::SerializeStructVariant;
                        let mut st =
                            s.serialize_struct_variant(info.name, tag, v.name, fields.len())?;
                        let offs = offsets(fields.iter().map(|f| f.ty)).map_err(err)?;
                        for (f, off) in fields.iter().zip(offs) {
                            st.serialize_field(f.name, &self.child(at + off, f.ty))?;
                        }
                        st.end()
                    }
                }
            }
            Shape::Option(inner) => match self.r.bytes(self.at, 1).map_err(err)?[0] {
                0 => s.serialize_none(),
                _ => s.serialize_some(&self.child(self.at + 1, *inner)),
            },
            Shape::List(elem) => {
                let stride = of(elem.info()).map_err(err)?.stride();
                let (pos, count) = self.r.region(self.at, stride).map_err(err)?;
                let mut st = s.serialize_seq(Some(count))?;
                for i in 0..count {
                    st.serialize_element(&self.child(pos + i * stride, *elem))?;
                }
                st.end()
            }
            Shape::Map { value } => {
                let stride = of(value.info()).map_err(err)?.stride();
                let (pos, count) = self.r.region(self.at, REF + stride).map_err(err)?;
                let mut st = s.serialize_map(Some(count))?;
                for i in 0..count {
                    let entry = pos + i * (REF + stride);
                    st.serialize_entry(
                        self.r.str(entry).map_err(err)?,
                        &self.child(entry + REF, *value),
                    )?;
                }
                st.end()
            }
            Shape::Any => {
                let text = self.r.str(self.at).map_err(err)?;
                let json: serde_json::Value = serde_json::from_str(text)
                    .map_err(|e| err(WireError::Mismatch(e.to_string())))?;
                json.serialize(s)
            }
        }
    }
}

/// Each reference with its absolute position, laid out from `at`.
fn refs_at(refs: &[TypeRef], at: usize) -> Result<Vec<(TypeRef, usize)>, WireError> {
    let offs = offsets(refs.iter().copied())?;
    Ok(refs
        .iter()
        .copied()
        .zip(offs.into_iter().map(|o| at + o))
        .collect())
}

/// Copies the value of `ty` at `from` in `r` to `to` in `w`, laying out its arena data in
/// canonical order (map entries sorted by key).
pub(crate) fn copy(
    r: &Reader<'_>,
    from: usize,
    w: &mut Writer,
    to: usize,
    ty: Ty,
    depth: usize,
) -> Result<(), WireError> {
    if depth >= MAX_DEPTH {
        return Err(WireError::Mismatch("nesting too deep".into()));
    }
    let info = match ty {
        Ty::Fields(fields) => return copy_fields(r, from, w, to, fields, depth),
        Ty::Items(refs) => return copy_refs(r, from, w, to, refs, depth),
        Ty::Info(info) => peel(info),
    };
    match &info.shape {
        Shape::Primitive(crate::Primitive::String | crate::Primitive::Pattern(_)) | Shape::Any => {
            w.put_bytes(to, r.str(from)?.as_bytes())
        }
        Shape::Primitive(_) => {
            let n = of(info)?.stride();
            w.put(to, r.bytes(from, n)?);
            Ok(())
        }
        Shape::Struct { fields, .. } => copy_fields(r, from, w, to, fields, depth),
        Shape::TupleStruct { fields } | Shape::Tuple(fields) => {
            copy_refs(r, from, w, to, fields, depth)
        }
        Shape::Array { elem, len } => {
            let stride = of(elem.info())?.stride();
            for i in 0..*len {
                copy(
                    r,
                    from + i * stride,
                    w,
                    to + i * stride,
                    Ty::Info(elem.info()),
                    depth + 1,
                )?;
            }
            Ok(())
        }
        Shape::Newtype { .. } | Shape::Unit => Ok(()),
        Shape::Enum { variants } => {
            let tag = r.u32(from)?;
            let v = variants
                .get(tag as usize)
                .ok_or_else(|| WireError::InvalidTag {
                    at: from,
                    tag,
                    type_name: info.name.to_owned(),
                })?;
            w.put(to, &tag.to_le_bytes());
            let ty = match &v.shape {
                VariantShape::Unit => return Ok(()),
                VariantShape::Newtype(r) => Ty::Info(r.info()),
                VariantShape::Tuple(refs) => Ty::Items(refs),
                VariantShape::Struct(fields) => Ty::Fields(fields),
            };
            copy(r, from + TAG, w, to + TAG, ty, depth + 1)
        }
        Shape::Option(inner) => match r.bytes(from, 1)?[0] {
            0 => Ok(()),
            1 => {
                w.put(to, &[1]);
                copy(r, from + 1, w, to + 1, Ty::Info(inner.info()), depth + 1)
            }
            tag => Err(WireError::InvalidTag {
                at: from,
                tag: tag.into(),
                type_name: info.name.to_owned(),
            }),
        },
        Shape::List(elem) => {
            let stride = of(elem.info())?.stride();
            let (src, count) = r.region(from, stride)?;
            let (dst, offset) = w.alloc(count * stride)?;
            w.put_ref(to, offset, count as u32);
            for i in 0..count {
                copy(
                    r,
                    src + i * stride,
                    w,
                    dst + i * stride,
                    Ty::Info(elem.info()),
                    depth + 1,
                )?;
            }
            Ok(())
        }
        Shape::Map { value } => {
            let stride = of(value.info())?.stride();
            let (src, count) = r.region(from, REF + stride)?;
            let mut entries = (0..count)
                .map(|i| {
                    let at = src + i * (REF + stride);
                    Ok((r.str(at)?, at))
                })
                .collect::<Result<Vec<_>, WireError>>()?;
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let (dst, offset) = w.alloc(count * (REF + stride))?;
            w.put_ref(to, offset, count as u32);
            for (i, (key, at)) in entries.into_iter().enumerate() {
                let entry = dst + i * (REF + stride);
                w.put_bytes(entry, key.as_bytes())?;
                copy(
                    r,
                    at + REF,
                    w,
                    entry + REF,
                    Ty::Info(value.info()),
                    depth + 1,
                )?;
            }
            Ok(())
        }
    }
}

fn copy_fields(
    r: &Reader<'_>,
    from: usize,
    w: &mut Writer,
    to: usize,
    fields: &'static [FieldInfo],
    depth: usize,
) -> Result<(), WireError> {
    let offs = offsets(fields.iter().map(|f| f.ty))?;
    for (f, off) in fields.iter().zip(offs) {
        copy(r, from + off, w, to + off, Ty::Info(f.ty.info()), depth + 1)?;
    }
    Ok(())
}

fn copy_refs(
    r: &Reader<'_>,
    from: usize,
    w: &mut Writer,
    to: usize,
    refs: &'static [TypeRef],
    depth: usize,
) -> Result<(), WireError> {
    let offs = offsets(refs.iter().copied())?;
    for (t, off) in refs.iter().zip(offs) {
        copy(r, from + off, w, to + off, Ty::Info(t.info()), depth + 1)?;
    }
    Ok(())
}
