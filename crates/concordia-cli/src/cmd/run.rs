//! `concordia run`: open the project's window on the main thread and play its main scene in it
//! (window input into the play world, drawn from the game camera; M2 2.5b.2b). `--edit` draws the
//! edit world from the editor camera instead; `--bench-latency` measures input-to-photon of the
//! engine alone (no editor, no webview) and implies `--edit`.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use concordia_api::{Engine, EngineHandle};
use concordia_app::Project;
use concordia_core::id::SessionId;
use concordia_platform::{Flow, NativeView, RunOptions, WindowApp, WindowEvent, run_window};
use serde_json::{Value, json};

use super::api_failure;
use crate::Failure;

/// `concordia run` arguments: plays the main scene in a window (window input reaches the play
/// world; it is drawn from the highest-priority camera).
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Project root.
    #[arg(long, default_value = ".")]
    project: PathBuf,
    /// Close the window after this many milliseconds (smoke tests).
    #[arg(long)]
    exit_after_ms: Option<u64>,
    /// Measure input-to-photon latency of the engine's own window and write the frames here.
    #[arg(long)]
    bench_latency: Option<PathBuf>,
    /// Camera moves the latency bench makes (one per display refresh).
    #[arg(long, default_value_t = 240, requires = "bench_latency")]
    bench_frames: u32,
    /// The display refresh rate the bench paces its input to.
    #[arg(long, default_value_t = 120.0, requires = "bench_latency")]
    bench_hz: f64,
    /// Open borderless fullscreen (overrides the manifest).
    #[arg(long)]
    fullscreen: bool,
    /// Show the edit world from the editor camera instead of playing.
    #[arg(long)]
    edit: bool,
}

pub(crate) fn run(args: &Args) -> Result<(), Failure> {
    let project = Project::load(&args.project).map_err(|e| Failure::usage(e.to_string()))?;
    let config = project
        .engine_config()
        .map_err(|e| Failure::usage(e.to_string()))?;
    if !(1.0..=1000.0).contains(&args.bench_hz) || args.bench_frames == 0 {
        return Err(Failure::usage(
            "--bench-hz must be 1..=1000 and --bench-frames > 0",
        ));
    }
    let handle = Engine::new(config).map_err(|e| api_failure(&e))?.spawn();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Failure::usage(format!("cannot start the async runtime: {e}")))?;
    let session = runtime
        .block_on(handle.open_session())
        .map_err(|e| api_failure(&e))?;
    if let Some(main) = &project.manifest.scene.main {
        runtime
            .block_on(handle.call(session, "scene.open", json!({ "path": main })))
            .map_err(|e| api_failure(&e))?;
    }
    let bench = args.bench_latency.clone().map(|out| Bench {
        out,
        frames: args.bench_frames,
        hz: args.bench_hz,
    });
    let exit_after = args.exit_after_ms.map(Duration::from_millis).or_else(|| {
        bench
            .as_ref()
            .map(|b| Duration::from_secs_f64(f64::from(b.frames) / b.hz + 6.0))
    });
    let player = (!args.edit && bench.is_none()).then(|| concordia_app::Player::new(false));
    let mut viewer = Viewer {
        handle,
        session,
        runtime,
        layer: None,
        size: (0, 0),
        bench,
        failure: None,
        player,
    };
    let mut window = project.manifest.window.clone();
    window.fullscreen |= args.fullscreen;
    run_window(&window, RunOptions { exit_after }, &mut viewer)
        .map_err(|e| Failure::usage(e.to_string()))?;
    viewer.failure.map_or(Ok(()), Err)
}

/// The latency bench's settings.
#[derive(Debug, Clone)]
struct Bench {
    out: PathBuf,
    frames: u32,
    hz: f64,
}

/// Draws the scene into the window (when the project renders) and runs the latency bench.
struct Viewer {
    handle: EngineHandle,
    session: SessionId,
    runtime: tokio::runtime::Runtime,
    #[cfg(target_os = "macos")]
    layer: Option<concordia_rhi_metal::ViewLayer>,
    #[cfg(not(target_os = "macos"))]
    layer: Option<()>,
    size: (u32, u32),
    bench: Option<Bench>,
    failure: Option<Failure>,
    /// Plays the scene (absent with `--edit` or `--bench-latency`).
    player: Option<concordia_app::Player>,
}

impl Viewer {
    fn call(&self, method: &str, params: Value) -> Result<Value, Failure> {
        self.runtime
            .block_on(self.handle.call(self.session, method, params))
            .map_err(|e| api_failure(&e))
    }

    /// Makes the window's content view the engine's viewport surface.
    #[cfg(target_os = "macos")]
    fn attach(&mut self, view: NativeView) -> Result<(), Failure> {
        // SAFETY: the view belongs to the open window; the layer is kept until the viewer drops.
        let (layer, target) = unsafe { concordia_rhi_metal::layer_for_view(view.0) };
        self.layer = Some(layer);
        let attached = self.runtime.block_on(
            self.handle
                // SAFETY: the layer lives in `self.layer` for the rest of the run.
                .run(move |engine| unsafe { concordia_render::attach_surface(engine, target) }),
        );
        attached
            .map_err(|e| api_failure(&e))?
            .map_err(|e| api_failure(&e))
    }

    #[cfg(not(target_os = "macos"))]
    fn attach(&mut self, _: NativeView) -> Result<(), Failure> {
        Err(Failure::usage(
            "drawing into the window needs Metal (macOS) in M1",
        ))
    }

    /// Draws the scene from the editor camera at the window's size.
    fn present(&self) -> Result<(), Failure> {
        let (width, height) = self.size;
        let view = self.call("viewport.navigate", json!({}))?["view"].clone();
        self.call(
            "viewport.present",
            json!({"view": view, "width": width, "height": height}),
        )?;
        Ok(())
    }

    fn opened(&mut self, width: u32, height: u32, view: Option<NativeView>) -> Result<(), Failure> {
        eprintln!("concordia run: window opened ({width}x{height} px)");
        self.size = (width.max(1), height.max(1));
        let (Some(view), true) = (view, renders(&self.handle, &self.runtime)) else {
            return Ok(());
        };
        self.attach(view)?;
        if let Some(player) = &mut self.player {
            *player = concordia_app::Player::new(true);
            return Ok(());
        }
        self.present()?;
        if let Some(bench) = self.bench.clone() {
            let handle = self.handle.clone();
            std::thread::spawn(move || match latency_bench(&handle, &bench) {
                Ok(frames) => eprintln!("concordia run: latency bench wrote {frames} frames"),
                Err(e) => eprintln!("concordia run: latency bench failed: {e}"),
            });
        }
        Ok(())
    }
}

impl Viewer {
    /// Hands `event` to the player, through the engine handle.
    fn play(&mut self, event: &WindowEvent) -> Result<(), Failure> {
        let Some(player) = &mut self.player else {
            return Ok(());
        };
        let (handle, session, runtime) = (&self.handle, self.session, &self.runtime);
        let mut api =
            |method: &str, params: Value| runtime.block_on(handle.call(session, method, params));
        if matches!(event, WindowEvent::Opened { .. }) {
            eprintln!("concordia run: playing the main scene");
        }
        player.event(event, &mut api).map_err(|e| api_failure(&e))
    }
}

impl WindowApp for Viewer {
    fn event(&mut self, event: WindowEvent) -> Flow {
        if self.player.is_some() {
            let opened = match &event {
                WindowEvent::Opened {
                    width,
                    height,
                    view,
                    ..
                } => self.opened(*width, *height, *view),
                _ => Ok(()),
            };
            let result = opened.and_then(|()| self.play(&event));
            return match (result, event) {
                (Err(failure), _) => {
                    self.failure = Some(failure);
                    Flow::Exit
                }
                (Ok(()), WindowEvent::CloseRequested) => Flow::Exit,
                (Ok(()), _) => Flow::Continue,
            };
        }
        let result = match event {
            WindowEvent::Opened {
                width,
                height,
                view,
                ..
            } => self.opened(width, height, view),
            WindowEvent::Resized { width, height } if self.layer.is_some() => {
                self.size = (width.max(1), height.max(1));
                self.present()
            }
            WindowEvent::CloseRequested => return Flow::Exit,
            _ => Ok(()),
        };
        match result {
            Ok(()) => Flow::Continue,
            Err(failure) => {
                self.failure = Some(failure);
                Flow::Exit
            }
        }
    }
}

/// Whether the project has the render module (`[render]`).
fn renders(handle: &EngineHandle, runtime: &tokio::runtime::Runtime) -> bool {
    runtime
        .block_on(handle.run(|engine| engine.methods().get("viewport.present").is_some()))
        .unwrap_or(false)
}

/// Orbits the camera once per display refresh through the engine's viewport control (the same
/// path the editor's native input takes), then writes `viewport.stats` to `bench.out`.
fn latency_bench(handle: &EngineHandle, bench: &Bench) -> Result<usize, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let control = runtime
        .block_on(handle.run(concordia_render::viewport_control))
        .map_err(|e| e.message)?
        .map_err(|e| e.message)?;
    let session = runtime
        .block_on(handle.open_session())
        .map_err(|e| e.message)?;
    let call = |method: &str, params: Value| {
        runtime
            .block_on(handle.call(session, method, params))
            .map_err(|e| e.message)
    };
    std::thread::sleep(Duration::from_millis(500));
    call("viewport.stats", json!({"clear": true}))?;
    let period = Duration::from_secs_f64(1.0 / bench.hz);
    let mut next = Instant::now();
    for i in 1..=u64::from(bench.frames) {
        next += period;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
        let params = concordia_render::ViewportNavigateParams {
            orbit: Some([3.0, 0.0]),
            seq: Some(i),
            input_time_ms: Some(unix_ms()),
            ..Default::default()
        };
        control.navigate(&params).map_err(|e| e.message)?;
    }
    std::thread::sleep(Duration::from_millis(500));
    let stats = call("viewport.stats", json!({}))?;
    let frames = stats["frames"].as_array().map_or(0, Vec::len);
    let report = json!({"version": 1, "hz": bench.hz, "frames": stats["frames"]});
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(&bench.out, text).map_err(|e| e.to_string())?;
    Ok(frames)
}

fn unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}
