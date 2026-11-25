//! `concordia run`: open the project's window on the main thread and play its main scene in it
//! (window input into the play world, drawn from the game camera; M2 2.5b.2b). `--edit` draws the
//! edit world from the editor camera instead; `--bench-latency` measures input-to-photon of the
//! engine alone (no editor, no webview) and implies `--edit`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use concordia_api::{Engine, EngineHandle};
use concordia_app::Project;
use concordia_core::id::SessionId;
use concordia_platform::{
    CursorIcon, Flow, NativeView, RunOptions, WindowApp, WindowEvent, run_window,
};
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
    // Sound plays on the default device here; everywhere else it mixes offline (ADR-0066).
    let config = project
        .engine_config_for(concordia_app::Playback::Windowed)
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
        done: Arc::new(AtomicBool::new(false)),
    });
    // A backstop only: the bench closes the window itself when it is done.
    let exit_after = args.exit_after_ms.map(Duration::from_millis).or_else(|| {
        bench
            .as_ref()
            .map(|b| BENCH_WARM_UP + Duration::from_secs_f64(f64::from(b.frames) / b.hz + 6.0))
    });
    let stop = bench.as_ref().map(|b| b.done.clone());
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
    run_window(&window, RunOptions { exit_after, stop }, &mut viewer)
        .map_err(|e| Failure::usage(e.to_string()))?;
    viewer.failure.map_or(Ok(()), Err)
}

/// The latency bench's settings.
#[derive(Debug, Clone)]
struct Bench {
    out: PathBuf,
    frames: u32,
    hz: f64,
    /// Set when the bench has finished, which closes the window.
    done: Arc<AtomicBool>,
}

/// The longest the bench waits for the window's first frame (pipelines and the scene upload
/// take seconds in a debug build).
const BENCH_WARM_UP: Duration = Duration::from_secs(30);

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
        let NativeView::AppKit(view) = view else {
            return Err(Failure::usage("a macOS window has an AppKit view"));
        };
        // SAFETY: the view belongs to the open window; the layer is kept until the viewer drops.
        let (layer, target) = unsafe { concordia_rhi_metal::layer_for_view(view) };
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

    /// Makes the window (X11 or Wayland) the engine's viewport surface, on Vulkan.
    #[cfg(not(target_os = "macos"))]
    fn attach(&mut self, view: NativeView) -> Result<(), Failure> {
        let target = vulkan_target(view).ok_or_else(|| {
            Failure::usage("this window has no X11 or Wayland handle to draw into")
        })?;
        let attached = self.runtime.block_on(
            self.handle
                // SAFETY: the window outlives the surface: the viewer detaches it on
                // `WindowEvent::Closing`, which the platform sends before destroying the window.
                .run(move |engine| unsafe { concordia_render::attach_surface(engine, target) }),
        );
        attached
            .map_err(|e| api_failure(&e))?
            .map_err(|e| api_failure(&e))?;
        self.layer = Some(());
        Ok(())
    }

    /// Releases the viewport surface before the platform destroys the window.
    fn detach(&mut self) {
        if self.layer.take().is_none() {
            return;
        }
        let detached = self
            .runtime
            .block_on(self.handle.run(concordia_render::detach_surface));
        if let Err(e) = detached
            .map_err(|e| e.message)
            .and_then(|r| r.map_err(|e| e.message))
        {
            eprintln!("concordia run: detaching the window surface failed: {e}");
        }
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
            std::thread::spawn(move || {
                match latency_bench(&handle, &bench) {
                    Ok(frames) => eprintln!("concordia run: latency bench wrote {frames} frames"),
                    Err(e) => eprintln!("concordia run: latency bench failed: {e}"),
                }
                bench.done.store(true, Ordering::Release);
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
        if event == WindowEvent::Closing {
            self.detach();
            return Flow::Exit;
        }
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

    fn cursor(&self) -> CursorIcon {
        self.player
            .as_ref()
            .map_or(CursorIcon::Arrow, concordia_app::Player::cursor)
    }
}

/// The Vulkan surface target for a Linux window (`None` for an AppKit view, which Metal draws
/// into through its own layer).
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn vulkan_target(view: NativeView) -> Option<concordia_rhi::SurfaceTarget> {
    use concordia_rhi::SurfaceTarget as T;
    match view {
        NativeView::AppKit(_) => None,
        NativeView::Xlib { display, window } => Some(T::Xlib { display, window }),
        NativeView::Xcb { connection, window } => Some(T::Xcb { connection, window }),
        NativeView::Wayland { display, surface } => Some(T::Wayland { display, surface }),
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
    // Until the window is on screen, frames find no drawable and are dropped; keep asking for
    // one (a still camera) and measure only once the window has shown a frame.
    wait_for(BENCH_WARM_UP, || {
        control
            .navigate(&concordia_render::ViewportNavigateParams::default())
            .map_err(|e| e.message)?;
        Ok(submitted(&call("viewport.stats", json!({}))?) > 0)
    })?;
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
    // Let the last frames reach the display before reading the log.
    wait_for(Duration::from_secs(5), || {
        Ok(presented(&call("viewport.stats", json!({}))?) >= bench.frames as usize)
    })
    .ok();
    let stats = call("viewport.stats", json!({}))?;
    let frames = stats["frames"].as_array().map_or(0, Vec::len);
    let report = json!({"version": 1, "hz": bench.hz, "frames": stats["frames"]});
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(&bench.out, text).map_err(|e| e.to_string())?;
    Ok(frames)
}

/// Polls `done` every 50 ms until it holds, failing after `limit`.
fn wait_for(limit: Duration, mut done: impl FnMut() -> Result<bool, String>) -> Result<(), String> {
    let start = Instant::now();
    while !done()? {
        if start.elapsed() > limit {
            return Err(format!("no frame reached the window within {limit:?}"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

/// How many logged frames have `field` set.
fn frames_with(stats: &Value, field: &str) -> usize {
    stats["frames"]
        .as_array()
        .map_or(0, |f| f.iter().filter(|f| !f[field].is_null()).count())
}

/// How many logged frames were submitted to the GPU.
fn submitted(stats: &Value) -> usize {
    frames_with(stats, "submitted_ms")
}

/// How many logged frames reached the display.
fn presented(stats: &Value) -> usize {
    frames_with(stats, "presented_ms")
}

fn unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use concordia_rhi::SurfaceTarget;
    use std::ptr::NonNull;

    /// Linux windows become Vulkan surface targets; AppKit views go through a Metal layer instead.
    #[test]
    fn linux_native_views_become_vulkan_surface_targets() {
        let (a, b) = (
            NonNull::<std::ffi::c_void>::dangling(),
            NonNull::new(8usize as *mut std::ffi::c_void).unwrap(),
        );
        assert_eq!(
            vulkan_target(NativeView::Xlib {
                display: a,
                window: 9
            }),
            Some(SurfaceTarget::Xlib {
                display: a,
                window: 9
            })
        );
        assert_eq!(
            vulkan_target(NativeView::Xcb {
                connection: a,
                window: 9
            }),
            Some(SurfaceTarget::Xcb {
                connection: a,
                window: 9
            })
        );
        assert_eq!(
            vulkan_target(NativeView::Wayland {
                display: a,
                surface: b
            }),
            Some(SurfaceTarget::Wayland {
                display: a,
                surface: b
            })
        );
        assert_eq!(vulkan_target(NativeView::AppKit(a)), None);
    }
}
