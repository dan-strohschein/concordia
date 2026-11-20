//! The binary run envelope for native modules (ADR-0061): a call (processor, tick context,
//! batch) in, a reply (log lines, then the output or an error) out. Little-endian, `u32`
//! lengths, column bytes copied whole. Decoding checks every length and never panics.
//! Descriptions cross as JSON instead (once per load, and `TypeDesc` is JSON already).

use concordia_core::id::EntityId;

use crate::{
    Batch, Column, Command, ProtocolError, RunOutput, SignalColumn, Target, TickContext, Value,
    ids_from_bytes, ids_to_bytes,
};

/// Why a guest produced no output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuestError {
    /// `Module::run` returned an error.
    Returned(String),
    /// The guest panicked (caught at the boundary).
    Panicked(String),
    /// The guest couldn't decode the call.
    BadCall(String),
}

/// A run's reply: log lines (kept even on error), then the outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// Lines the guest logged, in order.
    pub logs: Vec<String>,
    /// The output, or why there is none.
    pub outcome: Result<RunOutput, GuestError>,
}

/// One run call, decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// The processor to run.
    pub processor: String,
    /// Time and seed.
    pub tick: TickContext,
    /// The processor's batch.
    pub batch: Batch,
}

/// Encodes a run call.
pub fn encode_call(processor: &str, tick: &TickContext, batch: &Batch) -> Vec<u8> {
    let mut w = Writer::default();
    w.str(processor);
    w.u64(tick.tick);
    w.u32(tick.dt.to_bits());
    w.u64(tick.time.to_bits());
    w.u64(tick.seed);
    w.ids(&batch.ids);
    w.len(batch.columns.len());
    for c in &batch.columns {
        w.str(&c.name);
        w.bytes(&c.data);
    }
    w.len(batch.signals.len());
    for s in &batch.signals {
        w.str(&s.name);
        w.ids(&s.targets);
        w.bytes(&s.data);
    }
    w.0
}

/// Decodes a run call.
pub fn decode_call(bytes: &[u8]) -> Result<Call, ProtocolError> {
    let mut r = Reader(bytes);
    let processor = r.string()?;
    let tick = TickContext {
        tick: r.u64()?,
        dt: f32::from_bits(r.u32()?),
        time: f64::from_bits(r.u64()?),
        seed: r.u64()?,
    };
    let ids = r.ids()?;
    let columns = r.list(|r| {
        Ok(Column {
            name: r.string()?,
            data: r.bytes()?,
        })
    })?;
    let signals = r.list(|r| {
        Ok(SignalColumn {
            name: r.string()?,
            targets: r.ids()?,
            data: r.bytes()?,
        })
    })?;
    r.end()?;
    Ok(Call {
        processor,
        tick,
        batch: Batch {
            ids,
            columns,
            signals,
        },
    })
}

/// Encodes a reply.
pub fn encode_reply(reply: &Reply) -> Vec<u8> {
    let mut w = Writer::default();
    w.len(reply.logs.len());
    for line in &reply.logs {
        w.str(line);
    }
    match &reply.outcome {
        Ok(out) => {
            w.u8(0);
            w.len(out.writes.len());
            for c in &out.writes {
                w.str(&c.name);
                w.bytes(&c.data);
            }
            w.len(out.commands.len());
            for c in &out.commands {
                w.command(c);
            }
        }
        Err(e) => {
            let (tag, message) = match e {
                GuestError::Returned(m) => (1, m),
                GuestError::Panicked(m) => (2, m),
                GuestError::BadCall(m) => (3, m),
            };
            w.u8(tag);
            w.str(message);
        }
    }
    w.0
}

/// Decodes a reply.
pub fn decode_reply(bytes: &[u8]) -> Result<Reply, ProtocolError> {
    let mut r = Reader(bytes);
    let logs = r.list(Reader::string)?;
    let outcome = match r.u8()? {
        0 => Ok(RunOutput {
            writes: r.list(|r| {
                Ok(Column {
                    name: r.string()?,
                    data: r.bytes()?,
                })
            })?,
            commands: r.list(Reader::command)?,
        }),
        1 => Err(GuestError::Returned(r.string()?)),
        2 => Err(GuestError::Panicked(r.string()?)),
        3 => Err(GuestError::BadCall(r.string()?)),
        tag => return Err(invalid(format!("unknown outcome tag {tag}"))),
    };
    r.end()?;
    Ok(Reply { logs, outcome })
}

fn invalid(message: String) -> ProtocolError {
    ProtocolError::InvalidEnvelope(message)
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    /// A length or count; envelopes never exceed `u32` sizes (columns are capped by the wire).
    fn len(&mut self, n: usize) {
        debug_assert!(u32::try_from(n).is_ok());
        self.u32(n as u32);
    }
    fn bytes(&mut self, b: &[u8]) {
        self.len(b.len());
        self.0.extend_from_slice(b);
    }
    fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
    fn ids(&mut self, ids: &[EntityId]) {
        self.bytes(&ids_to_bytes(ids));
    }
    fn value(&mut self, v: &Value) {
        self.str(&v.name);
        self.bytes(&v.data);
    }
    fn target(&mut self, t: &Target) {
        match t {
            Target::Entity(id) => {
                self.u8(0);
                self.0.extend_from_slice(id.as_uuid().as_bytes());
            }
            Target::Pending(n) => {
                self.u8(1);
                self.u32(*n);
            }
        }
    }
    fn command(&mut self, c: &Command) {
        match c {
            Command::Spawn(values) => {
                self.u8(0);
                self.len(values.len());
                values.iter().for_each(|v| self.value(v));
            }
            Command::SpawnTemplate(name) => {
                self.u8(1);
                self.str(name);
            }
            Command::Insert(t, v) => {
                self.u8(2);
                self.target(t);
                self.value(v);
            }
            Command::Remove(t, name) => {
                self.u8(3);
                self.target(t);
                self.str(name);
            }
            Command::Despawn(t) => {
                self.u8(4);
                self.target(t);
            }
            Command::Signal(t, v) => {
                self.u8(5);
                self.target(t);
                self.value(v);
            }
        }
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ProtocolError> {
        if self.0.len() < n {
            return Err(invalid(format!(
                "needs {n} more bytes, has {}",
                self.0.len()
            )));
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProtocolError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.array::<1>()?[0])
    }
    fn u32(&mut self) -> Result<u32, ProtocolError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64, ProtocolError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn bytes(&mut self) -> Result<Vec<u8>, ProtocolError> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn string(&mut self) -> Result<String, ProtocolError> {
        String::from_utf8(self.bytes()?).map_err(|e| invalid(e.to_string()))
    }
    fn ids(&mut self) -> Result<Vec<EntityId>, ProtocolError> {
        ids_from_bytes(&self.bytes()?)
    }
    /// A counted list; the count is checked against the bytes left (each item takes 1+).
    fn list<T>(
        &mut self,
        mut item: impl FnMut(&mut Self) -> Result<T, ProtocolError>,
    ) -> Result<Vec<T>, ProtocolError> {
        let n = self.u32()? as usize;
        if n > self.0.len() {
            return Err(invalid(format!("{n} items in {} bytes", self.0.len())));
        }
        (0..n).map(|_| item(self)).collect()
    }
    fn value(&mut self) -> Result<Value, ProtocolError> {
        Ok(Value {
            name: self.string()?,
            data: self.bytes()?,
        })
    }
    fn target(&mut self) -> Result<Target, ProtocolError> {
        match self.u8()? {
            0 => {
                let ids = ids_from_bytes(&self.array::<16>()?)?;
                Ok(Target::Entity(ids[0]))
            }
            1 => Ok(Target::Pending(self.u32()?)),
            tag => Err(invalid(format!("unknown target tag {tag}"))),
        }
    }
    fn command(&mut self) -> Result<Command, ProtocolError> {
        Ok(match self.u8()? {
            0 => Command::Spawn(self.list(Self::value)?),
            1 => Command::SpawnTemplate(self.string()?),
            2 => Command::Insert(self.target()?, self.value()?),
            3 => Command::Remove(self.target()?, self.string()?),
            4 => Command::Despawn(self.target()?),
            5 => Command::Signal(self.target()?, self.value()?),
            tag => return Err(invalid(format!("unknown command tag {tag}"))),
        })
    }
    fn end(&self) -> Result<(), ProtocolError> {
        match self.0.len() {
            0 => Ok(()),
            n => Err(invalid(format!("{n} trailing bytes"))),
        }
    }
}

#[cfg(test)]
mod tests;
