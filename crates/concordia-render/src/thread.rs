//! The render thread: owns the [`Renderer`] so frames render off the engine thread. Viewport
//! frames are queued and coalesced (the newest view wins); captures and surface changes are
//! answered in order.

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime};

use concordia_rhi::{RhiError, Surface, SurfaceTarget};

use crate::frame_log::{FrameLog, presented_callback, remove_frame, unix_ms, update_frame};
use crate::{Image, MemoryAssets, RenderError, RenderRequest, RenderScene, Renderer, SceneDelta};

/// Which world's scene a job updates: 0 = edit, 1 = play.
pub(crate) type SceneSlot = usize;

/// A world's changes since the thread last saw it, and asset data it has not seen yet.
pub(crate) struct SceneUpdate {
    pub(crate) slot: SceneSlot,
    pub(crate) delta: SceneDelta,
    pub(crate) assets: MemoryAssets,
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
    Capture {
        scene: SceneUpdate,
        request: RenderRequest,
        reply: mpsc::Sender<Captured>,
    },
    Attach {
        target: SurfaceTarget,
        reply: mpsc::Sender<Result<(), RenderError>>,
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
                    Worker::new(renderer).run(&queue);
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
    pub(crate) fn capture(&self, scene: SceneUpdate, request: RenderRequest) -> Captured {
        let (reply, answer) = mpsc::channel();
        self.send(Job::Capture {
            scene,
            request,
            reply,
        })?;
        answer.recv().map_err(|_| stopped())?
    }

    /// Makes `target` the viewport surface (replacing any earlier one).
    ///
    /// # Safety
    /// The native object behind `target` must outlive the render thread.
    pub(crate) unsafe fn attach(&self, target: SurfaceTarget) -> Result<(), RenderError> {
        let (reply, answer) = mpsc::channel();
        self.send(Job::Attach { target, reply })?;
        answer.recv().map_err(|_| stopped())?
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
}

impl Worker {
    fn new(renderer: Renderer) -> Self {
        Self {
            renderer,
            store: MemoryAssets::default(),
            scenes: Default::default(),
            surface: None,
        }
    }

    /// Handles jobs until the handle drops; presents only the newest queued viewport frame.
    fn run(mut self, queue: &mpsc::Receiver<Job>) {
        while let Ok(first) = queue.recv() {
            let mut latest = None;
            let mut next = Some(first);
            while let Some(job) = next {
                latest = self.handle(job, latest);
                next = queue.try_recv().ok();
            }
            if let Some(frame) = latest {
                self.present(frame);
            }
        }
    }

    /// Handles one job; returns the newest viewport frame still to present.
    fn handle(&mut self, job: Job, latest: Option<ViewportFrame>) -> Option<ViewportFrame> {
        match job {
            Job::Present(mut frame) => {
                // Every delta is applied, even for frames that will be skipped.
                self.update(&mut frame.scene);
                if let Some(stale) = latest {
                    remove_frame(&stale.log, stale.id);
                }
                Some(frame)
            }
            Job::Capture {
                mut scene,
                request,
                reply,
            } => {
                self.update(&mut scene);
                let world = self.scenes[scene.slot].world();
                let image = self.renderer.render(world, &mut self.store, &request);
                let _ = reply.send(image.map(|image| (image, self.renderer.frames())));
                latest
            }
            Job::Attach { target, reply } => {
                let _ = reply.send(self.attach(target));
                latest
            }
        }
    }

    /// Applies a scene update (taking its delta and assets).
    fn update(&mut self, scene: &mut SceneUpdate) {
        self.store.merge(std::mem::take(&mut scene.assets));
        let empty = SceneDelta::Changes {
            upserts: Vec::new(),
            removed: Vec::new(),
        };
        self.scenes[scene.slot].apply(std::mem::replace(&mut scene.delta, empty));
    }

    fn attach(&mut self, target: SurfaceTarget) -> Result<(), RenderError> {
        // SAFETY: `RenderThread::attach`'s caller guarantees the target outlives this thread.
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
        let on_presented = presented_callback(log.clone(), id);
        let world = self.scenes[scene.slot].world();
        let presented = self.renderer.present_with(
            world,
            &mut self.store,
            &request,
            surface,
            Some(on_presented),
        );
        match presented {
            Ok(true) => {
                let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
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
