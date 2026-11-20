//! The WIT binding of the protocol on the guest side (used by [`export_module!`](crate::export_module)).

use concordia_core::id::EntityId;
use concordia_script_protocol as p;

use crate::Module;

/// Guest bindings of `concordia:game/gameplay`.
#[allow(missing_docs, clippy::all)]
pub mod bindings {
    wit_bindgen::generate!({
        world: "gameplay",
        path: "../../wit/concordia-game",
        pub_export_macro: true,
        default_bindings_module: "concordia_script_guest::wasm::bindings",
    });
}

use bindings::concordia::game::{host, types as w};

/// `describe`, converted to WIT types.
pub fn describe<M: Module>() -> w::ModuleDesc {
    let d = M::describe();
    let json = |t: &concordia_reflect::TypeDesc| {
        serde_json::to_string(t).expect("a type description serializes")
    };
    w::ModuleDesc {
        name: d.name,
        version: d.version,
        types: d.types.iter().map(json).collect(),
        components: d.components.iter().map(json).collect(),
        signals: d.signals.iter().map(json).collect(),
        traits: d.traits.into_iter().map(trait_desc).collect(),
        processors: d.processors.into_iter().map(processor_desc).collect(),
    }
}

/// `run`, converted from and to WIT types.
pub fn run<M: Module>(
    processor: &str,
    ctx: w::TickContext,
    batch: w::Batch,
) -> Result<w::RunOutput, String> {
    let batch = from_batch(batch)?;
    let tick = p::TickContext {
        tick: ctx.tick,
        dt: ctx.dt,
        time: ctx.time,
        seed: ctx.seed,
    };
    let mut log = |line: &str| host::log(line);
    let out = crate::run_module::<M>(processor, tick, &batch, &mut log)?;
    Ok(w::RunOutput {
        writes: out.writes.into_iter().map(column).collect(),
        commands: out.commands.into_iter().map(command).collect(),
    })
}

fn trait_desc(t: p::TraitDesc) -> w::TraitDesc {
    w::TraitDesc {
        name: t.name,
        docs: t.docs,
        components: t.components,
        requires: t.requires,
    }
}

fn phase(phase: p::Phase) -> w::Phase {
    match phase {
        p::Phase::PrePhysics => w::Phase::PrePhysics,
        p::Phase::Physics => w::Phase::Physics,
        p::Phase::PostPhysics => w::Phase::PostPhysics,
        p::Phase::FrameEnd => w::Phase::FrameEnd,
        p::Phase::Presentation => w::Phase::Presentation,
    }
}

fn processor_desc(d: p::ProcessorDesc) -> w::ProcessorDesc {
    w::ProcessorDesc {
        name: d.name,
        phase: phase(d.phase),
        reads: d.reads,
        writes: d.writes,
        with: d.with,
        without: d.without,
        receives: d.receives,
        after: d.after,
        before: d.before,
    }
}

fn from_batch(b: w::Batch) -> Result<p::Batch, String> {
    let ids = p::ids_from_bytes(&b.ids).map_err(|e| e.to_string())?;
    let mut signals = Vec::with_capacity(b.signals.len());
    for s in b.signals {
        signals.push(p::SignalColumn {
            name: s.name,
            targets: p::ids_from_bytes(&s.targets).map_err(|e| e.to_string())?,
            data: s.data,
        });
    }
    Ok(p::Batch {
        ids,
        columns: b
            .columns
            .into_iter()
            .map(|c| p::Column {
                name: c.name,
                data: c.data,
            })
            .collect(),
        signals,
    })
}

fn column(c: p::Column) -> w::Column {
    w::Column {
        name: c.name,
        data: c.data,
    }
}

fn value(v: p::Value) -> w::Value {
    w::Value {
        name: v.name,
        data: v.data,
    }
}

fn entity(id: EntityId) -> w::EntityId {
    let n = id.as_uuid().as_u128();
    w::EntityId {
        hi: (n >> 64) as u64,
        lo: n as u64,
    }
}

fn target(t: p::Target) -> w::Target {
    match t {
        p::Target::Entity(id) => w::Target::Entity(entity(id)),
        p::Target::Pending(n) => w::Target::Pending(n),
    }
}

fn command(c: p::Command) -> w::Command {
    match c {
        p::Command::Spawn(values) => w::Command::Spawn(values.into_iter().map(value).collect()),
        p::Command::SpawnTemplate(name) => w::Command::SpawnTemplate(name),
        p::Command::Insert(t, v) => w::Command::Insert((target(t), value(v))),
        p::Command::Remove(t, name) => w::Command::Remove((target(t), name)),
        p::Command::Despawn(t) => w::Command::Despawn(target(t)),
        p::Command::Signal(t, v) => w::Command::Signal((target(t), value(v))),
    }
}
