//! Reloading a module (ADR-0061): new behavior on the same world; changed component types are
//! migrated through reflect, all or nothing; changed processors are refused.

use std::sync::Arc;

use concordia_script::conformance::Rig;
use concordia_script::{ModuleSource, ScriptBackend, ScriptError, ScriptModule, TestBackend};
use concordia_script_guest::{Module, ModuleDesc, Phase, RunCtx, describe_type, processor};
use serde_json::json;

macro_rules! hp_module {
    ($module:ident, $hp:ty, $step:expr, $extra:expr) => {
        struct $module;
        impl Module for $module {
            fn describe() -> ModuleDesc {
                let mut processors = vec![
                    processor("r.heal", Phase::PostPhysics)
                        .writes::<$hp>()
                        .build(),
                ];
                if $extra {
                    processors.push(processor("r.extra", Phase::PostPhysics).build());
                }
                ModuleDesc {
                    name: "r".into(),
                    version: 1,
                    components: vec![describe_type::<$hp>()],
                    processors,
                    ..ModuleDesc::default()
                }
            }
            fn run(name: &str, ctx: &mut RunCtx<'_>) -> Result<(), String> {
                if name != "r.heal" {
                    return Ok(());
                }
                let mut hp: Vec<$hp> = ctx.read()?;
                hp.iter_mut().for_each($step);
                ctx.write(&hp)
            }
        }
    };
}

mod v1 {
    use concordia_reflect::Reflect;
    use serde::{Deserialize, Serialize};

    /// Health, version 1.
    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
    #[reflect(name = "r.Hp", default)]
    pub struct Hp {
        /// Current.
        pub value: u32,
        /// A note (dropped in version 2).
        pub note: String,
    }
}

mod v2 {
    use concordia_reflect::Reflect;
    use serde::{Deserialize, Serialize};

    /// Health, version 2.
    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
    #[reflect(name = "r.Hp", default)]
    pub struct Hp {
        /// Current.
        pub value: u32,
        /// Maximum (new in version 2).
        #[reflect(default = 100)]
        pub max: u32,
    }
}

mod bad {
    use concordia_reflect::Reflect;
    use serde::{Deserialize, Serialize};

    /// Health whose `value` became text: stored numbers can't convert.
    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
    #[reflect(name = "r.Hp", default)]
    pub struct Hp {
        /// Current.
        pub value: String,
        /// A note.
        pub note: String,
    }
}

hp_module!(V1, v1::Hp, |h| h.value += 1, false);
hp_module!(V1Extra, v1::Hp, |h| h.value += 1, true);
hp_module!(V2, v2::Hp, |h| h.value += 10, false);
hp_module!(Bad, bad::Hp, |h| h.value.push('!'), false);

fn load<M: Module>() -> Arc<dyn ScriptModule> {
    TestBackend::new()
        .link::<M>()
        .load(&ModuleSource::Linked("r".into()))
        .unwrap()
}

fn rig() -> (Rig, Vec<concordia_core::id::EntityId>) {
    let mut rig = Rig::with_host(load::<V1>(), 1, &[], |_| {});
    let ids = (0..2)
        .map(|i| rig.spawn(&[("r.Hp", json!({"value": i, "note": "n"}))]))
        .collect();
    rig.step(1);
    (rig, ids)
}

#[test]
fn reload_migrates_changed_component_types_and_reports_fields() {
    let (mut rig, ids) = rig();
    let report = rig
        .host
        .reload(load::<V2>(), &mut rig.registry, &mut rig.world)
        .unwrap();
    assert_eq!(report.migrated.len(), 1, "{report:?}");
    let m = &report.migrated[0];
    assert_eq!((m.name.as_str(), m.values), ("r.Hp", 2));
    assert_eq!(
        (m.added.as_slice(), m.removed.as_slice()),
        (&["/max".to_owned()][..], &["/note".to_owned()][..])
    );
    assert_eq!(
        rig.get(ids[1], "r.Hp"),
        Some(json!({"value": 2, "max": 100}))
    );
    rig.step(1);
    assert_eq!(
        rig.get(ids[1], "r.Hp"),
        Some(json!({"value": 12, "max": 100})),
        "v2 runs on v2 values"
    );
    assert!(rig.host.faults().is_empty(), "{:?}", rig.host.faults());
    rig.spawn(&[("r.Hp", json!({"value": 0, "max": 5}))]);
}

#[test]
fn unchanged_types_are_not_migrated() {
    let (mut rig, _) = rig();
    let before = rig.hash();
    let report = rig
        .host
        .reload(load::<V1>(), &mut rig.registry, &mut rig.world)
        .unwrap();
    assert!(report.migrated.is_empty(), "{report:?}");
    assert_eq!(rig.hash(), before);
    let id = rig.spawn(&[("r.Hp", json!({"value": 5, "note": "after"}))]);
    rig.step(1);
    assert_eq!(
        rig.get(id, "r.Hp"),
        Some(json!({"value": 6, "note": "after"}))
    );
    assert!(rig.host.faults().is_empty(), "{:?}", rig.host.faults());
}

#[test]
fn failed_migration_leaves_the_world_and_types_consistent() {
    let (mut rig, ids) = rig();
    let before = rig.hash();
    let err = rig
        .host
        .reload(load::<Bad>(), &mut rig.registry, &mut rig.world)
        .unwrap_err();
    assert!(matches!(err, ScriptError::Migration { .. }), "{err}");
    assert!(err.to_string().contains("r.Hp"), "{err}");
    assert_eq!(rig.hash(), before, "the world is unchanged");
    rig.step(1);
    assert_eq!(
        rig.get(ids[1], "r.Hp"),
        Some(json!({"value": 3, "note": "n"})),
        "v1 still runs"
    );
    assert!(rig.host.faults().is_empty(), "{:?}", rig.host.faults());
    rig.spawn(&[("r.Hp", json!({"value": 0, "note": "new"}))]);
}

#[test]
fn reload_refuses_changed_processors() {
    let (mut rig, _) = rig();
    let err = rig
        .host
        .reload(load::<V1Extra>(), &mut rig.registry, &mut rig.world)
        .unwrap_err();
    assert!(
        matches!(err, ScriptError::ReloadChangesProcessors { .. }),
        "{err}"
    );
}
