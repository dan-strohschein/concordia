//! Loading native module libraries (copy-on-load) and calling their function tables.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use concordia_script::{
    Batch, ModuleDesc, ModuleSource, ModuleStats, RunReport, ScriptBackend, ScriptError,
    ScriptFault, ScriptModule, TestBackend, TickContext,
};
use concordia_script_abi::{
    ABI_VERSION, Buffer, ModuleV1, STATUS_OK, TABLE_SYMBOL, TableFn, VERSION_SYMBOL, VersionFn,
};
use concordia_script_guest::Module;
use concordia_script_protocol::codec::{GuestError, decode_reply, encode_call};
use libloading::Library;

/// Distinguishes copies made by different backends in one process.
static NEXT_COPY: AtomicU64 = AtomicU64::new(0);

/// The native backend: dylib modules, plus modules linked into the engine.
pub struct NativeBackend {
    linked: TestBackend,
    copy_dir: PathBuf,
    /// Every library loaded, kept until the backend is dropped (play stops).
    libraries: Mutex<Vec<(Arc<Library>, PathBuf)>>,
}

impl std::fmt::Debug for NativeBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeBackend")
            .field("copy_dir", &self.copy_dir)
            .field("loaded", &self.loaded_libraries())
            .finish()
    }
}

fn load_error(path: &Path, message: impl std::fmt::Display) -> ScriptError {
    ScriptError::Load(format!("{}: {message}", path.display()))
}

impl NativeBackend {
    /// A backend that copies libraries into `copy_dir` before loading them.
    pub fn new(copy_dir: impl Into<PathBuf>) -> Self {
        Self {
            linked: TestBackend::new(),
            copy_dir: copy_dir.into(),
            libraries: Mutex::default(),
        }
    }

    /// Links module `M` into the engine (the static path, `ModuleSource::Linked`).
    #[must_use]
    pub fn link<M: Module>(mut self) -> Self {
        self.linked = std::mem::take(&mut self.linked).link::<M>();
        self
    }

    /// Loads the module library at `path`: copies it, checks its ABI version, then reads its
    /// function table.
    pub fn load_library(&self, path: &Path) -> Result<Arc<NativeModule>, ScriptError> {
        let copy = self.copy(path)?;
        // SAFETY: loading runs the library's initializers. Module libraries are built from the
        // project's own sources with the guest SDK (the same trust as linking them statically);
        // sandboxed modules use the WASM backend instead.
        let library = unsafe { Library::new(&copy) }.map_err(|e| load_error(path, e))?;
        let library = Arc::new(library);
        let table = read_table(&library, path)?;
        self.libraries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((Arc::clone(&library), copy.clone()));
        Ok(Arc::new(NativeModule {
            table,
            _library: library,
            copy,
            calls_in: AtomicU64::new(0),
        }))
    }

    /// Copies `path` to a fresh name in the copy directory.
    fn copy(&self, path: &Path) -> Result<PathBuf, ScriptError> {
        std::fs::create_dir_all(&self.copy_dir).map_err(|e| load_error(&self.copy_dir, e))?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| load_error(path, "not a library file name"))?;
        let n = NEXT_COPY.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "{stem}-{}-{n}.{}",
            std::process::id(),
            std::env::consts::DLL_EXTENSION
        );
        let copy = self.copy_dir.join(name);
        std::fs::copy(path, &copy).map_err(|e| load_error(path, e))?;
        Ok(copy)
    }

    /// Libraries loaded so far (old builds stay loaded until the backend is dropped).
    pub fn loaded_libraries(&self) -> usize {
        self.libraries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

impl Drop for NativeBackend {
    fn drop(&mut self) {
        let libraries = std::mem::take(
            self.libraries
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for (_, copy) in libraries {
            // Unix keeps a mapped library alive after its file is removed; elsewhere this may
            // fail while a module is still in use, which only leaves a stale copy behind.
            let _ = std::fs::remove_file(copy);
        }
    }
}

impl ScriptBackend for NativeBackend {
    fn name(&self) -> &'static str {
        "native"
    }

    fn load(&self, source: &ModuleSource) -> Result<Arc<dyn ScriptModule>, ScriptError> {
        match source {
            ModuleSource::Linked(_) => self.linked.load(source),
            ModuleSource::File(path) => Ok(self.load_library(path)?),
            ModuleSource::Bytes(_) => Err(ScriptError::Unsupported {
                backend: self.name(),
                source_kind: "bytes (load a library file)".into(),
            }),
        }
    }
}

/// Checks the ABI version, then reads the table (in that order: a module for another ABI
/// may lay its table out differently).
fn read_table(library: &Library, path: &Path) -> Result<ModuleV1, ScriptError> {
    // SAFETY: the symbol is declared by the ABI as `extern "C" fn() -> u32`.
    let version = unsafe { library.get::<VersionFn>(VERSION_SYMBOL.as_bytes()) }
        .map_err(|e| load_error(path, format!("not a Concordia module library ({e})")))?;
    let found = version();
    if found != ABI_VERSION {
        return Err(load_error(
            path,
            format!(
                "the module speaks native ABI version {found}; this engine speaks version {ABI_VERSION}"
            ),
        ));
    }
    // SAFETY: a version-1 library declares this symbol as `extern "C" fn() -> *const ModuleV1`.
    let table_fn = unsafe { library.get::<TableFn>(TABLE_SYMBOL.as_bytes()) }
        .map_err(|e| load_error(path, e))?;
    let table = table_fn();
    if table.is_null() {
        return Err(load_error(path, "the module returned no function table"));
    }
    // SAFETY: non-null, and a version-1 library returns a pointer to its static table.
    let table = unsafe { *table };
    if table.abi_version != ABI_VERSION {
        return Err(load_error(
            path,
            "the table's ABI version disagrees with the library's",
        ));
    }
    Ok(table)
}

/// A loaded module library.
pub struct NativeModule {
    table: ModuleV1,
    /// Keeps the code `table` points into loaded.
    _library: Arc<Library>,
    copy: PathBuf,
    calls_in: AtomicU64,
}

impl NativeModule {
    /// Where the loaded copy lives.
    pub fn copy_path(&self) -> &Path {
        &self.copy
    }

    /// Takes a buffer the module filled, then frees it with the module's `free`.
    fn take(&self, buffer: Buffer) -> Vec<u8> {
        let bytes = if buffer.ptr.is_null() || buffer.len == 0 {
            Vec::new()
        } else {
            // SAFETY: the module wrote `len` initialized bytes at `ptr` (the ABI contract).
            unsafe { std::slice::from_raw_parts(buffer.ptr, buffer.len) }.to_vec()
        };
        // SAFETY: the buffer came from this module and is freed exactly once.
        unsafe { (self.table.free)(buffer) };
        bytes
    }
}

impl ScriptModule for NativeModule {
    fn describe(&self) -> Result<ModuleDesc, ScriptFault> {
        self.calls_in.fetch_add(1, Ordering::Relaxed);
        let mut out = Buffer::EMPTY;
        // SAFETY: `out` is a valid buffer slot for the call.
        let status = unsafe { (self.table.describe)(&mut out) };
        let bytes = self.take(out);
        if status != STATUS_OK {
            return Err(ScriptFault::Trap(
                String::from_utf8_lossy(&bytes).into_owned(),
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|e| ScriptFault::Protocol(format!("description: {e}")))
    }

    fn run(&self, processor: &str, ctx: &TickContext, batch: &Batch) -> RunReport {
        self.calls_in.fetch_add(1, Ordering::Relaxed);
        let call = encode_call(processor, ctx, batch);
        let mut out = Buffer::EMPTY;
        // SAFETY: `call` is a live slice of `call.len()` bytes and `out` a valid buffer slot.
        let status = unsafe { (self.table.run)(call.as_ptr(), call.len(), &mut out) };
        let bytes = self.take(out);
        if status != STATUS_OK {
            let message = String::from_utf8_lossy(&bytes).into_owned();
            return RunReport {
                result: Err(ScriptFault::Protocol(message)),
                logs: Vec::new(),
            };
        }
        match decode_reply(&bytes) {
            Ok(reply) => RunReport {
                result: reply.outcome.map_err(|e| match e {
                    GuestError::Returned(m) => ScriptFault::Returned(m),
                    GuestError::Panicked(m) => ScriptFault::Trap(m),
                    GuestError::BadCall(m) => ScriptFault::Protocol(m),
                }),
                logs: reply.logs,
            },
            Err(e) => RunReport {
                result: Err(ScriptFault::Protocol(e.to_string())),
                logs: Vec::new(),
            },
        }
    }

    fn stats(&self) -> ModuleStats {
        ModuleStats {
            calls_in: self.calls_in.load(Ordering::Relaxed),
            calls_out: 0,
        }
    }
}
