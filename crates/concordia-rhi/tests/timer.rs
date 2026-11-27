//! Timer scopes in command lists (M4 phase 4.1a): paired, not nested, unique, outside passes.

use concordia_rhi::{
    ClearValue, ColorAttachment, Command, CommandList, LoadOp, RenderPassDesc, StoreOp, TimerScope,
};

fn list(commands: Vec<Command>) -> CommandList {
    CommandList {
        label: "timers".into(),
        commands,
    }
}

fn begin_pass() -> Command {
    // Any handle will do: validation checks structure, not handles.
    Command::BeginRenderPass(RenderPassDesc {
        colors: vec![ColorAttachment {
            texture: concordia_rhi::Texture::from_raw(0, 0),
            resolve: None,
            load: LoadOp::Clear(ClearValue::Color([0.0; 4])),
            store: StoreOp::Store,
        }],
        depth: None,
        extent: None,
    })
}

/// The list is rejected, and the reason names `reason`.
fn rejected(commands: Vec<Command>, reason: &str) {
    let err = list(commands).validate().expect_err(reason);
    assert!(err.to_string().contains(reason), "{reason}: {err}");
}

#[test]
fn paired_scopes_validate() {
    let ok = list(vec![
        Command::BeginTimer(TimerScope(0)),
        begin_pass(),
        Command::EndRenderPass,
        Command::EndTimer,
        Command::BeginTimer(TimerScope(1)),
        Command::EndTimer,
    ]);
    ok.validate().expect("two sequential scopes");
}

#[test]
fn timer_scopes_must_pair_without_nesting() {
    rejected(vec![Command::BeginTimer(TimerScope(0))], "not ended");
    rejected(vec![Command::EndTimer], "without an open timer");
    rejected(
        vec![
            Command::BeginTimer(TimerScope(0)),
            Command::BeginTimer(TimerScope(1)),
            Command::EndTimer,
            Command::EndTimer,
        ],
        "no nesting",
    );
}

#[test]
fn timer_scope_ids_are_unique_per_list() {
    rejected(
        vec![
            Command::BeginTimer(TimerScope(4)),
            Command::EndTimer,
            Command::BeginTimer(TimerScope(4)),
            Command::EndTimer,
        ],
        "used twice",
    );
}

#[test]
fn timer_scopes_open_and_close_outside_render_passes() {
    rejected(
        vec![
            begin_pass(),
            Command::BeginTimer(TimerScope(0)),
            Command::EndRenderPass,
            Command::EndTimer,
        ],
        "outside render passes",
    );
    rejected(
        vec![
            Command::BeginTimer(TimerScope(0)),
            begin_pass(),
            Command::EndTimer,
            Command::EndRenderPass,
        ],
        "outside render passes",
    );
}
