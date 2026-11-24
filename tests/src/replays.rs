//! Replay file helpers for tests.

/// A canonical version 1 replay as version 0 (M2 2.8) would have written it: no platform,
/// manifest hash, task or per-tick hashes, and `version: 0`.
pub fn as_version_0(v1: &str) -> String {
    let mut out = Vec::new();
    let mut in_hashes = false;
    for line in v1.lines() {
        let t = line.trim_start();
        if in_hashes {
            in_hashes = !t.starts_with(']');
            continue;
        }
        if t.starts_with("hashes: [") {
            in_hashes = true;
            continue;
        }
        if ["platform:", "manifest_hash:", "task:"]
            .iter()
            .any(|f| t.starts_with(f))
        {
            continue;
        }
        out.push(line.replace("version: 1,", "version: 0,"));
    }
    out.join("\n") + "\n"
}
