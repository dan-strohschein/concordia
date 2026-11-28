//! Replay file helpers for tests.

/// A canonical replay in the current format as version 0 (M2 2.8) would have written it: no
/// platform, manifest hash, task, external outputs or per-tick hashes, and `version: 0`.
pub fn as_version_0(current: &str) -> String {
    let mut out = Vec::new();
    let mut in_list = false;
    let mut versions = 0;
    for line in current.lines() {
        let t = line.trim_start();
        if in_list {
            in_list = !t.starts_with(']');
            continue;
        }
        if t.starts_with("hashes: [") || t.starts_with("external: [") {
            in_list = !t.ends_with("],");
            continue;
        }
        if ["platform:", "manifest_hash:", "task:"]
            .iter()
            .any(|f| t.starts_with(f))
        {
            continue;
        }
        if t.starts_with("version: ") {
            versions += 1;
            let indent = &line[..line.len() - t.len()];
            out.push(format!("{indent}version: 0,"));
            continue;
        }
        out.push(line.to_owned());
    }
    assert_eq!(versions, 1, "a replay has one version line");
    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::as_version_0;

    /// A file in the current format (2 since M4 4.12d) comes out as version 0, without the
    /// fields and lists version 0 never had.
    #[test]
    fn the_current_format_becomes_version_0() {
        let current = format!(
            "Replay(\n    version: {},\n    platform: \"macos-aarch64\",\n    external: [\n        (tick: 3),\n    ],\n    hashes: [\n        \"ab\",\n    ],\n    ticks: 4,\n)\n",
            concordia_replay::FORMAT_VERSION
        );
        let v0 = as_version_0(&current);
        assert_eq!(v0, "Replay(\n    version: 0,\n    ticks: 4,\n)\n");
    }
}
