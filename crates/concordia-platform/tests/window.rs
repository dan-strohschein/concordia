//! Window description and winit key mapping (M1 phase 1.1).

use std::num::NonZeroU32;

use concordia_input::{KeyCode, MouseButton};
use concordia_platform::{WindowDesc, key_from_winit, mouse_button_from_winit};
use concordia_scene::{read_document, write_document};

#[test]
fn window_desc_round_trips_through_reflect() {
    let desc = WindowDesc {
        title: "Stack".into(),
        width: NonZeroU32::new(1280).unwrap(),
        height: NonZeroU32::new(720).unwrap(),
        resizable: true,
        fullscreen: true,
    };
    let text = write_document(&desc).unwrap();
    assert_eq!(read_document::<WindowDesc>(&text).unwrap(), desc);
}

#[test]
fn window_desc_rejects_zero_size() {
    let text = r#"WindowDesc(title: "x", width: 0, height: 720, resizable: true)"#;
    assert!(read_document::<WindowDesc>(text).is_err());
}

#[test]
fn winit_keys_and_buttons_map_to_engine_vocabulary() {
    use winit::event::MouseButton as W;
    use winit::keyboard::KeyCode as K;
    assert_eq!(key_from_winit(K::KeyW), Some(KeyCode::W));
    assert_eq!(key_from_winit(K::Digit7), Some(KeyCode::Digit7));
    assert_eq!(key_from_winit(K::ShiftLeft), Some(KeyCode::ShiftLeft));
    assert_eq!(key_from_winit(K::ArrowUp), Some(KeyCode::ArrowUp));
    assert_eq!(key_from_winit(K::F12), Some(KeyCode::F12));
    assert_eq!(key_from_winit(K::NumLock), None);
    assert_eq!(mouse_button_from_winit(W::Left), Some(MouseButton::Left));
    assert_eq!(mouse_button_from_winit(W::Other(9)), None);
}
