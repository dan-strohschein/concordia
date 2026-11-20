//! The conformance suite on the WASM backend, against `conformance_guest.wasm` (`just wasm`).

use std::sync::Arc;

use concordia_script::{ModuleSource, ScriptBackend, ScriptModule};
use concordia_script_wasm::{WasmBackend, WasmConfig};

fn load() -> Arc<dyn ScriptModule> {
    let path =
        concordia_tests::wasm_fixture_path("conformance_guest").unwrap_or_else(|e| panic!("{e}"));
    WasmBackend::new(WasmConfig::default())
        .expect("engine")
        .load(&ModuleSource::File(path))
        .expect("the conformance component loads")
}

concordia_script::conformance_suite!(load);
