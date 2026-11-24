//! Lamplight's menus and HUD in the engine (M2 phase 2.10c.3), driven by real input: pointer
//! clicks at a widget's centre (from `ui.inspect`), typed text and keys.
//!
//! By hand (60 Hz; a widget's signal raised on tick t is heard on t + 1, and the night hears the
//! controls' settings a tick after that):
//! - The town opens on the title screen: the title shown, the HUD, pause menu and end screen
//!   hidden.
//! - Typing "Ada" into the focused name box sets the night's name. Two ↑ on the focused spin box
//!   (180 s, step 30) make 240 s. Enter, ↓, Enter on the focused fog dropdown (Normal) picks
//!   Thick, density 1.75. Start makes the night `playing` with those values, hides the title and
//!   shows the HUD.
//! - While the name box has the keyboard, W reads as released in the lamplighter's `walk`.
//! - The HUD reads the night: the oil bar holds the can (100, then 60 when the can is set to
//!   60), the spirit bar 10, "Dawn in 3:00" a few ticks into a 180 s night (the seconds left
//!   round up), and "Lamps lit 0".
//! - P pauses: the clock holds still and the pause menu shows. A click at the Music slider's
//!   left end sets it to its minimum, −40 dB, and `audio.buses` reports Music at −40. Resume
//!   plays on.
//! - A night won at dawn shows the end screen, "Dawn came. The town held." and one record row
//!   naming Ada. "Another night" goes back to the title; starting again resets the town: every
//!   lamp dark, no wisps, a full can, 10 spirit, and the record stays (one row).

#[path = "lamplight_world.rs"]
mod world;

use serde_json::{Value, json};
use world::Night;

fn night_field(n: &mut Night, field: &str) -> Value {
    n.night()[field].clone()
}

/// Starts the night from the title screen as Ada.
fn start_as_ada(n: &mut Night) {
    let name = n.named("Name box").0;
    n.click(&name);
    n.type_text("Ada");
    let start = n.named("Start").0;
    n.click(&start);
    n.step(3);
}

#[test]
fn the_town_opens_on_the_title_screen() {
    let mut n = Night::new("title");
    n.step(2);
    assert!(n.shown("Title screen"));
    for hidden in ["HUD", "Pause menu", "End screen"] {
        assert!(!n.shown(hidden), "{hidden} is hidden");
    }
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn the_title_screen_starts_the_night_with_its_choices() {
    let mut n = Night::new("title");
    n.step(2);
    let name = n.named("Name box").0;
    n.click(&name);
    n.type_text("Ada");
    let walker = n.one("lamplight.Lamplighter", "edit").0;
    let walk = n.play(&walker, "InputActions")["states"]["walk"].clone();
    assert_eq!(walk["pressed"], false, "typing W never walks: {walk}");
    let length = n.named("Night length").0;
    n.click(&length);
    n.tap("ArrowUp");
    n.tap("ArrowUp");
    let fog = n.named("Fog").0;
    n.click(&fog);
    n.tap("Escape"); // the click opened it; close, then drive it by keys
    n.tap("Enter");
    n.tap("ArrowDown");
    n.tap("Enter");
    let start = n.named("Start").0;
    n.click(&start);
    n.step(3);
    let state = n.night();
    assert_eq!(state["phase"], "playing", "{state}");
    assert_eq!(state["name"], "Ada");
    assert_eq!(state["length"], 240.0);
    assert_eq!(state["density"], 1.75);
    assert!(!n.shown("Title screen"));
    assert!(n.shown("HUD"));
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn the_hud_follows_the_night() {
    let mut n = Night::new("title");
    n.step(2);
    start_as_ada(&mut n);
    let oil = n.named("Oil").0;
    let spirit = n.named("Spirit").0;
    assert_eq!(n.play(&oil, "UiProgress")["value"], 100.0);
    assert_eq!(n.play(&spirit, "UiProgress")["value"], 10.0);
    let clock = n.named("Dawn clock").0;
    let text = n.play(&clock, "UiText")["runs"][0]["text"].clone();
    assert_eq!(text, "Dawn in 3:00");
    let lamps = n.named("Lamps lit").0;
    assert_eq!(n.play(&lamps, "UiText")["runs"][0]["text"], "Lamps lit 0");
    let walker = n.one("lamplight.Lamplighter", "edit").0;
    n.ok(
        "component.patch",
        json!({"entity": walker, "component": "lamplight.Lamplighter", "world": "play", "patch": {"can": 60.0}, "actor": "agent:test"}),
    );
    n.step(3);
    assert_eq!(n.play(&oil, "UiProgress")["value"], 60.0);
}

#[test]
fn pausing_holds_the_clock_and_the_sliders_set_the_buses() {
    let mut n = Night::new("title");
    n.step(2);
    start_as_ada(&mut n);
    n.tap("P");
    n.step(2);
    assert_eq!(night_field(&mut n, "phase"), "paused");
    assert!(n.shown("Pause menu"));
    let clock = night_field(&mut n, "clock");
    n.step(30);
    assert_eq!(night_field(&mut n, "clock"), clock, "the clock holds");
    let music = n.named("Music volume").0;
    let report = n.ok("ui.inspect", json!({"world": "play", "entity": music}));
    let rect = report["nodes"][0]["rect"].clone();
    let (x, y) = (
        rect["x"].as_f64().unwrap() + 1.0,
        rect["y"].as_f64().unwrap() + rect["h"].as_f64().unwrap() / 2.0,
    );
    let t = n.tick;
    n.ok(
        "input.inject",
        json!({"events": [
            {"tick": t, "event": {"PointerMove": {"x": x, "y": y}}},
            {"tick": t, "event": {"MouseButton": {"button": "Left", "pressed": true}}},
            {"tick": t + 1, "event": {"MouseButton": {"button": "Left", "pressed": false}}},
        ], "actor": "agent:test"}),
    );
    n.step(4);
    let buses = n.ok("audio.buses", json!({}));
    let gain = buses["buses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == "Music")
        .unwrap()["gain_db"]
        .clone();
    assert_eq!(gain, -40.0, "{buses}");
    let resume = n.named("Resume").0;
    n.click(&resume);
    n.step(2);
    assert_eq!(night_field(&mut n, "phase"), "playing");
    assert!(!n.shown("Pause menu"));
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn the_end_screen_keeps_the_record_and_another_night_resets_the_town() {
    let mut n = Night::new("title");
    n.step(2);
    start_as_ada(&mut n);
    let night = n.one("lamplight.Night", "edit").0;
    n.ok(
        "component.patch",
        json!({"entity": night, "component": "lamplight.Night", "world": "play", "patch": {"length": 10.0}, "actor": "agent:test"}),
    );
    let (lamp, _) = n.lamp(0);
    n.ok(
        "component.patch",
        json!({"entity": lamp, "component": "lamplight.Lamp", "world": "play", "patch": {"lit": true, "oil": 60.0}, "actor": "agent:test"}),
    );
    n.step(620);
    assert_eq!(night_field(&mut n, "phase"), "won");
    assert!(n.shown("End screen"));
    assert!(!n.shown("HUD"));
    let result = n.named("Result").0;
    assert_eq!(
        n.play(&result, "UiText")["runs"][0]["text"],
        "Dawn came. The town held."
    );
    let rows: Vec<Value> = n
        .all("lamplight.RecordRow", "play")
        .into_iter()
        .map(|(_, c)| c)
        .collect();
    assert_eq!(rows.len(), 1);
    let row = rows[0]["UiText"]["runs"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(row.contains("Ada"), "{row}");
    let again = n.named("Another night").0;
    n.click(&again);
    n.step(2);
    assert_eq!(night_field(&mut n, "phase"), "title");
    assert!(n.shown("Title screen"));
    let start = n.named("Start").0;
    n.click(&start);
    n.step(3);
    let state = n.night();
    assert_eq!(state["phase"], "playing");
    assert_eq!(state["spirit"], 10);
    assert!(state["clock"].as_f64().unwrap() < 0.2, "{state}");
    assert_eq!(
        n.play(&lamp, "lamplight.Lamp")["lit"],
        false,
        "lamps go dark"
    );
    assert!(n.all("lamplight.Wisp", "play").is_empty(), "no wisps");
    let walker = n.one("lamplight.Lamplighter", "edit").0;
    assert_eq!(n.play(&walker, "lamplight.Lamplighter")["can"], 100.0);
    assert_eq!(
        n.all("lamplight.RecordRow", "play").len(),
        1,
        "the record stays"
    );
    assert_eq!(n.errors(), json!([]));
}

/// Every line of text stays inside its node's content box on all four screens (M3 3.0c, the
/// author's "text runs outside its boxes"): at 1280×720 (the canvas), 1512×982 (a 14" laptop's
/// logical size) and 800×600, each screen shown in the play world, every `ui.inspect` line (placed
/// from its node's content corner) must lie within the `content` size, allowing half a pixel.
#[test]
fn lamplight_text_stays_in_its_boxes() {
    let mut n = Night::new("title");
    n.step(1);
    let screens = ["Title screen", "HUD", "Pause menu", "End screen"];
    for screen in screens {
        let id = n.named(screen).0;
        n.ok(
            "component.patch",
            json!({"entity": id, "component": "UiNode", "world": "play",
                   "patch": {"visible": true}, "actor": world::ACTOR}),
        );
    }
    let mut outside = Vec::new();
    for (w, h) in [(1280, 720), (1512, 982), (800, 600)] {
        let report = n.ok(
            "ui.inspect",
            json!({"world": "play", "width": w, "height": h}),
        );
        for node in report["nodes"].as_array().unwrap() {
            let c = &node["content"];
            let f = |v: &Value, k: &str| v[k].as_f64().unwrap();
            let (cx, cy, cw, ch) = (f(c, "x"), f(c, "y"), f(c, "w"), f(c, "h"));
            for line in node["lines"].as_array().into_iter().flatten() {
                let (x, top) = (f(line, "x"), f(line, "top"));
                let (lw, lh) = (f(line, "width"), f(line, "height"));
                // Lines are placed from the content box's corner.
                let inside = x >= -0.5 && top >= -0.5 && x + lw <= cw + 0.5 && top + lh <= ch + 0.5;
                if !inside {
                    outside.push(format!(
                        "{w}x{h} {:?} {:?}: line ({x:.1},{top:.1},{lw:.1}×{lh:.1}) content ({cx:.1},{cy:.1},{cw:.1}×{ch:.1})",
                        node["entity"], line["text"]
                    ));
                }
            }
        }
    }
    assert!(
        outside.is_empty(),
        "{} lines outside:\n{}",
        outside.len(),
        outside.join("\n")
    );
}
