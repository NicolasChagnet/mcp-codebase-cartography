//! Integration tests for structural Git/JJ-aware AST diffs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use core::diff::{get_ast_diff, ChangeStatus, DiffError};
use core::index::Engine;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temporary directory that is removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "core-diff-{tag}-{}-{n}",
            std::process::id()
        ));
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

fn run(tmp: &TempDir, args: &[&str]) {
    let status = Command::new(args[0])
        .args(&args[1..])
        .current_dir(tmp.path())
        .status()
        .unwrap();
    assert!(status.success(), "command {args:?} failed");
}

fn git_init(tmp: &TempDir) {
    run(tmp, &["git", "init", "-q"]);
    run(tmp, &["git", "config", "user.email", "test@example.com"]);
    run(tmp, &["git", "config", "user.name", "Test"]);
}

fn git_commit(tmp: &TempDir, msg: &str) {
    run(tmp, &["git", "add", "-A"]);
    run(tmp, &["git", "commit", "-q", "-m", msg]);
}

fn engine(tmp: &TempDir) -> Engine {
    Engine::new(tmp.path()).unwrap()
}

fn changes(tmp: &TempDir, base_ref: &str) -> Vec<(ChangeStatus, String, String)> {
    let mut e = engine(tmp);
    get_ast_diff(&mut e, base_ref)
        .unwrap()
        .into_iter()
        .map(|c| (c.status, c.name, c.file))
        .collect()
}

#[test]
fn git_reports_added_deleted_modified() {
    let tmp = TempDir::new("git");
    git_init(&tmp);
    tmp.write(
        "lib.rs",
        "pub fn foo() {}\n\npub fn bar() {\n    let x = 1;\n}\n",
    );
    git_commit(&tmp, "base");

    tmp.write(
        "lib.rs",
        "pub fn bar() {\n    let x = 2;\n}\n\npub fn baz() {}\n",
    );

    let got = changes(&tmp, "HEAD");
    assert!(got.contains(&(ChangeStatus::Deleted, "foo".into(), "lib.rs".into())));
    assert!(got.contains(&(ChangeStatus::Modified, "bar".into(), "lib.rs".into())));
    assert!(got.contains(&(ChangeStatus::Added, "baz".into(), "lib.rs".into())));
    assert_eq!(got.len(), 3);
}

#[test]
fn git_ignores_formatting_only_edits() {
    let tmp = TempDir::new("fmt");
    git_init(&tmp);
    tmp.write(
        "lib.rs",
        "pub fn fmt() {\n    let x = 1;\n}\n\npub fn real() {\n    let y = 1;\n}\n",
    );
    git_commit(&tmp, "base");

    // Reindent `fmt` (whitespace only) and change `real`'s body.
    tmp.write(
        "lib.rs",
        "pub fn fmt() {\n  let x = 1;\n}\n\npub fn real() {\n    let y = 2;\n}\n",
    );

    let got = changes(&tmp, "HEAD");
    assert_eq!(got, vec![(ChangeStatus::Modified, "real".into(), "lib.rs".into())]);
}

#[test]
fn git_reports_untracked_files_as_added() {
    let tmp = TempDir::new("untracked");
    git_init(&tmp);
    tmp.write("keep.rs", "pub fn keep() {}\n");
    git_commit(&tmp, "base");

    // New file never committed to git.
    tmp.write("new.rs", "pub fn fresh() {}\n");

    let got = changes(&tmp, "HEAD");
    assert_eq!(got, vec![(ChangeStatus::Added, "fresh".into(), "new.rs".into())]);
}

#[test]
fn git_invalid_ref_errors() {
    let tmp = TempDir::new("badref");
    git_init(&tmp);
    tmp.write("lib.rs", "pub fn foo() {}\n");
    git_commit(&tmp, "base");

    let mut e = engine(&tmp);
    let err = get_ast_diff(&mut e, "does-not-exist").unwrap_err();
    assert!(matches!(err, DiffError::InvalidRef(_)));
}

#[test]
fn git_binary_file_errors() {
    let tmp = TempDir::new("binary");
    git_init(&tmp);
    tmp.write("lib.rs", "pub fn foo() {}\n");
    git_commit(&tmp, "base");

    // Untracked binary file with NUL bytes.
    fs::write(tmp.path().join("blob.bin"), [0u8, 1, 2, 3]).unwrap();

    let mut e = engine(&tmp);
    let err = get_ast_diff(&mut e, "HEAD").unwrap_err();
    match err {
        DiffError::Binary { file } => assert_eq!(file, "blob.bin"),
        other => panic!("expected Binary, got {other:?}"),
    }
}

#[test]
fn jj_fallback_reports_symbol_changes() {
    let tmp = TempDir::new("jj");
    run(&tmp, &["jj", "git", "init"]);
    tmp.write(
        "lib.rs",
        "pub fn foo() {}\n\npub fn bar() {\n    let x = 1;\n}\n",
    );
    run(&tmp, &["jj", "describe", "-m", "base"]);

    // Edit the working copy; `@-` is the committed base.
    tmp.write(
        "lib.rs",
        "pub fn bar() {\n    let x = 2;\n}\n\npub fn baz() {}\n",
    );

    let got = changes(&tmp, "@-");
    assert!(got.contains(&(ChangeStatus::Deleted, "foo".into(), "lib.rs".into())));
    assert!(got.contains(&(ChangeStatus::Modified, "bar".into(), "lib.rs".into())));
    assert!(got.contains(&(ChangeStatus::Added, "baz".into(), "lib.rs".into())));
    assert_eq!(got.len(), 3);
}

#[test]
fn no_vcs_errors() {
    let tmp = TempDir::new("novcs");
    tmp.write("lib.rs", "pub fn foo() {}\n");

    let mut e = engine(&tmp);
    let err = get_ast_diff(&mut e, "HEAD").unwrap_err();
    assert!(matches!(err, DiffError::NoVcs));
}
