//! Codebase map tool: a structured directory tree of the indexed repository.

use std::collections::BTreeMap;

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

/// Kind of a node in the codebase map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapNodeKind {
    Dir,
    File,
}

/// A node in the structured codebase map.
///
/// `children` holds the expanded entries of a directory. When `max_depth`
/// prevents a directory from being expanded, `collapsed_entries` reports how
/// many entries it contains so callers can represent truncation explicitly.
#[derive(Debug, Clone)]
pub struct MapNode {
    pub name: String,
    pub path: String,
    pub kind: MapNodeKind,
    pub children: Vec<MapNode>,
    pub collapsed_entries: Option<usize>,
}

impl MapNode {
    fn dir(name: &str, path: &str) -> Self {
        MapNode {
            name: name.to_string(),
            path: path.to_string(),
            kind: MapNodeKind::Dir,
            children: Vec::new(),
            collapsed_entries: None,
        }
    }

    fn file(name: &str, path: &str) -> Self {
        MapNode {
            name: name.to_string(),
            path: path.to_string(),
            kind: MapNodeKind::File,
            children: Vec::new(),
            collapsed_entries: None,
        }
    }
}

/// Build the tree of root-relative paths, expanding directories up to
/// `max_depth` levels below the root. Deeper directories are collapsed to an
/// explicit entry count. Children are kept in a `BTreeMap` so the result is
/// deterministic (sorted by name).
fn build_tree(files: &[String], max_depth: usize) -> MapNode {
    // Intermediate tree keyed by name for deterministic (sorted) expansion.
    #[derive(Default)]
    struct Raw {
        is_dir: bool,
        children: BTreeMap<String, Raw>,
    }

    let mut root = Raw {
        is_dir: true,
        children: BTreeMap::new(),
    };
    for path in files {
        let mut parts = path.split('/').peekable();
        let mut cur = &mut root;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                cur.children.entry(part.to_string()).or_insert_with(|| Raw {
                    is_dir: false,
                    children: BTreeMap::new(),
                });
            } else {
                cur = cur.children.entry(part.to_string()).or_insert_with(|| Raw {
                    is_dir: true,
                    children: BTreeMap::new(),
                });
            }
        }
    }

    fn expand(raw: &Raw, name: &str, path: &str, depth: usize, max_depth: usize) -> MapNode {
        if !raw.is_dir {
            return MapNode::file(name, path);
        }
        let mut node = MapNode::dir(name, path);
        if depth < max_depth {
            node.children = raw
                .children
                .iter()
                .map(|(child_name, child)| {
                    let child_path = if path == "." {
                        child_name.clone()
                    } else {
                        format!("{path}/{child_name}")
                    };
                    expand(child, child_name, &child_path, depth + 1, max_depth)
                })
                .collect();
        } else if !raw.children.is_empty() {
            node.collapsed_entries = Some(raw.children.len());
        }
        node
    }

    expand(&root, ".", ".", 0, max_depth)
}

/// Return a structured directory tree of the repository, filtering out ignored
/// files (via the engine's `ignore`-based traversal). `max_depth` limits how
/// many directory levels are expanded; deeper directories are collapsed with
/// an explicit entry count.
pub fn get_codebase_map(engine: &Engine, max_depth: usize) -> Result<MapNode, TreeError> {
    let files = engine.list_files()?;
    let paths: Vec<String> = files.iter().map(|rec| rec.path.clone()).collect();
    Ok(build_tree(&paths, max_depth))
}
