//! Checks a SPIR-V module before a driver sees it (pure): drivers may crash on malformed code.

/// The SPIR-V magic number.
const MAGIC: u32 = 0x0723_0203;
/// `OpEntryPoint`.
const OP_ENTRY_POINT: u32 = 15;
/// Highest SPIR-V version Vulkan 1.3 consumes (1.6).
const MAX_MINOR: u32 = 6;

/// A shader stage's SPIR-V execution model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Model {
    /// `Vertex`.
    Vertex = 0,
    /// `Fragment`.
    Fragment = 4,
    /// `GLCompute`.
    Compute = 5,
}

/// Ok if `words` is a SPIR-V module (≤ 1.6) with entry point `entry` for `model`; else the log.
pub(crate) fn check(words: &[u32], entry: &str, model: Model) -> Result<(), String> {
    let entries = entry_points(words)?;
    if entries
        .iter()
        .any(|(m, name)| *m == model as u32 && name == entry)
    {
        return Ok(());
    }
    let found: Vec<String> = entries
        .iter()
        .map(|(m, name)| format!("{name} (model {m})"))
        .collect();
    Err(format!(
        "no {model:?} entry point {entry:?}; the module has: {}",
        if found.is_empty() {
            "none".into()
        } else {
            found.join(", ")
        }
    ))
}

/// Every `(execution model, name)` the module declares, after checking its header and framing.
fn entry_points(words: &[u32]) -> Result<Vec<(u32, String)>, String> {
    header(words)?;
    let mut entries = Vec::new();
    let mut at = 5;
    while at < words.len() {
        let count = (words[at] >> 16) as usize;
        let op = words[at] & 0xffff;
        if count == 0 || at + count > words.len() {
            return Err(format!("instruction at word {at} has a bad length {count}"));
        }
        if op == OP_ENTRY_POINT && count >= 4 {
            let name = literal(&words[at + 3..at + count])
                .ok_or_else(|| format!("entry point at word {at} has no terminated name"))?;
            entries.push((words[at + 1], name));
        }
        at += count;
    }
    Ok(entries)
}

fn header(words: &[u32]) -> Result<(), String> {
    if words.len() < 5 {
        return Err(format!("{} words; a SPIR-V header alone is 5", words.len()));
    }
    if words[0] != MAGIC {
        return Err(format!("magic {:#010x}, expected {MAGIC:#010x}", words[0]));
    }
    let (major, minor) = ((words[1] >> 16) & 0xff, (words[1] >> 8) & 0xff);
    if major != 1 || minor > MAX_MINOR {
        return Err(format!(
            "SPIR-V {major}.{minor}; Vulkan 1.3 takes 1.0 to 1.{MAX_MINOR}"
        ));
    }
    Ok(())
}

/// A nul-terminated UTF-8 string packed little-endian into words.
fn literal(words: &[u32]) -> Option<String> {
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let end = bytes.iter().position(|b| *b == 0)?;
    String::from_utf8(bytes[..end].to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed(name: &str) -> Vec<u32> {
        let mut bytes = name.as_bytes().to_vec();
        bytes.resize((bytes.len() / 4 + 1) * 4, 0);
        bytes
            .chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    fn module(entries: &[(Model, &str)]) -> Vec<u32> {
        let mut words = vec![MAGIC, 0x0001_0500, 0, 100, 0];
        for (i, (model, name)) in entries.iter().enumerate() {
            let name = packed(name);
            let count = 3 + name.len() as u32;
            words.extend([count << 16 | OP_ENTRY_POINT, *model as u32, i as u32 + 1]);
            words.extend(name);
        }
        words
    }

    #[test]
    fn finds_entry_points_by_name_and_stage() {
        let words = module(&[(Model::Vertex, "vs"), (Model::Fragment, "fs_main")]);
        assert_eq!(check(&words, "vs", Model::Vertex), Ok(()));
        assert_eq!(check(&words, "fs_main", Model::Fragment), Ok(()));
        let wrong_stage = check(&words, "vs", Model::Fragment).unwrap_err();
        assert!(
            wrong_stage.contains("vs (model 0)") && wrong_stage.contains("fs_main"),
            "{wrong_stage}"
        );
        assert!(
            check(&words, "cs", Model::Compute)
                .unwrap_err()
                .contains("\"cs\"")
        );
    }

    #[test]
    fn malformed_modules_are_refused_with_a_reason() {
        let cases: [(Vec<u32>, &str); 4] = [
            (vec![0xdead_beef], "1 words"),
            (vec![0xdead_beef, 0x0001_0000, 0, 1, 0], "magic"),
            (vec![MAGIC, 0x0002_0000, 0, 1, 0], "SPIR-V 2.0"),
            (
                vec![MAGIC, 0x0001_0500, 0, 1, 0, 9 << 16 | OP_ENTRY_POINT],
                "bad length",
            ),
        ];
        for (words, reason) in cases {
            let log = check(&words, "main", Model::Compute).unwrap_err();
            assert!(log.contains(reason), "{log} should mention {reason}");
        }
        let zero = vec![MAGIC, 0x0001_0000, 0, 1, 0, 0];
        assert!(
            check(&zero, "main", Model::Compute)
                .unwrap_err()
                .contains("bad length 0")
        );
    }

    #[test]
    fn unterminated_names_are_refused() {
        let mut words = vec![MAGIC, 0x0001_0600, 0, 1, 0];
        words.extend([
            4 << 16 | OP_ENTRY_POINT,
            Model::Compute as u32,
            1,
            u32::from_le_bytes(*b"main"),
        ]);
        assert!(
            check(&words, "main", Model::Compute)
                .unwrap_err()
                .contains("terminated")
        );
    }
}
