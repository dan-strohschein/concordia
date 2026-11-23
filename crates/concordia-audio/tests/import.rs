//! The `sound` and `cue` importers through `AssetDb` and the DDC.
//!
//! A 16-bit mono WAV of [0, 16384] imports decoded: frames [0, 0.5]. Importing again is a DDC
//! hit. Setting `gain_db: -6.0206` in the sidecar changes the import key, so it re-imports, and
//! the loaded sound's linear gain is 0.5 (samples are kept as decoded). An Ogg over 10 s streams
//! by default and keeps its bytes; the 1 s fixture is decoded unless `stream: Some(true)`.

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use concordia_assets::{AssetDb, AssetKind, AssetRef};
use concordia_audio::{
    CueImporter, LoadedBody, LoadedSound, Sound, SoundBody, SoundImporter, read_cue,
};
use concordia_core::id::DeterministicIdGen;

struct Project {
    dir: tempfile::TempDir,
    db: AssetDb,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut db = AssetDb::new(dir.path(), Box::new(DeterministicIdGen::new(7))).unwrap();
        db.register(Arc::new(SoundImporter));
        db.register(Arc::new(CueImporter));
        Self { dir, db }
    }

    fn write(&self, rel: &str, bytes: &[u8]) {
        let path = self.dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn sound(&mut self, rel: &str) -> (AssetRef, Sound) {
        let id = self.db.import(Path::new(rel)).unwrap().id;
        let r: AssetRef = format!("asset:{id}").parse().unwrap();
        let s = self.db.load::<Sound>(&r, AssetKind::Sound).unwrap();
        (r, s)
    }
}

fn wav16(samples: &[i16]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = Cursor::new(Vec::new());
    let mut w = hound::WavWriter::new(&mut bytes, spec).unwrap();
    for &s in samples {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
    bytes.into_inner()
}

fn tone() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/audio/tone-440.ogg"),
    )
    .unwrap()
}

#[test]
fn sound_importer_caches_decoded_frames_in_the_ddc() {
    let mut p = Project::new();
    p.write("sfx/blip.wav", &wav16(&[0, 16384]));
    let first = p.db.import(Path::new("sfx/blip.wav")).unwrap();
    let again = p.db.import(Path::new("sfx/blip.wav")).unwrap();
    assert!(!first.cached && again.cached, "{first:?} {again:?}");
    let (_, sound) = p.sound("sfx/blip.wav");
    match &sound.body {
        SoundBody::Decoded(d) => assert_eq!(d.samples(), &[0.0, 0.5]),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        (sound.gain_db, sound.loop_start, sound.loop_end),
        (0.0, 0, None)
    );

    let sidecar = p.dir.path().join("sfx/blip.wav.import.ron");
    let text = std::fs::read_to_string(&sidecar).unwrap();
    assert!(text.contains("\"gain_db\": 0.0"), "{text}");
    std::fs::write(
        &sidecar,
        text.replace("\"gain_db\": 0.0", "\"gain_db\": -6.0206"),
    )
    .unwrap();
    let changed = p.db.import(Path::new("sfx/blip.wav")).unwrap();
    assert!(!changed.cached, "a changed setting re-imports");
    let (_, sound) = p.sound("sfx/blip.wav");
    let loaded = LoadedSound::from(sound);
    assert!((loaded.gain - 0.5).abs() < 1e-5, "{}", loaded.gain);
}

#[test]
fn short_ogg_decodes_and_stream_setting_keeps_its_bytes() {
    let mut p = Project::new();
    p.write("music/tone.ogg", &tone());
    let (_, sound) = p.sound("music/tone.ogg");
    assert!(matches!(&sound.body, SoundBody::Decoded(d) if d.frames() == 48_000));
    let sidecar = p.dir.path().join("music/tone.ogg.import.ron");
    let text = std::fs::read_to_string(&sidecar).unwrap();
    assert!(text.contains("\"stream\": None"), "{text}");
    std::fs::write(
        &sidecar,
        text.replace("\"stream\": None", "\"stream\": Some(true)"),
    )
    .unwrap();
    let (_, sound) = p.sound("music/tone.ogg");
    assert!(matches!(&sound.body, SoundBody::Streamed(b) if *b == tone()));
    assert!(matches!(
        LoadedSound::from(sound).body,
        LoadedBody::Stream(_)
    ));
}

/// The default streams Ogg longer than 10 s: at 48 kHz, 480,000 frames is exactly 10 s (decoded)
/// and 480,001 is longer (streamed). WAV never streams; `Some(_)` overrides for Ogg.
#[test]
fn long_ogg_streams_by_default() {
    use concordia_audio::SoundSettings;
    let d = SoundSettings::default();
    assert!(!d.streams(true, 480_000, 48_000));
    assert!(d.streams(true, 480_001, 48_000));
    assert!(!d.streams(false, 10_000_000, 48_000));
    let never = SoundSettings {
        stream: Some(false),
        ..d.clone()
    };
    assert!(!never.streams(true, 10_000_000, 48_000));
    let always = SoundSettings {
        stream: Some(true),
        ..d
    };
    assert!(always.streams(true, 1, 48_000));
    assert!(!always.streams(false, 1, 48_000), "WAV can't stream");
}

#[test]
fn bad_sources_and_settings_are_import_errors_naming_the_file() {
    let mut p = Project::new();
    p.write("sfx/bad.wav", b"RIFF nope");
    let e =
        p.db.import(Path::new("sfx/bad.wav"))
            .unwrap_err()
            .to_string();
    assert!(e.contains("sfx/bad.wav"), "{e}");
    p.write("sfx/blip.wav", &wav16(&[0, 1, 2]));
    p.db.import(Path::new("sfx/blip.wav")).unwrap();
    let sidecar = p.dir.path().join("sfx/blip.wav.import.ron");
    let text = std::fs::read_to_string(&sidecar).unwrap();
    std::fs::write(
        &sidecar,
        text.replace("\"loop_end\": None", "\"loop_end\": Some(9)"),
    )
    .unwrap();
    let e =
        p.db.import(Path::new("sfx/blip.wav"))
            .unwrap_err()
            .to_string();
    assert!(e.contains("loop_end"), "{e}");
}

#[test]
fn cue_importer_reads_and_validates_cue_files() {
    let mut p = Project::new();
    let text = r#"(
    sounds: [(sound: "asset:0190b5c0-0000-7000-8000-000000000001", weight: 2)],
    bus: "SFX",
    seed: 7,
)
"#;
    p.write("sfx/hit.cue.ron", text.as_bytes());
    let id = p.db.import(Path::new("sfx/hit.cue.ron")).unwrap().id;
    let r: AssetRef = format!("asset:{id}").parse().unwrap();
    let cue =
        p.db.load::<concordia_audio::Cue>(&r, AssetKind::Cue)
            .unwrap();
    assert_eq!(cue, read_cue(text).unwrap());
    assert_eq!(cue.sounds[0].weight, 2);
    p.write("sfx/empty.cue.ron", b"(sounds: [])");
    let e =
        p.db.import(Path::new("sfx/empty.cue.ron"))
            .unwrap_err()
            .to_string();
    assert!(e.contains("sounds"), "{e}");
}
