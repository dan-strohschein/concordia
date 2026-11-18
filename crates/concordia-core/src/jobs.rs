//! Data-parallel jobs on an owned thread pool with deterministic chunking.
//!
//! Chunk size is always chosen by the caller, never derived from the thread
//! count, and reductions combine in chunk-index order, so results are
//! bit-identical for any number of threads.

use std::num::NonZeroUsize;

use rayon::prelude::*;

/// Errors from creating a job system.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// The OS refused to create worker threads.
    #[error("failed to build job thread pool: {0}")]
    Pool(#[from] rayon::ThreadPoolBuildError),
}

/// A thread pool owned by the engine (never rayon's global pool).
///
/// ```
/// use std::num::NonZeroUsize;
/// use concordia_core::jobs::JobSystem;
/// let jobs = JobSystem::new(NonZeroUsize::new(2).unwrap())?;
/// let chunk = NonZeroUsize::new(2).unwrap();
/// let sum = jobs.par_map_reduce(&[1, 2, 3, 4, 5], chunk, |c| c.iter().sum::<i32>(), |a, b| a + b);
/// assert_eq!(sum, Some(15));
/// # Ok::<(), concordia_core::jobs::JobError>(())
/// ```
#[derive(Debug)]
pub struct JobSystem {
    pool: rayon::ThreadPool,
    threads: NonZeroUsize,
}

impl JobSystem {
    /// Creates a pool with exactly `threads` workers.
    pub fn new(threads: NonZeroUsize) -> Result<Self, JobError> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads.get())
            .thread_name(|i| format!("concordia-job-{i}"))
            .build()?;
        Ok(Self { pool, threads })
    }

    /// Number of worker threads.
    pub fn threads(&self) -> NonZeroUsize {
        self.threads
    }

    /// Calls `f(chunk_index, chunk)` for each `chunk`-sized slice of `data`, in parallel.
    pub fn par_chunks_mut<T, F>(&self, data: &mut [T], chunk: NonZeroUsize, f: F)
    where
        T: Send,
        F: Fn(usize, &mut [T]) + Sync,
    {
        self.pool.install(|| {
            data.par_chunks_mut(chunk.get())
                .enumerate()
                .for_each(|(index, slice)| f(index, slice));
        });
    }

    /// Maps each chunk in parallel, then reduces the results in chunk order; `None` if `data` is empty.
    pub fn par_map_reduce<T, A, M, R>(
        &self,
        data: &[T],
        chunk: NonZeroUsize,
        map: M,
        reduce: R,
    ) -> Option<A>
    where
        T: Sync,
        A: Send,
        M: Fn(&[T]) -> A + Sync,
        R: Fn(A, A) -> A,
    {
        let partials: Vec<A> = self
            .pool
            .install(|| data.par_chunks(chunk.get()).map(&map).collect());
        partials.into_iter().reduce(reduce)
    }
}
