//! Shutting the engine thread down (M4 4.1v(c)): a process must not exit while the engine (and its
//! render and GPU threads) is still running, or drivers' exit handlers race those threads.

mod common;

use std::time::{Duration, Instant};

use concordia_api::{Engine, join_engine};

#[test]
fn the_engine_thread_finishes_once_every_handle_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, thread) = Engine::new(common::config(&dir))
        .unwrap()
        .spawn_with_thread();
    drop(handle);
    assert!(join_engine(thread, Duration::from_secs(10)), "joined");
}

#[test]
fn a_handle_still_held_makes_the_join_give_up_instead_of_hanging() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, thread) = Engine::new(common::config(&dir))
        .unwrap()
        .spawn_with_thread();
    let start = Instant::now();
    assert!(!join_engine(thread, Duration::from_millis(200)));
    assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
    drop(handle);
}
