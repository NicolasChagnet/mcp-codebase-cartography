//! Integration tests for AST symbol extraction and lookup tools.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use core::index::Engine;
use core::symbols::{get_file_outline, get_symbol_definition, SymbolError};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temporary directory that is removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "core-ast-test-{}-{n}",
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

fn engine(tmp: &TempDir) -> Engine {
    Engine::new(tmp.path()).unwrap()
}

#[test]
fn outline_rust_functions_structs_and_traits() {
    let tmp = TempDir::new();
    tmp.write(
        "src/lib.rs",
        r#"pub fn greet(name: &str) -> String {
    format!("hi {name}")
}

pub struct Point {
    pub x: i32,
}

impl Point {
    pub fn new(x: i32) -> Self {
        Point { x }
    }
}

pub trait Shape {
    fn area(&self) -> f64;
}
"#,
    );

    let mut e = engine(&tmp);
    let syms = get_file_outline(&mut e, &tmp.path().join("src/lib.rs")).unwrap();
    let kinds: Vec<_> = syms.iter().map(|s| (s.name.as_str(), s.kind.as_str())).collect();
    assert_eq!(
        kinds,
        vec![
            ("greet", "function"),
            ("Point", "struct"),
            ("new", "function"),
            ("Shape", "trait"),
            ("area", "function"),
        ]
    );
}

#[test]
fn outline_python_classes_and_methods() {
    let tmp = TempDir::new();
    tmp.write(
        "app.py",
        r#"def top_level():
    pass


class Greeter:
    """Greets people."""

    def __init__(self, name):
        self.name = name

    def hello(self):
        return f"hi {self.name}"
"#,
    );

    let mut e = engine(&tmp);
    let syms = get_file_outline(&mut e, &tmp.path().join("app.py")).unwrap();
    let kinds: Vec<_> = syms.iter().map(|s| (s.name.as_str(), s.kind.as_str())).collect();
    assert_eq!(
        kinds,
        vec![
            ("top_level", "function"),
            ("Greeter", "class"),
            ("__init__", "function"),
            ("hello", "function"),
        ]
    );
}

#[test]
fn outline_typescript_interfaces() {
    let tmp = TempDir::new();
    tmp.write(
        "types.ts",
        r#"export interface User {
    id: number;
    name: string;
}

export class Service {
    run(): void {}
}

export function helper(): void {}
"#,
    );

    let mut e = engine(&tmp);
    let syms = get_file_outline(&mut e, &tmp.path().join("types.ts")).unwrap();
    let kinds: Vec<_> = syms.iter().map(|s| (s.name.as_str(), s.kind.as_str())).collect();
    assert_eq!(
        kinds,
        vec![
            ("User", "interface"),
            ("Service", "class"),
            ("run", "method"),
            ("helper", "function"),
        ]
    );
}

#[test]
fn outline_nested_symbols_captured() {
    let tmp = TempDir::new();
    tmp.write(
        "nested.rs",
        r#"mod outer {
    pub struct Inner {
        pub value: i32,
    }

    impl Inner {
        pub fn get(&self) -> i32 {
            self.value
        }
    }
}
"#,
    );

    let mut e = engine(&tmp);
    let syms = get_file_outline(&mut e, &tmp.path().join("nested.rs")).unwrap();
    let kinds: Vec<_> = syms.iter().map(|s| (s.name.as_str(), s.kind.as_str())).collect();
    assert_eq!(
        kinds,
        vec![
            ("outer", "module"),
            ("Inner", "struct"),
            ("get", "function"),
        ]
    );
}

#[test]
fn docstrings_do_not_create_symbols() {
    let tmp = TempDir::new();
    tmp.write(
        "doc.py",
        r#""""Module docstring."""

def f():
    """Function docstring."""
    return 1
"#,
    );

    let mut e = engine(&tmp);
    let syms = get_file_outline(&mut e, &tmp.path().join("doc.py")).unwrap();
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].name, "f");
    assert_eq!(syms[0].kind, "function");
}

#[test]
fn unsupported_file_returns_error() {
    let tmp = TempDir::new();
    tmp.write("notes.txt", "just some text");

    let mut e = engine(&tmp);
    let err = get_file_outline(&mut e, &tmp.path().join("notes.txt")).unwrap_err();
    assert!(matches!(err, SymbolError::Unsupported));
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

#[test]
fn outline_reports_exact_line_ranges() {
    let tmp = TempDir::new();
    tmp.write(
        "span.rs",
        "pub fn a() {}\n\npub fn b() {\n    let x = 1;\n}\n",
    );

    let mut e = engine(&tmp);
    let syms = get_file_outline(&mut e, &tmp.path().join("span.rs")).unwrap();
    assert_eq!(syms[0].name, "a");
    assert_eq!((syms[0].line_start, syms[0].line_end), (1, 1));
    assert_eq!(syms[1].name, "b");
    assert_eq!((syms[1].line_start, syms[1].line_end), (3, 5));
}
