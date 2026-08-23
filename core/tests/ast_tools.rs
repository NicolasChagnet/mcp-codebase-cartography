//! Integration tests for AST symbol extraction and lookup tools.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use core::index::Engine;
use core::symbols::{SymbolError, get_symbol_definition};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temporary directory that is removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("core-ast-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, contents: &str) {
        let full = self.0.join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, contents).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn engine(tmp: &TempDir) -> Engine {
    Engine::new(tmp.path()).unwrap()
}

#[test]
fn missing_symbol_returns_not_found() {
    let tmp = TempDir::new();
    tmp.write("a.rs", "pub fn present() {}");

    let mut e = engine(&tmp);
    let err = get_symbol_definition(&mut e, "absent", None).unwrap_err();
    assert!(matches!(err, SymbolError::NotFound));
}

#[test]
fn ambiguous_symbol_across_files_errors() {
    let tmp = TempDir::new();
    tmp.write("a.rs", "pub fn dup() {}");
    tmp.write("b.rs", "pub fn dup() {}");

    let mut e = engine(&tmp);
    let err = get_symbol_definition(&mut e, "dup", None).unwrap_err();
    match err {
        SymbolError::Ambiguous { files } => {
            assert_eq!(files, vec!["a.rs".to_string(), "b.rs".to_string()]);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

#[test]
fn symbol_definition_returns_exact_source_range() {
    let tmp = TempDir::new();
    tmp.write(
        "lib.rs",
        r#"pub fn foo(x: i32) -> i32 {
    x + 1
}

pub fn bar() {}
"#,
    );

    let mut e = engine(&tmp);
    let def = get_symbol_definition(&mut e, "foo", None).unwrap();
    assert_eq!(def, "pub fn foo(x: i32) -> i32 {\n    x + 1\n}");
}

#[test]
fn symbol_definition_scoped_to_file() {
    let tmp = TempDir::new();
    tmp.write("a.rs", "pub fn shared() {}\npub fn other() {}");
    tmp.write("b.rs", "pub fn shared() {}");

    let mut e = engine(&tmp);
    // Without a file path this is ambiguous.
    assert!(matches!(
        get_symbol_definition(&mut e, "shared", None),
        Err(SymbolError::Ambiguous { .. })
    ));
    // Scoped to a file it resolves deterministically.
    let def = get_symbol_definition(&mut e, "shared", Some(&tmp.path().join("a.rs"))).unwrap();
    assert_eq!(def, "pub fn shared() {}");
}
