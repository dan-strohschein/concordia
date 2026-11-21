//! Stable sub-asset names: source names where present and unique, else indices.

use std::collections::BTreeMap;

/// Each name, or its index when missing; duplicates get `#<index>`. `/` is replaced (it separates sub-paths).
pub fn unique_names<'a>(names: impl IntoIterator<Item = Option<&'a str>>) -> Vec<String> {
    let names: Vec<Option<String>> = names
        .into_iter()
        .map(|n| n.filter(|n| !n.is_empty()).map(|n| n.replace('/', "_")))
        .collect();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for name in names.iter().flatten() {
        *counts.entry(name.as_str()).or_default() += 1;
    }
    names
        .iter()
        .enumerate()
        .map(|(i, n)| match n {
            Some(n) if counts.get(n.as_str()) == Some(&1) => n.clone(),
            Some(n) => format!("{n}#{i}"),
            None => i.to_string(),
        })
        .collect()
}
