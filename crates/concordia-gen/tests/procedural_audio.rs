//! Synthesized sound (M3 phase 3.7b): sound-effect presets and scored music, as 48 kHz mono
//! 16-bit WAV.

use concordia_gen::ProviderRegistry;
use serde_json::{Value, json};

fn generate(provider: &str, request: Value, seed: u64) -> (Vec<i16>, Value) {
    let registry = ProviderRegistry::procedural();
    let p = registry
        .get(provider)
        .unwrap_or_else(|| panic!("{provider}"));
    let out = p
        .generate(
            &request,
            seed,
            &concordia_gen::GenContext {
                project: std::path::Path::new("."),
            },
        )
        .unwrap_or_else(|e| panic!("{request}: {e}"));
    let reader = hound::WavReader::new(std::io::Cursor::new(out.bytes)).unwrap();
    let spec = reader.spec();
    assert_eq!(
        (spec.channels, spec.sample_rate, spec.bits_per_sample),
        (1, 48_000, 16)
    );
    (
        reader.into_samples::<i16>().map(Result::unwrap).collect(),
        out.info,
    )
}

/// Rising zero crossings per sample over a window: twice the frequency over the rate.
fn crossings(samples: &[i16]) -> f64 {
    let n = samples
        .windows(2)
        .filter(|w| (w[0] < 0) != (w[1] < 0))
        .count();
    n as f64 / samples.len() as f64
}

/// `jump` sweeps its pitch up by `pitch_sweep` over its length (exponentially), so the first
/// fifth's average frequency is sweep^(0.1) of the base and the last fifth's sweep^(0.9): their
/// ratio is 1.4^0.8 ≈ 1.309 for a sweep of 1.4. Zero crossings measure it within 10%.
#[test]
fn sfx_preset_jump_has_rising_pitch_sweep() {
    let (s, info) = generate(
        "sfx",
        json!({"preset": "jump", "pitch_sweep": 1.4, "duration_ms": 400}),
        1,
    );
    assert_eq!(s.len(), 48_000 * 400 / 1000, "400 ms");
    assert_eq!(info["frames"], 19_200);
    let fifth = s.len() / 5;
    let ratio = crossings(&s[4 * fifth..]) / crossings(&s[..fifth]);
    assert!((ratio - 1.4f64.powf(0.8)).abs() < 0.13, "ratio {ratio}");
}

/// Every preset peaks at −1 dBFS: 10^(−1/20) · 32767 ≈ 29204 (±2 for rounding).
#[test]
fn sfx_output_is_normalized_below_minus_one_dbfs() {
    for preset in ["pickup", "hit", "jump", "explosion", "ui_click"] {
        let (s, _) = generate("sfx", json!({"preset": preset}), 7);
        let peak = s.iter().map(|v| i32::from(*v).abs()).max().unwrap();
        assert!((peak - 29_204).abs() <= 2, "{preset}: peak {peak}");
    }
}

/// Noise-based presets vary with the seed and repeat for the same seed.
#[test]
fn sfx_variations_come_from_the_seed() {
    let req = json!({"preset": "explosion"});
    assert_eq!(
        generate("sfx", req.clone(), 1).0,
        generate("sfx", req.clone(), 1).0
    );
    assert_ne!(generate("sfx", req.clone(), 1).0, generate("sfx", req, 2).0);
}

/// A score of 2 bars of 4 beats at 120 bpm lasts 8 · 0.5 s = 4 s = 192,000 frames; looping
/// from bar 1 starts the loop at 4 beats = 2 s = 96,000 frames and ends it at the end. The
/// import settings carry the loop.
#[test]
fn score_render_has_expected_length_and_loop_points() {
    let score = json!({
        "bpm": 120.0, "beats_per_bar": 4, "bars": 2, "loop_from_bar": 1,
        "tracks": [
            {"instrument": "Pluck", "volume": 0.8, "notes": [
                {"beat": 0.0, "beats": 1.0, "pitch": 60}, {"beat": 1.0, "beats": 1.0, "pitch": 64},
                {"beat": 4.0, "beats": 2.0, "pitch": 67}]},
            {"instrument": "Kick", "volume": 1.0, "notes": [
                {"beat": 0.0, "beats": 0.5, "pitch": 36}, {"beat": 4.0, "beats": 0.5, "pitch": 36}]},
        ],
    });
    let (s, info) = generate("music", score, 0);
    assert_eq!(s.len(), 192_000);
    assert_eq!(
        (&info["loop_start"], &info["loop_end"]),
        (&json!(96_000), &json!(192_000))
    );
    let registry = ProviderRegistry::procedural();
    let out = registry
        .get("music")
        .unwrap()
        .generate(
            &json!({"bpm": 120.0, "beats_per_bar": 4, "bars": 2, "tracks": []}),
            0,
            &concordia_gen::GenContext {
                project: std::path::Path::new("."),
            },
        )
        .unwrap();
    assert_eq!(
        out.settings,
        Some(json!({"loop_start": 0, "loop_end": 192_000}))
    );
}

/// Requests are validated, naming the field.
#[test]
fn bad_sound_requests_are_refused() {
    let registry = ProviderRegistry::procedural();
    for (provider, request, field) in [
        (
            "sfx",
            json!({"preset": "jump", "duration_ms": 0}),
            "duration_ms",
        ),
        (
            "sfx",
            json!({"preset": "jump", "pitch_sweep": -1.0}),
            "pitch_sweep",
        ),
        (
            "music",
            json!({"bpm": 0.0, "beats_per_bar": 4, "bars": 1, "tracks": []}),
            "bpm",
        ),
        (
            "music",
            json!({"bpm": 100.0, "beats_per_bar": 4, "bars": 1, "tracks": [{"instrument": "Sine", "volume": 1.0, "notes": [{"beat": 0.0, "beats": 1.0, "pitch": 200}]}]}),
            "pitch",
        ),
    ] {
        let err = registry
            .get(provider)
            .unwrap()
            .generate(
                &request,
                0,
                &concordia_gen::GenContext {
                    project: std::path::Path::new("."),
                },
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains(field), "{request}: {err}");
    }
}
