//! Finds non-finite floats (NaN, ±inf), which JSON and scene files cannot represent.

use serde::ser::{self, Serialize};

use crate::dynamic::DynReflect;

/// JSON Pointer of the first non-finite float in `value`, following serde's JSON layout.
///
/// ```
/// # use concordia_reflect::find_non_finite;
/// assert_eq!(find_non_finite(&vec![1.0f32, f32::NAN]), Some("/1".to_owned()));
/// ```
pub fn find_non_finite(value: &dyn DynReflect) -> Option<String> {
    let mut path = Vec::new();
    match value.as_serialize().serialize(Finder(&mut path)) {
        Err(Stop::Found(pointer)) => Some(pointer),
        Err(Stop::Other(_)) | Ok(()) => None,
    }
}

#[derive(Debug)]
enum Stop {
    Found(String),
    Other(String),
}

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Found(p) => write!(f, "non-finite at {p}"),
            Self::Other(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Stop {}

impl ser::Error for Stop {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self::Other(msg.to_string())
    }
}

fn pointer(path: &[String]) -> String {
    path.iter()
        .map(|t| format!("/{}", t.replace('~', "~0").replace('/', "~1")))
        .collect()
}

struct Finder<'a>(&'a mut Vec<String>);

impl Finder<'_> {
    fn float(self, finite: bool) -> Result<(), Stop> {
        if finite {
            Ok(())
        } else {
            Err(Stop::Found(pointer(self.0)))
        }
    }

    fn nested<T: Serialize + ?Sized>(&mut self, token: String, value: &T) -> Result<(), Stop> {
        self.0.push(token);
        let result = value.serialize(Finder(self.0));
        self.0.pop();
        result
    }
}

/// Walks compound values, naming each child.
struct Children<'a> {
    finder: Finder<'a>,
    index: usize,
    key: Option<String>,
    /// True if a variant name was pushed and must be popped at `end`.
    in_variant: bool,
}

impl Children<'_> {
    fn finish(self) -> Result<(), Stop> {
        if self.in_variant {
            self.finder.0.pop();
        }
        Ok(())
    }

    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        let token = self.index.to_string();
        self.index += 1;
        self.finder.nested(token, value)
    }
}

impl<'a> ser::Serializer for Finder<'a> {
    type Ok = ();
    type Error = Stop;
    type SerializeSeq = Children<'a>;
    type SerializeTuple = Children<'a>;
    type SerializeTupleStruct = Children<'a>;
    type SerializeTupleVariant = Children<'a>;
    type SerializeMap = Children<'a>;
    type SerializeStruct = Children<'a>;
    type SerializeStructVariant = Children<'a>;

    fn serialize_f32(self, v: f32) -> Result<(), Stop> {
        self.float(v.is_finite())
    }
    fn serialize_f64(self, v: f64) -> Result<(), Stop> {
        self.float(v.is_finite())
    }
    fn serialize_bool(self, _: bool) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_i8(self, _: i8) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_i16(self, _: i16) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_i32(self, _: i32) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_i64(self, _: i64) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_u8(self, _: u8) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_u16(self, _: u16) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_u32(self, _: u32) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_u64(self, _: u64) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_char(self, _: char) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_str(self, _: &str) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_none(self) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Stop> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Stop> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        mut self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), Stop> {
        self.nested(variant.to_owned(), value)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Children<'a>, Stop> {
        Ok(Children {
            finder: self,
            index: 0,
            key: None,
            in_variant: false,
        })
    }
    fn serialize_tuple(self, _: usize) -> Result<Children<'a>, Stop> {
        self.serialize_seq(None)
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Children<'a>, Stop> {
        self.serialize_seq(None)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Children<'a>, Stop> {
        self.0.push(variant.to_owned());
        Ok(Children {
            finder: self,
            index: 0,
            key: None,
            in_variant: true,
        })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Children<'a>, Stop> {
        self.serialize_seq(None)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Children<'a>, Stop> {
        self.serialize_seq(None)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Children<'a>, Stop> {
        self.0.push(variant.to_owned());
        Ok(Children {
            finder: self,
            index: 0,
            key: None,
            in_variant: true,
        })
    }
}

macro_rules! positional {
    ($trait:ident, $method:ident) => {
        impl ser::$trait for Children<'_> {
            type Ok = ();
            type Error = Stop;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
                self.element(value)
            }
            fn end(self) -> Result<(), Stop> {
                self.finish()
            }
        }
    };
}

positional!(SerializeSeq, serialize_element);
positional!(SerializeTuple, serialize_element);
positional!(SerializeTupleStruct, serialize_field);
positional!(SerializeTupleVariant, serialize_field);

macro_rules! named {
    ($trait:ident) => {
        impl ser::$trait for Children<'_> {
            type Ok = ();
            type Error = Stop;
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), Stop> {
                self.finder.nested(key.to_owned(), value)
            }
            fn end(self) -> Result<(), Stop> {
                self.finish()
            }
        }
    };
}

named!(SerializeStruct);
named!(SerializeStructVariant);

impl ser::SerializeMap for Children<'_> {
    type Ok = ();
    type Error = Stop;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Stop> {
        let text = serde_json::to_value(key).map_err(|e| Stop::Other(e.to_string()))?;
        self.key = Some(
            text.as_str()
                .map_or_else(|| text.to_string(), str::to_owned),
        );
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        let key = self.key.take().unwrap_or_default();
        self.finder.nested(key, value)
    }
    fn end(self) -> Result<(), Stop> {
        Ok(())
    }
}
