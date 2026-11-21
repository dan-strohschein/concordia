//! Players in Terra's fixed step: time, the world hash, thread counts and spawned players (2.4.1).

mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;

use common::Project;
use concordia_anim::{AnimLibrary, AnimPlayer, Pose, add_processors, insert_rest_poses};
use concordia_assets::{AssetRef, ImportReport, Skinned};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::time::TickRate;
use concordia_reflect::TypeRegistry;
use concordia_terra::{Schedule, SimClock, SimLoop, TimeScale, World, WorldHash};

struct Rig {
    project: Project,
    /// `skinned.fbx`: the character (mesh and skin).
    character: ImportReport,
    /// `anim_stacks.fbx`: the same rig's actions, bound to the character by bone name.
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

    /// Spawns a skinned character playing `player`.
    fn spawn(&mut self, world: &mut World, player: AnimPlayer) -> EntityId {
        let id = world.spawn(&mut self.ids).unwrap();
        world.insert(id, Skinned { skin: self.skin() }).unwrap();
        world.insert(id, player).unwrap();
        id
    }

    /// Loads the library for `world`, gives its players rest poses and builds the loop.
    fn start(&mut self, world: &mut World) -> (SimLoop, Arc<AnimLibrary>) {
        let (library, report) =
            AnimLibrary::load(&mut self.project.db, &registry(), world).unwrap();
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        insert_rest_poses(&library, world);
        let library = Arc::new(library);
        let mut schedule = Schedule::new();
        add_processors(&mut schedule, library.clone());
        let sim = SimLoop::new(TickRate::HZ_60, schedule, world).unwrap();
        (sim, library)
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

fn player(world: &World, id: EntityId) -> AnimPlayer {
    world.get::<AnimPlayer>(id).unwrap().clone()
}

fn pose(world: &World, id: EntityId) -> Pose {
    world.get::<Pose>(id).expect("a pose").clone()
}

#[test]
fn library_loads_every_skin_and_the_clip_files_clips() {
    let mut rig = Rig::new();
    let mut world = World::new();
    rig.spawn(&mut world, AnimPlayer::new(rig.clip("Walk")));
    let (library, report) =
        AnimLibrary::load(&mut rig.project.db, &registry(), &mut world).unwrap();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    let skeleton = library.skeleton(&rig.skin()).expect("the skin's skeleton");
    assert_eq!(skeleton.len(), 7);
    for name in ["Walk", "Idle"] {
        let clip = library.clip(&rig.clip(name), &rig.skin());
        assert!(
            clip.is_some(),
            "{name}: every clip of the played clip's file"
        );
    }
    let missing: AssetRef = format!("asset:{}#clip/Nope", rig.actions.id)
        .parse()
        .unwrap();
    rig.spawn(&mut world, AnimPlayer::new(missing));
    let (_, report) = AnimLibrary::load(&mut rig.project.db, &registry(), &mut world).unwrap();
    assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
    assert!(
        report.problems[0].contains("clip/Nope"),
        "{:?}",
        report.problems
    );
}

#[test]
fn player_advances_at_fixed_dt_and_time_scale() {
    let mut rig = Rig::new();
    let mut world = World::new();
    let id = rig.spawn(&mut world, AnimPlayer::new(rig.clip("Idle")));
    let (mut sim, library) = rig.start(&mut world);
    let jobs = jobs(2);
    assert_eq!(
        pose(&world, id).bones.len(),
        7,
        "a rest pose before the first tick"
    );
    sim.step(&mut world, &jobs, 15);
    let t = player(&world, id).time;
    assert!((t - 0.25).abs() < 1e-5, "15 ticks at 60 Hz: {t}");
    let bound = library.clip(&rig.clip("Idle"), &rig.skin()).unwrap();
    let mut expected = library.skeleton(&rig.skin()).unwrap().rest_pose();
    bound.sample(t, &mut expected).unwrap();
    assert_eq!(
        pose(&world, id),
        expected,
        "the pose is the clip at the player's time"
    );

    world.resource_mut::<SimClock>().unwrap().time_scale = TimeScale::new(0.0).unwrap();
    let frozen = pose(&world, id);
    sim.step(&mut world, &jobs, 10);
    assert_eq!(player(&world, id).time, t, "time scale 0 freezes playback");
    assert_eq!(pose(&world, id), frozen);

    world.resource_mut::<SimClock>().unwrap().time_scale = TimeScale::new(0.5).unwrap();
    world.get_mut::<AnimPlayer>(id).unwrap().speed = 2.0;
    sim.step(&mut world, &jobs, 6);
    let t2 = player(&world, id).time;
    // Idle is 0.5 s and loops: 0.25 + 6 × (1/60) × 0.5 × 2 = 0.35.
    assert!((t2 - 0.35).abs() < 1e-5, "{t2}");
    assert_ne!(pose(&world, id), frozen, "it moves again");
}

#[test]
fn pose_is_part_of_the_world_hash() {
    let mut rig = Rig::new();
    let mut world = World::new();
    let id = rig.spawn(&mut world, AnimPlayer::new(rig.clip("Walk")));
    rig.start(&mut world);
    let before = world.state_hash().unwrap();
    world.get_mut::<Pose>(id).unwrap().bones[3].translation.x += 0.001;
    assert_ne!(
        world.state_hash().unwrap(),
        before,
        "a bone moved, the hash changed"
    );
}

/// 100 characters with varied clips, speeds and start times; hashes at `ticks`.
fn crowd(threads: usize, ticks: &[u32]) -> Vec<WorldHash> {
    let mut rig = Rig::new();
    let mut world = World::new();
    for i in 0..100u16 {
        let mut p = AnimPlayer::new(rig.clip(if i % 3 == 0 { "Idle" } else { "Walk" }));
        p.speed = 0.5 + f32::from(i) * 0.013;
        p.time = f32::from(i) * 0.007;
        p.looping = i % 7 != 0;
        rig.spawn(&mut world, p);
    }
    let (mut sim, _) = rig.start(&mut world);
    let jobs = jobs(threads);
    let mut done = 0;
    ticks
        .iter()
        .map(|&target| {
            sim.step(&mut world, &jobs, target - done);
            done = target;
            world.state_hash().unwrap()
        })
        .collect()
}

#[test]
fn anim_in_fixed_step_is_deterministic_across_thread_counts() {
    let ticks = [1, 60, 300];
    let one = crowd(1, &ticks);
    assert_eq!(crowd(8, &ticks), one, "8 threads");
    assert_eq!(crowd(3, &ticks), one, "3 threads");
    assert_ne!(one[0], one[1], "the crowd animates");
}

#[test]
fn players_spawned_during_play_get_a_rest_pose_then_animate() {
    let mut rig = Rig::new();
    let mut world = World::new();
    rig.spawn(&mut world, AnimPlayer::new(rig.clip("Walk")));
    let (mut sim, library) = rig.start(&mut world);
    let jobs = jobs(2);
    sim.step(&mut world, &jobs, 1);
    let late = rig.spawn(&mut world, AnimPlayer::new(rig.clip("Walk")));
    assert!(world.get::<Pose>(late).is_none());
    sim.step(&mut world, &jobs, 1);
    let rest = library.skeleton(&rig.skin()).unwrap().rest_pose();
    assert_eq!(
        pose(&world, late),
        rest,
        "a rest pose at the end of its first tick"
    );
    sim.step(&mut world, &jobs, 10);
    assert_ne!(pose(&world, late), rest, "then it plays");
}

#[test]
fn switching_clips_returns_bones_the_new_clip_leaves_alone_to_rest() {
    use common::{clip, joint};
    use concordia_anim::{BoundClip, Skeleton};
    use concordia_assets::{ChannelPath, Interpolation};
    use concordia_core::math::Vec3;

    let skin: AssetRef = "asset:018bcfe5-6800-7000-a424-6c55076b137f#skin/0"
        .parse()
        .unwrap();
    let (a, b): (AssetRef, AssetRef) = (
        "asset:018bcfe5-6800-7000-a424-6c55076b137f#clip/a"
            .parse()
            .unwrap(),
        "asset:018bcfe5-6800-7000-a424-6c55076b137f#clip/b"
            .parse()
            .unwrap(),
    );
    let skeleton = Skeleton::new(vec![
        joint("root", None, Vec3::ZERO),
        joint("tip", Some(0), Vec3::Y),
    ])
    .unwrap();
    let lin = Interpolation::Linear;
    let moves_tip = clip("tip", ChannelPath::Translation, lin, &[0.0, 1.0], &[5.; 6]);
    let moves_root = clip("root", ChannelPath::Translation, lin, &[0.0, 1.0], &[2.; 6]);
    let mut library = AnimLibrary::new();
    library.insert_clip(
        a.clone(),
        skin.clone(),
        BoundClip::bind(&moves_tip, &skeleton).unwrap(),
    );
    library.insert_clip(
        b.clone(),
        skin.clone(),
        BoundClip::bind(&moves_root, &skeleton).unwrap(),
    );
    library.insert_skeleton(skin.clone(), skeleton);
    let library = Arc::new(library);
    let mut world = World::new();
    let mut ids = DeterministicIdGen::new(5);
    let id = world.spawn(&mut ids).unwrap();
    world.insert(id, Skinned { skin }).unwrap();
    world.insert(id, AnimPlayer::new(a)).unwrap();
    insert_rest_poses(&library, &mut world);
    let mut schedule = Schedule::new();
    add_processors(&mut schedule, library);
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    let jobs = jobs(1);
    sim.step(&mut world, &jobs, 2);
    assert_eq!(pose(&world, id).bones[1].translation, Vec3::splat(5.0));
    world.get_mut::<AnimPlayer>(id).unwrap().clip = b;
    sim.step(&mut world, &jobs, 1);
    let bones = pose(&world, id).bones;
    assert_eq!(
        bones[0].translation,
        Vec3::splat(2.0),
        "the new clip drives the root"
    );
    assert_eq!(bones[1].translation, Vec3::Y, "the tip is back at rest");
}
