//! The wire codec (ADR-0062): reflect-described fixed layouts for crossing the script boundary.
//!
//! Every type has a fixed stride derived from its [`Shape`](crate::Shape): little-endian scalars,
//! no padding, `Option` as a `u8` tag plus its payload, enums as a `u32` tag plus the largest
//! payload (unused bytes zero), UUIDs as 16 bytes. Strings, lists, maps and `Any` are arena
//! references (`u32` offset, `u32` length) into a side arena that follows the fixed rows. The
//! encoding is canonical: one value has exactly one byte sequence.

mod de;
mod layout;
mod ser;
mod walk;

use serde::Serialize;
use serde::de::DeserializeOwned;

use self::de::{De, Reader};
use self::layout::Ty;
use self::ser::{Writer, put_value};
use self::walk::{Show, copy};
use crate::info::TypeInfo;
use crate::value::DynamicValue;

/// Why bytes could not be encoded or decoded.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WireError {
    /// The input ended early.
    #[error("at byte {at}: needed {needed} bytes, {available} available")]
    Truncated {
        /// Offset.
        at: usize,
        /// Bytes needed.
        needed: usize,
        /// Bytes left.
        available: usize,
    },
    /// An `Option`, `bool` or enum tag is out of range.
    #[error("at byte {at}: invalid tag {tag} for {type_name}")]
    InvalidTag {
        /// Offset.
        at: usize,
        /// The tag found.
        tag: u32,
        /// The type being read.
        type_name: String,
    },
    /// An arena reference points outside the arena.
    #[error("at byte {at}: arena range {offset}+{len} is outside the arena ({arena} bytes)")]
    ArenaOutOfBounds {
        /// Offset of the reference.
        at: usize,
        /// Referenced offset.
        offset: usize,
        /// Referenced length in bytes.
        len: usize,
        /// Arena size.
        arena: usize,
    },
    /// A string is not UTF-8.
    #[error("at byte {at}: invalid UTF-8")]
    InvalidUtf8 {
        /// Offset.
        at: usize,
    },
    /// A scalar is invalid (e.g. a zero `NonZero*`, an invalid `char`).
    #[error("at {pointer:?}: invalid {expected}")]
    InvalidScalar {
        /// Where, as a JSON Pointer.
        pointer: String,
        /// What was expected.
        expected: String,
    },
    /// A number is outside its declared range.
    #[error("at {pointer:?}: {value} is outside the range [{min}, {max}]")]
    OutOfRange {
        /// Where, as a JSON Pointer.
        pointer: String,
        /// The value.
        value: f64,
        /// Lower bound.
        min: f64,
        /// Upper bound.
        max: f64,
    },
    /// Bytes are left after the value.
    #[error("{extra} unexpected bytes after the value")]
    TrailingBytes {
        /// Extra bytes.
        extra: usize,
    },
    /// The type cannot be laid out (e.g. it contains itself inline).
    #[error("{type_name} has no wire layout: {reason}")]
    Unsupported {
        /// The type.
        type_name: String,
        /// Why.
        reason: String,
    },
    /// A value did not match the type while encoding, or serde rejected decoded data.
    #[error("{0}")]
    Mismatch(String),
}

/// A type's fixed layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    stride: usize,
    has_arena: bool,
}

impl Layout {
    /// Bytes per value in a column.
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// True if values may reference the arena (strings, lists, maps, `Any`).
    pub fn has_arena(&self) -> bool {
        self.has_arena
    }
}

/// The layout of `info`.
pub fn layout(info: &'static TypeInfo) -> Result<Layout, WireError> {
    layout::of(info)
}

/// Byte offsets of a struct's fields within one value, in declaration order (M4 4.10b: compiled
/// graphs read and write fields in place). Other shapes have no fields.
pub fn field_offsets(info: &'static TypeInfo) -> Result<Vec<usize>, WireError> {
    match &layout::peel(info).shape {
        crate::Shape::Struct { fields, .. } => layout::offsets(fields.iter().map(|f| f.ty)),
        _ => Err(WireError::Unsupported {
            type_name: info.name.to_owned(),
            reason: "only structs have field offsets".into(),
        }),
    }
}

/// Encodes one value (fixed part, then its arena); `value` may be the Rust type or JSON.
pub fn encode<T: Serialize + ?Sized>(
    value: &T,
    info: &'static TypeInfo,
) -> Result<Vec<u8>, WireError> {
    let stride = layout(info)?.stride();
    let mut w = Writer::new(stride);
    put_value(&mut w, 0, info, value)?;
    if w.unsorted {
        return canonical(&w.buf, stride, 1, info);
    }
    Ok(w.buf)
}

/// Decodes one value encoded by [`encode`] (validating it, but not its canonical order).
pub fn decode<T: DeserializeOwned>(bytes: &[u8], info: &'static TypeInfo) -> Result<T, WireError> {
    let stride = layout(info)?.stride();
    let r = Reader::new(bytes, stride, false)?;
    T::deserialize(root(&r, 0, info))
}

/// Checks that `bytes` are exactly one canonical, valid value of `info` (tags, arena bounds and
/// order, UTF-8, scalars, ranges, zero padding).
pub fn validate(bytes: &[u8], info: &'static TypeInfo) -> Result<(), WireError> {
    let stride = layout(info)?.stride();
    let r = Reader::new(bytes, stride, true)?;
    serde::de::IgnoredAny::deserialize(root(&r, 0, info))?;
    match r.unread_arena() {
        0 => Ok(()),
        extra => Err(WireError::TrailingBytes { extra }),
    }
}

use serde::Deserialize as _;

fn root<'r, 'a>(r: &'r Reader<'a>, at: usize, info: &'static TypeInfo) -> De<'r, 'a> {
    De {
        r,
        at,
        ty: Ty::Info(info),
        field: None,
        depth: 0,
    }
}

/// Re-encodes `rows` values in canonical arena order.
fn canonical(
    bytes: &[u8],
    stride: usize,
    rows: usize,
    info: &'static TypeInfo,
) -> Result<Vec<u8>, WireError> {
    let base = stride * rows;
    let r = Reader::new(bytes, base, false)?;
    let mut w = Writer::new(base);
    for row in 0..rows {
        copy(&r, row * stride, &mut w, row * stride, Ty::Info(info), 0)?;
    }
    Ok(w.buf)
}

/// Builds a column: `rows` values of one type, then their shared arena.
pub struct ColumnWriter {
    info: &'static TypeInfo,
    stride: usize,
    rows: usize,
    w: Writer,
    written: Vec<bool>,
    next: usize,
}

impl std::fmt::Debug for ColumnWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ColumnWriter({}, {} rows)", self.info.name, self.rows)
    }
}

impl ColumnWriter {
    /// A column of `rows` values of `info`; every row must be written before [`finish`](Self::finish).
    pub fn new(info: &'static TypeInfo, rows: usize) -> Result<Self, WireError> {
        let stride = layout(info)?.stride();
        let fixed = stride
            .checked_mul(rows)
            .ok_or_else(|| WireError::Mismatch(format!("{rows} rows of {} overflow", info.name)))?;
        Ok(Self {
            info,
            stride,
            rows,
            w: Writer::new(fixed),
            written: vec![false; rows],
            next: 0,
        })
    }

    fn claim(&mut self, row: usize) -> Result<usize, WireError> {
        if row >= self.rows {
            return Err(WireError::Mismatch(format!(
                "row {row} is outside a column of {} rows",
                self.rows
            )));
        }
        if std::mem::replace(&mut self.written[row], true) {
            return Err(WireError::Mismatch(format!("row {row} is written twice")));
        }
        if row != self.next {
            self.w.unsorted = true;
        }
        self.next = row + 1;
        Ok(row * self.stride)
    }

    /// Encodes `value` (the Rust type or JSON) as row `row`.
    pub fn write<T: Serialize + ?Sized>(&mut self, row: usize, value: &T) -> Result<(), WireError> {
        let at = self.claim(row)?;
        put_value(&mut self.w, at, self.info, value)
    }

    /// Copies a dynamic value of this column's type into row `row`.
    pub fn write_value(&mut self, row: usize, value: &DynamicValue) -> Result<(), WireError> {
        if value.info().name != self.info.name || layout(value.info())? != layout(self.info)? {
            return Err(WireError::Mismatch(format!(
                "a {} value cannot go in a {} column",
                value.info().name,
                self.info.name
            )));
        }
        let at = self.claim(row)?;
        let r = Reader::new(value.bytes(), self.stride, false)?;
        copy(&r, 0, &mut self.w, at, Ty::Info(self.info), 0)
    }

    /// The finished column; fails if a row was never written.
    pub fn finish(self) -> Result<WireColumn, WireError> {
        if let Some(row) = self.written.iter().position(|w| !w) {
            return Err(WireError::Mismatch(format!("row {row} was never written")));
        }
        let bytes = if self.w.unsorted {
            canonical(&self.w.buf, self.stride, self.rows, self.info)?
        } else {
            self.w.buf
        };
        Ok(WireColumn {
            info: self.info,
            rows: self.rows,
            stride: self.stride,
            bytes,
        })
    }
}

/// A column of wire-encoded values: `rows * stride` fixed bytes, then the arena.
#[derive(Debug, Clone)]
pub struct WireColumn {
    info: &'static TypeInfo,
    rows: usize,
    stride: usize,
    bytes: Vec<u8>,
}

impl PartialEq for WireColumn {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.info, other.info) && self.rows == other.rows && self.bytes == other.bytes
    }
}

impl WireColumn {
    /// Validates raw column bytes (e.g. from a script) as `rows` values of `info`.
    pub fn from_bytes(
        info: &'static TypeInfo,
        rows: usize,
        bytes: Vec<u8>,
    ) -> Result<Self, WireError> {
        let stride = layout(info)?.stride();
        let base = stride
            .checked_mul(rows)
            .ok_or_else(|| WireError::Mismatch(format!("{rows} rows of {} overflow", info.name)))?;
        let r = Reader::new(&bytes, base, false)?;
        for row in 0..rows {
            serde::de::IgnoredAny::deserialize(root(&r, row * stride, info))?;
        }
        Ok(Self {
            info,
            rows,
            stride,
            bytes,
        })
    }

    /// The column's type.
    pub fn info(&self) -> &'static TypeInfo {
        self.info
    }

    /// Number of rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Bytes per row.
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// The raw bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consumes the column, returning its bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    fn reader(&self, row: usize) -> Result<Reader<'_>, WireError> {
        if row >= self.rows {
            return Err(WireError::Mismatch(format!(
                "row {row} is outside a column of {} rows",
                self.rows
            )));
        }
        Reader::new(&self.bytes, self.stride * self.rows, false)
    }

    /// Decodes row `row` as `T`.
    pub fn read<T: DeserializeOwned>(&self, row: usize) -> Result<T, WireError> {
        let r = self.reader(row)?;
        T::deserialize(root(&r, row * self.stride, self.info))
    }

    /// Row `row` as a dynamic value (canonical bytes).
    pub fn value(&self, row: usize) -> Result<DynamicValue, WireError> {
        let r = self.reader(row)?;
        let mut w = Writer::new(self.stride);
        copy(&r, row * self.stride, &mut w, 0, Ty::Info(self.info), 0)?;
        DynamicValue::from_wire(self.info, &w.buf)
    }

    /// Serializes row `row` with any serde serializer, as the Rust type of its shape would.
    pub fn serialize_row<S: serde::Serializer>(&self, row: usize, s: S) -> Result<S::Ok, S::Error> {
        let r = self.reader(row).map_err(serde::ser::Error::custom)?;
        Show {
            r: &r,
            at: row * self.stride,
            info: self.info,
            depth: 0,
        }
        .serialize(s)
    }
}

/// Serializes one encoded value (fixed part, then its arena) as the type `info`.
pub(crate) fn serialize_value<S: serde::Serializer>(
    bytes: &[u8],
    info: &'static TypeInfo,
    s: S,
) -> Result<S::Ok, S::Error> {
    let stride = layout(info).map_err(serde::ser::Error::custom)?.stride();
    let r = Reader::new(bytes, stride, false).map_err(serde::ser::Error::custom)?;
    Show {
        r: &r,
        at: 0,
        info,
        depth: 0,
    }
    .serialize(s)
}
