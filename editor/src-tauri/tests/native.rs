//! The native viewport's platform-neutral rules (M4 4.1v(c)): which native features a platform and
//! session have, the page's flags, and the slot's place as an X11 child window.

use concordia_editor::init_script;
use concordia_editor::native::{
    NativeSupport, PixelRect, SlotRect, child_rect, support, x11_session,
};

fn slot(x: f64, y: f64, width: f64, height: f64, scale: f64, viewport_height: f64) -> SlotRect {
    SlotRect {
        x,
        y,
        width,
        height,
        scale,
        viewport_height,
    }
}

#[test]
fn the_slot_maps_to_device_pixels_below_the_pages_top() {
    // The window is 56 px taller than the page (a header above the webview), at scale 2.
    let rect = child_rect(&slot(10.0, 20.0, 300.0, 200.0, 2.0, 450.0), 956).unwrap();
    assert_eq!(
        rect,
        PixelRect {
            x: 20,
            y: 96,
            width: 600,
            height: 400
        }
    );
}

#[test]
fn child_windows_are_never_empty_and_never_above_the_window() {
    let rect = child_rect(&slot(0.0, 0.0, 0.0, 0.2, 1.0, 800.0), 600).unwrap();
    assert_eq!((rect.y, rect.width, rect.height), (0, 1, 1));
}

#[test]
fn invalid_slots_are_refused() {
    for bad in [
        slot(f64::NAN, 0.0, 10.0, 10.0, 1.0, 100.0),
        slot(0.0, 0.0, -1.0, 10.0, 1.0, 100.0),
        slot(0.0, 0.0, 10.0, 10.0, 0.0, 100.0),
    ] {
        assert!(child_rect(&bad, 100).is_err(), "{bad:?}");
    }
}

#[test]
fn gtk_picks_x11_as_gdk_backend_and_the_session_say() {
    let cases = [
        ((Some("x11"), Some("wayland-0"), Some(":0")), true),
        ((Some("wayland,x11"), Some("wayland-0"), Some(":0")), false),
        ((Some("*"), None, Some(":0")), true),
        ((None, Some("wayland-0"), Some(":0")), false),
        ((None, Some(""), Some(":99")), true),
        ((None, None, None), false),
        ((Some("x11"), None, None), false),
    ];
    for ((backend, wayland, display), x11) in cases {
        assert_eq!(
            x11_session(backend, wayland, display),
            x11,
            "{backend:?} {wayland:?} {display:?}"
        );
    }
}

#[test]
fn macos_has_native_input_and_linux_a_native_viewport_only_on_x11() {
    let both = NativeSupport {
        viewport: true,
        input: true,
    };
    assert_eq!(support("macos", false), both);
    assert_eq!(
        support("linux", true),
        NativeSupport {
            viewport: true,
            input: false
        }
    );
    assert_eq!(support("linux", false), NativeSupport::NONE);
    assert_eq!(support("windows", true), NativeSupport::NONE);
}

#[test]
fn the_init_script_tells_the_page_what_the_shell_does() {
    let linux = NativeSupport {
        viewport: true,
        input: false,
    };
    let script = init_script("ws://127.0.0.1:1/api", linux, None);
    assert!(
        script.contains(r#"window.__CONCORDIA_ENGINE__ = "ws://127.0.0.1:1/api";"#),
        "{script}"
    );
    assert!(
        script.contains("window.__CONCORDIA_NATIVE__ = true;"),
        "{script}"
    );
    assert!(
        script.contains("window.__CONCORDIA_NATIVE_INPUT__ = false;"),
        "{script}"
    );
    assert!(!script.contains("__CONCORDIA_BENCH__"), "{script}");
    let bench = init_script("u", NativeSupport::NONE, Some(240));
    assert!(
        bench.contains("window.__CONCORDIA_NATIVE__ = false;")
            && bench.contains("window.__CONCORDIA_BENCH__ = { frames: 240 };"),
        "{bench}"
    );
}
