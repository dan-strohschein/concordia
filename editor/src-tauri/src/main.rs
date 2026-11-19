//! `concordia-editor [--project <dir>] [--smoke-test]`: the editor window over an in-process engine.
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
}

#[cfg(target_os = "macos")]
#[tauri::command]
fn slot_rect(
    rect: concordia_editor::native::SlotRect,
    window: tauri::WebviewWindow,
    shell: tauri::State<'_, Arc<Shell>>,
) -> Result<(), String> {
    rect.validate()?;
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
    let host = match EngineHost::start(&project) {
        Ok(host) => Arc::new(host),
        Err(e) => {
            eprintln!("concordia-editor: {e}");
            std::process::exit(2);
        }
    };
    let url = serde_json::to_string(&host.url()).unwrap_or_default();
    let init = format!("window.__CONCORDIA_ENGINE__ = {url}; window.__CONCORDIA_NATIVE__ = true;");
    let shell = Arc::new(Shell {
        slot_seen: AtomicBool::new(false),
    });
    let (host_setup, shell_setup) = (host.clone(), shell.clone());
    let result = tauri::Builder::default()
        .manage(shell)
        .invoke_handler(tauri::generate_handler![slot_rect])
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
        return Err(format!("only {presented} frames presented"));
    }
    let capture = host.call(
        "capture.screenshot",
        serde_json::json!({"view": view, "width": 64, "height": 36}),
    )?;
    let colors = distinct_colors(capture["image"]["data_base64"].as_str().unwrap_or(""))?;
    if colors < 8 {
        return Err(format!("the capture is nearly uniform ({colors} colors)"));
    }
    Ok(format!("presented={presented} capture_colors={colors}"))
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
