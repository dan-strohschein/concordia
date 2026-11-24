//! Running an external tool with input, a time limit and its whole process group killed when
//! the limit passes (no `unsafe`: the group is signalled with the system `kill` command).

use std::ffi::OsString;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::GenError;

/// A finished process.
#[derive(Debug)]
pub(crate) struct Finished {
    /// Its exit code (none when killed by a signal).
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `command` with `stdin`, waiting at most `limit`.
pub(crate) fn run(
    command: &[OsString],
    stdin: &[u8],
    limit: Duration,
) -> Result<Finished, GenError> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| GenError::invalid("command", "is empty"))?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd
        .spawn()
        .map_err(|e| GenError::Output(format!("{}: {e}", program.to_string_lossy())))?;
    let input = stdin.to_vec();
    let mut pipe_in = child.stdin.take();
    let writer = std::thread::spawn(move || {
        if let Some(p) = pipe_in.as_mut() {
            let _ = p.write_all(&input);
        }
    });
    let read = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = r.read_to_string(&mut s);
            s
        })
    };
    let out = child.stdout.take().map(|s| read(Box::new(s)));
    let err = child.stderr.take().map(|s| read(Box::new(s)));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| GenError::Output(e.to_string()))?
        {
            break status;
        }
        if started.elapsed() > limit {
            kill_group(&mut child);
            let _ = child.wait();
            return Err(GenError::Output(format!(
                "timed out after {} s",
                limit.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _ = writer.join();
    let text = |h: Option<std::thread::JoinHandle<String>>| {
        h.and_then(|h| h.join().ok()).unwrap_or_default()
    };
    Ok(Finished {
        code: status.code(),
        stdout: text(out),
        stderr: text(err),
    })
}

/// Kills the child and every process it started (its process group on Unix).
fn kill_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// The last `n` lines of `text`, for error messages.
pub(crate) fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}
