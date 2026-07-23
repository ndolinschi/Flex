use std::path::Path;

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Class,
    Const,
    Interface,
    Enum,
    Heading,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    TypeScript,
    Tsx,
    JavaScript,
    Rust,
    Python,
    Go,
    Markdown,
    Unknown,
}

impl Language {
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("ts") => Language::TypeScript,
            Some("tsx") => Language::Tsx,
            Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => Language::JavaScript,
            Some("rs") => Language::Rust,
            Some("py") => Language::Python,
            Some("go") => Language::Go,
            Some("md") | Some("markdown") => Language::Markdown,
            _ => Language::Unknown,
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
            Language::JavaScript => "javascript",
            Language::Rust => "rust",
            Language::Python => "python",
            Language::Go => "go",
            Language::Markdown => "markdown",
            Language::Unknown => "unknown",
        }
    }
}

pub fn extract_symbols(rel_path: &str, source: &str) -> Vec<Symbol> {
    let language = Language::from_path(Path::new(rel_path));
    match language {
        Language::Markdown => extract_markdown_headings(rel_path, source),
        Language::Unknown => Vec::new(),
        _ => extract_via_tree_sitter(language, rel_path, source).unwrap_or_default(),
    }
}

fn ts_language(language: Language) -> Option<tree_sitter::Language> {
    match language {
        Language::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        Language::Tsx => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        Language::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),
        Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        Language::Python => Some(tree_sitter_python::LANGUAGE.into()),
        Language::Go => Some(tree_sitter_go::LANGUAGE.into()),
        Language::Markdown | Language::Unknown => None,
    }
}

fn extract_via_tree_sitter(
    language: Language,
    rel_path: &str,
    source: &str,
) -> Option<Vec<Symbol>> {
    let ts_lang = ts_language(language)?;
    let mut parser = Parser::new();
    parser.set_language(&ts_lang).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let mut symbols = Vec::new();
    walk_top_level(root, source.as_bytes(), rel_path, language, &mut symbols);
    Some(symbols)
}

fn walk_top_level(
    node: Node,
    src: &[u8],
    rel_path: &str,
    language: Language,
    out: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(sym) = classify_node(child, src, rel_path, language) {
            out.push(sym);
            continue;
        }
        if is_wrapper_node(child.kind(), language) {
            walk_top_level(child, src, rel_path, language, out);
        }
    }
}

fn is_wrapper_node(kind: &str, language: Language) -> bool {
    match language {
        Language::TypeScript | Language::Tsx | Language::JavaScript => {
            matches!(kind, "export_statement" | "program" | "declaration")
        }
        Language::Rust => matches!(kind, "impl_item" | "mod_item" | "declaration_list"),
        Language::Go | Language::Python => false,
        Language::Markdown | Language::Unknown => false,
    }
}

fn classify_node(node: Node, src: &[u8], rel_path: &str, language: Language) -> Option<Symbol> {
    let kind_str = node.kind();
    let (name, symbol_kind) = match language {
        Language::TypeScript | Language::Tsx | Language::JavaScript => {
            classify_ts_js(node, src, kind_str)?
        }
        Language::Rust => classify_rust(node, src, kind_str)?,
        Language::Python => classify_python(node, src, kind_str)?,
        Language::Go => classify_go(node, src, kind_str)?,
        Language::Markdown | Language::Unknown => return None,
    };

    Some(Symbol {
        name,
        kind: symbol_kind,
        path: rel_path.to_owned(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
    })
}

fn node_text<'a>(node: Node, src: &'a [u8]) -> Option<&'a str> {
    node.utf8_text(src).ok()
}

fn named_child_text<'a>(node: Node, field: &str, src: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name(field)
        .and_then(|n| node_text(n, src))
}

fn classify_ts_js(node: Node, src: &[u8], kind_str: &str) -> Option<(String, SymbolKind)> {
    match kind_str {
        "function_declaration" | "generator_function_declaration" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Function))
        }
        "class_declaration" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Class))
        }
        "interface_declaration" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Interface))
        }
        "lexical_declaration" | "variable_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declarator" {
                    let name = named_child_text(child, "name", src)?;
                    return Some((name.to_owned(), SymbolKind::Const));
                }
            }
            None
        }
        _ => None,
    }
}

fn classify_rust(node: Node, src: &[u8], kind_str: &str) -> Option<(String, SymbolKind)> {
    match kind_str {
        "function_item" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Function))
        }
        "struct_item" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Struct))
        }
        "enum_item" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Enum))
        }
        "trait_item" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Interface))
        }
        "const_item" | "static_item" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Const))
        }
        _ => None,
    }
}

fn classify_python(node: Node, src: &[u8], kind_str: &str) -> Option<(String, SymbolKind)> {
    match kind_str {
        "function_definition" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Function))
        }
        "class_definition" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Class))
        }
        _ => None,
    }
}

fn classify_go(node: Node, src: &[u8], kind_str: &str) -> Option<(String, SymbolKind)> {
    match kind_str {
        "function_declaration" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Function))
        }
        "method_declaration" => {
            let name = named_child_text(node, "name", src)?;
            Some((name.to_owned(), SymbolKind::Method))
        }
        "type_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "type_spec" {
                    let name = named_child_text(child, "name", src)?;
                    return Some((name.to_owned(), SymbolKind::Struct));
                }
            }
            None
        }
        "const_declaration" | "var_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "const_spec" || child.kind() == "var_spec" {
                    if let Some(name) = named_child_text(child, "name", src) {
                        return Some((name.to_owned(), SymbolKind::Const));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn extract_markdown_headings(rel_path: &str, source: &str) -> Vec<Symbol> {
    let lines: Vec<&str> = source.lines().collect();
    let mut headings = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if hashes == 0 || hashes > 6 {
            continue;
        }
        let rest = trimmed[hashes..].trim();
        if rest.is_empty() {
            continue;
        }
        headings.push((idx, rest.to_owned()));
    }

    let mut symbols = Vec::with_capacity(headings.len());
    for (i, (line_idx, title)) in headings.iter().enumerate() {
        let end_line = headings
            .get(i + 1)
            .map(|(next_idx, _)| *next_idx)
            .unwrap_or(lines.len());
        symbols.push(Symbol {
            name: title.clone(),
            kind: SymbolKind::Heading,
            path: rel_path.to_owned(),
            start_line: line_idx + 1,
            end_line: end_line.max(line_idx + 1),
        });
    }
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_rust_symbols() {
        let source = r#"
struct Foo {
    x: i32,
}

const MAX: i32 = 10;

fn bar() -> i32 {
    MAX
}

impl Foo {
    fn method(&self) -> i32 {
        self.x
    }
}
"#;
        let symbols = extract_symbols("src/lib.rs", source);
        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Foo"), "{names:?}");
        assert!(names.contains(&"MAX"), "{names:?}");
        assert!(names.contains(&"bar"), "{names:?}");
        assert!(names.contains(&"method"), "{names:?}");
    }

    #[test]
    fn extracts_typescript_symbols() {
        let source = r#"
export function greet(name: string): string {
    return `hi ${name}`;
}

export class Widget {
    render() {}
}

export const answer = 42;
"#;
        let symbols = extract_symbols("src/app.ts", source);
        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"greet"), "{names:?}");
        assert!(names.contains(&"Widget"), "{names:?}");
        assert!(names.contains(&"answer"), "{names:?}");
    }

    #[test]
    fn extracts_python_symbols() {
        let source = "\
def greet(name):\n    return f'hi {name}'\n\n\nclass Widget:\n    def render(self):\n        pass\n";
        let symbols = extract_symbols("app.py", source);
        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"greet"), "{names:?}");
        assert!(names.contains(&"Widget"), "{names:?}");
    }

    #[test]
    fn extracts_go_symbols() {
        let source = "\
package main\n\nfunc Greet(name string) string {\n\treturn name\n}\n\ntype Widget struct {\n\tX int\n}\n";
        let symbols = extract_symbols("main.go", source);
        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Greet"), "{names:?}");
        assert!(names.contains(&"Widget"), "{names:?}");
    }

    #[test]
    fn extracts_markdown_headings() {
        let source =
            "# Title\n\nSome intro.\n\n## Section One\n\nBody.\n\n## Section Two\n\nMore.\n";
        let symbols = extract_symbols("README.md", source);
        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Title", "Section One", "Section Two"]);
        assert!(matches!(symbols[0].kind, SymbolKind::Heading));
    }

    #[test]
    fn unknown_language_yields_no_symbols() {
        let symbols = extract_symbols("data.xyz", "whatever content");
        assert!(symbols.is_empty());
    }

    #[test]
    fn symbol_bounds_stay_within_file() {
        let source = "fn only() -> i32 {\n    1\n}\n";
        let symbols = extract_symbols("a.rs", source);
        assert_eq!(symbols.len(), 1);
        assert!(symbols[0].start_line >= 1);
        assert!(symbols[0].end_line <= source.lines().count());
    }
}
