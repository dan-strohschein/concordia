//! The mix callback never allocates or frees (author decision A2: `allocation-counter`, a
//! test-only crate, counts this thread's allocations, so no unsafe in our code).
//!
//! 32 voices (memory, looping, pitched, spatial and a stream), 200 queued commands that steal,
//! stop, re-gain and re-bus, all applied and mixed inside `measure`: zero allocations, and the
//! net count and bytes stay zero (nothing freed either, since frees would go negative).

mod common;

use std::num::NonZeroU16;

use common::*;
use concordia_audio::{LoadedBody, LoadedSound, MASTER, PlayRequest};

#[test]
fn audio_callback_never_allocates() {
    let g = layout(0.0, -3.0, -6.0);
    let (mut m, mut h) = mixer(g.clone(), 32);
    let sfx = g.index("SFX").unwrap();
    let music = g.index("Music").unwrap();
    let stream = LoadedSound {
        body: LoadedBody::Stream(tone_bytes().into()),
        gain: 1.0,
        loop_start: 0,
        loop_end: None,
    };
    h.play(PlayRequest {
        looping: true,
        ..PlayRequest::new(stream, music)
    })
    .unwrap();
    let mut out = vec![0.0f32; 1024 * 2];
    // Warm up: the first render applies the stream and fills the pool.
    let mut ids = Vec::new();
    for i in 0..40 {
        let mut r = PlayRequest::new(dc(0.01 * i as f32, 9_600), sfx);
        r.priority = (i % 5) as u8;
        r.pitch = 0.5 + (i % 4) as f32 * 0.25;
        r.looping = i % 2 == 0;
        r.spatial = i % 3 == 0;
        r.gains = [0.3, 0.7];
        r.group = (i % 6 == 0).then(|| (9, NonZeroU16::new(3).unwrap()));
        ids.push(h.play(r).unwrap());
    }
    m.render(&mut out);
    h.collect();
    h.pump_streams();
    for (i, id) in ids.iter().enumerate().take(50) {
        match i % 4 {
            0 => h.stop(*id).unwrap(),
            1 => h.set_gains(*id, [0.5, 0.5]).unwrap(),
            2 => h.set_bus(sfx, -(i as f32), i % 8 == 2).unwrap(),
            _ => {
                h.play(PlayRequest {
                    priority: 4,
                    ..PlayRequest::new(dc(0.1, 4800), sfx)
                })
                .unwrap();
            }
        }
    }
    for i in 0..150 {
        h.play(PlayRequest {
            priority: (i % 7) as u8,
            ..PlayRequest::new(dc(0.02, 480), sfx)
        })
        .unwrap();
    }
    h.stop_all().unwrap();
    let info = allocation_counter::measure(|| {
        for _ in 0..8 {
            m.render(&mut out);
        }
    });
    assert_eq!(info.count_total, 0, "{info:?}");
    assert_eq!((info.count_current, info.bytes_current), (0, 0), "{info:?}");
    h.collect();
    assert!(h.stats().steals() > 0);
    let _ = MASTER;
}
