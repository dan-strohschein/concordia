//! `.cue.ron`: canonical text, validation by field, and seeded variation.
//!
//! Variation draws SplitMix64's stream from s = seed ⊕ entity ⊕ (tick × 2⁸) ⊕ n:
//! out_k = mix(s + (k + 1)·γ). For s = 0 these are SplitMix64's published first outputs:
//! 0xE220A8397B1DCDAF, 0x6E789E6AA1B965F4, 0x06C45D188009454F.
//! - The sound: out₀ mod (3 + 1) = 0xAF mod 4 = 3, past weight 3's [0, 3), so sound 1.
//! - The volume: u = (out₁ >> 40) / 2²⁴ = 0x6E789E / 16777216 = 0.43152797;
//!   −6 + 6u = −3.4108322 dB.
//! - The pitch: u = 0x06C45D / 2²⁴ = 0.02643377; −2 + 4u = −1.8942649 semitones, a ratio of
//!   2^(−1.8942649 / 12) = 0.8963565.
//!
//! Weights 3:1 over 10,000 draws pick sound 0 about 7,500 times (within 2%: 7,300–7,700).

use concordia_assets::AssetRef;
use concordia_audio::{Cue, CueError, CueSound, DrawKey, Span, Spatial, read_cue, write_cue};

fn sound(n: u32) -> AssetRef {
    format!("asset:0190b5c0-0000-7000-8000-00000000000{n}")
        .parse()
        .unwrap()
}

fn cue() -> Cue {
    Cue {
        sounds: vec![
            CueSound {
                sound: sound(1),
                weight: 3,
            },
            CueSound {
                sound: sound(2),
                weight: 1,
            },
        ],
        volume_db: Span {
            min: -6.0,
            max: 0.0,
        },
        pitch_semitones: Span {
            min: -2.0,
            max: 2.0,
        },
        bus: "SFX".into(),
        priority: 200,
        looping: false,
        max_instances: Some(4),
        spatial: Some(Spatial {
            min_distance: 2.0,
            max_distance: 30.0,
        }),
        seed: 0,
    }
}

#[test]
fn cue_files_roundtrip_canonical() {
    let text = write_cue(&cue()).unwrap();
    insta::assert_snapshot!(text);
    assert_eq!(read_cue(&text).unwrap(), cue());
    assert_eq!(write_cue(&read_cue(&text).unwrap()).unwrap(), text);
}

fn field(cue: &Cue) -> String {
    match cue.validate().unwrap_err() {
        CueError::Field { field, .. } => field,
        other => panic!("{other:?}"),
    }
}

#[test]
fn cue_validation_names_the_field() {
    assert!(cue().validate().is_ok());
    let with = |f: fn(&mut Cue)| {
        let mut c = cue();
        f(&mut c);
        field(&c)
    };
    assert_eq!(with(|c| c.sounds.clear()), "sounds");
    assert_eq!(with(|c| c.sounds[1].weight = 0), "sounds[1].weight");
    assert_eq!(
        with(|c| c.volume_db = Span { min: 1.0, max: 0.0 }),
        "volume_db"
    );
    assert_eq!(with(|c| c.volume_db.max = 30.0), "volume_db.max");
    assert_eq!(
        with(|c| c.pitch_semitones.min = -25.0),
        "pitch_semitones.min"
    );
    assert_eq!(
        with(|c| c.pitch_semitones.max = f32::NAN),
        "pitch_semitones.max"
    );
    assert_eq!(with(|c| c.max_instances = Some(0)), "max_instances");
    assert_eq!(with(|c| c.bus.clear()), "bus");
    assert_eq!(
        with(|c| c.spatial = Some(Spatial {
            min_distance: 5.0,
            max_distance: 1.0
        })),
        "spatial"
    );
    let e = read_cue("(sounds: [])").unwrap_err();
    assert!(e.to_string().contains("sounds"), "{e}");
    assert!(matches!(
        read_cue("(sounds: [], surprise: 1)"),
        Err(CueError::Parse(_))
    ));
}

#[test]
fn cue_variation_is_seeded_and_in_range() {
    let c = cue();
    let d = c.draw(DrawKey {
        entity: 0,
        tick: 0,
        n: 0,
    });
    assert_eq!(d.sound, 1);
    assert!((d.volume_db - -3.410_832_2).abs() < 1e-5, "{}", d.volume_db);
    assert!((d.pitch - 0.896_356_5).abs() < 1e-5, "{}", d.pitch);
    // The same key, the same draw; the seed, entity, tick and n each change it.
    assert_eq!(
        c.draw(DrawKey {
            entity: 0,
            tick: 0,
            n: 0
        }),
        d
    );
    let base = DrawKey {
        entity: 9,
        tick: 4,
        n: 1,
    };
    let other = [
        Cue {
            seed: 1,
            ..c.clone()
        }
        .draw(base),
        c.draw(DrawKey { entity: 10, ..base }),
        c.draw(DrawKey { tick: 5, ..base }),
        c.draw(DrawKey { n: 2, ..base }),
    ];
    for o in other {
        assert_ne!(o, c.draw(base));
    }
    let mut first = 0;
    for tick in 0..10_000 {
        let d = c.draw(DrawKey {
            entity: 77,
            tick,
            n: 0,
        });
        assert!(d.sound < 2);
        assert!((-6.0..=0.0).contains(&d.volume_db), "{d:?}");
        let low = 2f32.powf(-2.0 / 12.0) - 1e-6;
        let high = 2f32.powf(2.0 / 12.0) + 1e-6;
        assert!((low..=high).contains(&d.pitch), "{d:?}");
        first += usize::from(d.sound == 0);
    }
    assert!((7_300..=7_700).contains(&first), "{first}");
    // A fixed range draws its one value.
    let fixed = Cue {
        volume_db: Span {
            min: -3.0,
            max: -3.0,
        },
        pitch_semitones: Span {
            min: 12.0,
            max: 12.0,
        },
        ..cue()
    };
    let d = fixed.draw(base);
    assert_eq!((d.volume_db, d.pitch), (-3.0, 2.0));
}
