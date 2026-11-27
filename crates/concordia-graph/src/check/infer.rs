//! Type inference in data order: literals and links checked against pin types, numeric generics
//! bound per node (H6).

use std::collections::BTreeMap;

use crate::{GenericKind, Graph, Input, Literal, NodeDef, PinDef, PinType};

use super::{Code, Diagnostic};

/// Output pin types: (node, pin) → type.
pub(super) type PinTypes = BTreeMap<(String, String), PinType>;

/// What arrives at an input.
enum Arrival<'a> {
    Typed(PinType),
    Value(&'a Literal),
    Unknown,
}

/// Infers and checks every node in `order`.
pub(super) fn infer(
    graph: &Graph,
    defs: &BTreeMap<&str, NodeDef>,
    order: &[&str],
    diags: &mut Vec<Diagnostic>,
) -> PinTypes {
    let mut types = PinTypes::new();
    for id in order {
        node(graph, id, &defs[id], &mut types, diags);
    }
    types
}

fn node(graph: &Graph, id: &str, def: &NodeDef, types: &mut PinTypes, diags: &mut Vec<Diagnostic>) {
    let arrivals: Vec<(&PinDef, Arrival)> = def
        .inputs
        .iter()
        .filter_map(|p| {
            graph.nodes[id]
                .inputs
                .get(&p.name)
                .map(|i| (p, arrival(i, types)))
        })
        .collect();
    let bound = bind_generics(id, def, &arrivals, diags);
    for (pin, arrival) in &arrivals {
        let Some(expected) = concrete(&pin.ty, &bound) else {
            continue;
        };
        if let Some(got) = mismatch(arrival, &expected) {
            diags.push(Diagnostic::at(
                Code::type_mismatch,
                Some(id),
                Some(&pin.name),
                format!(
                    "`{}`'s `{}` expects {expected} but gets {got}",
                    def.op, pin.name
                ),
            ));
        }
    }
    for p in &def.outputs {
        if let Some(ty) = concrete(&p.ty, &bound) {
            types.insert((id.to_owned(), p.name.clone()), ty);
        }
    }
}

fn arrival<'a>(input: &'a Input, types: &PinTypes) -> Arrival<'a> {
    match input {
        Input::Value(v) => Arrival::Value(v),
        Input::Link(src, pin) => types
            .get(&(src.clone(), pin.clone()))
            .map_or(Arrival::Unknown, |t| Arrival::Typed(t.clone())),
    }
}

/// Binds each generic to the first linked type (else the first value's type) on its pins.
fn bind_generics(
    id: &str,
    def: &NodeDef,
    arrivals: &[(&PinDef, Arrival)],
    diags: &mut Vec<Diagnostic>,
) -> BTreeMap<GenericKind, PinType> {
    let mut bound = BTreeMap::new();
    for kind in [GenericKind::Numeric, GenericKind::Scalar] {
        let on_kind = || {
            arrivals
                .iter()
                .filter(|(p, _)| p.ty == PinType::Generic(kind))
        };
        let linked = on_kind().find_map(|(p, a)| match a {
            Arrival::Typed(t) => Some((p, t.clone())),
            _ => None,
        });
        let valued = || {
            on_kind().find_map(|(p, a)| match a {
                Arrival::Value(v) => Some((p, literal_type(v))),
                _ => None,
            })
        };
        let Some((pin, ty)) = linked.or_else(valued) else {
            continue;
        };
        match kind.admits(&ty) {
            true => {
                bound.insert(kind, ty);
            }
            false => diags.push(Diagnostic::at(
                Code::generic_unresolved,
                Some(id),
                Some(&pin.name),
                format!(
                    "`{}` takes a {} type here, not {ty}",
                    def.op,
                    PinType::Generic(kind)
                ),
            )),
        }
    }
    bound
}

/// The pin's type with generics replaced (`None`: an unbound generic).
fn concrete(ty: &PinType, bound: &BTreeMap<GenericKind, PinType>) -> Option<PinType> {
    match ty {
        PinType::Generic(kind) => bound.get(kind).cloned(),
        other => Some(other.clone()),
    }
}

/// What arrived, when it isn't `expected`.
fn mismatch(arrival: &Arrival, expected: &PinType) -> Option<String> {
    match arrival {
        Arrival::Typed(t) if t != expected => Some(t.to_string()),
        Arrival::Value(v) if !literal_fits(v, expected) => {
            Some(format!("a {} value", literal_type(v)))
        }
        _ => None,
    }
}

/// A value's own type.
fn literal_type(v: &Literal) -> PinType {
    match v {
        Literal::Bool(_) => PinType::Bool,
        Literal::Int(_) => PinType::I32,
        Literal::Float(_) => PinType::F32,
        Literal::Text(_) => PinType::String,
        Literal::Vec2(_) => PinType::Vec2,
        Literal::Vec3(_) => PinType::Vec3,
        Literal::Vec4(_) => PinType::Vec4,
    }
}

/// Whether a value can stand for `ty` (integers for numbers, text for names and assets).
fn literal_fits(v: &Literal, ty: &PinType) -> bool {
    match (v, ty) {
        (_, PinType::Option(inner)) => literal_fits(v, inner),
        (Literal::Bool(_), PinType::Bool)
        | (Literal::Int(_), PinType::I32 | PinType::I64 | PinType::F32)
        | (Literal::Float(_), PinType::F32)
        | (Literal::Text(_), PinType::String | PinType::AssetRef | PinType::Named(_))
        | (Literal::Vec2(_), PinType::Vec2)
        | (Literal::Vec3(_), PinType::Vec3)
        | (Literal::Vec4(_), PinType::Vec4 | PinType::Quat) => true,
        _ => false,
    }
}
