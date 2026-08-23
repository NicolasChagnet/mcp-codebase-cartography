//! Symbol graph construction and traversal for reference analysis.
//!
//! Builds a directed graph from the parsed index: nodes are symbol
//! definitions, and an edge `A -> B` means symbol `A` references symbol `B`
//! (e.g. `A` calls `B`). Reference sites (identifier/call occurrences) are
//! retained with file/line/context metadata for upstream queries.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};
use tree_sitter::{Parser, TreeCursor};

use crate::ast::{self, Symbol};
use crate::index::Engine;
use crate::languages::Language;
use crate::refs::{CallerRecord, ImpactPath, RefError, RefSpot};

/// A definition node in the symbol graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolNode {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line_start: usize,
    pub line_end: usize,
}

/// A directed graph of symbol references.
pub struct SymbolGraph {
    /// node key (`file:name`) -> node index
    keys: HashMap<String, NodeIndex>,
    /// the directed graph: edge `A -> B` means `A` references `B`
    graph: DiGraph<SymbolNode, ()>,
    /// symbol name -> reference sites (identifier/call occurrences)
    ref_sites: HashMap<String, Vec<RefSpot>>,
}

/// A raw identifier/call occurrence in a file.
struct RawSite {
    name: String,
    byte_start: usize,
    byte_end: usize,
    line: usize,
}

impl SymbolGraph {
    /// Build the graph from all indexed files. Called lazily by
    /// [`Engine::graph`]; callers should go through the engine's cached
    /// accessor rather than building a graph per query.
    pub(crate) fn build(engine: &mut Engine) -> Result<Self, RefError> {
        let mut graph = SymbolGraph {
            keys: HashMap::new(),
            graph: DiGraph::new(),
            ref_sites: HashMap::new(),
        };

        // Pass 1: parse every file, register definition nodes, keep raw sites.
        let mut file_data: Vec<(String, Vec<Symbol>, Vec<RawSite>, String)> = Vec::new();
        for rec in engine.list_files()? {
            let Some(lang) = Language::from_path(&rec.path) else {
                continue;
            };
            let bytes = engine.read_file(Path::new(&rec.path))?;
            let source = String::from_utf8_lossy(&bytes).into_owned();
            let symbols = ast::parse(lang, &source);
            let sites = collect_sites(lang, &source);
            for sym in &symbols {
                let key = node_key(&rec.path, &sym.name);
                let idx = graph.graph.add_node(SymbolNode {
                    name: sym.name.clone(),
                    kind: sym.kind.clone(),
                    file: rec.path.clone(),
                    line_start: sym.line_start,
                    line_end: sym.line_end,
                });
                graph.keys.insert(key, idx);
            }
            file_data.push((rec.path, symbols, sites, source));
        }

        // Index defined names -> node keys for edge resolution.
        let mut name_to_keys: HashMap<String, Vec<String>> = HashMap::new();
        for key in graph.keys.keys() {
            let name = key.rsplit_once(':').unwrap().1.to_string();
            name_to_keys.entry(name).or_default().push(key.clone());
        }

        // Pass 2: build edges and reference sites.
        for (file, symbols, sites, source) in file_data {
            for sym in &symbols {
                let key = node_key(&file, &sym.name);
                let from = graph.keys[&key];
                let mut refs: Vec<String> = Vec::new();
                for site in &sites {
                    if site.byte_start >= sym.start_byte
                        && site.byte_end <= sym.end_byte
                        && !(site.byte_start >= sym.name_start_byte
                            && site.byte_end <= sym.name_end_byte)
                        && let Some(targets) = name_to_keys.get(&site.name)
                    {
                        refs.extend(targets.iter().cloned());
                    }
                }
                refs.sort();
                refs.dedup();
                refs.retain(|r| r != &key);
                for r in &refs {
                    let to = graph.keys[r];
                    graph.graph.add_edge(from, to, ());
                }
            }

            for site in &sites {
                let is_def_name = symbols.iter().any(|s| {
                    s.name == site.name
                        && site.byte_start >= s.name_start_byte
                        && site.byte_end <= s.name_end_byte
                });
                if is_def_name {
                    continue;
                }
                let context = source
                    .lines()
                    .nth(site.line - 1)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                graph
                    .ref_sites
                    .entry(site.name.clone())
                    .or_default()
                    .push(RefSpot {
                        file: file.clone(),
                        line: site.line,
                        context,
                    });
            }
        }

        Ok(graph)
    }

    /// Resolve a symbol key (`name` or `file:name`) to a canonical node key.
    pub fn resolve_key(&self, key: &str) -> Result<String, RefError> {
        if self.keys.contains_key(key) {
            return Ok(key.to_string());
        }
        if let Some((file, name)) = key.split_once(':') {
            let candidate = format!("{file}:{name}");
            if self.keys.contains_key(&candidate) {
                return Ok(candidate);
            }
        }
        let matches: Vec<&String> = self
            .keys
            .keys()
            .filter(|k| k.rsplit_once(':').map(|(_, n)| n == key).unwrap_or(false))
            .collect();
        match matches.len() {
            0 => Err(RefError::NotFound),
            1 => Ok(matches[0].clone()),
            _ => {
                let mut files: Vec<String> = matches
                    .iter()
                    .map(|k| k.rsplit_once(':').unwrap().0.to_string())
                    .collect();
                files.sort();
                files.dedup();
                Err(RefError::Ambiguous { files })
            }
        }
    }

    /// Bounded BFS over reverse edges: who references `key`, transitively.
    pub fn downstream(&self, key: &str, max_depth: usize) -> (Vec<CallerRecord>, Vec<ImpactPath>) {
        let mut callers: Vec<CallerRecord> = Vec::new();
        let mut paths: Vec<ImpactPath> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(key.to_string());

        let start = self.keys[key];
        let mut frontier: Vec<(NodeIndex, Vec<String>)> = vec![(start, vec![key.to_string()])];
        for depth in 1..=max_depth {
            let mut next: Vec<(NodeIndex, Vec<String>)> = Vec::new();
            for (node, path) in &frontier {
                for r in self.graph.neighbors_directed(*node, Direction::Incoming) {
                    let r_key = self.node_key_of(r);
                    if !visited.insert(r_key.clone()) {
                        continue;
                    }
                    let mut new_path = path.clone();
                    new_path.push(r_key.clone());
                    let info = &self.graph[r];
                    callers.push(CallerRecord {
                        symbol: info.name.clone(),
                        kind: info.kind.clone(),
                        file: info.file.clone(),
                        line_start: info.line_start,
                        line_end: info.line_end,
                        depth,
                    });
                    paths.push(ImpactPath {
                        path: new_path.clone(),
                    });
                    next.push((r, new_path));
                }
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }

        callers.sort_by(|a, b| {
            a.depth
                .cmp(&b.depth)
                .then(a.file.cmp(&b.file))
                .then(a.line_start.cmp(&b.line_start))
                .then(a.symbol.cmp(&b.symbol))
        });
        paths.sort_by(|a, b| a.path.len().cmp(&b.path.len()).then(a.path.cmp(&b.path)));
        (callers, paths)
    }

    /// All reference sites for a symbol name, in stable order.
    pub fn upstream_spots(&self, name: &str) -> Vec<RefSpot> {
        let mut spots = self.ref_sites.get(name).cloned().unwrap_or_default();
        spots.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        spots
    }

    /// Whether any definition has the given name.
    pub fn has_name(&self, name: &str) -> bool {
        self.graph.node_weights().any(|n| n.name == name)
    }

    /// Reconstruct the canonical node key (`file:name`) for a node index.
    fn node_key_of(&self, idx: NodeIndex) -> String {
        let node = &self.graph[idx];
        node_key(&node.file, &node.name)
    }
}

fn node_key(file: &str, name: &str) -> String {
    format!("{file}:{name}")
}

/// Collect all identifier-like nodes as raw reference sites.
fn collect_sites(lang: Language, source: &str) -> Vec<RawSite> {
    let mut parser = Parser::new();
    parser.set_language(&lang.grammar()).expect("valid grammar");
    let tree = parser.parse(source, None).expect("tree-sitter parse");
    let mut sites = Vec::new();
    let mut cursor = tree.walk();
    collect_sites_rec(&mut cursor, source, &mut sites);
    sites
}

fn collect_sites_rec(cursor: &mut TreeCursor, source: &str, out: &mut Vec<RawSite>) {
    loop {
        let node = cursor.node();
        if is_identifier_kind(node.kind())
            && let Some(name) = node.utf8_text(source.as_bytes()).ok()
        {
            out.push(RawSite {
                name: name.to_string(),
                byte_start: node.start_byte(),
                byte_end: node.end_byte(),
                line: node.start_position().row + 1,
            });
        }
        if cursor.goto_first_child() {
            collect_sites_rec(cursor, source, out);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn is_identifier_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "field_identifier"
            | "type_identifier"
            | "property_identifier"
            | "shorthand_property_identifier"
            | "variable_name"
    )
}
