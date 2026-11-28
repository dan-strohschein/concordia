//! Build-independent fingerprints of native modules (ADR-0072 amendment, 2026-11-27): a native
//! library's bytes differ between builds of the same source, so a native module is fingerprinted
//! by its description and its source folder's Rust files instead.

use std::path::Path;

/// BLAKE3 (hex) of a native module's description JSON and, when it names one, every `.rs` file
/// under its source folder (relative path and contents, in path order).
pub(crate) fn native_fingerprint(
    description: &[u8],
    source: Option<&Path>,
) -> Result<String, String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(description);
    if let Some(dir) = source {
        for file in concordia_script_native::module_source_files(dir).map_err(|e| e.to_string())? {
            let text = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let rel = file.strip_prefix(dir).unwrap_or(&file).to_string_lossy();
            let rel = rel.replace('\\', "/");
            for part in [rel.as_bytes(), &text] {
                hasher.update(&(part.len() as u64).to_le_bytes());
                hasher.update(part);
            }
        }
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::native_fingerprint;
    use std::path::Path;

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn source(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        files
            .iter()
            .for_each(|(rel, text)| write(dir.path(), rel, text));
        dir
    }

    const LIB: (&str, &str) = ("lib.rs", "pub fn f() {}\n");
    const SUB: (&str, &str) = ("sub/mod.rs", "pub fn g() {}\n");

    fn print(desc: &[u8], dir: &tempfile::TempDir) -> String {
        native_fingerprint(desc, Some(dir.path())).unwrap()
    }

    #[test]
    fn the_same_source_in_two_folders_has_one_fingerprint() {
        let (a, b) = (source(&[LIB, SUB]), source(&[SUB, LIB]));
        assert_eq!(print(b"{}", &a), print(b"{}", &b));
    }

    #[test]
    fn edits_renames_new_files_and_descriptions_change_it() {
        let base = print(b"{}", &source(&[LIB, SUB]));
        let changed = [
            print(b"{}", &source(&[("lib.rs", "pub fn f() { }\n"), SUB])),
            print(b"{}", &source(&[("main.rs", LIB.1), SUB])),
            print(b"{}", &source(&[LIB, SUB, ("extra.rs", "")])),
            print(b"{\"name\":\"x\"}", &source(&[LIB, SUB])),
        ];
        for (i, c) in changed.iter().enumerate() {
            assert_ne!(*c, base, "change {i}");
        }
    }

    #[test]
    fn other_files_do_not_count() {
        let mut dir = source(&[LIB, SUB]);
        let base = print(b"{}", &dir);
        write(dir.path(), "notes.md", "not code");
        assert_eq!(print(b"{}", &dir), base);
        dir = source(&[LIB, SUB]);
        assert_eq!(print(b"{}", &dir), base);
    }

    #[test]
    fn without_a_source_it_is_the_description_hash() {
        assert_eq!(
            native_fingerprint(b"{}", None).unwrap(),
            blake3::hash(b"{}").to_hex().to_string()
        );
    }

    #[test]
    fn a_missing_source_folder_is_an_error_naming_it() {
        let err = native_fingerprint(b"{}", Some(Path::new("/no/such/src"))).unwrap_err();
        assert!(err.contains("/no/such/src"), "{err}");
    }
}
