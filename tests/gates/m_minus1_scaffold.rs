//! M-1 gate: the seed crate links and a WASM component round-trips through wasmtime.

use concordia_tests::wasm_fixture_path;
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

/// Host bindings generated from the fixture WIT.
#[allow(missing_docs)]
mod bindings {
    wasmtime::component::bindgen!({ world: "hello", path: "fixtures/wasm/hello-guest/wit" });
}
use bindings::Hello;

/// Host state with a closed WASI sandbox: no inherited stdio, env or filesystem.
struct HostState {
    log: Vec<String>,
    wasi: WasiCtx,
    table: ResourceTable,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            log: Vec::new(),
            wasi: WasiCtx::builder().build(),
            table: ResourceTable::new(),
        }
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl bindings::concordia::fixtures::host::Host for HostState {
    fn log(&mut self, message: String) {
        self.log.push(message);
    }
}

fn engine() -> Engine {
    let mut config = Config::new();
    config.wasm_component_model(true);
    Engine::new(&config).expect("engine")
}

fn guest_bytes() -> Vec<u8> {
    let path = wasm_fixture_path("hello_guest").unwrap_or_else(|e| panic!("{e}"));
    std::fs::read(path).expect("read fixture")
}

fn instantiate(engine: &Engine) -> (Store<HostState>, Hello) {
    let component = Component::new(engine, guest_bytes()).expect("compile component");
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).expect("link wasi");
    Hello::add_to_linker::<HostState, HasSelf<HostState>>(&mut linker, |s| s).expect("link host");
    let mut store = Store::new(engine, HostState::default());
    let hello = Hello::instantiate(&mut store, &component, &linker).expect("instantiate");
    (store, hello)
}

#[test]
fn seed_crate_is_linkable_from_gates() {
    assert!(!concordia_core::version().is_empty());
}

#[test]
fn guest_artifact_is_a_wasm_component() {
    assert!(wasmparser::Parser::is_component(&guest_bytes()));
}

#[test]
fn guest_component_exports_expected_world() {
    let engine = engine();
    let component = Component::new(&engine, guest_bytes()).expect("compile component");
    let mut names: Vec<String> = component
        .component_type()
        .exports(&engine)
        .map(|(name, _)| name.to_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["greet", "trap"]);
}

#[test]
fn host_calls_guest_greet_and_gets_greeting() {
    let (mut store, hello) = instantiate(&engine());
    assert_eq!(
        hello.call_greet(&mut store, "concordia").unwrap(),
        "Hello, concordia!"
    );
}

#[test]
fn guest_calls_host_log_during_greet() {
    let (mut store, hello) = instantiate(&engine());
    hello.call_greet(&mut store, "concordia").unwrap();
    assert_eq!(store.data().log, ["greet called with concordia"]);
}

#[test]
fn guest_trap_is_contained_and_host_recovers() {
    let engine = engine();
    let (mut store, hello) = instantiate(&engine);
    assert!(
        hello.call_trap(&mut store).is_err(),
        "trap must surface as an error"
    );
    let (mut fresh, hello) = instantiate(&engine);
    assert_eq!(
        hello.call_greet(&mut fresh, "again").unwrap(),
        "Hello, again!"
    );
}

#[test]
fn missing_fixture_error_names_the_fix() {
    let err = wasm_fixture_path("does_not_exist").unwrap_err();
    assert!(err.contains("just wasm"), "{err}");
}
