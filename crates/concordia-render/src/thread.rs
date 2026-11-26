//! The render thread: owns the [`Renderer`] so frames render off the engine thread. Viewport
//! frames are queued and coalesced (the newest view wins); captures and surface changes are
//! answered in order.

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use concordia_rhi::{RhiError, Surface, SurfaceTarget};

use crate::frame_log::{
    FrameLog, FrameRecord, gpu_time_callback, presented_callback, push_frame, remove_frame,
    unix_ms, update_frame,
};
use crate::stats::{self, LastFrame};
use crate::{
    FrameCallbacks, Image, MemoryAssets, RenderError, RenderRequest, RenderScene, Renderer,
    SceneDelta,
};

/// Which world's scene a job updates: 0 = edit, 1 = play.
pub(crate) type SceneSlot = usize;

/// A world's changes since the thread last saw it, and asset data it has not seen yet.
pub(crate) struct SceneUpdate {
    pub(crate) slot: SceneSlot,
    pub(crate) delta: SceneDelta,
    pub(crate) assets: MemoryAssets,
    /// Set when the update is a new simulation tick of this length (drawn interpolated).
    pub(crate) tick: Option<Duration>,
}

/// A viewport frame to present: the scene update, the view, and its `viewport.stats` record.
pub(crate) struct ViewportFrame {
    pub(crate) scene: SceneUpdate,
    pub(crate) request: RenderRequest,
    pub(crate) log: FrameLog,
    pub(crate) id: u64,
}

/// A capture's image and the renderer's frame count.
pub(crate) type Captured = Result<(Image, u64), RenderError>;

enum Job {
    Present(ViewportFrame),
    /// Present the latest scene again from another view (camera moves off the engine thread).
    Redraw {
        view: crate::View,
        log: FrameLog,
        id: u64,
    },
    Capture {
        scene: SceneUpdate,
        request: RenderRequest,
        /// With temporal on, frames to render before reading back (None: a jitter cycle).
        settle: Option<u32>,
        reply: mpsc::Sender<Captured>,
    },
    /// Switch the temporal resolve (`render.set_temporal`).
    Temporal(crate::temporal::Temporal),
    Attach {
        target: SurfaceTarget,
        reply: mpsc::Sender<Result<(), RenderError>>,
    },
    /// Release the viewport surface (its window is about to go).
    Detach {
        reply: mpsc::Sender<()>,
    },
}

/// The render thread's handle; dropping it stops the thread after its queued work.
pub(crate) struct RenderThread {
    jobs: Option<mpsc::Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl RenderThread {
    /// Starts the thread and makes the renderer on it, reporting device or shader errors.
    pub(crate) fn spawn(
        make: impl FnOnce() -> Result<Renderer, RenderError> + Send + 'static,
        last: LastFrame,
    ) -> Result<Self, RenderError> {
        let (jobs, queue) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("concordia-render".into())
            .spawn(move || match make() {
                Ok(mut renderer) => {
                    // The editor viewport favours latency: its GPU frames are short.
                    let _ = renderer.set_frames_in_flight(1);
                    let _ = ready.send(Ok(()));
                    Worker::new(renderer, last).run(&queue);
                }
                Err(e) => {
                    let _ = ready.send(Err(e));
                }
            })
            .map_err(|e| RenderError::Gpu(RhiError::invalid("render thread", e.to_string())))?;
        started.recv().map_err(|_| stopped())??;
        Ok(Self {
            jobs: Some(jobs),
            thread: Some(thread),
        })
    }

    /// Queues a viewport frame; returns at once.
    pub(crate) fn present(&self, frame: ViewportFrame) -> Result<(), RenderError> {
        self.send(Job::Present(frame))
    }

    /// Renders offscreen and waits for the image.
    pub(crate) fn capture(
        &self,
        scene: SceneUpdate,
        request: RenderRequest,
        settle: Option<u32>,
    ) -> Captured {
        let (reply, answer) = mpsc::channel();
        self.send(Job::Capture {
            scene,
            request,
            settle,
            reply,
        })?;
        answer.recv().map_err(|_| stopped())?
    }

    /// Switches the temporal resolve; returns at once.
    pub(crate) fn set_temporal(
        &self,
        temporal: crate::temporal::Temporal,
    ) -> Result<(), RenderError> {
        self.send(Job::Temporal(temporal))
    }

    /// Makes `target` the viewport surface (replacing any earlier one).
    ///
    /// # Safety
    /// The native object behind `target` must stay alive until [`RenderThread::detach`] returns
    /// or the render thread ends.
    pub(crate) unsafe fn attach(&self, target: SurfaceTarget) -> Result<(), RenderError> {
        let (reply, answer) = mpsc::channel();
        self.send(Job::Attach { target, reply })?;
        answer.recv().map_err(|_| stopped())?
    }

    /// Releases the viewport surface and waits until it is gone.
    pub(crate) fn detach(&self) -> Result<(), RenderError> {
        let (reply, answer) = mpsc::channel();
        self.send(Job::Detach { reply })?;
        answer.recv().map_err(|_| stopped())
    }

    /// A handle that asks this thread for redraws from any thread.
    pub(crate) fn redrawer(&self) -> Redrawer {
        Redrawer(self.jobs.clone())
    }

    fn send(&self, job: Job) -> Result<(), RenderError> {
        self.jobs
            .as_ref()
            .and_then(|jobs| jobs.send(job).ok())
            .ok_or_else(stopped)
    }
}

impl Drop for RenderThread {
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Sends redraw requests to the render thread.
#[derive(Clone)]
pub(crate) struct Redrawer(Option<mpsc::Sender<Job>>);

impl Redrawer {
    /// Queues a redraw of the latest scene from `view`, logged as frame `id`.
    pub(crate) fn redraw(
        &self,
        view: crate::View,
        log: FrameLog,
        id: u64,
    ) -> Result<(), RenderError> {
        self.0
            .as_ref()
            .and_then(|jobs| jobs.send(Job::Redraw { view, log, id }).ok())
            .ok_or_else(stopped)
    }
}

fn stopped() -> RenderError {
    RenderError::Gpu(RhiError::invalid(
        "render thread",
        "the render thread stopped",
    ))
}

/// The thread's state: the renderer, every asset it has been sent, the edit and play scenes,
/// and the viewport surface.
struct Worker {
    renderer: Renderer,
    store: MemoryAssets,
    scenes: [RenderScene; 2],
    surface: Option<Surface>,
    /// The latest viewport frame's scene and request, which redraws reuse.
    last: Option<(SceneSlot, RenderRequest)>,
    /// When each scene's latest simulation tick arrived, and the tick length.
    ticks: [Option<(Instant, Duration)>; 2],
    /// The `viewport.stats` log, and ids for frames this thread draws on its own (between ticks),
    /// counted from 2^63 so they never meet the engine's.
    log: Option<FrameLog>,
    own_ids: u64,
    /// The last frame drawn, for `render.stats`.
    last_frame: LastFrame,
}

impl Worker {
    fn new(renderer: Renderer, last_frame: LastFrame) -> Self {
        Self {
            renderer,
            store: MemoryAssets::default(),
            scenes: Default::default(),
            surface: None,
            last: None,
            ticks: [None; 2],
            log: None,
            own_ids: 1 << 63,
            last_frame,
        }
    }

    /// Handles jobs until the handle drops; presents only the newest queued viewport frame.
    /// Handles jobs until the handle drops; presents only the newest queued viewport frame. While
    /// the viewport shows a ticking simulation, it also presents between ticks, every display
    /// refresh (paced by the display), interpolating motion.
    fn run(mut self, queue: &mpsc::Receiver<Job>) {
        loop {
            let first = match self.animating() {
                true => match queue.try_recv() {
                    Ok(job) => Some(job),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                },
                false => match queue.recv() {
                    Ok(job) => Some(job),
                    Err(_) => return,
                },
            };
            let mut latest = None;
            let animating = first.is_none();
            match first {
                Some(first) => latest = self.handle(first, latest),
                // Waiting on background work, not motion: no need to redraw every refresh.
                None if !self.ticking() => std::thread::sleep(Duration::from_millis(16)),
                None => {}
            }
            // Late latching: hold the next display image first (this is where the thread waits
            // for the display), then take everything that arrived meanwhile, so the frame shows
            // the newest camera and scene rather than what was current before the wait.
            if latest.is_none() && !animating {
                continue; // a capture or surface change: nothing new to show
            }
            self.hold_image(latest.as_ref());
            while let Ok(job) = queue.try_recv() {
                latest = self.handle(job, latest);
            }
            match latest {
                Some(frame) => self.present(frame),
                None => self.present_between_ticks(),
            }
        }
    }

    /// Acquires the viewport's next image ahead of rendering, at the size of `frame` (or the last
    /// frame's).
    fn hold_image(&mut self, frame: Option<&ViewportFrame>) {
        let size = frame
            .map(|f| &f.request)
            .or(self.last.as_ref().map(|(_, r)| r))
            .map(|r| (r.width, r.height));
        if let (Some(surface), Some((width, height))) = (self.surface, size)
            && let Err(e) = self.renderer.acquire_ahead(surface, width, height)
        {
            tracing::warn!(target: "concordia::render", error = %e, "viewport image unavailable");
        }
    }

    /// Whether the viewport needs redrawing without new jobs: its scene is ticking, or background
    /// work (environment prefiltering) is about to change it.
    fn animating(&self) -> bool {
        self.last.is_some()
            && self.surface.is_some()
            && (self.ticking() || self.renderer.has_pending_work())
    }

    /// Whether the viewport shows a scene that ticked within the last two tick lengths.
    fn ticking(&self) -> bool {
        let Some((slot, _)) = &self.last else {
            return false;
        };
        self.ticks[*slot].is_some_and(|(at, dt)| at.elapsed() < dt * 2)
    }

    /// How far between its last two ticks scene `slot` should be drawn now (1 = the latest).
    fn alpha(&self, slot: SceneSlot) -> f32 {
        match self.ticks[slot] {
            Some((at, dt)) if !dt.is_zero() => {
                (at.elapsed().as_secs_f32() / dt.as_secs_f32()).min(1.0)
            }
            _ => 1.0,
        }
    }

    /// Presents the latest viewport request again, interpolated, as its own `viewport.stats` frame.
    fn present_between_ticks(&mut self) {
        let (Some((slot, request)), Some(log)) = (self.last.clone(), self.log.clone()) else {
            return;
        };
        self.own_ids += 1;
        let record = FrameRecord {
            seq: None,
            input_time_ms: None,
            queued_ms: unix_ms(SystemTime::now()),
            cpu_ms: 0.0,
            submitted_ms: None,
            presented_ms: None,
            gpu_ms: None,
        };
        push_frame(&log, self.own_ids, record);
        let scene = SceneUpdate {
            slot,
            delta: SceneDelta::Changes {
                upserts: Vec::new(),
                removed: Vec::new(),
            },
            assets: MemoryAssets::default(),
            tick: None,
        };
        let id = self.own_ids;
        self.present(ViewportFrame {
            scene,
            request,
            log,
            id,
        });
    }

    /// Handles one job; returns the newest viewport frame still to present.
    fn handle(&mut self, job: Job, latest: Option<ViewportFrame>) -> Option<ViewportFrame> {
        match job {
            Job::Present(mut frame) => {
                // Every delta is applied, even for frames that will be skipped.
                self.update(&mut frame.scene);
                self.last = Some((frame.scene.slot, frame.request.clone()));
                self.log = Some(frame.log.clone());
                if let Some(stale) = latest {
                    remove_frame(&stale.log, stale.id);
                }
                Some(frame)
            }
            Job::Capture {
                mut scene,
                request,
                settle,
                reply,
            } => {
                self.update(&mut scene);
                let world = self.scenes[scene.slot].world();
                let settling = u64::from(self.renderer.settle_frames(&request, settle));
                let number = self.renderer.frames() + settling;
                let start = Instant::now();
                let callbacks = FrameCallbacks {
                    presented: None,
                    gpu_time: Some(stats::gpu_callback(self.last_frame.clone(), number)),
                };
                let image = self.renderer.render_settled(
                    world,
                    &mut self.store,
                    &request,
                    (settle, callbacks),
                );
                if image.is_ok() {
                    let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
                    let frame_stats = self.renderer.stats();
                    stats::record(
                        &self.last_frame,
                        number,
                        &request,
                        frame_stats,
                        cpu_ms,
                        &self.renderer,
                    );
                }
                let _ = reply.send(image.map(|image| (image, self.renderer.frames())));
                latest
            }
            Job::Redraw { view, log, id } => {
                let Some((slot, mut request)) = self.last.clone() else {
                    remove_frame(&log, id);
                    return latest;
                };
                request.view = Some(view);
                if let Some(stale) = latest {
                    remove_frame(&stale.log, stale.id);
                }
                Some(ViewportFrame {
                    scene: SceneUpdate {
                        slot,
                        delta: SceneDelta::Changes {
                            upserts: Vec::new(),
                            removed: Vec::new(),
                        },
                        assets: MemoryAssets::default(),
                        tick: None,
                    },
                    request,
                    log,
                    id,
                })
            }
            Job::Attach { target, reply } => {
                let _ = reply.send(self.attach(target));
                latest
            }
            Job::Temporal(temporal) => {
                self.renderer.set_temporal(temporal);
                latest
            }
            Job::Detach { reply } => {
                if let Some(old) = self.surface.take() {
                    self.renderer.destroy_surface(old);
                }
                let _ = reply.send(());
                latest
            }
        }
    }

    /// Applies a scene update (taking its delta and assets). An asset sent again changed (hot
    /// reload), so the renderer drops its GPU copy and uploads the new data.
    fn update(&mut self, scene: &mut SceneUpdate) {
        scene.assets.refs().for_each(|r| self.renderer.forget(r));
        self.store.merge(std::mem::take(&mut scene.assets));
        let empty = SceneDelta::Changes {
            upserts: Vec::new(),
            removed: Vec::new(),
        };
        let delta = std::mem::replace(&mut scene.delta, empty);
        if matches!(delta, SceneDelta::Full(_)) {
            // A new or re-sent scene: nothing on screen moved from last frame's.
            self.renderer.reset_motion_history();
        }
        match scene.tick {
            Some(dt) => {
                self.scenes[scene.slot].apply_tick(delta);
                self.ticks[scene.slot] = Some((Instant::now(), dt));
            }
            None => self.scenes[scene.slot].apply(delta),
        }
    }

    fn attach(&mut self, target: SurfaceTarget) -> Result<(), RenderError> {
        // SAFETY: `RenderThread::attach`'s caller keeps the target alive until it is detached
        // (`Job::Detach`) or this thread ends.
        let surface = unsafe { self.renderer.create_surface(target) }?;
        if let Some(old) = self.surface.replace(surface) {
            self.renderer.destroy_surface(old);
        }
        Ok(())
    }

    fn present(&mut self, frame: ViewportFrame) {
        let ViewportFrame {
            scene,
            request,
            log,
            id,
        } = frame;
        let Some(surface) = self.surface else {
            return remove_frame(&log, id);
        };
        let start = Instant::now();
        let number = self.renderer.frames() + 1;
        let (to_log, to_stats) = (
            gpu_time_callback(log.clone(), id),
            stats::gpu_callback(self.last_frame.clone(), number),
        );
        let callbacks = FrameCallbacks {
            presented: Some(presented_callback(log.clone(), id)),
            gpu_time: Some(Box::new(move |ms| {
                to_log(ms.clone());
                to_stats(ms);
            })),
        };
        let world = self.scenes[scene.slot].world_at(self.alpha(scene.slot));
        let presented =
            self.renderer
                .present_with(&world, &mut self.store, &request, surface, callbacks);
        match presented {
            Ok(true) => {
                let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
                let frame_stats = self.renderer.stats();
                stats::record(
                    &self.last_frame,
                    number,
                    &request,
                    frame_stats,
                    cpu_ms,
                    &self.renderer,
                );
                let submitted = unix_ms(SystemTime::now());
                update_frame(&log, id, |f| {
                    f.cpu_ms = cpu_ms;
                    f.submitted_ms = Some(submitted);
                });
            }
            Ok(false) => remove_frame(&log, id),
            Err(e) => {
                tracing::warn!(target: "concordia::render", error = %e, "viewport frame failed");
                remove_frame(&log, id);
            }
        }
    }
}
