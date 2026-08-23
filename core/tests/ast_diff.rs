//! Integration tests for structural Git/JJ-aware AST diffs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use core::diff::{ChangeStatus, DiffError, get_ast_diff};
use core::index::Engine;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temporary directory that is removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("core-diff-{tag}-{}-{n}", std::process::id()));
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
    assert_eq!(
        got,
        vec![(ChangeStatus::Modified, "real".into(), "lib.rs".into())]
    );
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
    assert_eq!(
        got,
        vec![(ChangeStatus::Added, "fresh".into(), "new.rs".into())]
    );
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

    // Untracked source file whose content is binary (NUL bytes).
    fs::write(tmp.path().join("blob.rs"), [0u8, 1, 2, 3]).unwrap();

    let mut e = engine(&tmp);
    let err = get_ast_diff(&mut e, "HEAD").unwrap_err();
    match err {
        DiffError::Binary { file } => assert_eq!(file, "blob.rs"),
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
    // Snapshot the base into a commit; `@-` is now the committed base.
    run(&tmp, &["jj", "commit", "-m", "base"]);

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
fn jj_colocated_with_git_resolves_via_jj() {
    let tmp = TempDir::new("colocated");
    run(&tmp, &["jj", "git", "init"]);
    // `jj git init` colocates by default: both markers are present.
    assert!(tmp.path().join(".jj").exists());
    assert!(tmp.path().join(".git").exists());

    tmp.write("lib.rs", "pub fn foo() {}\n");
    run(&tmp, &["jj", "commit", "-m", "base"]);
    tmp.write("lib.rs", "pub fn bar() {}\n");

    // `@-` is a JJ revset; resolving it proves the JJ path was taken despite
    // the colocated `.git` marker.
    let got = changes(&tmp, "@-");
    assert!(got.contains(&(ChangeStatus::Deleted, "foo".into(), "lib.rs".into())));
    assert!(got.contains(&(ChangeStatus::Added, "bar".into(), "lib.rs".into())));
}

#[test]
fn jj_missing_executable_errors_clearly() {
    let tmp = TempDir::new("jjmissing");
    run(&tmp, &["jj", "git", "init"]);
    tmp.write("lib.rs", "pub fn foo() {}\n");
    run(&tmp, &["jj", "commit", "-m", "base"]);

    // Re-exec this test binary with a PATH that cannot find `jj`, running only
    // the child scenario. This simulates a missing `jj` without touching the
    // real environment.
    let exe = std::env::current_exe().unwrap();
    let empty_bin = TempDir::new("emptybin");
    let status = Command::new(exe)
        .args(["--exact", "jj_missing_executable_child"])
        .env("PATH", empty_bin.path())
        .env("JJ_MISSING_REPO", tmp.path())
        .status()
        .unwrap();
    assert!(status.success(), "child scenario failed");
}

#[test]
fn jj_missing_executable_child() {
    let Ok(repo) = std::env::var("JJ_MISSING_REPO") else {
        return; // not the re-executed child
    };
    let mut e = Engine::new(Path::new(&repo)).unwrap();
    let err = get_ast_diff(&mut e, "@-").unwrap_err();
    match err {
        DiffError::Command(msg) => {
            assert!(
                msg.contains("jj executable not found"),
                "unexpected message: {msg}"
            );
        }
        other => panic!("expected Command error, got {other:?}"),
    }
}

#[test]
fn no_vcs_errors() {
    let tmp = TempDir::new("novcs");
    tmp.write("lib.rs", "pub fn foo() {}\n");

    let mut e = engine(&tmp);
    let err = get_ast_diff(&mut e, "HEAD").unwrap_err();
    assert!(matches!(err, DiffError::NoVcs));
}
