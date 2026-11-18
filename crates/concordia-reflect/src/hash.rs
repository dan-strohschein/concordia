//! Canonical hashing of reflected values.
//!
//! Values are encoded as tagged, length-prefixed bytes: floats by their bits
//! (so `-0.0 != 0.0`), integers widened to `i128`, struct fields and map
//! entries sorted by key (so declaration order never matters), type names
//! excluded (so renames do not change hashes). The encoding is versioned by
//! the `canonical_hash_stable_snapshot` test.

use serde::ser::{self, Serialize};

use crate::dynamic::DynReflect;
use crate::error::ReflectError;

/// Feeds the canonical encoding of `value` into `hasher`.
pub fn canonical_hash(
    value: &dyn DynReflect,
    hasher: &mut blake3::Hasher,
) -> Result<(), ReflectError> {
    let bytes = encode(value.as_serialize()).map_err(|e| ReflectError::Serialize {
        type_name: value.info().name.to_owned(),
        message: e.0,
    })?;
    hasher.update(&bytes);
    Ok(())
}

fn encode<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    value.serialize(Canon(&mut out))?;
    Ok(out)
}

mod tag {
    pub const BOOL: u8 = 1;
    pub const INT: u8 = 2;
    pub const F32: u8 = 3;
    pub const F64: u8 = 4;
    pub const CHAR: u8 = 5;
    pub const STR: u8 = 6;
    pub const BYTES: u8 = 7;
    pub const NONE: u8 = 8;
    pub const SOME: u8 = 9;
    pub const UNIT: u8 = 10;
    pub const UNIT_VARIANT: u8 = 11;
    pub const NEWTYPE_VARIANT: u8 = 12;
    pub const SEQ: u8 = 13;
    pub const TUPLE_VARIANT: u8 = 14;
    pub const MAP: u8 = 15;
    pub const STRUCT: u8 = 16;
    pub const STRUCT_VARIANT: u8 = 17;
}

/// Serialization error carrying serde's message.
#[derive(Debug)]
struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}

struct Canon<'a>(&'a mut Vec<u8>);

impl Canon<'_> {
    fn tag(&mut self, tag: u8) {
        self.0.push(tag);
    }

    fn len(&mut self, len: usize) {
        self.0.extend_from_slice(&(len as u64).to_le_bytes());
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.len(bytes.len());
        self.0.extend_from_slice(bytes);
    }

    fn int(self, v: i128) -> Result<(), Error> {
        self.0.push(tag::INT);
        self.0.extend_from_slice(&v.to_le_bytes());
        Ok(())
    }
}

/// Collects sequence items in order.
struct Seq<'a> {
    out: &'a mut Vec<u8>,
    items: Vec<u8>,
    count: usize,
}

impl Seq<'_> {
    fn push<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(Canon(&mut self.items))?;
        self.count += 1;
        Ok(())
    }

    fn finish(self) -> Result<(), Error> {
        let mut canon = Canon(self.out);
        canon.len(self.count);
        canon.0.extend_from_slice(&self.items);
        Ok(())
    }
}

/// Collects keyed entries, then emits them sorted by encoded key.
struct Sorted<'a> {
    out: &'a mut Vec<u8>,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    pending_key: Option<Vec<u8>>,
}

impl Sorted<'_> {
    fn entry<T: Serialize + ?Sized>(&mut self, key: Vec<u8>, value: &T) -> Result<(), Error> {
        self.entries.push((key, encode(value)?));
        Ok(())
    }

    fn finish(mut self) -> Result<(), Error> {
        self.entries.sort();
        let mut canon = Canon(self.out);
        canon.len(self.entries.len());
        for (key, value) in &self.entries {
            canon.bytes(key);
            canon.bytes(value);
        }
        Ok(())
    }
}

fn field_key(name: &str) -> Vec<u8> {
    name.as_bytes().to_vec()
}

impl<'a> ser::Serializer for Canon<'a> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Seq<'a>;
    type SerializeTuple = Seq<'a>;
    type SerializeTupleStruct = Seq<'a>;
    type SerializeTupleVariant = Seq<'a>;
    type SerializeMap = Sorted<'a>;
    type SerializeStruct = Sorted<'a>;
    type SerializeStructVariant = Sorted<'a>;

    fn serialize_bool(mut self, v: bool) -> Result<(), Error> {
        self.tag(tag::BOOL);
        self.0.push(u8::from(v));
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<(), Error> {
        self.int(v.into())
    }
    fn serialize_i16(self, v: i16) -> Result<(), Error> {
        self.int(v.into())
    }
    fn serialize_i32(self, v: i32) -> Result<(), Error> {
        self.int(v.into())
    }
    fn serialize_i64(self, v: i64) -> Result<(), Error> {
        self.int(v.into())
    }
    fn serialize_i128(self, v: i128) -> Result<(), Error> {
        self.int(v)
    }
    fn serialize_u8(self, v: u8) -> Result<(), Error> {
        self.int(v.into())
    }
    fn serialize_u16(self, v: u16) -> Result<(), Error> {
        self.int(v.into())
    }
    fn serialize_u32(self, v: u32) -> Result<(), Error> {
        self.int(v.into())
    }
    fn serialize_u64(self, v: u64) -> Result<(), Error> {
        self.int(v.into())
    }
    fn serialize_f32(mut self, v: f32) -> Result<(), Error> {
        self.tag(tag::F32);
        self.0.extend_from_slice(&v.to_bits().to_le_bytes());
        Ok(())
    }
    fn serialize_f64(mut self, v: f64) -> Result<(), Error> {
        self.tag(tag::F64);
        self.0.extend_from_slice(&v.to_bits().to_le_bytes());
        Ok(())
    }
    fn serialize_char(mut self, v: char) -> Result<(), Error> {
        self.tag(tag::CHAR);
        self.0.extend_from_slice(&u32::from(v).to_le_bytes());
        Ok(())
    }
    fn serialize_str(mut self, v: &str) -> Result<(), Error> {
        self.tag(tag::STR);
        self.bytes(v.as_bytes());
        Ok(())
    }
    fn serialize_bytes(mut self, v: &[u8]) -> Result<(), Error> {
        self.tag(tag::BYTES);
        self.bytes(v);
        Ok(())
    }
    fn serialize_none(mut self) -> Result<(), Error> {
        self.tag(tag::NONE);
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(mut self, value: &T) -> Result<(), Error> {
        self.tag(tag::SOME);
        value.serialize(self)
    }
    fn serialize_unit(mut self) -> Result<(), Error> {
        self.tag(tag::UNIT);
        Ok(())
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), Error> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(
        mut self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), Error> {
        self.tag(tag::UNIT_VARIANT);
        self.bytes(variant.as_bytes());
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        mut self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.tag(tag::NEWTYPE_VARIANT);
        self.bytes(variant.as_bytes());
        value.serialize(self)
    }
    fn serialize_seq(mut self, _: Option<usize>) -> Result<Seq<'a>, Error> {
        self.tag(tag::SEQ);
        Ok(Seq {
            out: self.0,
            items: Vec::new(),
            count: 0,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Seq<'a>, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<Seq<'a>, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        mut self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Seq<'a>, Error> {
        self.tag(tag::TUPLE_VARIANT);
        self.bytes(variant.as_bytes());
        self.serialize_seq(Some(len))
    }
    fn serialize_map(mut self, _: Option<usize>) -> Result<Sorted<'a>, Error> {
        self.tag(tag::MAP);
        Ok(sorted(self.0))
    }
    fn serialize_struct(mut self, _: &'static str, _: usize) -> Result<Sorted<'a>, Error> {
        self.tag(tag::STRUCT);
        Ok(sorted(self.0))
    }
    fn serialize_struct_variant(
        mut self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Sorted<'a>, Error> {
        self.tag(tag::STRUCT_VARIANT);
        self.bytes(variant.as_bytes());
        Ok(sorted(self.0))
    }
}

fn sorted(out: &mut Vec<u8>) -> Sorted<'_> {
    Sorted {
        out,
        entries: Vec::new(),
        pending_key: None,
    }
}

macro_rules! seq_impl {
    ($trait:ident, $method:ident) => {
        impl ser::$trait for Seq<'_> {
            type Ok = ();
            type Error = Error;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
                self.push(value)
            }
            fn end(self) -> Result<(), Error> {
                self.finish()
            }
        }
    };
}

seq_impl!(SerializeSeq, serialize_element);
seq_impl!(SerializeTuple, serialize_element);
seq_impl!(SerializeTupleStruct, serialize_field);
seq_impl!(SerializeTupleVariant, serialize_field);

impl ser::SerializeMap for Sorted<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        self.pending_key = Some(encode(key)?);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        let key = self
            .pending_key
            .take()
            .ok_or_else(|| Error("map value serialized before its key".into()))?;
        self.entry(key, value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}

macro_rules! struct_impl {
    ($trait:ident) => {
        impl ser::$trait for Sorted<'_> {
            type Ok = ();
            type Error = Error;
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), Error> {
                self.entry(field_key(key), value)
            }
            fn end(self) -> Result<(), Error> {
                self.finish()
            }
        }
    };
}

struct_impl!(SerializeStruct);
struct_impl!(SerializeStructVariant);
