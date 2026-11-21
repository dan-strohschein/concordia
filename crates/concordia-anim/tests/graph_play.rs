//! Graph players in Terra's fixed step (2.4.3): the library, the shared pose path, parameter
//! timing, spawned players and thread counts, on the FBX rig with clips from a second file.

mod common;

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use common::Project;
use concordia_anim::{
    AnimGraphPlayer, AnimGraphState, AnimLibrary, AnimPlayer, LoadReport, Pose, add_processors,
    insert_rest_poses,
};
use concordia_assets::{AssetRef, ImportReport, Skinned};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::time::TickRate;
use concordia_reflect::TypeRegistry;
use concordia_terra::{Phase, Schedule, SimLoop, SystemCtx, World, WorldHash, system};

struct Rig {
    project: Project,
    character: ImportReport,
    actions: ImportReport,
    ids: DeterministicIdGen,
}

impl Rig {
    fn new() -> Self {
        let mut project = Project::new();
        let character = project.fbx("skinned.fbx");
        let actions = project.fbx("anim_stacks.fbx");
        Self {
            project,
            character,
            actions,
            ids: DeterministicIdGen::new(3),
        }
    }

    fn skin(&self) -> AssetRef {
        Project::sub(&self.character, "skin/0")
    }

    fn clip(&self, name: &str) -> AssetRef {
        Project::sub(&self.actions, &format!("clip/Armature|{name}"))
    }

    /// Writes and imports `anim/<name>.animgraph.ron`; `{walk}`/`{idle}` become clip references.
    fn graph(&mut self, name: &str, text: &str) -> AssetRef {
        let text = text
            .replace("{walk}", &self.clip("Walk").to_string())
            .replace("{idle}", &self.clip("Idle").to_string());
        let dir = self.project.dir.path().join("anim");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.animgraph.ron")), text).unwrap();
        let report = self
            .project
            .db
            .import(&Path::new("anim").join(format!("{name}.animgraph.ron")))
            .unwrap();
        format!("asset:{}", report.id).parse().unwrap()
    }

    fn spawn_with(
        &mut self,
        world: &mut World,
        insert: impl FnOnce(&mut World, EntityId),
    ) -> EntityId {
        let id = world.spawn(&mut self.ids).unwrap();
        world.insert(id, Skinned { skin: self.skin() }).unwrap();
        insert(world, id);
        id
    }

    fn spawn(&mut self, world: &mut World, player: AnimGraphPlayer) -> EntityId {
        self.spawn_with(world, |w, id| {
            w.insert(id, player).unwrap();
        })
    }

    fn load(&mut self, world: &mut World) -> (AnimLibrary, LoadReport) {
        AnimLibrary::load(&mut self.project.db, &registry(), world).unwrap()
    }

    /// Loads the library, gives players their poses and states, and builds the loop with `extra`.
    fn start_with(
        &mut self,
        world: &mut World,
        extra: impl FnOnce(&mut Schedule),
    ) -> (SimLoop, Arc<AnimLibrary>) {
        let (library, report) = self.load(world);
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        insert_rest_poses(&library, world);
        let library = Arc::new(library);
        let mut schedule = Schedule::new();
        extra(&mut schedule);
        add_processors(&mut schedule, library.clone());
        let sim = SimLoop::new(TickRate::HZ_60, schedule, world).unwrap();
        (sim, library)
    }

    fn start(&mut self, world: &mut World) -> (SimLoop, Arc<AnimLibrary>) {
        self.start_with(world, |_| {})
    }
}

fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    concordia_assets::register_types(&mut registry).unwrap();
    concordia_anim::register_types(&mut registry).unwrap();
    registry
}

fn jobs(threads: usize) -> JobSystem {
    JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap()
}

fn pose(world: &World, id: EntityId) -> Pose {
    world.get::<Pose>(id).expect("a pose").clone()
}

fn state(world: &World, id: EntityId) -> AnimGraphState {
    world
        .get::<AnimGraphState>(id)
        .expect("a graph state")
        .clone()
}

const WALK_ONLY: &str = r#"(nodes: {"walk": Clip(clip: "{walk}")}, root: "walk")"#;

/// `idle` and `walk` states; `moving` switches to walk at once, `attack` fires a one-shot of
/// Walk over the blend.
const LOCOMOTION: &str = r#"(
    parameters: {"moving": Bool(false), "speed": Float(0.0), "attack": Trigger},
    nodes: {
        "idle": Clip(clip: "{idle}"),
        "walk": Clip(clip: "{walk}"),
        "walk_b": Clip(clip: "{walk}"),
        "idle_b": Clip(clip: "{idle}"),
        "stroll": Clip(clip: "{walk}", looping: false),
        "gait": Blend2(a: "idle_b", b: "walk_b", amount: "speed"),
        "action": OneShot(base: "gait", shot: "stroll", request: "attack", fade_in: 0.1, fade_out: 0.2),
        "moves": StateMachine(
            states: {"idle": "idle", "walk": "walk", "mixed": "action"},
            start: "idle",
            transitions: [
                (from: "idle", to: "walk", advance: Auto, condition: Some("moving")),
                (from: "walk", to: "mixed", advance: Auto, xfade: 0.25),
                (from: "mixed", to: "idle", xfade: 0.1),
            ],
        ),
    },
    root: "moves",
)"#;

#[test]
fn library_loads_graphs_and_the_clips_they_name() {
    let mut rig = Rig::new();
    let mut world = World::new();
    let graph = rig.graph("locomotion", LOCOMOTION);
    rig.spawn(&mut world, AnimGraphPlayer::new(graph.clone()));
    let (library, report) = rig.load(&mut world);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert!(library.graph(&graph, &rig.skin()).is_some());
    for name in ["Walk", "Idle"] {
        assert!(
            library.clip(&rig.clip(name), &rig.skin()).is_some(),
            "{name}: every clip the graph names"
        );
    }

    let broken = rig.graph(
        "broken",
        &WALK_ONLY.replace("{walk}", &format!("asset:{}#clip/Nope", rig.actions.id)),
    );
    rig.spawn(&mut world, AnimGraphPlayer::new(broken));
    let idle = rig.clip("Idle");
    let both = rig.spawn_with(&mut world, |w, id| {
        w.insert(id, AnimGraphPlayer::new(graph.clone())).unwrap();
        w.insert(id, AnimPlayer::new(idle)).unwrap();
    });
    let (library, report) = rig.load(&mut world);
    assert_eq!(report.problems.len(), 2, "{:?}", report.problems);
    let text = report.problems.join("\n");
    assert!(
        text.contains("\"walk\"") && text.contains("clip/Nope"),
        "{text}"
    );
    assert!(
        text.contains(&both.to_string()) && text.contains("graph plays"),
        "{text}"
    );
    assert!(
        library.graph(&graph, &rig.skin()).is_some(),
        "good graphs still load"
    );
}

#[test]
fn graph_player_shares_the_clip_players_pose_path() {
    let mut rig = Rig::new();
    let mut world = World::new();
    let graph = rig.graph("walk", WALK_ONLY);
    let walker = rig.spawn(&mut world, AnimGraphPlayer::new(graph));
    let clip = rig.clip("Walk");
    let twin = rig.spawn_with(&mut world, |w, id| {
        w.insert(id, AnimPlayer::new(clip)).unwrap();
    });
    let (mut sim, library) = rig.start(&mut world);
    let rest = library.skeleton(&rig.skin()).unwrap().rest_pose();
    assert_eq!(
        pose(&world, walker),
        rest,
        "a rest pose before the first tick"
    );
    assert_eq!(state(&world, walker).nodes.len(), 1, "one entry per node");
    sim.step(&mut world, &jobs(2), 15);
    assert_eq!(
        pose(&world, walker),
        pose(&world, twin),
        "a one-clip graph is the clip"
    );
    assert_ne!(pose(&world, walker), rest);
    assert!((state(&world, walker).nodes[0].time - 0.25).abs() < 1e-5);

    let before = world.state_hash().unwrap();
    world.get_mut::<AnimGraphState>(walker).unwrap().nodes[0].time += 0.001;
    assert_ne!(world.state_hash().unwrap(), before, "graph state is hashed");
}

#[test]
fn graph_parameters_set_in_post_physics_take_effect_the_same_tick() {
    let mut rig = Rig::new();
    let mut world = World::new();
    let graph = rig.graph("locomotion", LOCOMOTION);
    let id = rig.spawn(&mut world, AnimGraphPlayer::new(graph.clone()));
    let (mut sim, library) = rig.start_with(&mut world, |schedule| {
        let mut tick = 0u32;
        schedule.add_in(
            Phase::PostPhysics,
            system("gameplay", move |world: &mut World, _: &SystemCtx<'_>| {
                tick += 1;
                if tick == 3 {
                    let mut q = world.query::<&mut AnimGraphPlayer>().unwrap();
                    q.for_each(world, |_, p| {
                        p.bools.insert("moving".into(), true);
                        p.triggers.push("attack".into());
                    });
                }
            }),
        );
    });
    let bound = library.graph(&graph, &rig.skin()).unwrap();
    let jobs = jobs(1);
    sim.step(&mut world, &jobs, 2);
    assert_eq!(
        bound.active_state(&state(&world, id), "moves"),
        Some("idle")
    );
    sim.step(&mut world, &jobs, 1);
    assert_eq!(
        bound.active_state(&state(&world, id), "moves"),
        Some("walk"),
        "set in PostPhysics of tick 3, taken in FrameEnd of tick 3"
    );
    let player = world.get::<AnimGraphPlayer>(id).unwrap();
    assert!(player.triggers.is_empty(), "FrameEnd cleared the trigger");
    assert_eq!(player.bools.get("moving"), Some(&true), "values stay");
}

#[test]
fn graph_players_spawned_during_play_get_a_state_then_animate() {
    let mut rig = Rig::new();
    let mut world = World::new();
    let graph = rig.graph("walk", WALK_ONLY);
    rig.spawn(&mut world, AnimGraphPlayer::new(graph.clone()));
    let (mut sim, library) = rig.start(&mut world);
    let jobs = jobs(2);
    sim.step(&mut world, &jobs, 1);
    let late = rig.spawn(&mut world, AnimGraphPlayer::new(graph));
    assert!(world.get::<Pose>(late).is_none());
    sim.step(&mut world, &jobs, 1);
    let rest = library.skeleton(&rig.skin()).unwrap().rest_pose();
    assert_eq!(
        pose(&world, late),
        rest,
        "a rest pose at the end of its first tick"
    );
    assert_eq!(state(&world, late).nodes.len(), 1);
    sim.step(&mut world, &jobs, 10);
    assert_ne!(pose(&world, late), rest, "then it plays");
}

/// 100 characters through the locomotion graph, driven by gameplay in `PostPhysics`.
fn crowd(threads: usize, ticks: &[u32]) -> Vec<WorldHash> {
    let mut rig = Rig::new();
    let mut world = World::new();
    let graph = rig.graph("locomotion", LOCOMOTION);
    for i in 0..100u16 {
        let mut p = AnimGraphPlayer::new(graph.clone());
        p.floats.insert("speed".into(), f32::from(i % 11) / 10.0);
        rig.spawn(&mut world, p);
    }
    let (mut sim, _) = rig.start_with(&mut world, |schedule| {
        let mut tick = 0u32;
        schedule.add_in(
            Phase::PostPhysics,
            system("gameplay", move |world: &mut World, _: &SystemCtx<'_>| {
                tick += 1;
                let mut i = 0u32;
                let mut q = world.query::<&mut AnimGraphPlayer>().unwrap();
                q.for_each(world, |_, p| {
                    i += 1;
                    let phase = (tick + i * 7) % 90;
                    p.bools.insert("moving".into(), phase > 20);
                    if phase == 50 {
                        p.triggers.push("attack".into());
                    }
                    if phase == 80 {
                        p.travel.insert("moves".into(), "idle".into());
                    }
                });
            }),
        );
    });
    let jobs = jobs(threads);
    let mut done = 0;
    let mut first: Option<Vec<Pose>> = None;
    let hashes = ticks
        .iter()
        .map(|&target| {
            sim.step(&mut world, &jobs, target - done);
            done = target;
            let poses = poses(&mut world);
            assert_eq!(poses.len(), 100, "every character is posed");
            first.get_or_insert(poses.clone());
            world.state_hash().unwrap()
        })
        .collect();
    let last = poses(&mut world);
    let moved = first
        .unwrap()
        .iter()
        .zip(&last)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        moved > 90,
        "the crowd animates ({moved} of 100 poses changed)"
    );
    hashes
}

/// Every pose, in entity order.
fn poses(world: &mut World) -> Vec<Pose> {
    let mut out = Vec::new();
    world
        .query::<&Pose>()
        .unwrap()
        .for_each(world, |_, p| out.push(p.clone()));
    out
}

#[test]
fn graph_evaluation_is_deterministic_across_thread_counts() {
    let ticks = [1, 60, 300];
    let one = crowd(1, &ticks);
    assert_eq!(crowd(8, &ticks), one, "8 threads");
    assert_eq!(crowd(3, &ticks), one, "3 threads");
    assert_ne!(one[1], one[2]);
}

#[test]
fn an_entity_with_both_players_plays_its_graph() {
    let mut rig = Rig::new();
    let mut world = World::new();
    let graph = rig.graph("walk", WALK_ONLY);
    let walker = rig.spawn(&mut world, AnimGraphPlayer::new(graph.clone()));
    let idle = rig.clip("Idle");
    let both = rig.spawn_with(&mut world, |w, id| {
        w.insert(id, AnimGraphPlayer::new(graph)).unwrap();
        w.insert(id, AnimPlayer::new(idle)).unwrap();
    });
    let (library, report) = rig.load(&mut world);
    assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
    insert_rest_poses(&library, &mut world);
    let mut schedule = Schedule::new();
    add_processors(&mut schedule, Arc::new(library));
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(2), 12);
    assert_eq!(pose(&world, both), pose(&world, walker), "the graph's pose");
    assert_eq!(
        world.get::<AnimPlayer>(both).unwrap().time,
        0.0,
        "the clip player is left alone"
    );
}
