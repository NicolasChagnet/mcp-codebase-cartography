//! Structural Git/JJ-aware AST diffs.
//!
//! Compares the working tree against a base revision by parsing both versions
//! of each changed file with the same language pack and diffing the resulting
//! symbols. Formatting-only edits (whitespace changes) are filtered out so only
//! meaningful added/modified/deleted functions and classes are reported.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::ast::{self, Symbol};
use crate::index::Engine;
use crate::languages::Language;

/// Status of a symbol-level change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Deleted,
    Modified,
}

/// A single symbol change with a concise summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolChange {
    pub status: ChangeStatus,
    pub name: String,
    pub kind: String,
    /// Root-relative file path.
    pub file: String,
    pub line_start: usize,
    pub line_end: usize,
    /// Concise human-readable summary.
    pub summary: String,
}

/// Errors from structural diffing.
#[derive(Debug)]
pub enum DiffError {
    /// No Git or JJ repository was found at the engine root.
    NoVcs,
    /// The requested base ref does not exist.
    InvalidRef(String),
    /// A changed file is binary and cannot be parsed.
    Binary { file: String },
    /// A file could not be read.
    File(String),
    /// A VCS command failed.
    Command(String),
}

impl std::fmt::Display for DiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffError::NoVcs => write!(f, "no Git or JJ repository found"),
            DiffError::InvalidRef(r) => write!(f, "invalid base ref: {r}"),
            DiffError::Binary { file } => write!(f, "binary file cannot be diffed: {file}"),
            DiffError::File(e) => write!(f, "{e}"),
            DiffError::Command(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DiffError {}

/// Which VCS backs the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vcs {
    Git,
    Jj,
}

/// Compare the working tree against `base_ref` and return symbol-level changes.
pub fn get_ast_diff(engine: &mut Engine, base_ref: &str) -> Result<Vec<SymbolChange>, DiffError> {
    let root = engine.root().to_path_buf();
    let vcs = detect_vcs(&root).ok_or(DiffError::NoVcs)?;
    if vcs == Vcs::Jj {
        ensure_jj_available(&root)?;
    }
    validate_ref(&root, vcs, base_ref)?;

    let base_files = list_base_files(&root, vcs, base_ref)?;
    let work_files: Vec<String> = engine
        .list_files()
        .map_err(|e| DiffError::File(e.to_string()))?
        .into_iter()
        .map(|r| r.path)
        .collect();

    // Union of base and working-tree files, in deterministic order.
    let mut files: Vec<String> = base_files.clone();
    for f in &work_files {
        if !files.contains(f) {
            files.push(f.clone());
        }
    }
    files.sort();

    let mut changes = Vec::new();
    for file in files {
        let Some(lang) = Language::from_path(&file) else {
            continue;
        };
        let base_bytes = if base_files.contains(&file) {
            materialize(&root, vcs, base_ref, &file)?
        } else {
            Vec::new()
        };
        let work_bytes = if work_files.contains(&file) {
            std::fs::read(root.join(&file)).map_err(|e| DiffError::File(e.to_string()))?
        } else {
            Vec::new()
        };
        // Skip unchanged files entirely.
        if base_bytes == work_bytes {
            continue;
        }
        if base_bytes.contains(&0) || work_bytes.contains(&0) {
            return Err(DiffError::Binary { file });
        }
        let base_src = String::from_utf8_lossy(&base_bytes).into_owned();
        let work_src = String::from_utf8_lossy(&work_bytes).into_owned();
        let base_syms = ast::parse(lang, &base_src);
        let work_syms = ast::parse(lang, &work_src);
        changes.extend(diff_symbols(
            &base_syms, &work_syms, &base_src, &work_src, &file,
        ));
    }
    Ok(changes)
}

/// Detect the VCS backing `root` by its marker directory.
///
/// Prefer JJ when both `.jj` and `.git` exist: a colocated repo (the default
/// for `jj git init`) is a Git repo underneath, but `@-` and friends are JJ
/// revsets that Git cannot resolve.
fn detect_vcs(root: &Path) -> Option<Vcs> {
    if root.join(".jj").exists() {
        Some(Vcs::Jj)
    } else if root.join(".git").exists() {
        Some(Vcs::Git)
    } else {
        None
    }
}

/// Ensure the `jj` executable is available before running any JJ command.
///
/// A missing executable is reported as a clear, structured error rather than a
/// generic "failed to run jj" surfaced mid-diff. Only called on the `Vcs::Jj`
/// path; Git repositories never probe for `jj`.
fn ensure_jj_available(root: &Path) -> Result<(), DiffError> {
    match Command::new("jj")
        .arg("--version")
        .current_dir(root)
        .output()
    {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(DiffError::Command(
            "jj executable not found; jj is required for jj-backed repositories".to_string(),
        )),
        Err(e) => Err(DiffError::Command(format!("failed to run jj: {e}"))),
    }
}

/// Verify `base_ref` resolves, returning a clear error otherwise.
fn validate_ref(root: &Path, vcs: Vcs, base_ref: &str) -> Result<(), DiffError> {
    let ok = match vcs {
        Vcs::Git => run(root, "git", &["rev-parse", "--verify", "--quiet", base_ref]).is_ok(),
        Vcs::Jj => run(root, "jj", &["log", "-r", base_ref, "--no-graph"]).is_ok(),
    };
    if ok {
        Ok(())
    } else {
        Err(DiffError::InvalidRef(base_ref.to_string()))
    }
}

/// List the files present in `base_ref`, one root-relative path per line.
fn list_base_files(root: &Path, vcs: Vcs, base_ref: &str) -> Result<Vec<String>, DiffError> {
    let out = match vcs {
        Vcs::Git => run(root, "git", &["ls-tree", "-r", "--name-only", base_ref])?,
        Vcs::Jj => run(root, "jj", &["file", "list", "-r", base_ref])?,
    };
    Ok(out.lines().map(str::to_string).collect())
}

/// Materialize the bytes of `file` at `base_ref`.
fn materialize(root: &Path, vcs: Vcs, base_ref: &str, file: &str) -> Result<Vec<u8>, DiffError> {
    match vcs {
        Vcs::Git => run_bytes(root, "git", &["show", &format!("{base_ref}:{file}")]),
        Vcs::Jj => run_bytes(root, "jj", &["file", "show", "-r", base_ref, file]),
    }
}

/// Run a command in `root`, returning stdout as UTF-8 text.
fn run(root: &Path, program: &str, args: &[&str]) -> Result<String, DiffError> {
    Ok(String::from_utf8_lossy(&run_bytes(root, program, args)?).into_owned())
}

/// Run a command in `root`, returning raw stdout bytes.
fn run_bytes(root: &Path, program: &str, args: &[&str]) -> Result<Vec<u8>, DiffError> {
    let out = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| DiffError::Command(format!("failed to run {program}: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(DiffError::Command(format!(
            "{program} {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(out.stdout)
}

/// Diff two symbol lists, pairing symbols by `(name, kind)` in document order.
fn diff_symbols(
    base: &[Symbol],
    work: &[Symbol],
    base_src: &str,
    work_src: &str,
    file: &str,
) -> Vec<SymbolChange> {
    let mut base_map: HashMap<(String, String), Vec<&Symbol>> = HashMap::new();
    for s in base {
        base_map
            .entry((s.name.clone(), s.kind.clone()))
            .or_default()
            .push(s);
    }
    let mut work_map: HashMap<(String, String), Vec<&Symbol>> = HashMap::new();
    for s in work {
        work_map
            .entry((s.name.clone(), s.kind.clone()))
            .or_default()
            .push(s);
    }

    let mut changes = Vec::new();
    // Deleted: base occurrences beyond what the working tree still has.
    for (key, base_list) in &base_map {
        let work_count = work_map.get(key).map_or(0, Vec::len);
        for (i, s) in base_list.iter().enumerate() {
            if i >= work_count {
                changes.push(SymbolChange {
                    status: ChangeStatus::Deleted,
                    name: s.name.clone(),
                    kind: s.kind.clone(),
                    file: file.to_string(),
                    line_start: s.line_start,
                    line_end: s.line_end,
                    summary: format!("deleted {} `{}`", s.kind, s.name),
                });
            }
        }
    }
    // Added / Modified: working-tree occurrences.
    for (key, work_list) in &work_map {
        let base_list = base_map.get(key).map_or(&[][..], |v| v.as_slice());
        for (i, s) in work_list.iter().enumerate() {
            if i >= base_list.len() {
                changes.push(SymbolChange {
                    status: ChangeStatus::Added,
                    name: s.name.clone(),
                    kind: s.kind.clone(),
                    file: file.to_string(),
                    line_start: s.line_start,
                    line_end: s.line_end,
                    summary: format!("added {} `{}`", s.kind, s.name),
                });
            } else {
                let b = base_list[i];
                let b_norm = normalize(&base_src[b.start_byte..b.end_byte]);
                let w_norm = normalize(&work_src[s.start_byte..s.end_byte]);
                if b_norm != w_norm {
                    changes.push(SymbolChange {
                        status: ChangeStatus::Modified,
                        name: s.name.clone(),
                        kind: s.kind.clone(),
                        file: file.to_string(),
                        line_start: s.line_start,
                        line_end: s.line_end,
                        summary: format!("modified {} `{}`", s.kind, s.name),
                    });
                }
            }
        }
    }
    changes
}

/// Strip all whitespace so formatting-only edits compare equal.
fn normalize(src: &str) -> String {
    src.chars().filter(|c| !c.is_whitespace()).collect()
}
