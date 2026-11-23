//! The cues and sounds a play session can use, loaded from the asset database: every imported
//! cue at play start, and any other on demand (previews, `audio.render`).

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_assets::{AssetDb, AssetKind, AssetRef};

use crate::cue::Cue;
use crate::sound::{LoadedSound, Sound};

/// Loaded cues and their sounds, by reference.
#[derive(Debug, Clone, Default)]
pub struct AudioLibrary {
    cues: BTreeMap<AssetRef, Arc<Cue>>,
    sounds: BTreeMap<AssetRef, LoadedSound>,
}

impl AudioLibrary {
    /// Every imported cue and the sounds they name; problems (a missing sound, a bad artifact)
    /// are described and the cue is left out.
    pub fn load_all(db: &mut AssetDb) -> (Self, Vec<String>) {
        let mut library = Self::default();
        let mut problems = Vec::new();
        let cues: Vec<AssetRef> = db
            .list()
            .into_iter()
            .filter(|a| a.importer == "cue")
            .filter_map(|a| format!("asset:{}", a.id).parse().ok())
            .collect();
        for cue in cues {
            if let Err(problem) = library.ensure(db, &cue) {
                problems.push(problem);
            }
        }
        (library, problems)
    }

    /// Loads `cue` and its sounds unless already loaded.
    pub fn ensure(&mut self, db: &mut AssetDb, cue: &AssetRef) -> Result<Arc<Cue>, String> {
        if let Some(loaded) = self.cues.get(cue) {
            return Ok(loaded.clone());
        }
        let loaded: Cue = db
            .load(cue, AssetKind::Cue)
            .map_err(|e| format!("cue {cue}: {e}"))?;
        for entry in &loaded.sounds {
            if self.sounds.contains_key(&entry.sound) {
                continue;
            }
            let sound: Sound = db
                .load(&entry.sound, AssetKind::Sound)
                .map_err(|e| format!("cue {cue}: sound {}: {e}", entry.sound))?;
            self.sounds.insert(entry.sound.clone(), sound.into());
        }
        let loaded = Arc::new(loaded);
        self.cues.insert(cue.clone(), loaded.clone());
        Ok(loaded)
    }

    /// A loaded cue.
    pub fn cue(&self, cue: &AssetRef) -> Option<&Arc<Cue>> {
        self.cues.get(cue)
    }

    /// A loaded sound.
    pub fn sound(&self, sound: &AssetRef) -> Option<&LoadedSound> {
        self.sounds.get(sound)
    }

    /// How many cues are loaded.
    pub fn cues(&self) -> usize {
        self.cues.len()
    }
}
