//! Measures a perf scene: opens its project with perf mode on (restoring generated files kept out
//! of git first), draws the warm-up and measured
//! frames along the camera path through `capture.screenshot`, and reads each frame's pass times
//! from `render.stats`. The perf folder holds `reference.ron`, `scenes/` and `budgets/`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use concordia_api::Engine;
use concordia_app::Project;
use concordia_core::id::SessionId;
use serde_json::{Value, json};

use crate::report::FrameSample;
use crate::{
    BudgetFile, CameraKey, HostProbe, PerfError, PerfReport, PerfScene, ReferenceMachine, Verdict,
    judge,
};

/// Overrides for a run (smoke tests measure a few frames).
#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    /// Warm-up frames instead of the scene's.
    pub warmup_frames: Option<u32>,
    /// Measured frames instead of the scene's.
    pub frames: Option<u32>,
}

/// A finished run: the report (with its verdict written in) and the verdict.
#[derive(Debug, Clone)]
pub struct Run {
    /// What was measured.
    pub report: PerfReport,
    /// Against the scene's budget, on the reference machine.
    pub verdict: Verdict,
}

impl Run {
    /// Writes `<out>/<scene>.perf.ron` and returns its path.
    pub fn write(&self, out: &Path) -> Result<PathBuf, PerfError> {
        let path = out.join(format!("{}.perf.ron", self.report.scene));
        std::fs::create_dir_all(out).map_err(|e| file_error(out, e))?;
        std::fs::write(&path, self.report.to_text()?).map_err(|e| file_error(&path, e))?;
        Ok(path)
    }
}

/// Measures the scene in `scene_file` (`<perf>/scenes/<name>.perf.ron`).
pub fn run(
    scene_file: &Path,
    options: &RunOptions,
    probe: &dyn HostProbe,
) -> Result<Run, PerfError> {
    let name = scene_name(scene_file)?;
    let scene = PerfScene::read(&read(scene_file)?).map_err(|e| in_file(scene_file, e))?;
    let root = scene_file.parent().and_then(Path::parent).ok_or_else(|| {
        PerfError::Invalid(format!("{} is not in <perf>/scenes/", scene_file.display()))
    })?;
    let reference_file = root.join("reference.ron");
    let reference =
        ReferenceMachine::read(&read(&reference_file)?).map_err(|e| in_file(&reference_file, e))?;
    let budget = read_budget(&root.join("budgets").join(format!("{name}.budget.ron")))?;
    let caps =
        concordia_rhi_metal::probe().map_err(|e| PerfError::Invalid(format!("no GPU: {e}")))?;
    let host = probe.host(&caps);
    let project = scene_file.parent().unwrap_or(root).join(&scene.project);
    let samples = Session::open(&project, &scene.scene)?.measure(&scene, options)?;
    let mut report = PerfReport::from_samples(&name, host, &samples)?;
    let verdict = judge(&report, budget.as_ref(), &reference);
    report.record(&verdict);
    Ok(Run { report, verdict })
}

/// `<name>` of `<name>.perf.ron`.
fn scene_name(file: &Path) -> Result<String, PerfError> {
    file.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(".perf.ron"))
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            PerfError::Invalid(format!("{} is not a <name>.perf.ron file", file.display()))
        })
}

fn read(path: &Path) -> Result<String, PerfError> {
    std::fs::read_to_string(path).map_err(|e| file_error(path, e))
}

/// The budget file, if the scene has one.
fn read_budget(path: &Path) -> Result<Option<BudgetFile>, PerfError> {
    if !path.exists() {
        return Ok(None);
    }
    BudgetFile::read(&read(path)?)
        .map(Some)
        .map_err(|e| in_file(path, e))
}

fn file_error(path: &Path, e: impl std::fmt::Display) -> PerfError {
    PerfError::File {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

fn in_file(path: &Path, e: PerfError) -> PerfError {
    file_error(path, e)
}

/// An engine on the perf scene's project, with perf mode on and the scene open.
struct Session {
    engine: Engine,
    session: SessionId,
}

impl Session {
    fn open(project: &Path, scene: &str) -> Result<Self, PerfError> {
        let app = |e: concordia_app::AppError| file_error(project, e);
        let mut loaded = Project::load(project).map_err(app)?;
        let render =
            loaded.manifest.render.as_mut().ok_or_else(|| {
                file_error(project, "the project has no [render] section to measure")
            })?;
        render.perf_mode = true;
        let config = loaded.engine_config().map_err(app)?;
        let mut engine = Engine::new(config).map_err(|e| file_error(project, format!("{e:?}")))?;
        let session = engine.open_session();
        let mut opened = Self { engine, session };
        opened.call(
            "session.open",
            json!({"client": "perf", "actor": "tool:perf"}),
        )?;
        opened.restore_generated()?;
        opened.call("scene.open", json!({"path": scene}))?;
        Ok(opened)
    }

    /// Remakes generated files left out of the project (`gen.restore`, when it has `[gen]`).
    fn restore_generated(&mut self) -> Result<(), PerfError> {
        let params = json!({"actor": "tool:perf"});
        match self.engine.call(self.session, "gen.restore", params) {
            Err(e) if e.kind == concordia_api::ApiErrorKind::MethodNotFound => Ok(()),
            Err(e) => Err(PerfError::Engine {
                method: "gen.restore".into(),
                message: e.message,
            }),
            Ok(_) => Ok(()),
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, PerfError> {
        self.engine
            .call(self.session, method, params)
            .map_err(|e| PerfError::Engine {
                method: method.to_owned(),
                message: e.message,
            })
    }

    /// Draws the warm-up frames, then returns each measured frame's GPU times.
    fn measure(
        &mut self,
        scene: &PerfScene,
        options: &RunOptions,
    ) -> Result<Vec<FrameSample>, PerfError> {
        let warmup = options.warmup_frames.unwrap_or(scene.warmup_frames);
        let frames = options.frames.unwrap_or(scene.frames).max(1);
        for _ in 0..warmup {
            self.frame(scene, scene.pose(0, frames))?;
        }
        (0..frames)
            .map(|i| self.frame(scene, scene.pose(i, frames)))
            .collect()
    }

    /// Draws one frame from `pose` and waits for its GPU times.
    fn frame(&mut self, scene: &PerfScene, pose: CameraKey) -> Result<FrameSample, PerfError> {
        let view = json!({"eye": pose.eye, "target": pose.target, "projection": {"Perspective": {
            "fov_y": scene.fov_y, "near": NEAR, "far": FAR}}});
        let shot = self.call(
            "capture.screenshot",
            json!({"view": view, "width": scene.width, "height": scene.height, "ui": false}),
        )?;
        let number = shot["frame"].as_u64();
        let deadline = Instant::now() + STATS_TIMEOUT;
        loop {
            let stats = self.call("render.stats", json!({}))?["frame"].clone();
            if stats["frame"].as_u64() == number
                && let Some(sample) = sample(&stats)
            {
                return Ok(sample);
            }
            if Instant::now() > deadline {
                return Err(PerfError::Engine {
                    method: "render.stats".into(),
                    message: format!("frame {number:?}'s GPU times never arrived: {stats}"),
                });
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// A frame's GPU times from `render.stats`, once they have all arrived.
fn sample(stats: &Value) -> Option<FrameSample> {
    let gpu_ms = stats["gpu_ms"].as_f64()?;
    let passes: Vec<(String, f64)> = stats["passes"]
        .as_array()?
        .iter()
        .filter_map(|p| Some((p["name"].as_str()?.to_owned(), p["gpu_ms"].as_f64()?)))
        .collect();
    let timed = stats["pass_timing"] == "measured";
    (!timed || !passes.is_empty()).then_some(FrameSample { gpu_ms, passes })
}

const NEAR: f32 = 0.1;
const FAR: f32 = 1000.0;
const STATS_TIMEOUT: Duration = Duration::from_secs(5);
