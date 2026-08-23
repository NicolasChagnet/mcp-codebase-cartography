//! Integration tests for the four basic exploration tools.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use core::compress::{CompressError, get_compressed_file};
use core::index::Engine;
use core::read::{ReadRangeError, read_file_range};
use core::search::{SearchError, search_codebase};
use core::tree::{MapNodeKind, get_codebase_map};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temporary directory that is removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("core-tree-test-{}-{n}", std::process::id()));
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
fn codebase_map_filters_by_depth() {
    let tmp = TempDir::new();
    tmp.write("a.rs", "x");
    tmp.write("src/b.rs", "x");
    tmp.write("src/deep/c.rs", "x");

    let e = engine(&tmp);
    let shallow = get_codebase_map(&e, 1).unwrap();
    let deep = get_codebase_map(&e, 2).unwrap();

    // max_depth=1 shows top-level entries and collapses src/.
    let src = shallow
        .children
        .iter()
        .find(|n| n.name == "src")
        .expect("src/ present");
    assert_eq!(src.kind, MapNodeKind::Dir);
    assert_eq!(src.collapsed_entries, Some(2));
    assert!(src.children.is_empty());
    assert!(shallow.children.iter().any(|n| n.name == "a.rs"));
    assert!(!shallow.children.iter().any(|n| n.name == "b.rs"));

    // max_depth=2 expands src/ but collapses src/deep/.
    let src = deep
        .children
        .iter()
        .find(|n| n.name == "src")
        .expect("src/ present");
    assert!(src.children.iter().any(|n| n.name == "b.rs"));
    let deep_dir = src
        .children
        .iter()
        .find(|n| n.name == "deep")
        .expect("deep/ present");
    assert_eq!(deep_dir.collapsed_entries, Some(1));
    assert!(!deep.children.iter().any(|n| n.name == "c.rs"));
}

#[test]
fn codebase_map_is_deterministic() {
    let tmp = TempDir::new();
    tmp.write("zeta.rs", "x");
    tmp.write("alpha.rs", "x");
    tmp.write("mid.rs", "x");

    let e = engine(&tmp);
    let out = get_codebase_map(&e, 2).unwrap();
    let names: Vec<&str> = out.children.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["alpha.rs", "mid.rs", "zeta.rs"]);
}

#[test]
fn compressed_file_strips_bodies() {
    let tmp = TempDir::new();
    tmp.write(
        "lib.rs",
        "use std::fmt;\n\npub fn greet(name: &str) -> String {\n    format!(\"hi {name}\")\n}\n\npub struct Point {\n    pub x: i32,\n}\n",
    );

    let mut e = engine(&tmp);
    let out = get_compressed_file(&mut e, &tmp.path().join("lib.rs")).unwrap();

    assert!(out.contains("use std::fmt;"));
    assert!(out.contains("pub fn greet(name: &str) -> String {"));
    assert!(out.contains("[Body hidden: 1 lines]"));
    assert!(out.contains("pub struct Point {"));
    // Body logic is stripped.
    assert!(!out.contains("format!"));
}

#[test]
fn compressed_file_keeps_closing_delimiters() {
    let tmp = TempDir::new();
    tmp.write(
        "lib.rs",
        "pub fn empty() {\n}\n\npub fn one_line() { return 1; }\n\npub fn multi() {\n    let x = 1;\n    x + 1\n}\n\npub fn trailing() {\n    let x = 1; }\n",
    );

    let mut e = engine(&tmp);
    let out = get_compressed_file(&mut e, &tmp.path().join("lib.rs")).unwrap();

    // Multi-line bodies keep their closing brace after the hidden marker, and
    // the hidden count reflects the interior (not the declaration span).
    assert!(out.contains("pub fn empty() {   // [Body hidden: 0 lines]\n  }"));
    assert!(out.contains("pub fn multi() {   // [Body hidden: 2 lines]\n  }"));
    // A closing brace sharing its line with the last statement is still kept.
    assert!(out.contains("pub fn trailing() {   // [Body hidden: 1 lines]\n  }"));
    // One-line bodies already carry both delimiters on the signature line.
    assert!(out.contains("pub fn one_line() { return 1; }   // [Body hidden: 0 lines]"));
}

#[test]
fn compressed_file_rejects_unsupported() {
    let tmp = TempDir::new();
    tmp.write("notes.txt", "just text");

    let mut e = engine(&tmp);
    let err = get_compressed_file(&mut e, &tmp.path().join("notes.txt")).unwrap_err();
    assert!(matches!(err, CompressError::Unsupported));
}

#[test]
fn read_file_range_uses_relative_numbers() {
    let tmp = TempDir::new();
    tmp.write("a.txt", "one\ntwo\nthree\nfour\n");

    let mut e = engine(&tmp);
    let out = read_file_range(&mut e, &tmp.path().join("a.txt"), 2, 3).unwrap();
    assert_eq!(out, "1: two\n2: three\n");
}

#[test]
fn read_file_range_clamps_end_line() {
    let tmp = TempDir::new();
    tmp.write("a.txt", "one\ntwo\n");

    let mut e = engine(&tmp);
    let out = read_file_range(&mut e, &tmp.path().join("a.txt"), 1, 99).unwrap();
    assert_eq!(out, "1: one\n2: two\n");
}

#[test]
fn read_file_range_rejects_invalid_bounds() {
    let tmp = TempDir::new();
    tmp.write("a.txt", "one\ntwo\n");

    let mut e = engine(&tmp);
    assert!(matches!(
        read_file_range(&mut e, &tmp.path().join("a.txt"), 0, 2),
        Err(ReadRangeError::InvalidRange)
    ));
    assert!(matches!(
        read_file_range(&mut e, &tmp.path().join("a.txt"), 3, 2),
        Err(ReadRangeError::InvalidRange)
    ));
    assert!(matches!(
        read_file_range(&mut e, &tmp.path().join("a.txt"), 5, 6),
        Err(ReadRangeError::InvalidRange)
    ));
}

#[test]
fn search_rejects_invalid_regex() {
    let tmp = TempDir::new();
    tmp.write("a.rs", "fn main() {}");

    let mut e = engine(&tmp);
    let err = search_codebase(&mut e, "(", None, 10).unwrap_err();
    assert!(matches!(err, SearchError::InvalidRegex(_)));
}

#[test]
fn search_rejects_invalid_extension() {
    let tmp = TempDir::new();
    tmp.write("a.rs", "fn main() {}");

    let mut e = engine(&tmp);
    let err = search_codebase(&mut e, "fn", Some("a/b"), 10).unwrap_err();
    assert!(matches!(err, SearchError::InvalidExtension));
}

#[test]
fn search_respects_max_results() {
    let tmp = TempDir::new();
    for i in 0..5 {
        tmp.write(&format!("f{i}.rs"), "fn foo() {}\nfn foo() {}\n");
    }

    let mut e = engine(&tmp);
    let results = search_codebase(&mut e, "foo", None, 3).unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn search_respects_gitignore() {
    let tmp = TempDir::new();
    tmp.write(".gitignore", "ignored.rs\n");
    tmp.write("keep.rs", "fn foo() {}");
    tmp.write("ignored.rs", "fn foo() {}");

    let mut e = engine(&tmp);
    let results = search_codebase(&mut e, "foo", None, 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file, "keep.rs");
}

#[test]
fn search_filters_by_extension() {
    let tmp = TempDir::new();
    tmp.write("a.rs", "fn foo() {}");
    tmp.write("b.py", "def foo(): pass");

    let mut e = engine(&tmp);
    let results = search_codebase(&mut e, "foo", Some("py"), 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file, "b.py");
}

#[test]
fn search_output_is_deterministic() {
    let tmp = TempDir::new();
    tmp.write("b.rs", "fn foo() {}");
    tmp.write("a.rs", "fn foo() {}");

    let mut e = engine(&tmp);
    let r1 = search_codebase(&mut e, "foo", None, 10).unwrap();
    let r2 = search_codebase(&mut e, "foo", None, 10).unwrap();
    assert_eq!(r1, r2);
    // Sorted by file path, then line.
    assert_eq!(r1[0].file, "a.rs");
    assert_eq!(r1[1].file, "b.rs");
}
