//! Tree-sitter language pack: extension mapping and grammar selection.

use tree_sitter::Language as TsLanguage;

/// A supported source language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    Tsx,
    JavaScript,
    Go,
    C,
    Cpp,
    Java,
}

impl Language {
    /// Map a root-relative file path to a supported language by extension.
    /// Returns `None` for unsupported or extension-less files.
    pub fn from_path(path: &str) -> Option<Language> {
        let ext = path.rsplit('.').next()?.to_ascii_lowercase();
        match ext.as_str() {
            "rs" => Some(Language::Rust),
            "py" => Some(Language::Python),
            "ts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "js" | "mjs" | "cjs" | "jsx" => Some(Language::JavaScript),
            "go" => Some(Language::Go),
            "c" | "h" => Some(Language::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hh" => Some(Language::Cpp),
            "java" => Some(Language::Java),
            _ => None,
        }
    }

    /// The tree-sitter grammar for this language.
    pub fn grammar(&self) -> TsLanguage {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Language::Go => tree_sitter_go::LANGUAGE.into(),
            Language::C => tree_sitter_c::LANGUAGE.into(),
            Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Language::Java => tree_sitter_java::LANGUAGE.into(),
        }
    }
}
