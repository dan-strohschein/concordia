//! `concordia-editor [--project <dir>] [--smoke-test] [--bench <out.json> [--bench-entities N]
//! [--bench-frames N]]`: the editor window over an in-process engine.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use concordia_editor::EngineHost;
use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Set once the page has reported its viewport slot (the UI is up).
struct Shell {
    slot_seen: AtomicBool,
    /// The latest slot, in physical pixels (width, height).
    slot_px: std::sync::Mutex<(u32, u32)>,
    /// The latest slot height in points (for panning).
    slot_height: std::sync::Mutex<f64>,
    /// The latest slot size in points (for gizmo drags).
    slot_size: std::sync::Mutex<(f64, f64)>,
    /// Where `--bench` writes its report.
    bench_out: Option<PathBuf>,
    bench_entities: usize,
    /// The display's refresh rate (set once the window is up).
    display_hz: std::sync::Mutex<Option<f64>>,
}

/// The display refresh rate for the bench (WebKit caps requestAnimationFrame at 60 Hz).
#[tauri::command]
fn bench_display_hz(shell: tauri::State<'_, Arc<Shell>>) -> Option<f64> {
    shell.display_hz.lock().ok().and_then(|hz| *hz)
}

/// Presents an orbit straight from Rust (no web layer): the bench's baseline.
#[tauri::command]
async fn bench_native_orbit(
    frames: usize,
    host: tauri::State<'_, Arc<EngineHost>>,
    shell: tauri::State<'_, Arc<Shell>>,
) -> Result<(), String> {
    if !(1..=10_000).contains(&frames) {
        return Err(format!("frames must be 1..=10000, got {frames}"));
    }
    let (host, (width, height)) = (
        host.inner().clone(),
        *shell.slot_px.lock().map_err(|e| e.to_string())?,
    );
    tauri::async_runtime::spawn_blocking(move || {
        let session = host.session()?;
        for (i, view) in concordia_editor::bench::orbit_views(frames).enumerate() {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            session.call(
                "viewport.present",
                serde_json::json!({"view": view, "width": width, "height": height, "seq": i + 1, "input_time_ms": now.as_secs_f64() * 1000.0}),
            )?;
            // Like a native UI: the next camera goes once the render thread has taken this one.
            concordia_editor::bench::wait_until_submitted(&session, i as u64 + 1)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Native input's `viewport.stats` tags start here, apart from the page's.
const NATIVE_SEQ_BASE: u64 = 1 << 32;

/// Applies viewport gestures in order: camera moves (merging any that queued up) go straight to
/// the engine camera and the page gets each new view; gizmo steps edit as `human:local`.
#[cfg(target_os = "macos")]
fn navigation_worker(
    host: &EngineHost,
    shell: &Shell,
    app: &tauri::AppHandle,
    queue: &std::sync::mpsc::Receiver<(concordia_editor::input::Action, f64)>,
) {
    use concordia_editor::input::Action;
    let Ok(human) = host.session_as("human:local") else {
        return eprintln!("concordia-editor: native input has no engine session");
    };
    let control = host
        .runtime()
        .block_on(host.handle().run(concordia_render::viewport_control));
    let control = match control {
        Ok(Ok(control)) => control,
        Ok(Err(e)) | Err(e) => return eprintln!("concordia-editor: native input: {}", e.message),
    };
    let mut seq = NATIVE_SEQ_BASE;
    let mut pending: Option<(Action, f64)> = None;
    loop {
        let next = match pending.take() {
            Some(next) => next,
            None => match queue.recv() {
                Ok(next) => next,
                Err(_) => return,
            },
        };
        let (mut action, mut earliest) = next;
        // Merge what queued up behind it, as long as it is the same kind of step.
        while let Ok((more, at)) = queue.try_recv() {
            match merge(&action, &more) {
                Some(merged) => {
                    action = merged;
                    earliest = earliest.min(at);
                }
                None => {
                    pending = Some((more, at));
                    break;
                }
            }
        }
        seq += 1;
        let input_time_ms = concordia_editor::native::uptime_to_unix_ms(earliest);
        match action {
            Action::Nav(nav) => navigate(&control, shell, app, nav, seq, input_time_ms),
            Action::Gizmo(step) => gizmo(&human, shell, step, seq, input_time_ms),
        }
    }
}

/// Two queued steps as one, if they combine (camera moves add; gizmo moves keep the latest).
#[cfg(target_os = "macos")]
fn merge(
    a: &concordia_editor::input::Action,
    b: &concordia_editor::input::Action,
) -> Option<concordia_editor::input::Action> {
    use concordia_editor::input::{Action, GizmoStep};
    match (a, b) {
        (Action::Nav(a), Action::Nav(b)) => Some(Action::Nav(a.merge(*b))),
        (Action::Gizmo(GizmoStep::Move { .. }), Action::Gizmo(GizmoStep::Move { .. })) => {
            Some(b.clone())
        }
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn navigate(
    control: &concordia_render::ViewportControl,
    shell: &Shell,
    app: &tauri::AppHandle,
    nav: concordia_editor::input::Nav,
    seq: u64,
    input_time_ms: f64,
) {
    use tauri::Emitter;
    let height = shell.slot_height.lock().map_or(360.0, |h| *h).max(1.0) as f32;
    let params = concordia_render::ViewportNavigateParams {
        orbit: nav.orbit,
        pan: nav.pan,
        zoom: nav.zoom,
        height: Some(height),
        seq: Some(seq),
        input_time_ms: Some(input_time_ms),
        ..Default::default()
    };
    // Straight to the render thread: a busy engine thread never delays the camera.
    match control.navigate(&params) {
        Ok(view) => {
            let _ = app.emit("viewport-view", view);
        }
        Err(e) => eprintln!("concordia-editor: navigation: {}", e.message),
    }
}

#[cfg(target_os = "macos")]
fn gizmo(
    human: &concordia_editor::ShellSession<'_>,
    shell: &Shell,
    step: concordia_editor::input::GizmoStep,
    seq: u64,
    input_time_ms: f64,
) {
    use concordia_editor::input::GizmoStep;
    let (width, height) = shell.slot_size.lock().map_or((640.0, 360.0), |s| *s);
    let params = match step {
        GizmoStep::Begin { entity, axis, x, y } => {
            serde_json::json!({"action": "begin", "entity": entity, "axis": axis, "x": x, "y": y})
        }
        GizmoStep::Move { x, y } => serde_json::json!({
            "action": "move", "x": x, "y": y, "width": width, "height": height,
            "seq": seq, "input_time_ms": input_time_ms,
        }),
        GizmoStep::End => serde_json::json!({"action": "end"}),
    };
    if let Err(e) = human.call("viewport.gizmo", params) {
        eprintln!("concordia-editor: gizmo: {e}");
    }
}

/// The page's gizmo handles, for native hit-testing (viewport pixels).
#[cfg(target_os = "macos")]
#[tauri::command]
fn gizmo_handles(
    entity: String,
    handles: Vec<concordia_editor::input::Handle>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if handles
        .iter()
        .any(|h| !(h.x.is_finite() && h.y.is_finite() && h.radius > 0.0))
    {
        return Err("gizmo handles need finite positions and a positive radius".into());
    }
    app.run_on_main_thread(move || concordia_editor::native::set_gizmo_handles(&entity, handles))
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn gizmo_handles() -> Result<(), String> {
    Err("native viewport input is macOS-only in M1".into())
}

/// Drags across the viewport with synthetic native mouse events, one step per display refresh
/// (the native-input bench).
#[cfg(target_os = "macos")]
#[tauri::command]
async fn bench_native_drag(
    frames: usize,
    from: Option<[f64; 2]>,
    alt: Option<bool>,
    shell: tauri::State<'_, Arc<Shell>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use concordia_editor::native::{DragStep, post_drag};
    if !(1..=10_000).contains(&frames) {
        return Err(format!("frames must be 1..=10000, got {frames}"));
    }
    let hz = shell
        .display_hz
        .lock()
        .ok()
        .and_then(|h| *h)
        .unwrap_or(60.0);
    tauri::async_runtime::spawn_blocking(move || {
        let alt = alt.unwrap_or(true);
        let post = |step| {
            app.run_on_main_thread(move || post_drag(step, from, alt))
                .map_err(|e| e.to_string())
        };
        post(DragStep::Down)?;
        for i in 1..=frames {
            std::thread::sleep(Duration::from_secs_f64(1.0 / hz));
            post(DragStep::Move(3.0 * i as f64))?;
        }
        post(DragStep::Up(3.0 * frames as f64))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn bench_native_drag() -> Result<(), String> {
    Err("native viewport input is macOS-only in M1".into())
}

/// Prints the bench's current stage (progress for whoever runs it).
#[tauri::command]
fn bench_progress(stage: String) {
    println!("BENCH stage {stage}");
}

/// Writes the bench report (with the run's settings and process memory) and exits.
#[tauri::command]
fn bench_report(
    report: String,
    shell: tauri::State<'_, Arc<Shell>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let out = shell.bench_out.as_ref().ok_or("not in bench mode")?;
    let ui: serde_json::Value = serde_json::from_str(&report).map_err(|e| e.to_string())?;
    let failed = ui.get("error").is_some();
    let full = serde_json::json!({
        "version": 1,
        "entities_added": shell.bench_entities,
        "editor_rss_mb": concordia_editor::bench::rss_mb(std::process::id()),
        "ui": ui,
    });
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        out,
        serde_json::to_string_pretty(&full).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "BENCH {} {}",
        if failed { "failed" } else { "ok" },
        out.display()
    );
    app.exit(i32::from(failed));
    Ok(())
}

#[cfg(target_os = "macos")]
#[tauri::command]
fn slot_rect(
    rect: concordia_editor::native::SlotRect,
    window: tauri::WebviewWindow,
    shell: tauri::State<'_, Arc<Shell>>,
) -> Result<(), String> {
    rect.validate()?;
    if let Ok(mut h) = shell.slot_height.lock() {
        *h = rect.height;
    }
    if let Ok(mut size) = shell.slot_size.lock() {
        *size = (rect.width, rect.height);
    }
    if let Ok(mut px) = shell.slot_px.lock() {
        *px = (
            (rect.width * rect.scale).round() as u32,
            (rect.height * rect.scale).round() as u32,
        );
    }
    shell.slot_seen.store(true, Ordering::SeqCst);
    window
        .run_on_main_thread(move || concordia_editor::native::place(rect))
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn slot_rect() -> Result<(), String> {
    Err("the native viewport is macOS-only in M1".into())
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let project = PathBuf::from(arg(&args, "--project").unwrap_or_else(|| ".".into()));
    let smoke = args.iter().any(|a| a == "--smoke-test");
    let bench_out = arg(&args, "--bench").map(PathBuf::from);
    let number =
        |name: &str, default: usize| arg(&args, name).map_or(Ok(default), |v| v.parse::<usize>());
    let (Ok(bench_entities), Ok(bench_frames)) =
        (number("--bench-entities", 0), number("--bench-frames", 240))
    else {
        eprintln!("concordia-editor: --bench-entities and --bench-frames take a number");
        std::process::exit(2);
    };
    let log = concordia_core::log::init(&concordia_core::log::LogConfig::from_env()).ok();
    let host = match EngineHost::start_with_log(&project, log.as_ref().map(|l| l.ring().clone())) {
        Ok(host) => Arc::new(host),
        Err(e) => {
            eprintln!("concordia-editor: {e}");
            std::process::exit(2);
        }
    };
    if bench_entities > 0
        && let Err(e) = concordia_editor::bench::add_entities(&host, bench_entities)
    {
        eprintln!("concordia-editor: bench scene: {e}");
        std::process::exit(2);
    }
    let url = serde_json::to_string(&host.url()).unwrap_or_default();
    let mut init =
        format!("window.__CONCORDIA_ENGINE__ = {url}; window.__CONCORDIA_NATIVE__ = true;");
    if bench_out.is_some() {
        init.push_str(&format!(
            " window.__CONCORDIA_BENCH__ = {{ frames: {bench_frames} }};"
        ));
    }
    let bench = bench_out.is_some();
    let shell = Arc::new(Shell {
        slot_seen: AtomicBool::new(false),
        slot_px: std::sync::Mutex::new((640, 360)),
        slot_height: std::sync::Mutex::new(360.0),
        slot_size: std::sync::Mutex::new((640.0, 360.0)),
        bench_out,
        bench_entities,
        display_hz: std::sync::Mutex::new(None),
    });
    let (host_setup, shell_setup) = (host.clone(), shell.clone());
    let result = tauri::Builder::default()
        .manage(shell)
        .manage(host.clone())
        .invoke_handler(tauri::generate_handler![
            slot_rect,
            bench_native_orbit,
            bench_progress,
            bench_native_drag,
            gizmo_handles,
            bench_display_hz,
            bench_report
        ])
        .setup(move |app| {
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("Concordia")
                    .inner_size(1400.0, 900.0)
                    .transparent(true)
                    .initialization_script(&init)
                    .build()?;
            #[cfg(target_os = "macos")]
            {
                let target = concordia_editor::native::install(&window)?;
                let (moves, queue) = std::sync::mpsc::channel();
                concordia_editor::native::install_input(move |action, at| {
                    let _ = moves.send((action, at));
                });
                let (host, shell, app) = (
                    host_setup.clone(),
                    shell_setup.clone(),
                    app.handle().clone(),
                );
                std::thread::Builder::new()
                    .name("concordia-navigation".into())
                    .spawn(move || navigation_worker(&host, &shell, &app, &queue))?;
                if let Ok(mut hz) = shell_setup.display_hz.lock() {
                    *hz = concordia_editor::native::display_hz();
                }
                let attached =
                    host_setup
                        .runtime()
                        .block_on(host_setup.handle().run(move |engine| {
                            // SAFETY: the layer lives in the native thread-local for the life of the app.
                            unsafe { concordia_render::attach_surface(engine, target) }
                                .map_err(|e| e.message)
                        }));
                attached.map_err(|e| e.message)??;
            }
            if smoke {
                let (host, shell, handle) = (
                    host_setup.clone(),
                    shell_setup.clone(),
                    app.handle().clone(),
                );
                std::thread::spawn(move || {
                    let code = match smoke_test(&host, &shell) {
                        Ok(report) => {
                            println!("SMOKE ok {report}");
                            0
                        }
                        Err(e) => {
                            eprintln!("SMOKE failed: {e}");
                            1
                        }
                    };
                    handle.exit(code);
                });
            }
            #[cfg(target_os = "macos")]
            if std::env::var_os("CONCORDIA_EDITOR_PRINT_WINDOW").is_some() {
                println!(
                    "EDITOR_WINDOW {}",
                    concordia_editor::native::window_number().unwrap_or(-1)
                );
            }
            if bench {
                // WebKit pauses requestAnimationFrame in occluded windows; keep the bench visible.
                let _ = window.set_always_on_top(true);
            }
            let _ = window.set_focus();
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("concordia-editor: {e}");
        std::process::exit(1);
    }
}

/// Waits for the UI, presents 10 frames into the native viewport, and checks a capture of the
/// same view is not blank.
fn smoke_test(host: &EngineHost, shell: &Shell) -> Result<String, String> {
    let start = Instant::now();
    while !shell.slot_seen.load(Ordering::SeqCst) {
        if start.elapsed() > Duration::from_secs(30) {
            return Err("the editor UI never reported its viewport".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let view = serde_json::json!({"eye": [7.0, 6.0, 10.0], "target": [0.0, 1.0, 0.0], "projection": {"Perspective": {"fov_y": 0.9, "near": 0.1, "far": 200.0}}});
    let mut presented = 0;
    for _ in 0..30 {
        if presented == 10 {
            break;
        }
        let r = host.call(
            "viewport.present",
            serde_json::json!({"view": view, "width": 640, "height": 360}),
        )?;
        presented += usize::from(r["presented"] == true);
    }
    if presented < 10 {
        return Err(format!("only {presented} frames queued"));
    }
    let start = Instant::now();
    let rendered = loop {
        let stats = host.call("viewport.stats", serde_json::json!({}))?;
        let rendered = stats["frames"].as_array().map_or(0, |f| {
            f.iter().filter(|f| !f["submitted_ms"].is_null()).count()
        });
        if rendered > 0 || start.elapsed() > Duration::from_secs(10) {
            break rendered;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    if rendered == 0 {
        return Err("no viewport frame was rendered".into());
    }
    let capture = host.call(
        "capture.screenshot",
        serde_json::json!({"view": view, "width": 64, "height": 36}),
    )?;
    let colors = distinct_colors(capture["image"]["data_base64"].as_str().unwrap_or(""))?;
    if colors < 8 {
        return Err(format!("the capture is nearly uniform ({colors} colors)"));
    }
    Ok(format!(
        "queued={presented} rendered={rendered} capture_colors={colors}"
    ))
}

fn distinct_colors(png_base64: &str) -> Result<usize, String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png_base64)
        .map_err(|e| e.to_string())?;
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .map_err(|e| e.to_string())?;
    let mut rgba = vec![0; reader.output_buffer_size().unwrap_or(0)];
    reader.next_frame(&mut rgba).map_err(|e| e.to_string())?;
    let mut colors: Vec<&[u8; 4]> = rgba.as_chunks::<4>().0.iter().collect();
    colors.sort_unstable();
    colors.dedup();
    Ok(colors.len())
}
