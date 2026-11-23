//! Lamplight's night in the engine (M2 phase 2.10c.1): the town scene with both modules loaded,
//! the rules as WASM and the fog as a native library, driven through the Engine API.
//!
//! Timing by hand, at 60 Hz. A signal raised on tick t is delivered on t + 1. The night
//! entity's `NightTick` is raised at the end of every tick, so the night counts as played from
//! tick 1:
//! - Walking: W held from tick 0 walks north (−Z) at 3.5 m/s from tick 1, so after ticks 0..=30
//!   the lamplighter has gone 30 × 3.5 / 60 = 1.75 m, from z = 12 to 10.25. They face −Z: the
//!   model faces +Z, so the walker is turned 180° about Y (rotation y = ±1). The body hears each
//!   `Stride` a tick later, so its `walk_amount` is 1 by tick 30. The camera sits at the walker's
//!   last broadcast position plus (0, 9, 11): two ticks behind, within 2 × 3.5 / 60 ≈ 0.12 m.
//! - Lighting: E pressed on tick 5 asks the body for the Light one-shot (tick 6). Its `lit` event
//!   is 0.5 s into the clip (about tick 36). The body's `Reached`, the walker's `Kindle`, the
//!   lamp's `Lit` and the walker's can each take one more tick, so by tick 70 the lamp is lit with
//!   about 59.5 s of oil, the can holds 100 − 20 = 80, and the night counts 1 lamp lit. A lamp
//!   3 m away is beyond the 2 m reach and stays dark.
//! - Burning: a wisp created at a lit lamp's foot is reported inside it from tick 1 and scorched
//!   from tick 2 (0.5 health per tick of its 40), so it pops on its 80th burn tick, tick 81. Burn
//!   numbers rise on burn ticks 30 and 60 ("15" each) and at the pop ("10"); each lasts 60 ticks,
//!   so all three are up on tick 89, and the night counts 1 wisp burned.
//! - The square: a wisp created 8 m south of the square, aimed 4.5 m south of it (wisps aim at a
//!   ring 4–6 m out; the fountain fills the middle), walks 3.5 m at 1.6 m/s and arrives in about
//!   2.2 s; Town Spirit drops from 10 to 9. With 1 spirit left, the same wisp loses the night.
//! - Dawn: a 10 s night is won by tick 605 (600 ticks of clock, plus a tick of lag).
//! - Lamps burn out: a lamp lit with 0.05 s of oil is dark within 4 ticks.
//! - Fog: the first wave (6 wisps at density 1) comes when the fog's 5 s timer runs out, about
//!   tick 301, each aimed 4–6 m from the square. On the title screen no wave comes.

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";

struct Night {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
}

impl Night {
    /// The town on a fresh copy of the example, with both modules built, the night `phase`.
    fn new(phase: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        concordia_tests::copy_example("m2-lamplight", root).unwrap();
        std::fs::create_dir_all(root.join("build")).unwrap();
        let rules =
            concordia_tests::wasm_fixture_path("lamplight_rules").unwrap_or_else(|e| panic!("{e}"));
        std::fs::copy(rules, root.join("build/lamplight_rules.wasm")).unwrap();
        let fog =
            concordia_tests::native_fixture_path("lamplight_fog").unwrap_or_else(|e| panic!("{e}"));
        let fog_file = format!("build/lamplight_fog.{}", std::env::consts::DLL_EXTENSION);
        std::fs::copy(fog, root.join(fog_file)).unwrap();
        let project = Project::load(root).unwrap();
        let mut config = EngineConfig::new(root);
        config.deterministic_ids = Some(5);
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut night = Self {
            _dir: dir,
            engine,
            session,
        };
        night.ok("scene.open", json!({"path": "scenes/town.scene.ron"}));
        let id = night.one("lamplight.Night", "edit").0;
        night.patch(&id, "lamplight.Night", json!({"phase": phase}));
        night
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    /// Every entity with `component` in `world`: (id, its components).
    fn all(&mut self, component: &str, world: &str) -> Vec<(String, Value)> {
        let rows = self.ok(
            "query.run",
            json!({"with": [component], "world": world, "limit": 1000}),
        )["rows"]
            .as_array()
            .unwrap()
            .clone();
        rows.iter()
            .map(|r| {
                (
                    r["id"].as_str().unwrap().to_owned(),
                    r["components"].clone(),
                )
            })
            .collect()
    }

    fn one(&mut self, component: &str, world: &str) -> (String, Value) {
        let mut all = self.all(component, world);
        assert_eq!(all.len(), 1, "one {component} in {world}");
        all.remove(0)
    }

    /// The entity named `name` in the edit world.
    fn named(&mut self, name: &str) -> (String, Value) {
        let rows = self.all("Name", "edit");
        rows.into_iter()
            .find(|(_, c)| c["Name"] == name)
            .unwrap_or_else(|| panic!("no entity named {name}"))
    }

    fn patch(&mut self, entity: &str, component: &str, patch: Value) {
        self.ok(
            "component.patch",
            json!({"entity": entity, "component": component, "patch": patch, "actor": ACTOR}),
        );
    }

    fn play(&mut self, entity: &str, component: &str) -> Value {
        self.ok(
            "component.get",
            json!({"entity": entity, "component": component, "world": "play"}),
        )["value"]
            .clone()
    }

    fn step(&mut self, ticks: u32) -> String {
        self.ok("sim.step", json!({"ticks": ticks}))["world_hash"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn keys(&mut self, events: &[(u64, &str, bool)]) {
        let events: Vec<Value> = events
            .iter()
            .map(|(tick, key, pressed)| json!({"tick": tick, "event": {"Key": {"key": key, "pressed": pressed}}}))
            .collect();
        self.ok("input.inject", json!({"events": events, "actor": ACTOR}));
    }

    /// Puts the lamplighter at `at` in the edit world.
    fn place_lamplighter(&mut self, at: [f64; 3]) -> String {
        let walker = self.one("lamplight.Lamplighter", "edit").0;
        self.patch(&walker, "Transform", json!({"translation": at}));
        walker
    }

    /// A lamp's light (`Lamp <index>`) and its ground position.
    fn lamp(&mut self, index: u32) -> (String, [f64; 3]) {
        let (id, components) = self.named(&format!("Lamp {index}"));
        let t = &components["Transform"]["translation"];
        (id, [t[0].as_f64().unwrap(), 0.0, t[2].as_f64().unwrap()])
    }

    fn light_lamp(&mut self, lamp: &str, oil: f64) {
        self.patch(lamp, "lamplight.Lamp", json!({"lit": true, "oil": oil}));
        self.patch(lamp, "PointLight", json!({"intensity_lumens": 2400.0}));
    }

    fn wisp_at(&mut self, at: [f64; 3]) -> String {
        self.ok(
            "entity.create",
            json!({"template": "wisp", "translation": at, "actor": ACTOR}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// A wisp at `at` drifting to the ring point 4.5 m south of the square.
    fn wisp_for_the_square(&mut self, at: [f64; 3]) -> String {
        let wisp = self.wisp_at(at);
        self.patch(&wisp, "fog.Drifter", json!({"target": [0.0, 0.0, 4.5]}));
        wisp
    }

    fn night(&mut self) -> Value {
        self.one("lamplight.Night", "play").1["lamplight.Night"].clone()
    }

    fn errors(&mut self) -> Value {
        self.ok("log.tail", json!({"level": "error"}))["records"].clone()
    }
}

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

#[test]
fn walking_moves_the_lamplighter_north_and_drives_the_walk_blend() {
    let mut n = Night::new("playing");
    let walker = n.one("lamplight.Lamplighter", "edit").0;
    let body = n.one("lamplight.Body", "edit").0;
    n.keys(&[(0, "W", true)]);
    n.step(31);
    let t = n.play(&walker, "Transform");
    let z = t["translation"][2].as_f64().unwrap();
    assert!(near(z, 10.25, 0.02), "z = {z}: {t}");
    assert!(
        near(t["rotation"][1].as_f64().unwrap().abs(), 1.0, 1e-3),
        "faces −Z: {t}"
    );
    let graph = n.play(&body, "AnimGraphPlayer");
    assert_eq!(graph["floats"]["walk_amount"], 1.0, "{graph}");
    let camera = n.one("lamplight.Chase", "play").0;
    let c = n.play(&camera, "Transform")["translation"].clone();
    let x = c[0].as_f64().unwrap();
    let (cy, cz) = (c[1].as_f64().unwrap(), c[2].as_f64().unwrap());
    assert!(
        near(x, 0.0, 0.01) && near(cy, 9.0, 0.01) && near(cz, 10.25 + 11.0, 0.15),
        "{c}"
    );
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn lighting_a_lamp_in_reach_takes_twenty_oil() {
    let mut n = Night::new("playing");
    let (lamp, at) = n.lamp(0);
    // 1.2 m from the post, toward the square.
    let r = (at[0] * at[0] + at[2] * at[2]).sqrt();
    let walker = n.place_lamplighter([at[0] * (r - 1.2) / r, 0.0, at[2] * (r - 1.2) / r]);
    n.keys(&[(5, "E", true), (6, "E", false)]);
    n.step(70);
    let state = n.play(&lamp, "lamplight.Lamp");
    assert_eq!(state["lit"], true, "{state}");
    let oil = state["oil"].as_f64().unwrap();
    assert!(oil > 59.0 && oil <= 60.0, "{state}");
    assert_eq!(n.play(&lamp, "PointLight")["intensity_lumens"], 2400.0);
    let lighter = n.play(&walker, "lamplight.Lamplighter");
    assert_eq!(lighter["can"], 80.0, "{lighter}");
    assert_eq!(lighter["lit"], 1, "{lighter}");
    assert_eq!(n.night()["lamps_lit"], 1);
    let others = n.all("lamplight.Lamp", "play");
    let lit = others
        .iter()
        .filter(|(_, c)| c["lamplight.Lamp"]["lit"] == true)
        .count();
    assert_eq!(lit, 1, "only the lamp in reach");
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn a_lamp_out_of_reach_stays_dark() {
    let mut n = Night::new("playing");
    let (lamp, at) = n.lamp(0);
    let r = (at[0] * at[0] + at[2] * at[2]).sqrt();
    let walker = n.place_lamplighter([at[0] * (r - 3.0) / r, 0.0, at[2] * (r - 3.0) / r]);
    n.keys(&[(5, "E", true), (6, "E", false)]);
    n.step(70);
    assert_eq!(n.play(&lamp, "lamplight.Lamp")["lit"], false);
    assert_eq!(n.play(&walker, "lamplight.Lamplighter")["can"], 100.0);
    assert_eq!(n.night()["lamps_lit"], 0);
}

#[test]
fn a_wisp_in_lamplight_burns_and_pops() {
    let mut n = Night::new("playing");
    let (lamp, at) = n.lamp(0);
    n.light_lamp(&lamp, 60.0);
    let wisp = n.wisp_at(at);
    n.step(81); // ticks 0..=80
    let left = n.play(&wisp, "lamplight.Wisp");
    assert_eq!(left["health"], 0.5, "one burn tick left: {left}");
    n.step(9); // ticks 81..=89
    assert!(
        n.all("lamplight.Wisp", "play")
            .iter()
            .all(|(id, _)| *id != wisp),
        "popped"
    );
    assert_eq!(n.night()["wisps_burned"], 1);
    let mut numbers: Vec<String> = n
        .all("lamplight.Number", "play")
        .iter()
        .map(|(_, c)| c["UiText"]["runs"][0]["text"].as_str().unwrap().to_owned())
        .collect();
    numbers.sort();
    assert_eq!(numbers, ["10", "15", "15"]);
    let inside = n.play(&lamp, "lamplight.Lamp")["inside"].clone();
    assert_eq!(inside, json!([]), "the lamp forgot the popped wisp");
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn a_wisp_reaching_the_square_costs_spirit() {
    let mut n = Night::new("playing");
    let wisp = n.wisp_for_the_square([0.0, 0.0, 8.0]);
    let mut ticks = 0;
    loop {
        n.step(10);
        ticks += 10;
        if n.night()["spirit"] != 10 || ticks >= 300 {
            break;
        }
    }
    assert_eq!(n.night()["spirit"], 9, "after {ticks} ticks");
    n.step(2);
    assert!(
        n.all("lamplight.Wisp", "play")
            .iter()
            .all(|(id, _)| *id != wisp),
        "gone"
    );
    assert_eq!(n.night()["phase"], "playing");
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn the_night_is_lost_when_spirit_runs_out() {
    let mut n = Night::new("playing");
    let night = n.one("lamplight.Night", "edit").0;
    n.patch(&night, "lamplight.Night", json!({"spirit": 1}));
    n.wisp_for_the_square([0.0, 0.0, 8.0]);
    n.step(300);
    let state = n.night();
    assert_eq!(state["phase"], "lost", "{state}");
    assert_eq!(state["spirit"], 0);
}

#[test]
fn a_short_night_is_won_at_dawn() {
    let mut n = Night::new("playing");
    let night = n.one("lamplight.Night", "edit").0;
    let fog = n.one("fog.Fog", "edit").0;
    n.patch(&night, "lamplight.Night", json!({"length": 10.0}));
    n.patch(&fog, "fog.Fog", json!({"next": 100.0}));
    n.step(590);
    assert_eq!(n.night()["phase"], "playing");
    n.step(15);
    let state = n.night();
    assert_eq!(state["phase"], "won", "{state}");
    assert!(state["clock"].as_f64().unwrap() >= 10.0);
}

#[test]
fn a_lamp_burns_out_when_its_oil_is_spent() {
    let mut n = Night::new("playing");
    let (lamp, _) = n.lamp(3);
    n.light_lamp(&lamp, 0.05);
    n.step(5);
    assert_eq!(n.play(&lamp, "lamplight.Lamp")["lit"], false);
    assert_eq!(n.play(&lamp, "PointLight")["intensity_lumens"], 0.0);
}

#[test]
fn the_fog_sends_a_wave_while_the_night_is_played() {
    let mut n = Night::new("playing");
    n.step(295);
    assert_eq!(
        n.all("lamplight.Wisp", "play").len(),
        0,
        "no wave before 5 s"
    );
    n.step(15);
    let wisps = n.all("NavAgent", "play");
    let wisps: Vec<_> = wisps
        .into_iter()
        .filter(|(_, c)| c.get("lamplight.Wisp").is_some())
        .collect();
    assert_eq!(wisps.len(), 6);
    for (_, c) in &wisps {
        let target = &c["NavAgent"]["target"];
        let (x, z) = (target[0].as_f64().unwrap(), target[2].as_f64().unwrap());
        let r = (x * x + z * z).sqrt();
        assert!(
            (4.0 - 1e-4..=6.0 + 1e-4).contains(&r),
            "aimed at the ring: {target}"
        );
        let at = &c["Transform"]["translation"];
        assert!(
            near(at[2].as_f64().unwrap(), 30.0, 2.5),
            "rose at the harbour: {at}"
        );
    }
    let mut title = Night::new("title");
    title.step(310);
    assert_eq!(
        title.all("lamplight.Wisp", "play").len(),
        0,
        "no fog on the title screen"
    );
}

#[test]
fn the_same_night_gives_the_same_hash() {
    let run = || {
        let mut n = Night::new("playing");
        n.keys(&[
            (0, "W", true),
            (40, "W", false),
            (50, "E", true),
            (51, "E", false),
        ]);
        n.step(400)
    };
    assert_eq!(run(), run());
}

#[test]
fn an_empty_can_lights_nothing() {
    // Written after the code (the light checks were already there): with 10 oil, less than the
    // 20 a lamp takes, pressing Light beside a lamp starts no reach and lights nothing.
    let mut n = Night::new("playing");
    let (lamp, at) = n.lamp(0);
    let r = (at[0] * at[0] + at[2] * at[2]).sqrt();
    let walker = n.place_lamplighter([at[0] * (r - 1.2) / r, 0.0, at[2] * (r - 1.2) / r]);
    n.patch(&walker, "lamplight.Lamplighter", json!({"can": 10.0}));
    n.keys(&[(5, "E", true), (6, "E", false)]);
    n.step(70);
    assert_eq!(n.play(&lamp, "lamplight.Lamp")["lit"], false);
    assert_eq!(n.play(&walker, "lamplight.Lamplighter")["can"], 10.0);
}
