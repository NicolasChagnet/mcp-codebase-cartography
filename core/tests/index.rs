//! Integration tests for the shared repository index and safe path layer.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use core::index::{Engine, ReadError};
use core::paths::PathError;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temporary directory that is removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "core-index-test-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn resolves_root_relative_path() {
    let tmp = TempDir::new();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    let file = tmp.path().join("src/lib.rs");
    fs::write(&file, "x").unwrap();

    let engine = Engine::new(tmp.path()).unwrap();
    assert_eq!(engine.resolve(&file).unwrap(), "src/lib.rs");
}

#[test]
fn rejects_paths_outside_root() {
    let tmp = TempDir::new();
    let engine = Engine::new(tmp.path()).unwrap();
    let outside = tmp.path().parent().unwrap();
    assert_eq!(engine.resolve(outside), Err(PathError::OutsideRoot));
}

#[test]
fn rejects_missing_paths() {
    let tmp = TempDir::new();
    let engine = Engine::new(tmp.path()).unwrap();
    assert_eq!(
        engine.resolve(&tmp.path().join("nope.rs")),
        Err(PathError::NotFound)
    );
}

#[test]
fn filters_ignored_directories() {
    let tmp = TempDir::new();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
    fs::create_dir_all(tmp.path().join("node_modules/pkg")).unwrap();
    fs::write(tmp.path().join("node_modules/pkg/index.js"), "x").unwrap();
    fs::create_dir_all(tmp.path().join(".git")).unwrap();
    fs::write(tmp.path().join(".git/config"), "x").unwrap();

    let engine = Engine::new(tmp.path()).unwrap();
    let files = engine.list_files().unwrap();
    let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["src/main.rs"]);
}

#[test]
fn lists_files_in_deterministic_order() {
    let tmp = TempDir::new();
    for name in ["zeta.rs", "alpha.rs", "mid.rs"] {
        fs::write(tmp.path().join(name), "x").unwrap();
    }

    let engine = Engine::new(tmp.path()).unwrap();
    let files = engine.list_files().unwrap();
    let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["alpha.rs", "mid.rs", "zeta.rs"]);
}

#[test]
fn cache_refreshes_after_edit() {
    let tmp = TempDir::new();
    let file = tmp.path().join("a.txt");
    fs::write(&file, "one").unwrap();

    let mut engine = Engine::new(tmp.path()).unwrap();
    assert_eq!(engine.read_file(&file).unwrap(), b"one");

    // Different length guarantees a metadata change.
    fs::write(&file, "two-three").unwrap();
    assert_eq!(engine.read_file(&file).unwrap(), b"two-three");
}

#[test]
fn rejects_binary_files() {
    let tmp = TempDir::new();
    let file = tmp.path().join("blob.bin");
    fs::write(&file, [0u8, 1, 2, 3]).unwrap();

    let mut engine = Engine::new(tmp.path()).unwrap();
    assert!(matches!(engine.read_file(&file), Err(ReadError::Binary)));
}

#[test]
fn stores_and_retrieves_symbol_refs() {
    let tmp = TempDir::new();
    let mut engine = Engine::new(tmp.path()).unwrap();
    engine.set_symbol_refs("foo", vec!["a.rs:1".into(), "b.rs:2".into()]);
    assert_eq!(
        engine.symbol_refs("foo").unwrap(),
        &["a.rs:1".to_string(), "b.rs:2".to_string()]
    );
    assert!(engine.symbol_refs("missing").is_none());
}
