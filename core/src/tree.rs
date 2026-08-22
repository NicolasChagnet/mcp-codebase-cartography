//! Codebase map tool: a compact directory tree of the indexed repository.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::index::Engine;

/// Errors from building the codebase map.
#[derive(Debug)]
pub enum TreeError {
    Io(std::io::Error),
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TreeError {}

impl From<std::io::Error> for TreeError {
    fn from(e: std::io::Error) -> Self {
        TreeError::Io(e)
    }
}

/// A node in the directory tree. Children are kept in a `BTreeMap` so the
/// rendered tree is deterministic (sorted by name).
#[derive(Debug, Default)]
struct Node {
    name: String,
    is_dir: bool,
    children: BTreeMap<String, Node>,
}

impl Node {
    fn dir(name: &str) -> Self {
        Node {
            name: name.to_string(),
            is_dir: true,
            children: BTreeMap::new(),
        }
    }

    fn file(name: &str) -> Self {
        Node {
            name: name.to_string(),
            is_dir: false,
            children: BTreeMap::new(),
        }
    }

    /// Insert a root-relative `/`-separated path into the tree.
    fn insert(&mut self, path: &str) {
        let mut parts = path.split('/').peekable();
        let mut cur = self;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                cur.children
                    .entry(part.to_string())
                    .or_insert_with(|| Node::file(part));
            } else {
                cur = cur
                    .children
                    .entry(part.to_string())
                    .or_insert_with(|| Node::dir(part));
            }
        }
    }
}

/// Render the tree as text, expanding directories up to `max_depth` levels
/// below the root. Deeper directories are collapsed to an entry count.
fn render(root: &Node, max_depth: usize) -> String {
    let mut out = String::new();
    out.push_str(&root.name);
    out.push('\n');
    render_into(root, 0, max_depth, "", &mut out);
    out
}

fn render_into(node: &Node, depth: usize, max_depth: usize, prefix: &str, out: &mut String) {
    let children: Vec<&Node> = node.children.values().collect();
    let n = children.len();
    for (i, child) in children.iter().enumerate() {
        let last = i + 1 == n;
        let connector = if last { "└── " } else { "├── " };
        let _ = write!(out, "{prefix}{connector}{}", child.name);
        if child.is_dir {
            out.push('/');
        }
        out.push('\n');
        let child_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
        if child.is_dir {
            if depth + 1 < max_depth {
                render_into(child, depth + 1, max_depth, &child_prefix, out);
            } else if !child.children.is_empty() {
                let _ = writeln!(
                    out,
                    "{child_prefix}└── ... ({} entries)",
                    child.children.len()
                );
            }
        }
    }
}

/// Return a compact directory tree of the repository, filtering out ignored
/// files (via the engine's `ignore`-based traversal). `max_depth` limits how
/// many directory levels are expanded; deeper directories are collapsed.
pub fn get_codebase_map(engine: &Engine, max_depth: usize) -> Result<String, TreeError> {
    let files = engine.list_files()?;
    let mut root = Node::dir(".");
    for rec in &files {
        root.insert(&rec.path);
    }
    Ok(render(&root, max_depth))
}
