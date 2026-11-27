//! Compiled Event graphs as processor programs (M4 4.10b, J13–J15) and the memory image they run
//! on. The interpreter here and the WASM module run the same program on the same image.
//!
//! Image layout (little-endian `u32` words, then data): `[jobs, dt (f32 bits), rows ptr, signal
//! ptr, signal stride, raise ptr, raise count, raise record size, rows, column ptr…]`, then the job rows
//! (`u32` each), the signal payloads (one per job), the columns (`rows × stride` each) and the
//! raise records (`job u32, site u32, payload`, one slot per job and site).

use concordia_reflect::TypeDesc;

use crate::ir::{Function, Scalar, ScalarTy};

/// What starts a processor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Every tick, once per matched entity (`event.on_tick`).
    Tick,
    /// Once per delivered signal of this type, in order (`on.<S>`).
    Signal(String),
}

/// How a scalar is stored in a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One byte, 0 or 1 (an `i32` in the IR).
    Bool,
    /// Four bytes.
    I32,
    /// Eight bytes.
    I64,
    /// Four bytes.
    F32,
}

impl Kind {
    /// The IR type it loads as.
    pub fn scalar(self) -> ScalarTy {
        match self {
            Self::Bool | Self::I32 => ScalarTy::I32,
            Self::I64 => ScalarTy::I64,
            Self::F32 => ScalarTy::F32,
        }
    }
}

/// A column the program touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSlot {
    /// Component name.
    pub name: String,
    /// Bytes per row.
    pub stride: usize,
}

/// Where a function parameter comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Load {
    /// A field of the job's row in column `slot`.
    Column {
        /// Column slot.
        slot: usize,
        /// Byte offset in the row.
        offset: usize,
        /// Stored as.
        kind: Kind,
    },
    /// A field of the job's signal payload.
    Signal {
        /// Byte offset in the payload.
        offset: usize,
        /// Stored as.
        kind: Kind,
    },
    /// The tick's step in seconds.
    Dt,
}

/// Where a function result goes: a field of the job's row (results come in store order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Store {
    /// Column slot.
    pub slot: usize,
    /// Byte offset in the row.
    pub offset: usize,
    /// Stored as.
    pub kind: Kind,
}

/// A `raise.<S>` node: after the stores, its results are a flag then the payload fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaiseSite {
    /// Signal type.
    pub signal: String,
    /// Payload bytes.
    pub stride: usize,
    /// Payload fields: (offset, kind).
    pub payload: Vec<(usize, Kind)>,
}

/// One processor of a compiled Event graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// Processor name.
    pub name: String,
    /// What starts it.
    pub entry: Entry,
    /// Components read only.
    pub reads: Vec<String>,
    /// Components written.
    pub writes: Vec<String>,
    /// Components a row must have (the graph's `Vars`).
    pub with: Vec<String>,
    pub(crate) columns: Vec<ColumnSlot>,
    pub(crate) loads: Vec<Load>,
    pub(crate) stores: Vec<Store>,
    pub(crate) raises: Vec<RaiseSite>,
    pub(crate) func: Function,
}

/// A compiled Event graph: its variables' component and its processors.
#[derive(Debug, Clone, PartialEq)]
pub struct EventModule {
    /// Module name.
    pub name: String,
    /// The `graph.<name>.Vars` component (the graph's variables).
    pub vars: TypeDesc,
    /// One per event node.
    pub processors: Vec<Program>,
}

/// A raised signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raise {
    /// The job that raised it (the row for ticks, the signal's index for signals).
    pub job: u32,
    /// Signal type.
    pub signal: String,
    /// One-row wire bytes.
    pub payload: Vec<u8>,
}

/// Why a program couldn't run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeError {
    /// The image doesn't fit the program.
    #[error("bad image: {0}")]
    Image(String),
    /// The step budget ran out (the interpreter's fuel).
    #[error("step budget exhausted")]
    Budget,
}

const HEADER_WORDS: usize = 9;

/// A program's memory image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The bytes (what the WASM module's memory holds).
    pub bytes: Vec<u8>,
}

impl Program {
    /// The program as a core WASM module exporting `memory` and `run()` (the image goes at 0).
    pub fn wasm(&self) -> Vec<u8> {
        crate::ir::emit_program(self)
    }

    /// The columns the image carries, in slot order.
    pub fn columns(&self) -> &[ColumnSlot] {
        &self.columns
    }

    /// The largest raise payload.
    fn record_size(&self) -> usize {
        8 + self.raises.iter().map(|r| r.stride).max().unwrap_or(0)
    }

    /// Runs every job on `image` (one step per instruction against the budget, if any).
    pub fn interpret(&self, image: &mut Image) -> Result<(), RuntimeError> {
        self.interpret_with(image, None)
    }

    /// Runs with at most `budget` instructions in all.
    pub fn interpret_with(
        &self,
        image: &mut Image,
        budget: Option<u64>,
    ) -> Result<(), RuntimeError> {
        let jobs = image.word(0)? as usize;
        let steps = (self.func.insts().len() as u64).max(1);
        if budget.is_some_and(|b| steps.saturating_mul(jobs as u64) > b) {
            return Err(RuntimeError::Budget);
        }
        let dt = f32::from_bits(image.word(1)?);
        let mut raised = 0u32;
        for job in 0..jobs {
            let row = image.word_at(image.word(2)? as usize + job * 4)? as usize;
            let args = self
                .loads
                .iter()
                .map(|l| image.load(self, *l, job, row, dt))
                .collect::<Result<Vec<_>, _>>()?;
            let results = self
                .func
                .eval(&args)
                .map_err(|e| RuntimeError::Image(e.to_string()))?;
            for (store, value) in self.stores.iter().zip(&results) {
                let at = image.column_at(self, store.slot, row)? + store.offset;
                image.put(at, store.kind, *value)?;
            }
            let mut rest = &results[self.stores.len()..];
            for (site, r) in self.raises.iter().enumerate() {
                let (flag, payload) = (rest[0], &rest[1..=r.payload.len()]);
                rest = &rest[1 + r.payload.len()..];
                if flag != Scalar::I32(0) {
                    let at = image.word(5)? as usize + raised as usize * self.record_size();
                    image.put(at, Kind::I32, Scalar::I32(job as i32))?;
                    image.put(at + 4, Kind::I32, Scalar::I32(site as i32))?;
                    for ((offset, kind), v) in r.payload.iter().zip(payload) {
                        image.put(at + 8 + offset, *kind, *v)?;
                    }
                    raised += 1;
                }
            }
        }
        image.set_word(6, raised)
    }
}

impl Image {
    /// An image for a tick: jobs are rows `0..rows`; `columns` in slot order.
    pub fn tick(
        p: &Program,
        rows: usize,
        dt: f32,
        columns: &[Vec<u8>],
    ) -> Result<Self, RuntimeError> {
        let jobs: Vec<u32> = (0..rows as u32).collect();
        Self::build(p, rows, &jobs, dt, &[], columns)
    }

    /// An image for signals: `targets` gives each signal's row (`None`: not matched, skipped);
    /// `payloads` holds one wire row per signal.
    pub fn signals(
        p: &Program,
        targets: &[Option<usize>],
        payloads: &[u8],
        columns: &[Vec<u8>],
    ) -> Result<Self, RuntimeError> {
        let stride = match targets.len() {
            0 => 0,
            n => payloads.len() / n,
        };
        let mut jobs = Vec::new();
        let mut kept = Vec::new();
        for (k, target) in targets.iter().enumerate() {
            if let Some(row) = target {
                jobs.push(*row as u32);
                kept.extend_from_slice(&payloads[k * stride..(k + 1) * stride]);
            }
        }
        let rows = columns
            .first()
            .zip(p.columns.first())
            .map_or(0, |(c, s)| c.len() / s.stride.max(1));
        let mut image = Self::build(p, rows, &jobs, 0.0, &kept, columns)?;
        image.set_word(4, stride as u32)?;
        Ok(image)
    }

    fn build(
        p: &Program,
        rows: usize,
        jobs: &[u32],
        dt: f32,
        payloads: &[u8],
        columns: &[Vec<u8>],
    ) -> Result<Self, RuntimeError> {
        if columns.len() != p.columns.len() {
            return Err(RuntimeError::Image(format!(
                "{} columns for {} slots",
                columns.len(),
                p.columns.len()
            )));
        }
        for (slot, bytes) in p.columns.iter().zip(columns) {
            if bytes.len() != slot.stride * rows {
                return Err(RuntimeError::Image(format!(
                    "column {} has {} bytes, not {} rows × {}",
                    slot.name,
                    bytes.len(),
                    rows,
                    slot.stride
                )));
            }
        }
        let header = 4 * (HEADER_WORDS + p.columns.len());
        let mut bytes = vec![0; header];
        let mut words = vec![jobs.len() as u32, dt.to_bits()];
        words.push(bytes.len() as u32);
        bytes.extend(jobs.iter().flat_map(|j| j.to_le_bytes()));
        words.push(bytes.len() as u32);
        words.push(0);
        bytes.extend_from_slice(payloads);
        let mut column_ptrs = Vec::new();
        for c in columns {
            column_ptrs.push(bytes.len() as u32);
            bytes.extend_from_slice(c);
        }
        words.push(bytes.len() as u32);
        words.push(0);
        words.push(p.record_size() as u32);
        words.push(rows as u32);
        bytes.resize(
            bytes.len() + jobs.len() * p.raises.len() * p.record_size(),
            0,
        );
        words.extend(column_ptrs);
        for (k, w) in words.iter().enumerate() {
            bytes[k * 4..k * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        Ok(Self { bytes })
    }

    fn word_at(&self, at: usize) -> Result<u32, RuntimeError> {
        self.bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap_or_default()))
            .ok_or_else(|| RuntimeError::Image(format!("no word at {at}")))
    }

    fn word(&self, k: usize) -> Result<u32, RuntimeError> {
        self.word_at(k * 4)
    }

    fn set_word(&mut self, k: usize, v: u32) -> Result<(), RuntimeError> {
        self.put(k * 4, Kind::I32, Scalar::I32(v as i32))
    }

    fn column_at(&self, p: &Program, slot: usize, row: usize) -> Result<usize, RuntimeError> {
        let stride = p
            .columns
            .get(slot)
            .ok_or_else(|| RuntimeError::Image(format!("no slot {slot}")))?
            .stride;
        Ok(self.word(HEADER_WORDS + slot)? as usize + row * stride)
    }

    fn load(
        &self,
        p: &Program,
        load: Load,
        job: usize,
        row: usize,
        dt: f32,
    ) -> Result<Scalar, RuntimeError> {
        match load {
            Load::Dt => Ok(Scalar::F32(dt)),
            Load::Column { slot, offset, kind } => {
                self.get(self.column_at(p, slot, row)? + offset, kind)
            }
            Load::Signal { offset, kind } => {
                let at = self.word(3)? as usize + job * self.word(4)? as usize + offset;
                self.get(at, kind)
            }
        }
    }

    fn get(&self, at: usize, kind: Kind) -> Result<Scalar, RuntimeError> {
        let width = match kind {
            Kind::Bool => 1,
            Kind::I32 | Kind::F32 => 4,
            Kind::I64 => 8,
        };
        let b = self
            .bytes
            .get(at..at + width)
            .ok_or_else(|| RuntimeError::Image(format!("no {width} bytes at {at}")))?;
        Ok(match kind {
            Kind::Bool => Scalar::I32(i32::from(b[0])),
            Kind::I32 => Scalar::I32(i32::from_le_bytes(b.try_into().unwrap_or_default())),
            Kind::I64 => Scalar::I64(i64::from_le_bytes(b.try_into().unwrap_or_default())),
            Kind::F32 => Scalar::F32(f32::from_le_bytes(b.try_into().unwrap_or_default())),
        })
    }

    fn put(&mut self, at: usize, kind: Kind, v: Scalar) -> Result<(), RuntimeError> {
        let bytes: Vec<u8> = match (kind, v) {
            (Kind::Bool, Scalar::I32(x)) => vec![u8::from(x != 0)],
            (Kind::I32, Scalar::I32(x)) => x.to_le_bytes().to_vec(),
            (Kind::I64, Scalar::I64(x)) => x.to_le_bytes().to_vec(),
            (Kind::F32, Scalar::F32(x)) => x.to_le_bytes().to_vec(),
            (kind, v) => return Err(RuntimeError::Image(format!("a {v:?} stored as {kind:?}"))),
        };
        let target = self
            .bytes
            .get_mut(at..at + bytes.len())
            .ok_or_else(|| RuntimeError::Image(format!("no room at {at}")))?;
        target.copy_from_slice(&bytes);
        Ok(())
    }

    /// Column `name`'s bytes after a run.
    pub fn column(&self, p: &Program, name: &str) -> Option<&[u8]> {
        let slot = p.columns.iter().position(|c| c.name == name)?;
        let rows = self.rows(p)?;
        let at = self.word(HEADER_WORDS + slot).ok()? as usize;
        self.bytes.get(at..at + rows * p.columns[slot].stride)
    }

    /// Rows per column.
    fn rows(&self, _: &Program) -> Option<usize> {
        self.word(8).ok().map(|r| r as usize)
    }

    /// The signals the run raised, in order.
    pub fn raises(&self, p: &Program) -> Result<Vec<Raise>, RuntimeError> {
        let (base, count, size) = (
            self.word(5)? as usize,
            self.word(6)? as usize,
            p.record_size(),
        );
        (0..count)
            .map(|k| {
                let at = base + k * size;
                let site = self.word_at(at + 4)? as usize;
                let r = p
                    .raises
                    .get(site)
                    .ok_or_else(|| RuntimeError::Image(format!("no raise site {site}")))?;
                let payload = self
                    .bytes
                    .get(at + 8..at + 8 + r.stride)
                    .ok_or_else(|| RuntimeError::Image("short raise record".into()))?;
                Ok(Raise {
                    job: self.word_at(at)?,
                    signal: r.signal.clone(),
                    payload: payload.to_vec(),
                })
            })
            .collect()
    }
}
