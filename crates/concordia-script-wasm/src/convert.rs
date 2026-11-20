//! Protocol types to and from their WIT bindings.

use concordia_core::id::EntityId;
use concordia_script::protocol::{self as p, ids_to_bytes};

use crate::bindings::concordia::game::types as w;

/// A description from the guest; type descriptions arrive as JSON.
pub(crate) fn module_desc(d: w::ModuleDesc) -> Result<p::ModuleDesc, String> {
    let types = |list: Vec<String>| -> Result<Vec<concordia_reflect::TypeDesc>, String> {
        list.iter()
            .map(|json| serde_json::from_str(json).map_err(|e| format!("type description: {e}")))
            .collect()
    };
    Ok(p::ModuleDesc {
        name: d.name,
        version: d.version,
        types: types(d.types)?,
        components: types(d.components)?,
        signals: types(d.signals)?,
        traits: d.traits.into_iter().map(trait_desc).collect(),
        processors: d.processors.into_iter().map(processor_desc).collect(),
    })
}

fn trait_desc(t: w::TraitDesc) -> p::TraitDesc {
    p::TraitDesc {
        name: t.name,
        docs: t.docs,
        components: t.components,
        requires: t.requires,
    }
}

fn phase(phase: w::Phase) -> p::Phase {
    match phase {
        w::Phase::PrePhysics => p::Phase::PrePhysics,
        w::Phase::Physics => p::Phase::Physics,
        w::Phase::PostPhysics => p::Phase::PostPhysics,
        w::Phase::FrameEnd => p::Phase::FrameEnd,
        w::Phase::Presentation => p::Phase::Presentation,
    }
}

fn processor_desc(d: w::ProcessorDesc) -> p::ProcessorDesc {
    p::ProcessorDesc {
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

/// A batch for the guest.
pub(crate) fn batch(b: &p::Batch) -> w::Batch {
    w::Batch {
        ids: ids_to_bytes(&b.ids),
        columns: b
            .columns
            .iter()
            .map(|c| w::Column {
                name: c.name.clone(),
                data: c.data.clone(),
            })
            .collect(),
        signals: b
            .signals
            .iter()
            .map(|s| w::SignalColumn {
                name: s.name.clone(),
                targets: ids_to_bytes(&s.targets),
                data: s.data.clone(),
            })
            .collect(),
    }
}

/// A tick context for the guest.
pub(crate) fn tick(t: &p::TickContext) -> w::TickContext {
    w::TickContext {
        tick: t.tick,
        dt: t.dt,
        time: t.time,
        seed: t.seed,
    }
}

/// A run's output from the guest.
pub(crate) fn run_output(o: w::RunOutput) -> Result<p::RunOutput, String> {
    Ok(p::RunOutput {
        writes: o
            .writes
            .into_iter()
            .map(|c| p::Column {
                name: c.name,
                data: c.data,
            })
            .collect(),
        commands: o
            .commands
            .into_iter()
            .map(command)
            .collect::<Result<_, _>>()?,
    })
}

fn value(v: w::Value) -> p::Value {
    p::Value {
        name: v.name,
        data: v.data,
    }
}

fn target(t: w::Target) -> Result<p::Target, String> {
    match t {
        w::Target::Entity(id) => {
            let uuid = uuid::Uuid::from_u128((u128::from(id.hi) << 64) | u128::from(id.lo));
            EntityId::try_from(uuid)
                .map(p::Target::Entity)
                .map_err(|e| format!("target {uuid}: {e}"))
        }
        w::Target::Pending(n) => Ok(p::Target::Pending(n)),
    }
}

fn command(c: w::Command) -> Result<p::Command, String> {
    Ok(match c {
        w::Command::Spawn(values) => p::Command::Spawn(values.into_iter().map(value).collect()),
        w::Command::SpawnTemplate(name) => p::Command::SpawnTemplate(name),
        w::Command::Insert((t, v)) => p::Command::Insert(target(t)?, value(v)),
        w::Command::Remove((t, name)) => p::Command::Remove(target(t)?, name),
        w::Command::Despawn(t) => p::Command::Despawn(target(t)?),
        w::Command::Signal((t, v)) => p::Command::Signal(target(t)?, value(v)),
    })
}
