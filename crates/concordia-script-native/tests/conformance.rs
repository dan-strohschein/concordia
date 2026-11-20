//! The conformance suite on the native backend's dylib path, against the conformance module
//! built as a library (`just native-fixtures`).

use std::sync::Arc;

use concordia_script::{ModuleSource, ScriptBackend, ScriptModule};
use concordia_script_native::NativeBackend;

fn load() -> Arc<dyn ScriptModule> {
    let path =
        concordia_tests::native_fixture_path("conformance_guest").unwrap_or_else(|e| panic!("{e}"));
    NativeBackend::new(std::env::temp_dir().join("concordia-native-tests"))
        .load(&ModuleSource::File(path))
        .expect("the conformance library loads")
}

concordia_script::conformance_suite!(load);
