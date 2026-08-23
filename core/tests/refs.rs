//! Integration tests for the reference graph (upstream/downstream queries).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use core::refs::{RefError, get_downstream_refs, get_upstream_refs};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temporary directory that is removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("core-refs-test-{}-{n}", std::process::id()));
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

fn write(tmp: &TempDir, name: &str, content: &str) {
    fs::write(tmp.path().join(name), content).unwrap();
}

/// A chain `leaf <- mid <- root` where `mid` calls `leaf` and `root` calls `mid`.
fn chain(tmp: &TempDir) {
    write(tmp, "leaf.rs", "pub fn leaf() {}\n");
    write(tmp, "mid.rs", "pub fn mid() {\n    leaf();\n}\n");
    write(tmp, "root.rs", "pub fn root() {\n    mid();\n}\n");
}

#[test]
fn empty_graph_reports_not_found() {
    let tmp = TempDir::new();
    let mut engine = core::Engine::new(tmp.path()).unwrap();
    assert!(matches!(
        get_downstream_refs(&mut engine, "foo", 3),
        Err(RefError::NotFound)
    ));
    assert!(matches!(
        get_upstream_refs(&mut engine, "foo"),
        Err(RefError::NotFound)
    ));
}

#[test]
fn downstream_respects_depth_limit() {
    let tmp = TempDir::new();
    chain(&tmp);
    let mut engine = core::Engine::new(tmp.path()).unwrap();

    let shallow = get_downstream_refs(&mut engine, "leaf", 1).unwrap();
    let symbols: Vec<_> = shallow.callers.iter().map(|c| c.symbol.as_str()).collect();
    assert_eq!(symbols, vec!["mid"]);
    assert_eq!(shallow.callers[0].depth, 1);
    assert_eq!(shallow.paths.len(), 1);
    assert_eq!(shallow.paths[0].path, vec!["leaf.rs:leaf", "mid.rs:mid"]);

    let deep = get_downstream_refs(&mut engine, "leaf", 2).unwrap();
    let symbols: Vec<_> = deep.callers.iter().map(|c| c.symbol.as_str()).collect();
    assert_eq!(symbols, vec!["mid", "root"]);
    assert_eq!(deep.callers[1].depth, 2);
    assert_eq!(deep.paths.len(), 2);
    assert_eq!(
        deep.paths[1].path,
        vec!["leaf.rs:leaf", "mid.rs:mid", "root.rs:root"]
    );
}

#[test]
fn reverse_lookup_returns_empty_when_no_callers() {
    let tmp = TempDir::new();
    write(&tmp, "leaf.rs", "pub fn leaf() {}\n");
    write(&tmp, "mid.rs", "pub fn mid() {\n    leaf();\n}\n");
    let mut engine = core::Engine::new(tmp.path()).unwrap();

    // Nothing references `mid`, so downstream is empty.
    let res = get_downstream_refs(&mut engine, "mid", 3).unwrap();
    assert!(res.callers.is_empty());
    assert!(res.paths.is_empty());
}

#[test]
fn upstream_finds_reference_sites() {
    let tmp = TempDir::new();
    chain(&tmp);
    let mut engine = core::Engine::new(tmp.path()).unwrap();

    let spots = get_upstream_refs(&mut engine, "leaf").unwrap();
    assert_eq!(spots.len(), 1);
    assert_eq!(spots[0].file, "mid.rs");
    assert!(spots[0].context.contains("leaf"));
}

#[test]
fn downstream_is_deterministic() {
    let tmp = TempDir::new();
    chain(&tmp);
    let mut engine = core::Engine::new(tmp.path()).unwrap();

    let a = get_downstream_refs(&mut engine, "leaf", 2).unwrap();
    let b = get_downstream_refs(&mut engine, "leaf", 2).unwrap();
    assert_eq!(a, b);
}

#[test]
fn ambiguous_name_reports_error() {
    let tmp = TempDir::new();
    write(&tmp, "a.rs", "pub fn shared() {}\n");
    write(&tmp, "b.rs", "pub fn shared() {}\n");
    let mut engine = core::Engine::new(tmp.path()).unwrap();

    // A bare name defined in multiple files cannot be resolved to one key.
    assert!(matches!(
        get_downstream_refs(&mut engine, "shared", 2),
        Err(RefError::Ambiguous { .. })
    ));
}
