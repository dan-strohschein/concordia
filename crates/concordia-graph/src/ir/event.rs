//! Lowering Event graphs to processor programs (M4 4.10b, J13–J15). Each event node becomes a
//! processor whose body is one predicated scalar function per job: exec paths become predicates,
//! `set` stores `select(pred, new, old)`, `raise` yields a flag and its payload.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use concordia_reflect::{
    FieldDesc, Primitive, Shape, ShapeDesc, TypeDesc, TypeInfo, TypeRefDesc, TypeRegistry, wire,
};

use crate::check::{Code, Diagnostic};
use crate::library::field_pin;
use crate::runtime::{
    ColumnSlot, Entry, EventModule, Kind, Load, Program, RaiseSite, Store, TraceMap,
};
use crate::{Graph, GraphKind, Input, Literal, NodeLibrary, PinType};

use super::lower::{Builder, Lower, Outputs, Sources};
use super::{BinOp, Scalar, ScalarTy, UnOp, ValueId, opt};

/// Checks and compiles an Event graph named `name` against `lib` and `registry`.
pub fn compile_event(
    name: &str,
    graph: &Graph,
    lib: &NodeLibrary,
    registry: &TypeRegistry,
) -> Result<EventModule, Vec<Diagnostic>> {
    let report = crate::check(graph, lib);
    if !report.is_ok() {
        return Err(report.diagnostics);
    }
    if graph.kind != GraphKind::Event {
        return Err(vec![unsupported(
            None,
            None,
            "only Event graphs compile to processors".into(),
        )]);
    }
    let mut errors = Vec::new();
    let vars = vars_desc(name, graph, &mut errors);
    let vars_info = vars
        .to_info(registry)
        .map_err(|e| vec![unsupported(None, None, format!("variables: {e}"))])?;
    let mut processors = Vec::new();
    for (id, node) in &graph.nodes {
        let entry = match node.op.as_str() {
            "event.on_tick" => Entry::Tick,
            op => match op.strip_prefix("on.") {
                Some(signal) => Entry::Signal(signal.to_owned()),
                None => continue,
            },
        };
        let sources = EventSources::new(registry, vars_info, id, entry.clone());
        let mut l = Lower {
            graph,
            lib,
            report: &report,
            b: Builder::default(),
            params: BTreeMap::new(),
            memo: BTreeMap::new(),
            errors: Vec::new(),
            sources,
            lowered: Vec::new(),
        };
        match lower_entry(&mut l, id) {
            Some(()) if l.errors.is_empty() => {
                processors.push(finish(l, name, &entry, processors.len()))
            }
            _ => errors.extend(l.errors),
        }
    }
    match errors.is_empty() {
        true => Ok(EventModule {
            name: name.to_owned(),
            vars,
            processors,
        }),
        false => Err(errors),
    }
}

fn unsupported(node: Option<&str>, pin: Option<&str>, message: String) -> Diagnostic {
    Diagnostic::at(Code::unsupported_node, node, pin, message)
}

/// The graph's variables as a component (`graph.<name>.Vars`), defaults from the declarations;
/// variables of types compiled graphs can't hold are reported and left out.
fn vars_desc(name: &str, graph: &Graph, errors: &mut Vec<Diagnostic>) -> TypeDesc {
    let scalar = |ty: &str| {
        PinType::parse(ty)
            .ok()
            .as_ref()
            .and_then(super::lower::scalars)
            .is_some()
    };
    for v in graph.variables.iter().filter(|v| !scalar(&v.ty)) {
        errors.push(unsupported(
            None,
            Some(&v.name),
            format!(
                "variable `{}` is a {}; compiled graphs hold numbers, bools and vectors",
                v.name, v.ty
            ),
        ));
    }
    let fields = graph
        .variables
        .iter()
        .filter(|v| scalar(&v.ty))
        .map(|v| FieldDesc {
            name: v.name.clone(),
            ty: TypeRefDesc::Named(v.ty.clone()),
            docs: format!("Graph variable `{}`.", v.name),
            default: v.default.as_ref().and_then(literal_json),
            range: None,
            widget: None,
            // Absent on input takes the default, so `{}` attaches the graph.
            optional: true,
        })
        .collect();
    TypeDesc {
        name: format!("graph.{name}.Vars"),
        docs: format!("The variables of graph `{name}` (adding it attaches the graph)."),
        shape: ShapeDesc::Struct {
            fields,
            deny_unknown: false,
        },
        widget: None,
        category: Some("Graphs".into()),
        default: None,
    }
}

fn literal_json(l: &Literal) -> Option<serde_json::Value> {
    Some(match l {
        Literal::Bool(v) => (*v).into(),
        Literal::Int(v) => (*v).into(),
        Literal::Float(v) => (*v).into(),
        Literal::Text(v) => v.clone().into(),
        Literal::Vec2(v) => serde_json::json!(v),
        Literal::Vec3(v) => serde_json::json!(v),
        Literal::Vec4(v) => serde_json::json!(v),
    })
}

/// A value's scalars in a wire row: (offset, kind).
type Scalars = Vec<(usize, Kind)>;

/// A type's scalars in a wire row: (offset, kind); `None` if any part isn't a scalar.
fn slots(info: &'static TypeInfo) -> Option<Vec<(usize, Kind)>> {
    match &info.shape {
        Shape::Primitive(Primitive::Bool) => Some(vec![(0, Kind::Bool)]),
        Shape::Primitive(Primitive::I32) => Some(vec![(0, Kind::I32)]),
        Shape::Primitive(Primitive::I64) => Some(vec![(0, Kind::I64)]),
        Shape::Primitive(Primitive::F32) => Some(vec![(0, Kind::F32)]),
        Shape::Array { elem, len } => {
            let inner = slots(elem.info())?;
            let stride = wire::layout(elem.info()).ok()?.stride();
            Some(
                (0..*len)
                    .flat_map(|k| inner.iter().map(move |&(o, kind)| (k * stride + o, kind)))
                    .collect(),
            )
        }
        Shape::Struct { fields, .. } => {
            let offsets = wire::field_offsets(info).ok()?;
            let mut out = Vec::new();
            for (f, base) in fields.iter().zip(offsets) {
                out.extend(slots(f.ty.info())?.into_iter().map(|(o, k)| (base + o, k)));
            }
            Some(out)
        }
        _ => None,
    }
}

/// A column the program touches, and its fields.
struct Column {
    name: String,
    info: &'static TypeInfo,
    /// (field name, its scalars).
    fields: Vec<(String, Option<Scalars>)>,
}

/// An event processor's state sources: columns, loads, the current value of each touched field.
struct EventSources<'r> {
    registry: &'r TypeRegistry,
    vars: &'static TypeInfo,
    entry_id: String,
    entry: Entry,
    columns: Vec<Column>,
    loads: Vec<Load>,
    cached: HashMap<LoadKey, ValueId>,
    state: BTreeMap<(usize, usize), Vec<ValueId>>,
    written: BTreeSet<(usize, usize)>,
    raises: Vec<(RaiseSite, ValueId, Vec<ValueId>)>,
    /// Statements in exec order, with the predicate under which each runs (for traces).
    statements: Vec<(String, ValueId)>,
}

/// A load's identity (for reusing its parameter).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum LoadKey {
    Column(usize, usize),
    Signal(usize),
    Dt,
}

impl<'r> EventSources<'r> {
    fn new(
        registry: &'r TypeRegistry,
        vars: &'static TypeInfo,
        entry_id: &str,
        entry: Entry,
    ) -> Self {
        Self {
            registry,
            vars,
            entry_id: entry_id.to_owned(),
            entry,
            columns: Vec::new(),
            loads: Vec::new(),
            cached: HashMap::new(),
            state: BTreeMap::new(),
            written: BTreeSet::new(),
            raises: Vec::new(),
            statements: Vec::new(),
        }
    }

    fn info(&self, name: &str) -> Option<&'static TypeInfo> {
        match name == self.vars.name {
            true => Some(self.vars),
            false => self.registry.get_by_name(name).map(|r| r.info()),
        }
    }

    /// The slot of component `name` (added on first use).
    fn slot(&mut self, name: &str) -> Option<usize> {
        if let Some(k) = self.columns.iter().position(|c| c.name == name) {
            return Some(k);
        }
        let info = self.info(name)?;
        let Shape::Struct { fields, .. } = &info.shape else {
            return None;
        };
        let offsets = wire::field_offsets(info).ok()?;
        let fields = fields
            .iter()
            .zip(offsets)
            .map(|(f, base)| {
                (
                    f.name.to_owned(),
                    slots(f.ty.info()).map(|s| s.into_iter().map(|(o, k)| (base + o, k)).collect()),
                )
            })
            .collect();
        self.columns.push(Column {
            name: name.to_owned(),
            info,
            fields,
        });
        Some(self.columns.len() - 1)
    }

    fn param(&mut self, b: &mut Builder, key: LoadKey, load: Load, kind: Kind) -> ValueId {
        if let Some(&v) = self.cached.get(&key) {
            return v;
        }
        debug_assert_eq!(self.loads.len(), b.param_count());
        let v = b.param(kind.scalar());
        self.loads.push(load);
        self.cached.insert(key, v);
        v
    }

    /// A field's current value (loaded on first use).
    fn field(&mut self, b: &mut Builder, slot: usize, field: usize) -> Option<Vec<ValueId>> {
        if let Some(v) = self.state.get(&(slot, field)) {
            return Some(v.clone());
        }
        let scalars = self.columns[slot].fields[field].1.clone()?;
        let values: Vec<ValueId> = scalars
            .iter()
            .map(|&(offset, kind)| {
                self.param(
                    b,
                    LoadKey::Column(slot, offset),
                    Load::Column { slot, offset, kind },
                    kind,
                )
            })
            .collect();
        self.state.insert((slot, field), values.clone());
        Some(values)
    }

    /// Component `name`'s field `field` (or every field), resolved from an op's rest.
    fn target(&mut self, rest: &str) -> Option<(usize, Option<usize>)> {
        if let Some(slot) = self
            .registry
            .get_by_name(rest)
            .map(|_| rest.to_owned())
            .and_then(|n| self.slot(&n))
        {
            return Some((slot, None));
        }
        let (component, field) = rest.rsplit_once('.')?;
        let slot = self.slot(component)?;
        let index = self.columns[slot]
            .fields
            .iter()
            .position(|(n, _)| n == field)?;
        Some((slot, Some(index)))
    }

    /// Every field of a whole component, concatenated.
    fn whole(&mut self, b: &mut Builder, slot: usize) -> Option<Vec<ValueId>> {
        let mut out = Vec::new();
        for f in 0..self.columns[slot].fields.len() {
            out.extend(self.field(b, slot, f)?);
        }
        Some(out)
    }

    /// Replaces a field's value where `pred` holds.
    fn assign(
        &mut self,
        b: &mut Builder,
        slot: usize,
        field: usize,
        new: &[ValueId],
        pred: ValueId,
    ) -> Option<()> {
        let old = self.field(b, slot, field)?;
        let kinds: Vec<ScalarTy> = self.columns[slot].fields[field]
            .1
            .clone()?
            .iter()
            .map(|(_, k)| k.scalar())
            .collect();
        let merged = old
            .iter()
            .zip(new)
            .zip(kinds)
            .map(|((&o, &n), t)| b.select(t, pred, n, o))
            .collect();
        self.state.insert((slot, field), merged);
        self.written.insert((slot, field));
        Some(())
    }

    /// The entry's signal payload fields: (pin, offset-shifted scalars).
    fn signal_fields(&self, signal: &str) -> Option<(&'static TypeInfo, Vec<(String, Scalars)>)> {
        let info = self.registry.get_by_name(signal)?.info();
        let mut fields = Vec::new();
        if let Shape::Struct { fields: fs, .. } = &info.shape {
            for (f, base) in fs.iter().zip(wire::field_offsets(info).ok()?) {
                let s = slots(f.ty.info())?;
                fields.push((
                    field_pin(f.name),
                    s.into_iter().map(|(o, k)| (base + o, k)).collect(),
                ));
            }
        }
        Some((info, fields))
    }

    fn entry_outputs(
        &mut self,
        b: &mut Builder,
        id: &str,
        op: &str,
    ) -> Result<Outputs, Diagnostic> {
        if id != self.entry_id {
            return Err(unsupported(
                Some(id),
                None,
                format!("`{op}` is another event's output; a processor reads only its own event"),
            ));
        }
        let mut out = Outputs::from([("entity".into(), vec![])]);
        match &self.entry {
            Entry::Tick => {
                let dt = self.param(b, LoadKey::Dt, Load::Dt, Kind::F32);
                out.insert("dt".into(), vec![dt]);
            }
            Entry::Signal(signal) => {
                let (_, fields) = self.signal_fields(signal).ok_or_else(|| {
                    unsupported(
                        Some(id),
                        None,
                        format!(
                            "signal `{signal}` has fields that aren't numbers, bools or vectors"
                        ),
                    )
                })?;
                let mut all = Vec::new();
                for (pin, scalars) in fields {
                    let values: Vec<ValueId> = scalars
                        .iter()
                        .map(|&(offset, kind)| {
                            self.param(
                                b,
                                LoadKey::Signal(offset),
                                Load::Signal { offset, kind },
                                kind,
                            )
                        })
                        .collect();
                    all.extend(&values);
                    out.insert(pin, values);
                }
                out.insert("signal".into(), all);
            }
        }
        Ok(out)
    }
}

/// Whether an `entity`/`target` input names the graph's own entity (absent means it does).
fn is_self(graph: &Graph, input: Option<&Input>) -> bool {
    match input {
        None => true,
        Some(Input::Link(src, pin)) => {
            let op = graph.nodes.get(src).map_or("", |n| n.op.as_str());
            pin == "entity"
                && (op == "entity.self" || op == "event.on_tick" || op.starts_with("on."))
        }
        Some(Input::Value(_)) => false,
    }
}

impl Sources for EventSources<'_> {
    fn read(
        &mut self,
        b: &mut Builder,
        graph: &Graph,
        id: &str,
    ) -> Option<Result<Outputs, Diagnostic>> {
        let node = &graph.nodes[id];
        let op = node.op.as_str();
        if op == "entity.self" {
            return Some(Ok(Outputs::from([("entity".into(), vec![])])));
        }
        if op == "event.on_tick" || op.starts_with("on.") {
            return Some(self.entry_outputs(b, id, op));
        }
        let field = |this: &mut Self, b: &mut Builder, slot, f: Option<usize>| match f {
            Some(f) => this.field(b, slot, f),
            None => this.whole(b, slot),
        };
        let found = if let Some(rest) = op.strip_prefix("get.") {
            if !is_self(graph, node.inputs.get("entity")) {
                return Some(Err(unsupported(Some(id), Some("entity"), "reading another entity's components isn't compiled yet (only the graph's own entity)".into())));
            }
            self.target(rest)
                .and_then(|(slot, f)| field(self, b, slot, f))
        } else {
            let var = op.strip_prefix("var.get.")?;
            let vars = self.vars.name;
            self.slot(vars).and_then(|slot| {
                let f = self.columns[slot]
                    .fields
                    .iter()
                    .position(|(n, _)| n == var)?;
                self.field(b, slot, f)
            })
        };
        Some(
            found
                .map(|v| Outputs::from([("value".into(), v)]))
                .ok_or_else(|| {
                    unsupported(
                        Some(id),
                        None,
                        format!("`{op}` reads a field that isn't a number, bool or vector"),
                    )
                }),
        )
    }

    fn layout(&self, ty: &PinType) -> Option<Vec<ScalarTy>> {
        let PinType::Named(name) = ty else {
            return None;
        };
        let info = self.info(name)?;
        Some(slots(info)?.into_iter().map(|(_, k)| k.scalar()).collect())
    }
}

/// Walks the exec flow from the entry with a constant-true predicate.
fn lower_entry(l: &mut Lower<'_, EventSources<'_>>, entry: &str) -> Option<()> {
    let yes = l.b.konst(Scalar::I32(1));
    walk(l, entry, "out", yes, &mut vec![entry.to_owned()])
}

/// Runs every statement linked from `from`'s exec output `pin`, under `pred`.
fn walk(
    l: &mut Lower<'_, EventSources<'_>>,
    from: &str,
    pin: &str,
    pred: ValueId,
    stack: &mut Vec<String>,
) -> Option<()> {
    let graph = l.graph;
    for link in graph.exec.iter().filter(|e| e.0 == from && e.1 == pin) {
        let to = &link.2;
        if stack.contains(to) {
            return l.unsupported(
                Some(to),
                None,
                "an exec loop can't be compiled yet (loops are a later phase)".into(),
            );
        }
        stack.push(to.clone());
        statement(l, to, pred, stack)?;
        stack.pop();
    }
    Some(())
}

/// A data input's value for a pin of type `ty`.
fn input(
    l: &mut Lower<'_, EventSources<'_>>,
    id: &str,
    pin: &str,
    ty: &PinType,
) -> Option<Vec<ValueId>> {
    let value = l.graph.nodes[id].inputs.get(pin)?.clone();
    l.value_of(&value, ty)
}

fn and(l: &mut Lower<'_, EventSources<'_>>, a: ValueId, b: ValueId) -> ValueId {
    l.b.bin(BinOp::And, ScalarTy::I32, a, b)
}

fn statement(
    l: &mut Lower<'_, EventSources<'_>>,
    id: &str,
    pred: ValueId,
    stack: &mut Vec<String>,
) -> Option<()> {
    let op = l.graph.nodes[id].op.clone();
    l.sources.statements.push((id.to_owned(), pred));
    match op.as_str() {
        "branch" => {
            let c = input(l, id, "cond", &PinType::Bool)?[0];
            let yes = and(l, pred, c);
            let not = l.b.un(UnOp::Not, ScalarTy::I32, c);
            let no = and(l, pred, not);
            walk(l, id, "true", yes, stack)?;
            walk(l, id, "false", no, stack)
        }
        "gate" => {
            let open = input(l, id, "open", &PinType::Bool)?[0];
            let p = and(l, pred, open);
            walk(l, id, "out", p, stack)
        }
        "sequence" => ["then0", "then1", "then2"]
            .iter()
            .try_for_each(|p| walk(l, id, p, pred, stack)),
        _ if op.starts_with("set.") || op.starts_with("var.set.") => {
            set(l, id, &op, pred)?;
            l.memo.clear();
            walk(l, id, "out", pred, stack)
        }
        _ if op.starts_with("raise.") => {
            raise(l, id, &op[6..], pred)?;
            walk(l, id, "out", pred, stack)
        }
        _ => l.unsupported(
            Some(id),
            None,
            format!("`{op}` has no compiled form in event graphs yet"),
        ),
    }
}

/// `set.<C>[.<field>]` or `var.set.<v>`.
fn set(l: &mut Lower<'_, EventSources<'_>>, id: &str, op: &str, pred: ValueId) -> Option<()> {
    let graph = l.graph;
    if !is_self(graph, graph.nodes[id].inputs.get("entity")) {
        return l.unsupported(
            Some(id),
            Some("entity"),
            "writing another entity's components isn't compiled yet (only the graph's own entity)"
                .into(),
        );
    }
    let (slot, field) = match op.strip_prefix("var.set.") {
        Some(var) => {
            let vars = l.sources.vars.name;
            let slot = l.sources.slot(vars)?;
            (
                slot,
                l.sources.columns[slot]
                    .fields
                    .iter()
                    .position(|(n, _)| n == var),
            )
        }
        None => l.sources.target(&op[4..])?,
    };
    let fields: Vec<usize> = match field {
        Some(f) => vec![f],
        None => (0..l.sources.columns[slot].fields.len()).collect(),
    };
    let ty = match field {
        Some(f) => PinType::of(&field_type(l.sources.columns[slot].info, f)?),
        None => PinType::Named(l.sources.columns[slot].name.clone()),
    };
    let Some(value) = input(l, id, "value", &ty) else {
        return l.unsupported(
            Some(id),
            Some("value"),
            format!("`{op}`'s value isn't a number, bool or vector"),
        );
    };
    let mut rest = value.as_slice();
    for f in fields {
        let n = l.sources.columns[slot].fields[f].1.as_ref()?.len();
        let (now, later) = rest.split_at(n.min(rest.len()));
        l.sources.assign(&mut l.b, slot, f, now, pred)?;
        rest = later;
    }
    Some(())
}

fn field_type(info: &'static TypeInfo, field: usize) -> Option<concordia_reflect::TypeRef> {
    match &info.shape {
        Shape::Struct { fields, .. } => fields.get(field).map(|f| f.ty),
        _ => None,
    }
}

/// `raise.<S>`: a flag (the predicate) and the payload's fields.
fn raise(l: &mut Lower<'_, EventSources<'_>>, id: &str, signal: &str, pred: ValueId) -> Option<()> {
    let graph = l.graph;
    if !is_self(graph, graph.nodes[id].inputs.get("target")) {
        return l.unsupported(
            Some(id),
            Some("target"),
            "raising on another entity isn't compiled yet (only the graph's own entity)".into(),
        );
    }
    let Some((info, fields)) = l.sources.signal_fields(signal) else {
        return l.unsupported(
            Some(id),
            None,
            format!("signal `{signal}` has fields that aren't numbers, bools or vectors"),
        );
    };
    let stride = wire::layout(info).ok()?.stride();
    let Shape::Struct { fields: infos, .. } = &info.shape else {
        let site = RaiseSite {
            signal: signal.to_owned(),
            stride,
            payload: vec![],
        };
        l.sources.raises.push((site, pred, vec![]));
        return Some(());
    };
    let mut payload = Vec::new();
    let mut values = Vec::new();
    for ((pin, scalars), f) in fields.iter().zip(infos) {
        values.extend(input(l, id, pin, &PinType::of(&f.ty))?);
        payload.extend(scalars.iter().copied());
    }
    let site = RaiseSite {
        signal: signal.to_owned(),
        stride,
        payload,
    };
    l.sources.raises.push((site, pred, values));
    Some(())
}

/// The program: stores for written fields, then each raise's flag and payload.
fn finish(l: Lower<'_, EventSources<'_>>, name: &str, entry: &Entry, index: usize) -> Program {
    let pins: Vec<(String, String, Vec<ValueId>)> = l
        .lowered
        .iter()
        .flat_map(|(node, outputs)| {
            outputs
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(move |(pin, v)| (node.clone(), pin.clone(), v.clone()))
        })
        .collect();
    let s = l.sources;
    let mut stores = Vec::new();
    let mut results = Vec::new();
    for &(slot, field) in &s.written {
        let scalars = s.columns[slot].fields[field].1.clone().unwrap_or_default();
        for ((offset, kind), v) in scalars.into_iter().zip(&s.state[&(slot, field)]) {
            stores.push(Store { slot, offset, kind });
            results.push(*v);
        }
    }
    let mut raises = Vec::new();
    for (site, flag, payload) in s.raises {
        results.push(flag);
        results.extend(payload);
        raises.push(site);
    }
    let written: BTreeSet<usize> = s.written.iter().map(|(slot, _)| *slot).collect();
    let names = |keep: bool| -> Vec<String> {
        s.columns
            .iter()
            .enumerate()
            .filter(|(k, _)| written.contains(k) == keep)
            .map(|(_, c)| c.name.clone())
            .collect()
    };
    let (writes, reads) = (names(true), names(false));
    let vars = s.vars.name.to_owned();
    let with = match s.columns.iter().any(|c| c.name == vars) {
        true => vec![],
        false => vec![vars],
    };
    let entry_name = match entry {
        Entry::Tick => "tick".to_owned(),
        Entry::Signal(signal) => format!("on.{signal}"),
    };
    let columns = s
        .columns
        .iter()
        .map(|c| ColumnSlot {
            name: c.name.clone(),
            stride: wire::layout(c.info).map_or(0, |l| l.stride()),
        })
        .collect();
    let raw = l.b.finish(results);
    Program {
        name: match index {
            0 => format!("graph.{name}.{entry_name}"),
            n => format!("graph.{name}.{entry_name}.{n}"),
        },
        entry: entry.clone(),
        reads,
        writes,
        with,
        columns,
        loads: s.loads,
        stores,
        raises,
        func: opt::optimize(raw.clone()),
        trace: TraceMap {
            raw,
            statements: s.statements,
            pins,
        },
    }
}
