//! `audio.collect`: the presentation processor that turns gameplay's requests into mixer
//! commands. It reads and receives only and writes nothing to the world; offline, it renders
//! each tick right after queuing that tick's commands, so renders are sample-exact.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use concordia_assets::AssetRef;
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Vec3};
use concordia_scene::builtin::{Parent, Transform, world_matrix_with};
use concordia_terra::{Phase, Processor, ProcessorDecl, SystemCtx, View, processor};

use crate::components::{AnimEventCues, AudioEmitter, AudioListener, PlayCue, SetBusGain, StopCue};
use crate::core::{AudioCore, AudioShared, Output, Start};
use crate::cue::DrawKey;
use crate::mixer::VoiceId;
use crate::spatial::Listener;

/// The processor's name.
pub const COLLECT: &str = "audio.collect";

/// What an emitter is doing.
#[derive(Debug, Clone)]
struct Track {
    cue: AssetRef,
    voice: Option<VoiceId>,
    was_playing: bool,
}

/// One tick's view of the world, gathered before the core is locked.
#[derive(Default)]
struct Gathered {
    transforms: BTreeMap<EntityId, Transform>,
    parents: BTreeMap<EntityId, EntityId>,
    emitters: Vec<(EntityId, AudioEmitter)>,
    listeners: Vec<(EntityId, AudioListener)>,
    anim_cues: BTreeMap<EntityId, AnimEventCues>,
    plays: Vec<(EntityId, PlayCue)>,
    stops: Vec<(EntityId, StopCue)>,
    gains: Vec<(EntityId, SetBusGain)>,
    events: Vec<(EntityId, String)>,
}

impl Gathered {
    fn matrix(&self, id: EntityId) -> Option<Mat4> {
        world_matrix_with(id, &|e| self.transforms.get(&e).copied(), &|e| {
            self.parents.get(&e).copied()
        })
    }

    fn position(&self, id: EntityId) -> Option<Vec3> {
        self.matrix(id).map(|m| m.w_axis.truncate())
    }
}

/// The processor and what it remembers between ticks (outside the world).
pub struct Collector {
    decl: ProcessorDecl,
    shared: AudioShared,
    anim_events: bool,
    tick_rate: NonZeroU32,
    emitters: BTreeMap<EntityId, Track>,
    started: BTreeMap<EntityId, Vec<(AssetRef, VoiceId)>>,
    warned: BTreeSet<String>,
    no_listener_warned: bool,
}

impl std::fmt::Debug for Collector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collector")
            .field("emitters", &self.emitters.len())
            .finish()
    }
}

impl Collector {
    /// The processor at `tick_rate`; with `anim_events`, it also receives `AnimEvent` (when
    /// `[anim]` is on).
    pub fn new(shared: AudioShared, anim_events: bool, tick_rate: NonZeroU32) -> Self {
        let mut decl = processor(COLLECT, Phase::Presentation)
            .reads::<Transform>()
            .reads::<Parent>()
            .reads::<AudioEmitter>()
            .reads::<AudioListener>()
            .reads::<AnimEventCues>()
            .receives::<PlayCue>()
            .receives::<StopCue>()
            .receives::<SetBusGain>();
        if anim_events {
            decl = decl.receives::<concordia_anim::AnimEvent>();
        }
        Self {
            decl,
            shared,
            anim_events,
            tick_rate,
            emitters: BTreeMap::new(),
            started: BTreeMap::new(),
            warned: BTreeSet::new(),
            no_listener_warned: false,
        }
    }

    fn gather(&self, view: &mut View<'_>) -> Gathered {
        let mut g = Gathered::default();
        if let Ok(q) = view.query::<&Transform>() {
            q.for_each(|id, t| {
                g.transforms.insert(id, *t);
            });
        }
        if let Ok(q) = view.query::<&Parent>() {
            q.for_each(|id, p| {
                g.parents.insert(id, p.0);
            });
        }
        if let Ok(q) = view.query::<&AudioEmitter>() {
            q.for_each(|id, e| g.emitters.push((id, e.clone())));
        }
        if let Ok(q) = view.query::<&AudioListener>() {
            q.for_each(|id, l| g.listeners.push((id, *l)));
        }
        if let Ok(q) = view.query::<&AnimEventCues>() {
            q.for_each(|id, c| {
                g.anim_cues.insert(id, c.clone());
            });
        }
        g.emitters.sort_by_key(|(id, _)| *id);
        g.listeners.sort_by_key(|(id, _)| *id);
        g.plays = view
            .signals::<PlayCue>()
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        g.stops = view
            .signals::<StopCue>()
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        g.gains = view
            .signals::<SetBusGain>()
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        if self.anim_events {
            g.events = view
                .signals::<concordia_anim::AnimEvent>()
                .map(|s| s.iter().map(|(id, e)| (*id, e.name.clone())).collect())
                .unwrap_or_default();
        }
        g
    }

    /// The lowest-id listener with a `Transform`, and its gain.
    fn listen(&mut self, g: &Gathered, core: &mut AudioCore) {
        let found = g.listeners.iter().find_map(|(id, l)| {
            let (_, rotation, position) = g.matrix(*id)?.to_scale_rotation_translation();
            Some((*id, Listener::new(position, rotation), l.gain))
        });
        core.listener = found.map(|(id, l, _)| (id, l));
        core.listener_gain = found.map_or(1.0, |(_, _, gain)| gain);
        let positioned = g
            .emitters
            .iter()
            .any(|(id, _)| g.transforms.contains_key(id));
        if found.is_none() && positioned && !self.no_listener_warned {
            self.no_listener_warned = true;
            tracing::warn!(target: "concordia::audio", "no AudioListener; positioned sounds play centred");
        }
    }

    /// Applies gameplay's `SetBusGain`s in delivery order, warning once per bad request.
    fn gains(&mut self, g: &Gathered, core: &mut AudioCore) {
        for (_, set) in &g.gains {
            let valid = set.gain_db.is_finite() && (-80.0..=24.0).contains(&set.gain_db);
            let applied = match core.buses().index(&set.bus) {
                Some(bus) if valid => core.set_bus(bus, Some(set.gain_db), None).is_ok(),
                _ => false,
            };
            let problem = format!(
                "SetBusGain {{ bus: {:?}, gain_db: {} }}",
                set.bus, set.gain_db
            );
            if !applied && self.warned.insert(problem.clone()) {
                tracing::warn!(target: "concordia::audio", "{problem} ignored: no such bus, or a gain outside [-80, 24]");
            }
        }
    }

    /// Starts a cue, warning once per cue that can't start.
    fn start(&mut self, core: &mut AudioCore, start: Start<'_>) -> Option<VoiceId> {
        match core.start(start) {
            Ok(voice) => Some(voice),
            Err(e) => {
                if self.warned.insert(e.to_string()) {
                    tracing::warn!(target: "concordia::audio", error = %e, "a cue could not play");
                }
                None
            }
        }
    }

    fn emitters(
        &mut self,
        g: &Gathered,
        core: &mut AudioCore,
        tick: u64,
        n: &mut BTreeMap<EntityId, u32>,
    ) {
        let present: BTreeSet<EntityId> = g.emitters.iter().map(|(id, _)| *id).collect();
        let gone: Vec<EntityId> = self
            .emitters
            .keys()
            .filter(|id| !present.contains(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(voice) = self.emitters.remove(&id).and_then(|t| t.voice) {
                core.stop(voice);
            }
        }
        for (id, emitter) in &g.emitters {
            let track = self.emitters.entry(*id).or_insert_with(|| Track {
                cue: emitter.cue.clone(),
                voice: None,
                was_playing: false,
            });
            if track.cue != emitter.cue {
                if let Some(voice) = track.voice.take() {
                    core.stop(voice);
                }
                track.cue = emitter.cue.clone();
                track.was_playing = false;
            }
            let (starts, stops) = (emitter.playing && !track.was_playing, !emitter.playing);
            track.was_playing = emitter.playing;
            if stops {
                if let Some(voice) = track.voice.take() {
                    core.stop(voice);
                }
                continue;
            }
            if !starts {
                continue;
            }
            let start = Start {
                cue: &emitter.cue,
                entity: Some(*id),
                position: g.position(*id),
                volume_db: emitter.volume_db,
                key: key(*id, tick, n),
            };
            let voice = self.start(core, start);
            if let Some(track) = self.emitters.get_mut(id) {
                track.voice = voice;
            }
        }
    }

    fn signals(
        &mut self,
        g: &Gathered,
        core: &mut AudioCore,
        tick: u64,
        n: &mut BTreeMap<EntityId, u32>,
    ) {
        for (id, stop) in &g.stops {
            let Some(started) = self.started.get_mut(id) else {
                continue;
            };
            started.retain(|(cue, voice)| {
                let hit = stop.cue.as_ref().is_none_or(|c| c == cue);
                if hit {
                    core.stop(*voice);
                }
                !hit
            });
        }
        let mut plays: Vec<(EntityId, AssetRef, f32)> = g
            .plays
            .iter()
            .map(|(id, p)| (*id, p.cue.clone(), p.volume_db))
            .collect();
        for (id, name) in &g.events {
            if let Some(cue) = g.anim_cues.get(id).and_then(|c| c.cues.get(name)) {
                plays.push((*id, cue.clone(), 0.0));
            }
        }
        for (id, cue, volume_db) in plays {
            let start = Start {
                cue: &cue,
                entity: Some(id),
                position: g.position(id),
                volume_db,
                key: key(id, tick, n),
            };
            if let Some(voice) = self.start(core, start) {
                self.started.entry(id).or_default().push((cue, voice));
            }
        }
    }

    /// Re-pans every positioned voice from its entity's position now.
    fn follow(g: &Gathered, core: &mut AudioCore) {
        let moving: Vec<(VoiceId, Vec3)> = core
            .playing
            .iter()
            .filter(|(_, info)| info.position.is_some())
            .map(|(voice, info)| {
                let at = info.entity.and_then(|e| g.position(e)).or(info.position);
                (*voice, at.unwrap_or_default())
            })
            .collect();
        for (voice, at) in moving {
            core.move_voice(voice, at);
        }
    }
}

/// The draw key: the entity's bits folded to 64, the tick, and which play this is.
fn key(id: EntityId, tick: u64, n: &mut BTreeMap<EntityId, u32>) -> DrawKey {
    let count = n.entry(id).or_default();
    let key = DrawKey {
        entity: fold(id),
        tick,
        n: *count,
    };
    *count += 1;
    key
}

fn fold(id: EntityId) -> u64 {
    let v = id.as_uuid().as_u128();
    (v as u64) ^ ((v >> 64) as u64)
}

impl Processor for Collector {
    fn decl(&self) -> &ProcessorDecl {
        &self.decl
    }

    fn run(&mut self, view: &mut View<'_>, ctx: &SystemCtx<'_>) {
        let g = self.gather(view);
        let shared = self.shared.clone();
        let mut core = shared.lock();
        let tick = ctx.tick.0;
        let mut n = BTreeMap::new();
        core.prune();
        self.listen(&g, &mut core);
        self.gains(&g, &mut core);
        self.emitters(&g, &mut core, tick, &mut n);
        self.signals(&g, &mut core, tick, &mut n);
        Self::follow(&g, &mut core);
        let base = *core.base_tick.get_or_insert(tick);
        if let Output::Offline(backend) = &mut core.output {
            for e in backend.render_tick(tick - base, self.tick_rate) {
                tracing::warn!(target: "concordia::audio", error = %e, "a stream ended early");
            }
        }
        core.prune();
        let live: BTreeSet<VoiceId> = core.playing.keys().copied().collect();
        for started in self.started.values_mut() {
            started.retain(|(_, voice)| live.contains(voice));
        }
        self.started.retain(|_, s| !s.is_empty());
    }
}
