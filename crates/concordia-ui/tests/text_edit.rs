//! Text box editing (M2 phase 2.7b.3): `edit` applies one typed input to a `UiTextBox` (byte
//! offsets `caret` and `anchor`, always on character boundaries) and the in-engine clipboard.
//! Derived by hand:
//!
//! - "helo", caret 3: insert "l" → "hello", caret 4; ← → 3; Home → 0; End → 5; Backspace →
//!   "hell" (caret 4); Home then Delete → "ell" (caret 0).
//! - "né" is 3 bytes (é is 2): from the end, ← lands on 1, not 2.
//! - "hello world", caret 0: five Shift-→ select 0..5; typing "Hi" replaces it → "Hi world",
//!   caret 2; select all → anchor 0, caret 8; ← without Shift collapses to 0.
//! - Copy of "world" (6..11) puts it on the clipboard and bumps its generation; cut removes it
//!   ("hello ", caret 6); paste puts it back ("hello world", caret 11). A password box never
//!   copies or cuts.
//! - `max_len` 5 counts characters: "abc" + "defg" → "abcde"; a paste is cut the same way.
//! - Enter submits a single-line box (text unchanged) and breaks the line in a multi-line one.
//!   A single-line box turns pasted newlines into spaces: "a\nb" → "a b".
//! - Up and Down move by lines, keeping the column in characters: "ab\ncdef\ng" (lines start at
//!   0, 3, 8), caret 5 (line 1, column 2): ↑ → 2; ↓ → 5; ↓ → line 2 has 1 character → 9.
//! - A box that isn't editable still moves, selects and copies, but never changes.

use concordia_input::{EditKey, TextInput};
use concordia_ui::*;

fn tb(text: &str, caret: u32) -> UiTextBox {
    UiTextBox {
        text: text.into(),
        caret,
        anchor: caret,
        ..UiTextBox::default()
    }
}

fn key(k: EditKey) -> TextInput {
    TextInput::Edit {
        key: k,
        select: false,
    }
}

fn shift(k: EditKey) -> TextInput {
    TextInput::Edit {
        key: k,
        select: true,
    }
}

fn ins(s: &str) -> TextInput {
    TextInput::Insert(s.into())
}

fn run(b: &mut UiTextBox, inputs: &[TextInput]) -> (UiClipboard, Vec<EditResult>) {
    let mut clip = UiClipboard::default();
    let results = inputs.iter().map(|i| edit(b, i, &mut clip)).collect();
    (clip, results)
}

#[test]
fn text_box_inserts_and_edits_at_the_caret() {
    let mut b = tb("helo", 3);
    let (_, r) = run(&mut b, &[ins("l")]);
    assert_eq!((b.text.as_str(), b.caret, b.anchor), ("hello", 4, 4));
    assert!(r[0].changed);
    for (input, caret) in [
        (key(EditKey::Left), 3),
        (key(EditKey::Home), 0),
        (key(EditKey::End), 5),
    ] {
        let (_, r) = run(&mut b, &[input]);
        assert_eq!(b.caret, caret);
        assert!(!r[0].changed, "moving changes no text");
    }
    run(&mut b, &[key(EditKey::Backspace)]);
    assert_eq!((b.text.as_str(), b.caret), ("hell", 4));
    run(&mut b, &[key(EditKey::Home), key(EditKey::Delete)]);
    assert_eq!((b.text.as_str(), b.caret), ("ell", 0));
}

#[test]
fn caret_stays_on_char_boundaries() {
    let mut b = tb("né", 3);
    run(&mut b, &[key(EditKey::Left)]);
    assert_eq!(b.caret, 1);
    run(&mut b, &[key(EditKey::Right), key(EditKey::Backspace)]);
    assert_eq!((b.text.as_str(), b.caret), ("n", 1));
    // A caret inside a character is refused.
    let bad = UiTextBox {
        caret: 2,
        anchor: 2,
        ..tb("né", 0)
    };
    assert_eq!(bad.check().unwrap_err().field_name(), Some("caret"));
}

#[test]
fn text_box_selection_by_shift_and_select_all() {
    let mut b = tb("hello world", 0);
    run(&mut b, &vec![shift(EditKey::Right); 5]);
    assert_eq!((b.anchor, b.caret), (0, 5));
    run(&mut b, &[ins("Hi")]);
    assert_eq!((b.text.as_str(), b.caret, b.anchor), ("Hi world", 2, 2));
    run(&mut b, &[key(EditKey::SelectAll)]);
    assert_eq!((b.anchor, b.caret), (0, 8));
    run(&mut b, &[key(EditKey::Left)]);
    assert_eq!((b.anchor, b.caret), (0, 0), "← collapses to the start");
}

#[test]
fn text_box_copy_cut_paste_through_the_clipboard() {
    let mut b = UiTextBox {
        anchor: 6,
        caret: 11,
        ..tb("hello world", 0)
    };
    let mut clip = UiClipboard::default();
    edit(&mut b, &key(EditKey::Copy), &mut clip);
    assert_eq!((clip.text.as_str(), clip.generation), ("world", 1));
    edit(&mut b, &key(EditKey::Cut), &mut clip);
    assert_eq!((b.text.as_str(), b.caret), ("hello ", 6));
    assert_eq!(clip.generation, 2);
    edit(&mut b, &key(EditKey::Paste), &mut clip);
    assert_eq!((b.text.as_str(), b.caret), ("hello world", 11));

    let mut secret = UiTextBox {
        password: true,
        anchor: 0,
        caret: 3,
        ..tb("pin", 0)
    };
    let mut clip = UiClipboard::default();
    edit(&mut secret, &key(EditKey::Copy), &mut clip);
    edit(&mut secret, &key(EditKey::Cut), &mut clip);
    assert_eq!((clip.generation, secret.text.as_str()), (0, "pin"));
}

#[test]
fn text_box_enforces_max_len_on_insert_and_paste() {
    let mut b = UiTextBox {
        max_len: Some(5),
        ..tb("abc", 3)
    };
    run(&mut b, &[ins("defg")]);
    assert_eq!((b.text.as_str(), b.caret), ("abcde", 5));
    let mut clip = UiClipboard {
        text: "xyz".into(),
        generation: 1,
    };
    let mut c = UiTextBox {
        max_len: Some(4),
        ..tb("ab", 2)
    };
    edit(&mut c, &key(EditKey::Paste), &mut clip);
    assert_eq!(c.text, "abxy");
}

#[test]
fn text_box_enter_submits_or_breaks_line() {
    let mut b = tb("go", 2);
    let (_, r) = run(&mut b, &[key(EditKey::Enter)]);
    assert!(r[0].submitted && !r[0].changed);
    assert_eq!(b.text, "go");
    let mut m = UiTextBox {
        multiline: true,
        ..tb("go", 2)
    };
    let (_, r) = run(&mut m, &[key(EditKey::Enter)]);
    assert!(!r[0].submitted && r[0].changed);
    assert_eq!((m.text.as_str(), m.caret), ("go\n", 3));
    let mut one = tb("", 0);
    run(&mut one, &[ins("a\nb")]);
    assert_eq!(one.text, "a b");
}

#[test]
fn up_and_down_move_by_lines_keeping_the_column() {
    let mut b = UiTextBox {
        multiline: true,
        ..tb("ab\ncdef\ng", 5)
    };
    for (k, want) in [(EditKey::Up, 2), (EditKey::Down, 5), (EditKey::Down, 9)] {
        run(&mut b, &[key(k)]);
        assert_eq!(b.caret, want, "{k:?}");
    }
}

#[test]
fn read_only_boxes_move_and_copy_but_never_change() {
    let mut b = UiTextBox {
        editable: false,
        ..tb("fixed", 5)
    };
    let mut clip = UiClipboard::default();
    for input in [
        ins("x"),
        key(EditKey::Backspace),
        key(EditKey::Cut),
        key(EditKey::Paste),
    ] {
        assert!(!edit(&mut b, &input, &mut clip).changed);
    }
    assert_eq!(b.text, "fixed");
    edit(&mut b, &key(EditKey::SelectAll), &mut clip);
    edit(&mut b, &key(EditKey::Copy), &mut clip);
    assert_eq!(clip.text, "fixed");
}

/// The text box's rules (2.7b.3): text at most 64 KiB, the placeholder at most 4096 bytes,
/// `max_len` at most 65536, caret and anchor inside the text on character boundaries, and a
/// single-line box holds no newline. A spin box: finite values, min < max, a positive step and at
/// most 6 decimals. A list: selected indices ascending without repeats, and only one unless
/// `multi`.
#[test]
fn text_box_spin_box_and_list_rules_name_their_field() {
    let field = |r: Result<(), UiError>| {
        r.map_or_else(|e| e.field_name().unwrap().to_owned(), |()| "ok".into())
    };
    assert_eq!(field(UiTextBox::default().check()), "ok");
    assert_eq!(field(tb(&"x".repeat(65537), 0).check()), "text");
    assert_eq!(
        field(
            UiTextBox {
                placeholder: "p".repeat(4097),
                ..UiTextBox::default()
            }
            .check()
        ),
        "placeholder"
    );
    assert_eq!(
        field(
            UiTextBox {
                max_len: Some(65537),
                ..UiTextBox::default()
            }
            .check()
        ),
        "max_len"
    );
    assert_eq!(field(tb("ab", 3).check()), "caret");
    assert_eq!(
        field(
            UiTextBox {
                anchor: 9,
                ..tb("ab", 0)
            }
            .check()
        ),
        "anchor"
    );
    assert_eq!(field(tb("a\nb", 0).check()), "text");
    assert_eq!(field(UiSpinBox::default().check()), "ok");
    let spin = |f: fn(&mut UiSpinBox)| {
        let mut s = UiSpinBox::default();
        f(&mut s);
        field(s.check())
    };
    assert_eq!(spin(|s| s.value = f32::NAN), "value");
    assert_eq!(spin(|s| s.min = 200.0), "max");
    assert_eq!(spin(|s| s.step = 0.0), "step");
    assert_eq!(spin(|s| s.decimals = 7), "decimals");
    let list = |selected: Vec<u32>, multi: bool| field(UiList { selected, multi }.check());
    assert_eq!(list(vec![2], false), "ok");
    assert_eq!(list(vec![1, 3], true), "ok");
    assert_eq!(list(vec![1, 3], false), "selected");
    assert_eq!(list(vec![3, 1], true), "selected");
    assert_eq!(list(vec![1, 1], true), "selected");
}

/// Defaults: a single-line, editable box wrapping words; a spin box 0..100 by 1 with no
/// decimals; a single-select list.
#[test]
fn text_box_spin_box_and_list_defaults() {
    let b = UiTextBox::default();
    assert!(!b.multiline && b.editable && !b.password && b.max_len.is_none());
    assert_eq!((b.caret, b.anchor, b.wrap), (0, 0, Wrap::Word));
    let s = UiSpinBox::default();
    assert_eq!(
        (s.value, s.min, s.max, s.step, s.decimals),
        (0.0, 0.0, 100.0, 1.0, 0)
    );
    let l = UiList::default();
    assert!(l.selected.is_empty() && !l.multi);
}
