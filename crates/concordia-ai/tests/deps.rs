//! The default build carries no LLM runtime and no network client (M4 4.12d, M9): a shipped game
//! brings its own provider, and the engine never reaches the network for AI.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

/// Crates that would mean an HTTP client or a model runtime in the tree.
const FORBIDDEN: [&str; 12] = [
    "reqwest",
    "hyper",
    "ureq",
    "isahc",
    "curl",
    "surf",
    "attohttpc",
    "llama-cpp-2",
    "candle-core",
    "ort",
    "tch",
    "mistralrs",
];

#[test]
fn default_build_has_no_llm_or_network_dependency() {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--offline"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let names: BTreeMap<&str, &str> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    let nodes: BTreeMap<&str, Vec<&str>> = meta["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            let normal = n["deps"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|d| {
                    d["dep_kinds"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|k| k["kind"].is_null())
                })
                .map(|d| d["pkg"].as_str().unwrap())
                .collect();
            (n["id"].as_str().unwrap(), normal)
        })
        .collect();
    let root = names
        .iter()
        .find(|(_, n)| **n == "concordia-ai")
        .map(|(id, _)| *id)
        .expect("concordia-ai");
    let mut seen = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if seen.insert(id) {
            stack.extend(nodes.get(id).into_iter().flatten().copied());
        }
    }
    let found: Vec<&str> = seen
        .iter()
        .map(|id| names[id])
        .filter(|n| FORBIDDEN.contains(n))
        .collect();
    assert!(
        found.is_empty(),
        "concordia-ai's default build pulls in {found:?}"
    );
    assert!(
        seen.len() > 20,
        "the walk covered the tree ({} crates)",
        seen.len()
    );
}
