//! Text entry, spin boxes and lists in a tick of interaction (2.7b.3): typed text goes to the
//! focused text box, clicks and drags place the caret, spin boxes step and commit typed
//! numbers, and lists select by click, Ctrl, Shift and arrows.

use concordia_core::id::EntityId;
use concordia_input::TextInput;

use super::{Keys, Signal, Tick};
use crate::parts::Parts;
use crate::text::byte_at;
use crate::textbox::edit;

impl Tick<'_> {
    /// Puts `w`'s caret at UI point `q` (extending the selection when `extend`).
    pub(super) fn place_caret(&mut self, w: EntityId, q: [f32; 2], extend: bool) {
        let Some(Parts::TextBox(p)) = self.layout.parts.get(&w) else {
            return;
        };
        let Some(b) = self.text_boxes.get_mut(&w) else {
            return;
        };
        let at = if p.placeholder {
            0
        } else {
            let shown = byte_at(&p.text, [q[0] - p.origin[0], q[1] - p.origin[1]]);
            b.text_offset(shown)
        };
        b.caret = at as u32;
        if !extend {
            b.anchor = b.caret;
        }
    }

    /// Steps a spin box by `dir` steps and shows the new value.
    pub(super) fn spin_step(&mut self, w: EntityId, dir: f32) {
        let Some(spin) = self.spins.get(&w).copied() else {
            return;
        };
        self.set_spin(w, spin.fit(spin.value + dir * spin.step));
    }

    /// Sets a spin box's value (signalling a change) and rewrites its text.
    fn set_spin(&mut self, w: EntityId, v: f32) {
        let Some(spin) = self.spins.get_mut(&w) else {
            return;
        };
        let changed = v != spin.value;
        spin.value = v;
        let text = spin.format(v);
        if changed {
            self.signals.push((w, Signal::Value(v)));
            self.cue(w, |s| &s.change);
        }
        if let Some(b) = self.text_boxes.get_mut(&w)
            && b.text != text
        {
            b.caret = text.len() as u32;
            b.anchor = b.caret;
            b.text = text;
        }
    }

    /// Reads a spin box's typed text back into its value (clamped, rounded).
    fn commit(&mut self, w: EntityId) {
        let (Some(spin), Some(b)) = (self.spins.get(&w).copied(), self.text_boxes.get(&w)) else {
            return;
        };
        let typed = b.text.trim().parse::<f32>().ok().filter(|v| v.is_finite());
        self.set_spin(w, typed.map_or(spin.value, |v| spin.fit(v)));
    }

    /// Selects list item `i`: alone; added or removed with Ctrl; a range from the anchor with
    /// Shift (multi-select lists only).
    pub(super) fn list_click(&mut self, w: EntityId, i: u32) {
        let (ctrl, shift) = (self.ctrl, self.shift);
        let anchor = self.states.get(&w).and_then(|s| s.item).unwrap_or(i);
        let Some(list) = self.lists.get_mut(&w) else {
            return;
        };
        let keep_anchor = list.multi && shift;
        list.selected = match (list.multi, ctrl, shift) {
            (true, _, true) => (anchor.min(i)..=anchor.max(i)).collect(),
            (true, true, false) => {
                let mut s = list.selected.clone();
                match s.binary_search(&i) {
                    Ok(at) => {
                        s.remove(at);
                    }
                    Err(at) => s.insert(at, i),
                }
                s
            }
            _ => vec![i],
        };
        if !keep_anchor {
            self.state(w).item = Some(i);
        }
        self.signals.push((w, Signal::Selected(i)));
        self.cue(w, |s| &s.change);
    }

    /// Keys a focused text box, spin box or list uses (`None`: not one of them). A text box
    /// swallows arrows, Accept and Page keys (they edit through typed text instead).
    pub(super) fn entry_keys(&mut self, f: EntityId, keys: &Keys) -> Option<bool> {
        if self.spins.contains_key(&f) && (keys.up || keys.down) {
            self.spin_step(f, if keys.up { 1.0 } else { -1.0 });
            return Some(true);
        }
        if self.text_boxes.contains_key(&f) {
            return Some(true);
        }
        let list = self.lists.get(&f)?.clone();
        if !(keys.up || keys.down) {
            return None;
        }
        let count = self.element(f).map_or(0, |e| e.children.len() as u32);
        if count == 0 {
            return Some(true);
        }
        let to = if keys.down {
            list.selected.last().map_or(0, |s| (s + 1).min(count - 1))
        } else {
            list.selected.first().map_or(0, |s| s.saturating_sub(1))
        };
        let (shift, ctrl) = (self.shift, self.ctrl);
        self.shift = shift && list.multi;
        self.ctrl = false;
        self.list_click(f, to);
        (self.shift, self.ctrl) = (shift, ctrl);
        Some(true)
    }

    /// Applies the tick's typed text to the focused, enabled text box.
    pub(super) fn type_text(&mut self, inputs: &[TextInput]) {
        let Some(f) = self.focused() else { return };
        if self.states.get(&f).is_some_and(|s| s.disabled) {
            return;
        }
        for input in inputs {
            let Some(b) = self.text_boxes.get_mut(&f) else {
                return;
            };
            let result = edit(b, input, &mut self.clipboard);
            let text = b.text.clone();
            if result.changed {
                self.signals.push((f, Signal::TextChanged(text.clone())));
            }
            if result.submitted {
                self.signals.push((f, Signal::Submitted(text)));
                self.commit(f);
            }
        }
    }

    /// Spin boxes show their value unless being typed in; leaving one commits what was typed.
    pub(super) fn sync_spins(&mut self) {
        let ids: Vec<EntityId> = self.spins.keys().copied().collect();
        for id in ids {
            let focused = self.states.get(&id).is_some_and(|s| s.focused);
            if self.focused_before == Some(id) && !focused {
                self.commit(id);
            }
            if !focused && let Some(spin) = self.spins.get(&id).copied() {
                self.set_spin(id, spin.value);
            }
        }
    }

    /// Whether a text box has focus (it takes the keyboard).
    pub(super) fn typing(&self) -> bool {
        self.focused()
            .is_some_and(|f| self.text_boxes.contains_key(&f))
    }
}
