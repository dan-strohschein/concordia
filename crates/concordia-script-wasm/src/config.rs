//! Backend settings: fuel, NaN canonicalization and the memory cap.

/// A run's fuel budget: `per_run`, plus `per_row` for each batch row and delivered signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fuel {
    /// Fuel every run gets.
    pub per_run: u64,
    /// Extra fuel per batch row and per delivered signal.
    pub per_row: u64,
}

impl Fuel {
    /// The budget for a run over `rows` rows.
    pub fn budget(&self, rows: usize) -> u64 {
        self.per_run
            .saturating_add(self.per_row.saturating_mul(rows as u64))
    }
}

/// Settings for a [`WasmBackend`](crate::WasmBackend).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasmConfig {
    /// Fuel per run; `None` runs unmetered (benchmarks only).
    pub fuel: Option<Fuel>,
    /// Canonicalize NaNs so float results are bit-identical on every host.
    pub nan_canonicalization: bool,
    /// Largest linear memory an instance may grow to, in bytes.
    pub memory_cap: usize,
    /// Most instances alive at once (the pooling allocator's size; parallel runs need one each).
    pub max_instances: u32,
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            fuel: Some(Fuel {
                per_run: 10_000_000,
                per_row: 50_000,
            }),
            nan_canonicalization: true,
            memory_cap: 64 << 20,
            max_instances: 64,
        }
    }
}

impl WasmConfig {
    /// No fuel and no NaN canonicalization: for benchmarks that measure the guards' cost.
    pub fn unguarded() -> Self {
        Self {
            fuel: None,
            nan_canonicalization: false,
            ..Self::default()
        }
    }
}
