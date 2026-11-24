//! The menus and HUD: screens shown by the night's phase, readouts of the night and the can, the
//! night-records list, and controls that report their values and presses to the night.

use concordia_script_guest::{EntityId, RunCtx};
use lamplight_types::engine::{SetBusGain, UiText};
use lamplight_types::night::NightTick;
use lamplight_types::ui::{
    Parent, UiDropdown, UiNode, UiPressed, UiProgress, UiSpinBox, UiTextBox, UiValueChanged,
};

use crate::rules;
use crate::{Command, Control, Ended, Readout, RecordRow, Records, Screen, Setting, Where};

/// The night as last broadcast, and the night entity.
fn night_tick(ctx: &RunCtx<'_>) -> Result<Option<(EntityId, NightTick)>, String> {
    Ok(ctx.signals::<NightTick>()?.into_iter().last())
}

/// Screens show in their phases.
pub(crate) fn screens(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let Some((_, tick)) = night_tick(ctx)? else {
        return Ok(());
    };
    let screens: Vec<Screen> = ctx.read()?;
    let mut nodes: Vec<UiNode> = ctx.read()?;
    for (screen, node) in screens.iter().zip(&mut nodes) {
        node.visible = screen.phases.contains(&tick.phase);
    }
    ctx.write(&nodes)
}

/// Text readouts: the dawn clock, lamps lit, and the night's result.
pub(crate) fn readouts(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let Some((_, tick)) = night_tick(ctx)? else {
        return Ok(());
    };
    let readouts: Vec<Readout> = ctx.read()?;
    let mut texts: Vec<UiText> = ctx.read()?;
    for (readout, text) in readouts.iter().zip(&mut texts) {
        let line = match readout.field.as_str() {
            "clock" => rules::dawn_in(tick.clock, tick.length),
            "lamps" => format!("Lamps lit {}", tick.lamps_lit),
            "result" => rules::result_line(&tick.phase).to_owned(),
            _ => continue,
        };
        match text.runs.first_mut() {
            Some(run) => run.text = line,
            None => *text = UiText::centred(&line),
        }
    }
    ctx.write(&texts)
}

/// Bars: the can's oil and Town Spirit.
pub(crate) fn bars(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let tick = night_tick(ctx)?.map(|(_, t)| t);
    let can = ctx
        .signals::<Where>()?
        .into_iter()
        .last()
        .map(|(_, w)| w.can);
    let readouts: Vec<Readout> = ctx.read()?;
    let mut bars: Vec<UiProgress> = ctx.read()?;
    for (readout, bar) in readouts.iter().zip(&mut bars) {
        match (readout.field.as_str(), &tick, can) {
            ("oil", _, Some(can)) => bar.value = can,
            ("spirit", Some(tick), _) => bar.value = tick.spirit as f32,
            _ => {}
        }
    }
    ctx.write(&bars)
}

/// The records list gains a row for each night that ends.
pub(crate) fn records(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let ended = ctx.signals::<Ended>()?;
    let mut lists: Vec<Records> = ctx.read()?;
    for (list, records) in ids.iter().zip(&mut lists) {
        for (_, end) in &ended {
            let who = if end.name.is_empty() {
                "The lamplighter"
            } else {
                end.name.as_str()
            };
            let how = if end.won {
                "saw the dawn"
            } else {
                "lost the square"
            };
            let line = format!(
                "{who} {how}: {} lamps lit, {} wisps burned",
                end.lamps, end.burned
            );
            let row = ctx.spawn_template("record_row");
            ctx.insert(row, &Parent(*list))?;
            ctx.insert(row, &UiText::centred(&line))?;
            ctx.insert(
                row,
                &RecordRow {
                    night: records.rows,
                },
            )?;
            records.rows += 1;
        }
    }
    ctx.write(&lists)
}

/// Tells the night a control's value.
fn report(ctx: &mut RunCtx<'_>, night: EntityId, setting: Setting) -> Result<(), String> {
    ctx.signal(night, &setting)
}

/// Spin boxes report their numbers (the night's length).
pub(crate) fn spinners(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let Some((night, _)) = night_tick(ctx)? else {
        return Ok(());
    };
    let controls: Vec<Control> = ctx.read()?;
    let spins: Vec<UiSpinBox> = ctx.read()?;
    for (control, spin) in controls.iter().zip(&spins) {
        let setting = Setting {
            role: control.role.clone(),
            value: spin.value,
            text: String::new(),
        };
        report(ctx, night, setting)?;
    }
    Ok(())
}

/// Dropdowns report their choices (the fog).
pub(crate) fn dropdowns(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let Some((night, _)) = night_tick(ctx)? else {
        return Ok(());
    };
    let controls: Vec<Control> = ctx.read()?;
    let drops: Vec<UiDropdown> = ctx.read()?;
    for (control, drop) in controls.iter().zip(&drops) {
        let choice = drop.selected.unwrap_or(1) as f32;
        let setting = Setting {
            role: control.role.clone(),
            value: choice,
            text: String::new(),
        };
        report(ctx, night, setting)?;
    }
    Ok(())
}

/// Text boxes report their text (the name).
pub(crate) fn textboxes(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let Some((night, _)) = night_tick(ctx)? else {
        return Ok(());
    };
    let controls: Vec<Control> = ctx.read()?;
    let boxes: Vec<UiTextBox> = ctx.read()?;
    for (control, text) in controls.iter().zip(&boxes) {
        let setting = Setting {
            role: control.role.clone(),
            value: 0.0,
            text: text.text.clone(),
        };
        report(ctx, night, setting)?;
    }
    Ok(())
}

/// Volume sliders set their bus's gain as they move.
pub(crate) fn sliders(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let controls: Vec<Control> = ctx.read()?;
    for (id, changed) in ctx.signals::<UiValueChanged>()? {
        if let Some(i) = ids.iter().position(|e| *e == id) {
            let gain = SetBusGain {
                bus: controls[i].role.clone(),
                gain_db: changed.value,
            };
            ctx.signal(id, &gain)?;
        }
    }
    Ok(())
}

/// Buttons ask the night to do what they say.
pub(crate) fn buttons(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let Some((night, _)) = night_tick(ctx)? else {
        return Ok(());
    };
    let ids = ctx.ids().to_vec();
    let controls: Vec<Control> = ctx.read()?;
    for (id, _) in ctx.signals::<UiPressed>()? {
        if let Some(i) = ids.iter().position(|e| *e == id) {
            ctx.signal(
                night,
                &Command {
                    action: controls[i].role.clone(),
                },
            )?;
        }
    }
    Ok(())
}
