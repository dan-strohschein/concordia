//! Checks a SPIR-V module before a driver sees it (pure): drivers may crash on malformed code.

use std::collections::{BTreeMap, BTreeSet};

/// The SPIR-V magic number.
const MAGIC: u32 = 0x0723_0203;
/// `OpEntryPoint`.
const OP_ENTRY_POINT: u32 = 15;
/// `OpTypePointer`.
const OP_TYPE_POINTER: u32 = 32;
/// `OpVariable`.
const OP_VARIABLE: u32 = 59;
/// `OpDecorate`.
const OP_DECORATE: u32 = 71;
/// Storage class `Function`.
const FUNCTION: u32 = 7;
/// Storage class `PhysicalStorageBuffer`.
const PHYSICAL_STORAGE_BUFFER: u32 = 5349;
/// Decoration `RestrictPointer`.
const RESTRICT_POINTER: u32 = 5355;
/// Decoration `AliasedPointer`.
const ALIASED_POINTER: u32 = 5356;
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

/// Ok if `words` is a SPIR-V module (≤ 1.6) with entry point `entry` for `model` and passes
/// [`check_module`]; else the log.
pub(crate) fn check(words: &[u32], entry: &str, model: Model) -> Result<(), String> {
    let facts = scan(words)?;
    facts.pointer_rule()?;
    if facts
        .entries
        .iter()
        .any(|(m, name)| *m == model as u32 && name == entry)
    {
        return Ok(());
    }
    let found: Vec<String> = facts
        .entries
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

/// The module-wide rules [`check`] applies (header, framing, pointer decorations), for checking
/// shaders without a device (render tests compile every entry point for SPIR-V).
pub fn check_module(words: &[u32]) -> Result<(), String> {
    scan(words)?.pointer_rule()
}

/// What one pass over the module finds.
#[derive(Default)]
struct Facts {
    /// Every `(execution model, name)` declared.
    entries: Vec<(u32, String)>,
    /// `OpTypePointer` result ids: storage class and pointee.
    pointers: BTreeMap<u32, (u32, u32)>,
    /// Function-storage `OpVariable`s: result id and type.
    function_variables: Vec<(u32, u32)>,
    /// Ids decorated `AliasedPointer` or `RestrictPointer`.
    pointer_decorated: BTreeSet<u32>,
}

impl Facts {
    /// A function variable holding a physical-storage-buffer pointer must say how it aliases.
    fn pointer_rule(&self) -> Result<(), String> {
        let holds_physical = |ty: u32| {
            self.pointers.get(&ty).is_some_and(|(_, pointee)| {
                self.pointers
                    .get(pointee)
                    .is_some_and(|(class, _)| *class == PHYSICAL_STORAGE_BUFFER)
            })
        };
        let bad: Vec<String> = self
            .function_variables
            .iter()
            .filter(|(id, ty)| holds_physical(*ty) && !self.pointer_decorated.contains(id))
            .map(|(id, _)| format!("%{id}"))
            .collect();
        match bad.is_empty() {
            true => Ok(()),
            false => Err(format!(
                "function variables {} hold physical-storage-buffer pointers without \
                 AliasedPointer or RestrictPointer (spirv-val, VUID-VkShaderModuleCreateInfo-pCode-08737); \
                 initialise such pointers where they are declared",
                bad.join(", ")
            )),
        }
    }
}

/// Checks the header and framing and gathers [`Facts`].
fn scan(words: &[u32]) -> Result<Facts, String> {
    header(words)?;
    let mut facts = Facts::default();
    let mut at = 5;
    while at < words.len() {
        let count = (words[at] >> 16) as usize;
        let op = words[at] & 0xffff;
        if count == 0 || at + count > words.len() {
            return Err(format!("instruction at word {at} has a bad length {count}"));
        }
        let args = &words[at + 1..at + count];
        match (op, args) {
            (OP_ENTRY_POINT, [model, _, name @ ..]) if !name.is_empty() => {
                let name = literal(name)
                    .ok_or_else(|| format!("entry point at word {at} has no terminated name"))?;
                facts.entries.push((*model, name));
            }
            (OP_TYPE_POINTER, [id, class, pointee]) => {
                facts.pointers.insert(*id, (*class, *pointee));
            }
            (OP_VARIABLE, [ty, id, FUNCTION, ..]) => facts.function_variables.push((*id, *ty)),
            (OP_DECORATE, [id, ALIASED_POINTER | RESTRICT_POINTER, ..]) => {
                facts.pointer_decorated.insert(*id);
            }
            _ => {}
        }
        at += count;
    }
    Ok(facts)
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

    /// `OpTypePointer PhysicalStorageBuffer` %2 to %1, `OpTypePointer Function` %3 to %2, and
    /// `OpVariable` %4 of type %3; `decoration` adds `OpDecorate %4 <decoration>`.
    fn pointer_variable(decoration: Option<u32>) -> Vec<u32> {
        let mut words = module(&[(Model::Fragment, "fs")]);
        words.extend([4 << 16 | OP_TYPE_POINTER, 2, PHYSICAL_STORAGE_BUFFER, 1]);
        words.extend([4 << 16 | OP_TYPE_POINTER, 3, FUNCTION, 2]);
        if let Some(d) = decoration {
            words.extend([3 << 16 | OP_DECORATE, 4, d]);
        }
        words.extend([4 << 16 | OP_VARIABLE, 3, 4, FUNCTION]);
        words
    }

    /// spirv-val's rule (VUID-VkShaderModuleCreateInfo-pCode-08737): a function variable holding a
    /// physical-storage-buffer pointer is `AliasedPointer` or `RestrictPointer`. Slang 2026.22
    /// sometimes leaves it out; drivers may mis-handle it, so the backend refuses the module.
    #[test]
    fn undecorated_physical_pointer_variables_are_refused() {
        let log = check(&pointer_variable(None), "fs", Model::Fragment).unwrap_err();
        assert!(
            log.contains("%4") && log.contains("AliasedPointer"),
            "{log}"
        );
        for decoration in [ALIASED_POINTER, RESTRICT_POINTER] {
            assert_eq!(
                check(&pointer_variable(Some(decoration)), "fs", Model::Fragment),
                Ok(())
            );
        }
    }
}
