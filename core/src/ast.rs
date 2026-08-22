//! Tree-sitter parsing and symbol extraction.
//!
//! Parses source with the grammar for a [`Language`] and normalizes declaration
//! nodes into [`Symbol`]s carrying a canonical kind and exact source span.

use tree_sitter::{Node, Parser, TreeCursor};

use crate::languages::Language;

/// A normalized symbol: a named declaration with its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    /// Canonical kind, e.g. `function`, `class`, `interface`, `method`.
    pub kind: String,
    /// 1-indexed first line of the declaration.
    pub line_start: usize,
    /// 1-indexed last line of the declaration.
    pub line_end: usize,
    /// Byte offset of the declaration start (for exact source slicing).
    pub start_byte: usize,
    /// Byte offset one past the declaration end.
    pub end_byte: usize,
}

/// Parse `source` and return all top-level and nested symbols in document order.
pub fn parse(lang: Language, source: &str) -> Vec<Symbol> {
    let mut parser = Parser::new();
    parser
        .set_language(&lang.grammar())
        .expect("valid tree-sitter grammar");
    let tree = parser.parse(source, None).expect("tree-sitter parse");
    let mut symbols = Vec::new();
    let mut cursor = tree.walk();
    collect(&mut cursor, source, &mut symbols);
    symbols
}

/// Map a tree-sitter node kind to a canonical symbol kind.
fn canonical_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "function_item"
        | "function_definition"
        | "function_declaration"
        | "function_signature"
        | "function_signature_item"
        | "generator_function_declaration"
        | "call_signature" => Some("function"),
        "method_definition" | "method_declaration" | "method_signature" => Some("method"),
        "class_definition"
        | "class_declaration"
        | "abstract_class_declaration"
        | "class_specifier" => Some("class"),
        "interface_declaration" => Some("interface"),
        "struct_item" | "struct_specifier" => Some("struct"),
        "enum_item" | "enum_declaration" | "enum_specifier" => Some("enum"),
        "trait_item" => Some("trait"),
        "mod_item" | "namespace_definition" => Some("module"),
        "type_item" | "type_alias_declaration" | "type_spec" => Some("type"),
        "const_item" => Some("const"),
        "static_item" => Some("static"),
        "macro_definition" => Some("macro"),
        "union_item" | "union_specifier" => Some("union"),
        "construct_signature" | "constructor_declaration" => Some("constructor"),
        "record_declaration" => Some("record"),
        "annotation_type_declaration" => Some("annotation"),
        _ => None,
    }
}

/// Depth-first walk collecting symbols, recursing into children so nested
/// declarations (methods inside classes, etc.) are captured.
fn collect(cursor: &mut TreeCursor, source: &str, out: &mut Vec<Symbol>) {
    loop {
        let node = cursor.node();
        if let Some(kind) = canonical_kind(node.kind())
            && let Some(name) = node_name(&node, source)
        {
            out.push(Symbol {
                name,
                kind: kind.to_string(),
                line_start: node.start_position().row + 1,
                line_end: node.end_position().row + 1,
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
            });
        }
        if cursor.goto_first_child() {
            collect(cursor, source, out);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Extract a declaration's name, preferring the `name` field and falling back
/// to the first identifier-like descendant outside any nested declaration.
fn node_name(node: &Node, source: &str) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        return text(&name, source);
    }
    first_name(node, source)
}

fn first_name(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        // Skip nested declarations so we don't pick up a child symbol's name.
        if canonical_kind(child.kind()).is_none() {
            if matches!(
                child.kind(),
                "identifier" | "field_identifier" | "type_identifier"
            ) {
                return text(&child, source);
            }
            if let Some(name) = first_name(&child, source) {
                return Some(name);
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    None
}

fn text(node: &Node, source: &str) -> Option<String> {
    node.utf8_text(source.as_bytes()).ok().map(str::to_string)
}
