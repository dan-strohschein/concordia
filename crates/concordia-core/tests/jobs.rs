//! Job system: parallel chunks with thread-count-invariant results (M0 phase 0.1).

use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};

use concordia_core::jobs::JobSystem;
use concordia_core::rng::SimRng;

const CHUNK: NonZeroUsize = NonZeroUsize::new(4096).unwrap();

fn jobs(threads: usize) -> JobSystem {
    JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap()
}

#[test]
fn par_chunks_mut_visits_every_element_once() {
    for threads in [1, 2, 4, 8] {
        let mut data = vec![0u8; 1_000_003];
        jobs(threads).par_chunks_mut(&mut data, CHUNK, |_, chunk| {
            chunk.iter_mut().for_each(|x| *x += 1);
        });
        assert!(data.iter().all(|&x| x == 1), "threads = {threads}");
    }
}

#[test]
fn par_chunks_mut_passes_chunk_index() {
    let mut data = vec![0usize; 10_000];
    jobs(4).par_chunks_mut(&mut data, CHUNK, |index, chunk| chunk.fill(index));
    assert_eq!(data[0], 0);
    assert_eq!(data[4096], 1);
    assert_eq!(data[9_999], 2);
}

#[test]
fn par_map_reduce_is_thread_count_invariant() {
    let mut rng = SimRng::new(9);
    let values: Vec<f32> = (0..1_000_000).map(|_| rng.next_f32() * 1e3).collect();
    let sum = |threads| {
        jobs(threads)
            .par_map_reduce(&values, CHUNK, |c| c.iter().sum::<f32>(), |a, b| a + b)
            .unwrap()
            .to_bits()
    };
    let expected = sum(1);
    for threads in [2, 3, 8] {
        assert_eq!(sum(threads), expected, "threads = {threads}");
    }
}

#[test]
fn par_map_reduce_of_empty_slice_is_none() {
    let empty: [u32; 0] = [];
    assert_eq!(
        jobs(2).par_map_reduce(&empty, CHUNK, |c| c.len(), |a, b| a + b),
        None
    );
}

#[test]
fn job_panic_propagates_to_caller() {
    let jobs = jobs(4);
    let mut data = vec![0u32; 100_000];
    let result = catch_unwind(AssertUnwindSafe(|| {
        jobs.par_chunks_mut(&mut data, CHUNK, |index, _| assert_ne!(index, 3, "boom"));
    }));
    assert!(result.is_err(), "panic must reach the caller");

    jobs.par_chunks_mut(&mut data, CHUNK, |_, c| c.fill(1));
    assert!(
        data.iter().all(|&x| x == 1),
        "pool still usable after a panic"
    );
}

#[test]
fn job_system_reports_thread_count() {
    assert_eq!(jobs(3).threads().get(), 3);
}
