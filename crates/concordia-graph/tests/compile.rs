//! Compiling pure Function graphs (M4 4.10a, J2–J4): IR, interpreter and WASM agree bit for bit.

use std::collections::BTreeMap;
use std::time::Instant;

use concordia_graph::ir::{Inst, Scalar};
use concordia_graph::{
    Code, Graph, GraphKind, Input, Literal, Node, NodeLibrary, PinDecl, Signature, check,
    compile_function,
};
use proptest::prelude::*;

fn id(n: u32) -> String {
    format!("0193b0f2-0000-7000-8000-{n:012x}")
}

fn link(n: u32, pin: &str) -> Input {
    Input::Link(id(n), pin.into())
}

fn decls(pins: &[(&str, &str)]) -> Vec<PinDecl> {
    pins.iter()
        .map(|(n, t)| PinDecl {
            name: (*n).into(),
            ty: (*t).into(),
        })
        .collect()
}

fn function(inputs: &[(&str, &str)], outputs: &[(&str, &str)], nodes: Vec<(u32, Node)>) -> Graph {
    Graph {
        id: id(1),
        kind: GraphKind::Function,
        version: 1,
        signature: Signature {
            inputs: decls(inputs),
            outputs: decls(outputs),
            pure: true,
        },
        variables: vec![],
        nodes: nodes.into_iter().map(|(n, node)| (id(n), node)).collect(),
        exec: vec![],
        layout: BTreeMap::new(),
    }
}

/// Runs the compiled module's `run` export with Wasmtime (NaN canonicalization on, as scripts).
fn run_wasm(wasm: &[u8], args: &[Scalar], results: usize) -> Vec<Scalar> {
    let mut config = wasmtime::Config::new();
    config.cranelift_nan_canonicalization(true);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let module = wasmtime::Module::new(&engine, wasm).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let run = instance
        .get_func(&mut store, "run")
        .expect("a `run` export");
    let params: Vec<wasmtime::Val> = args
        .iter()
        .map(|a| match *a {
            Scalar::I32(v) => wasmtime::Val::I32(v),
            Scalar::I64(v) => wasmtime::Val::I64(v),
            Scalar::F32(v) => wasmtime::Val::F32(v.to_bits()),
        })
        .collect();
    let mut out = vec![wasmtime::Val::I32(0); results];
    run.call(&mut store, &params, &mut out).unwrap();
    out.iter()
        .map(|v| match *v {
            wasmtime::Val::I32(x) => Scalar::I32(x),
            wasmtime::Val::I64(x) => Scalar::I64(x),
            wasmtime::Val::F32(x) => Scalar::F32(f32::from_bits(x)),
            ref other => panic!("unexpected result {other:?}"),
        })
        .collect()
}

fn bits(values: &[Scalar]) -> Vec<(u8, u64)> {
    values.iter().map(Scalar::bits).collect()
}

/// A random well-typed pure graph: a recipe list over typed value pools.
mod generate {
    use super::*;

    #[derive(Clone, Copy, PartialEq)]
    pub enum T {
        F,
        V,
        I,
        B,
    }

    pub struct Builder {
        pub nodes: Vec<(u32, Node)>,
        pools: [Vec<Input>; 4],
        next: u32,
    }

    const FLOATS: [f64; 8] = [0.0, -0.0, 1.0, -2.5, 0.5, 1e30, -1e-30, 3.0];
    const INTS: [i64; 6] = [0, 1, -1, 7, i32::MAX as i64, i32::MIN as i64];

    impl Builder {
        pub fn new() -> Self {
            let mut b = Self {
                nodes: vec![(2, Node::new("graph.input"))],
                pools: Default::default(),
                next: 10,
            };
            for p in ["a", "b", "c"] {
                b.pools[T::F as usize].push(link(2, p));
            }
            for p in ["v", "w"] {
                b.pools[T::V as usize].push(link(2, p));
            }
            for p in ["n", "m"] {
                b.pools[T::I as usize].push(link(2, p));
            }
            b
        }

        /// An input of type `t`: a pooled value or (one in five) a literal.
        fn pick(&self, t: T, seed: u8) -> Input {
            let pool = &self.pools[t as usize];
            let literal = seed.is_multiple_of(5) || pool.is_empty();
            let s = seed as usize / 5;
            match (literal, t) {
                (true, T::F) => Input::Value(Literal::Float(FLOATS[s % FLOATS.len()])),
                (true, T::I) => Input::Value(Literal::Int(INTS[s % INTS.len()])),
                (true, T::V) => {
                    let f = |k: usize| FLOATS[(s + k) % FLOATS.len()] as f32;
                    Input::Value(Literal::Vec3([f(0), f(1), f(2)]))
                }
                (true, T::B) => Input::Value(Literal::Bool(s.is_multiple_of(2))),
                (false, _) => pool[s % pool.len()].clone(),
            }
        }

        fn add(&mut self, op: &str, inputs: &[(&str, T)], outputs: &[(&str, T)], seeds: [u8; 3]) {
            let n = self.next;
            self.next += 1;
            let mut node = Node::new(op);
            for (k, (pin, t)) in inputs.iter().enumerate() {
                node = node.with_input(pin, self.pick(*t, seeds[k % 3].wrapping_add(k as u8 * 37)));
            }
            self.nodes.push((n, node));
            for (pin, t) in outputs {
                self.pools[*t as usize].push(link(n, pin));
            }
        }

        /// Applies recipe `r` (some need a bool already made; those fall back to a float op).
        pub fn recipe(&mut self, r: u8, seeds: [u8; 3]) {
            use T::*;
            let bools = !self.pools[B as usize].is_empty();
            let arith = ["math.add", "math.sub", "math.mul", "math.div"];
            let cmp = [
                "math.gt", "math.lt", "math.ge", "math.le", "math.eq", "math.ne",
            ];
            let o = |t| [("out", t)];
            match r % 24 {
                0 => self.add(
                    arith[seeds[0] as usize % 4],
                    &[("a", F), ("b", F)],
                    &o(F),
                    seeds,
                ),
                1 => self.add(
                    arith[seeds[0] as usize % 3],
                    &[("a", V), ("b", V)],
                    &o(V),
                    seeds,
                ),
                2 => self.add(
                    arith[seeds[0] as usize % 4],
                    &[("a", I), ("b", I)],
                    &o(I),
                    seeds,
                ),
                3 => self.add("math.lerp", &[("a", F), ("b", F), ("t", F)], &o(F), seeds),
                4 => self.add("math.lerp", &[("a", V), ("b", V), ("t", F)], &o(V), seeds),
                5 => self.add(
                    ["math.min", "math.max"][seeds[0] as usize % 2],
                    &[("a", F), ("b", F)],
                    &o(F),
                    seeds,
                ),
                6 => self.add(
                    ["math.min", "math.max"][seeds[0] as usize % 2],
                    &[("a", I), ("b", I)],
                    &o(I),
                    seeds,
                ),
                7 => self.add(
                    "math.clamp",
                    &[("x", F), ("lo", F), ("hi", F)],
                    &o(F),
                    seeds,
                ),
                8 => self.add(
                    "math.clamp",
                    &[("x", I), ("lo", I), ("hi", I)],
                    &o(I),
                    seeds,
                ),
                9 => self.add(
                    "math.abs",
                    &[("x", [F, I][seeds[0] as usize % 2])],
                    &o([F, I][seeds[0] as usize % 2]),
                    seeds,
                ),
                10 => self.add("math.neg", &[("x", V)], &o(V), seeds),
                11 => self.add("math.saturate", &[("x", F)], &o(F), seeds),
                12 => self.add(
                    cmp[seeds[0] as usize % 6],
                    &[("a", F), ("b", F)],
                    &o(B),
                    seeds,
                ),
                13 => self.add(
                    cmp[seeds[0] as usize % 6],
                    &[("a", I), ("b", I)],
                    &o(B),
                    seeds,
                ),
                14 if bools => self.add(
                    ["logic.and", "logic.or"][seeds[0] as usize % 2],
                    &[("a", B), ("b", B)],
                    &o(B),
                    seeds,
                ),
                15 if bools => self.add("logic.not", &[("x", B)], &o(B), seeds),
                16 => self.add("conv.i32_to_f32", &[("x", I)], &o(F), seeds),
                17 => self.add("conv.f32_to_i32", &[("x", F)], &o(I), seeds),
                18 => self.add("conv.vec3", &[("x", F), ("y", F), ("z", F)], &o(V), seeds),
                19 => self.add(
                    "conv.split_vec3",
                    &[("v", V)],
                    &[("x", F), ("y", F), ("z", F)],
                    seeds,
                ),
                20 => self.add(
                    ["vector.length", "vector.normalize"][seeds[0] as usize % 2],
                    &[("v", V)],
                    &o([F, V][seeds[0] as usize % 2]),
                    seeds,
                ),
                21 => self.add(
                    ["vector.dot", "vector.distance"][seeds[0] as usize % 2],
                    &[("a", V), ("b", V)],
                    &o(F),
                    seeds,
                ),
                22 => self.add(
                    "math.neg",
                    &[("x", [F, I][seeds[0] as usize % 2])],
                    &o([F, I][seeds[0] as usize % 2]),
                    seeds,
                ),
                _ => self.add("math.mul", &[("a", F), ("b", F)], &o(F), seeds),
            }
        }

        /// The graph: the last value of each type is an output.
        pub fn finish(mut self) -> Graph {
            let mut outputs = vec![];
            let mut out = Node::new("graph.output");
            for (t, name, ty) in [
                (T::F, "of", "f32"),
                (T::V, "ov", "Vec3"),
                (T::I, "oi", "i32"),
                (T::B, "ob", "bool"),
            ] {
                if let Some(last) = self.pools[t as usize].last() {
                    out = out.with_input(name, last.clone());
                    outputs.push((name, ty));
                }
            }
            self.nodes.push((3, out));
            let inputs = [
                ("a", "f32"),
                ("b", "f32"),
                ("c", "f32"),
                ("v", "Vec3"),
                ("w", "Vec3"),
                ("n", "i32"),
                ("m", "i32"),
            ];
            function(&inputs, &outputs, self.nodes)
        }
    }
}

fn special_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        4 => -1e3f32..1e3f32,
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
        1 => Just(f32::NEG_INFINITY),
        1 => Just(-0.0f32),
        1 => Just(0.0f32),
        1 => any::<f32>(),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn compiled_graph_matches_interpreter(
        recipes in prop::collection::vec((any::<u8>(), any::<[u8; 3]>()), 1..40),
        floats in prop::collection::vec(special_f32(), 9),
        ints in prop::collection::vec(any::<i32>(), 2),
    ) {
        let mut b = generate::Builder::new();
        for (r, seeds) in recipes {
            b.recipe(r, seeds);
        }
        let graph = b.finish();
        let lib = NodeLibrary::core();
        let report = check(&graph, &lib);
        prop_assert!(report.is_ok(), "{:#?}", report.diagnostics);
        let compiled = compile_function(&graph, &lib).map_err(|d| TestCaseError::fail(format!("{d:#?}")))?;
        // a, b, c, v.xyz, w.xyz, n, m.
        let mut args: Vec<Scalar> = floats.iter().map(|f| Scalar::F32(*f)).collect();
        args.extend(ints.iter().map(|i| Scalar::I32(*i)));
        let interpreted = compiled.ir.eval(&args).unwrap();
        let wasm = run_wasm(&compiled.wasm, &args, compiled.results.len());
        prop_assert_eq!(bits(&interpreted), bits(&wasm));
    }
}

/// f(x, v) = (x + 2 × 3, |v|), with a node nothing uses.
fn small() -> Graph {
    function(
        &[("x", "f32"), ("v", "Vec3")],
        &[("y", "f32"), ("len", "f32")],
        vec![
            (2, Node::new("graph.input")),
            (
                4,
                Node::new("math.mul")
                    .with_input("a", Input::Value(Literal::Float(2.0)))
                    .with_input("b", Input::Value(Literal::Float(3.0))),
            ),
            (
                5,
                Node::new("math.add")
                    .with_input("a", link(2, "x"))
                    .with_input("b", link(4, "out")),
            ),
            (
                6,
                Node::new("math.mul")
                    .with_input("a", link(2, "x"))
                    .with_input("b", link(2, "x")),
            ),
            (7, Node::new("vector.length").with_input("v", link(2, "v"))),
            (
                3,
                Node::new("graph.output")
                    .with_input("y", link(5, "out"))
                    .with_input("len", link(7, "out")),
            ),
        ],
    )
}

#[test]
fn a_function_graph_computes_its_outputs() {
    let compiled = compile_function(&small(), &NodeLibrary::core()).unwrap();
    let args = [
        Scalar::F32(1.5),
        Scalar::F32(3.0),
        Scalar::F32(0.0),
        Scalar::F32(4.0),
    ];
    let want = [Scalar::F32(7.5), Scalar::F32(5.0)];
    assert_eq!(bits(&compiled.ir.eval(&args).unwrap()), bits(&want));
    assert_eq!(bits(&run_wasm(&compiled.wasm, &args, 2)), bits(&want));
}

#[test]
fn constant_folding_and_dead_nodes_are_removed() {
    let compiled = compile_function(&small(), &NodeLibrary::core()).unwrap();
    let insts = compiled.ir.insts();
    // 2 × 3 folded to 6; x × x gone.
    assert!(
        insts
            .iter()
            .any(|i| matches!(i, Inst::Const(Scalar::F32(v)) if *v == 6.0)),
        "{insts:?}"
    );
    let muls = insts
        .iter()
        .filter(|i| format!("{i:?}").contains("Mul"))
        .count();
    // |v| = sqrt(x² + y² + z²): three multiplies, none from the dead node or the folded one.
    assert_eq!(muls, 3, "{insts:?}");
}

#[test]
fn compiled_module_validates() {
    let compiled = compile_function(&small(), &NodeLibrary::core()).unwrap();
    wasmparser::Validator::new()
        .validate_all(&compiled.wasm)
        .expect("a valid core module");
}

#[test]
fn unsupported_nodes_are_located() {
    let g = function(
        &[],
        &[("e", "EntityId")],
        vec![
            (2, Node::new("entity.self")),
            (
                3,
                Node::new("graph.output").with_input("e", link(2, "entity")),
            ),
        ],
    );
    let lib = NodeLibrary::core();
    let errors = compile_function(&g, &lib).unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == Code::unsupported_node),
        "{errors:?}"
    );
    // A graph that doesn't check reports the checker's diagnostics instead.
    let mut broken = small();
    broken.nodes.get_mut(&id(5)).unwrap().op = "math.addd".into();
    let errors = compile_function(&broken, &lib).unwrap_err();
    assert_eq!(errors[0].code, Code::unknown_op);
}

#[test]
fn graph_compile_time_under_50ms_for_500_nodes() {
    let mut nodes = vec![(2, Node::new("graph.input"))];
    let mut prev = link(2, "x");
    for n in 0..500u32 {
        let op = ["math.add", "math.mul", "math.max", "math.sub"][n as usize % 4];
        nodes.push((
            10 + n,
            Node::new(op)
                .with_input("a", prev)
                .with_input("b", Input::Value(Literal::Float(1.25))),
        ));
        prev = link(10 + n, "out");
    }
    nodes.push((3, Node::new("graph.output").with_input("y", prev)));
    let g = function(&[("x", "f32")], &[("y", "f32")], nodes);
    let lib = NodeLibrary::core();
    let start = Instant::now();
    let compiled = compile_function(&g, &lib).unwrap();
    let ms = start.elapsed().as_secs_f64() * 1e3;
    println!("500 nodes: {ms:.2} ms, {} bytes", compiled.wasm.len());
    assert!(ms < 50.0, "{ms:.2} ms");
}
