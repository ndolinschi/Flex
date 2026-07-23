use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::IndexStore;
use crate::symbols::{Language, Symbol};

const REPOMAP_CACHE_FILE: &str = "repomap.cache.json";

const DAMPING: f64 = 0.85;
const PAGERANK_ITERS: usize = 20;
const CHARS_PER_TOKEN: usize = 4;
const MAX_SYMBOLS_PER_FILE: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RepoMapCache {
    fingerprint: String,
    budget: usize,
    map: String,
}

pub fn build_repo_map(store: &IndexStore, token_budget: usize) -> String {
    build_repo_map_uncached(store, token_budget)
}

pub fn build_repo_map_cached(store: &IndexStore, token_budget: usize) -> (String, usize, bool) {
    let file_count = store.indexed_file_count();
    if file_count == 0 {
        return (
            "Repo map: no indexed files yet. Call again after the index builds.".to_owned(),
            0,
            false,
        );
    }

    let fingerprint = store.manifest_fingerprint();
    let cache_path = store.index_dir().join(REPOMAP_CACHE_FILE);
    if let Some(cached) = read_cache(&cache_path) {
        if cached.fingerprint == fingerprint && cached.budget == token_budget {
            return (cached.map, file_count, true);
        }
    }

    let map = build_repo_map_uncached(store, token_budget);
    let _ = write_cache(
        &cache_path,
        &RepoMapCache {
            fingerprint,
            budget: token_budget,
            map: map.clone(),
        },
    );
    (map, file_count, false)
}

fn read_cache(path: &Path) -> Option<RepoMapCache> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_cache(path: &Path, cache: &RepoMapCache) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(cache).map_err(std::io::Error::other)?;
    fs::write(path, bytes)
}

fn build_repo_map_uncached(store: &IndexStore, token_budget: usize) -> String {
    let budget_chars = token_budget.saturating_mul(CHARS_PER_TOKEN).max(200);
    let paths: Vec<String> = store.indexed_paths().map(str::to_owned).collect();
    if paths.is_empty() {
        return "Repo map: no indexed files yet. Call again after the index builds.".to_owned();
    }

    let path_set: HashSet<&str> = paths.iter().map(String::as_str).collect();
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    for path in &paths {
        edges.insert(path.clone(), Vec::new());
    }

    for path in &paths {
        let abs = store.repo_root().join(path);
        let Ok(source) = fs::read_to_string(&abs) else {
            continue;
        };
        let language = Language::from_path(Path::new(path));
        for target in extract_imports(path, &source, language, &path_set) {
            if let Some(outs) = edges.get_mut(path) {
                if !outs.contains(&target) {
                    outs.push(target);
                }
            }
        }
    }

    let symbol_boost = symbol_counts(store.symbols());
    let scores = pagerank(&paths, &edges, &symbol_boost);

    let mut ranked: Vec<(&String, f64)> = paths
        .iter()
        .map(|p| (p, *scores.get(p.as_str()).unwrap_or(&0.0)))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    let symbols_by_path = group_symbols(store.symbols());
    render_map(&ranked, &symbols_by_path, budget_chars)
}

fn symbol_counts(symbols: &[Symbol]) -> HashMap<String, f64> {
    let mut counts: HashMap<String, f64> = HashMap::new();
    for sym in symbols {
        *counts.entry(sym.path.clone()).or_insert(0.0) += 1.0;
    }
    counts
}

fn group_symbols(symbols: &[Symbol]) -> HashMap<&str, Vec<&Symbol>> {
    let mut map: HashMap<&str, Vec<&Symbol>> = HashMap::new();
    for sym in symbols {
        map.entry(sym.path.as_str()).or_default().push(sym);
    }
    for list in map.values_mut() {
        list.sort_by(|a, b| {
            a.start_line
                .cmp(&b.start_line)
                .then_with(|| a.name.cmp(&b.name))
        });
    }
    map
}

fn pagerank(
    nodes: &[String],
    edges: &HashMap<String, Vec<String>>,
    symbol_boost: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    let n = nodes.len().max(1) as f64;
    let mut rank: HashMap<String, f64> = nodes.iter().map(|p| (p.clone(), 1.0 / n)).collect();

    let mut inbound: HashMap<&str, Vec<&str>> = HashMap::new();
    for (src, outs) in edges {
        for dst in outs {
            inbound.entry(dst.as_str()).or_default().push(src.as_str());
        }
    }

    for _ in 0..PAGERANK_ITERS {
        let mut next: HashMap<String, f64> = HashMap::new();
        for node in nodes {
            let mut sum = 0.0;
            if let Some(sources) = inbound.get(node.as_str()) {
                for src in sources {
                    let out_deg = edges.get(*src).map(Vec::len).unwrap_or(0).max(1) as f64;
                    sum += rank.get(*src).copied().unwrap_or(0.0) / out_deg;
                }
            }
            let boost = 1.0 + (symbol_boost.get(node).copied().unwrap_or(0.0) * 0.02);
            next.insert(node.clone(), (1.0 - DAMPING) / n + DAMPING * sum * boost);
        }
        let total: f64 = next.values().sum::<f64>().max(f64::EPSILON);
        for value in next.values_mut() {
            *value /= total;
        }
        rank = next;
    }
    rank
}

fn render_map(
    ranked: &[(&String, f64)],
    symbols_by_path: &HashMap<&str, Vec<&Symbol>>,
    budget_chars: usize,
) -> String {
    let mut out = String::from("Repo map (PageRank over imports + symbols):\n");
    for (path, _score) in ranked {
        let mut line = format!("- {path}");
        if let Some(syms) = symbols_by_path.get(path.as_str()) {
            let names: Vec<&str> = syms
                .iter()
                .take(MAX_SYMBOLS_PER_FILE)
                .map(|s| s.name.as_str())
                .collect();
            if !names.is_empty() {
                line.push_str(" — ");
                line.push_str(&names.join(", "));
                if syms.len() > MAX_SYMBOLS_PER_FILE {
                    line.push_str(", …");
                }
            }
        }
        line.push('\n');
        if out.len() + line.len() > budget_chars {
            out.push_str("… (truncated to token budget)\n");
            break;
        }
        out.push_str(&line);
    }
    out
}

fn extract_imports(
    from_path: &str,
    source: &str,
    language: Language,
    known: &HashSet<&str>,
) -> Vec<String> {
    match language {
        Language::Rust => extract_rust_imports(from_path, source, known),
        Language::TypeScript | Language::Tsx | Language::JavaScript => {
            extract_js_imports(from_path, source, known)
        }
        Language::Python => extract_python_imports(from_path, source, known),
        Language::Go => extract_go_imports(from_path, source, known),
        Language::Markdown | Language::Unknown => Vec::new(),
    }
}

fn extract_js_imports(from_path: &str, source: &str, known: &HashSet<&str>) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(spec) = quoted_after(trimmed, "from ") {
            push_resolved(&mut out, from_path, spec, known);
        }
        if let Some(rest) = trimmed.strip_prefix("require(").or_else(|| {
            trimmed
                .find("require(")
                .map(|i| &trimmed[i + "require(".len()..])
        }) {
            if let Some(spec) = first_quoted(rest) {
                push_resolved(&mut out, from_path, spec, known);
            }
        }
        if let Some(rest) = trimmed.strip_prefix("import ") {
            if let Some(spec) = first_quoted(rest) {
                if spec.starts_with('.') {
                    push_resolved(&mut out, from_path, spec, known);
                }
            }
        }
    }
    out
}

fn extract_python_imports(from_path: &str, source: &str, known: &HashSet<&str>) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("from ") {
            let spec = rest.split_whitespace().next().unwrap_or("");
            if spec.is_empty() {
                continue;
            }
            if let Some(rel) = resolve_python_relative(from_path, spec) {
                for candidate in with_source_extensions(&rel) {
                    push_if_known(&mut out, &candidate, known);
                }
                push_if_known(&mut out, &format!("{rel}.py"), known);
                push_if_known(&mut out, &format!("{rel}/__init__.py"), known);
            } else {
                push_python_module(&mut out, spec, known);
            }
        } else if let Some(rest) = trimmed.strip_prefix("import ") {
            let module = rest
                .split(|c: char| c == ',' || c.is_whitespace())
                .next()
                .unwrap_or("");
            if !module.is_empty() {
                push_python_module(&mut out, module, known);
            }
        }
    }
    out
}

fn extract_go_imports(from_path: &str, source: &str, known: &HashSet<&str>) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("import ") && !trimmed.starts_with('"') {
            continue;
        }
        if let Some(spec) = first_quoted(trimmed) {
            if spec.starts_with("./") || spec.starts_with("../") {
                push_resolved(&mut out, from_path, spec, known);
            } else if !spec.contains('.') {
                for candidate in [format!("{spec}.go"), format!("{spec}/doc.go")] {
                    push_if_known(&mut out, &candidate, known);
                }
            }
        }
    }
    out
}

fn extract_rust_imports(from_path: &str, source: &str, known: &HashSet<&str>) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("use ") {
            let path_spec = rest
                .trim_end_matches(';')
                .split([' ', '{'])
                .next()
                .unwrap_or("")
                .trim();
            if let Some(crate_path) = path_spec.strip_prefix("crate::") {
                push_rust_crate_path(&mut out, crate_path, known);
            } else if let Some(rest) = path_spec.strip_prefix("super::") {
                if let Some(parent) = parent_module_dir(from_path) {
                    let joined = format!("{parent}::{rest}");
                    push_rust_crate_path(&mut out, &joined.replace('/', "::"), known);
                }
            }
        }
        let mod_line = trimmed
            .strip_prefix("pub mod ")
            .or_else(|| trimmed.strip_prefix("mod "));
        if let Some(rest) = mod_line {
            if rest.ends_with(';') {
                let name = rest.trim_end_matches(';').trim();
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    let dir = Path::new(from_path)
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| PathBuf::from(""));
                    for candidate in [
                        dir.join(format!("{name}.rs")),
                        dir.join(name).join("mod.rs"),
                    ] {
                        push_if_known(&mut out, &path_to_rel(&candidate), known);
                    }
                }
            }
        }
    }
    out
}

fn push_rust_crate_path(out: &mut Vec<String>, crate_path: &str, known: &HashSet<&str>) {
    let segments: Vec<&str> = crate_path.split("::").filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return;
    }
    let joined = segments.join("/");
    for candidate in [
        format!("src/{joined}.rs"),
        format!("src/{joined}/mod.rs"),
        format!("{joined}.rs"),
        format!("{joined}/mod.rs"),
    ] {
        push_if_known(out, &candidate, known);
    }
    if segments.len() > 1 {
        let parent = segments[..segments.len() - 1].join("/");
        for candidate in [
            format!("src/{parent}.rs"),
            format!("src/{parent}/mod.rs"),
            format!("{parent}.rs"),
            format!("{parent}/mod.rs"),
        ] {
            push_if_known(out, &candidate, known);
        }
    }
}

fn parent_module_dir(from_path: &str) -> Option<String> {
    let path = Path::new(from_path);
    let parent = path.parent()?;
    if path.file_name().and_then(|s| s.to_str()) == Some("mod.rs") {
        return parent.parent().map(|p| p.display().to_string());
    }
    Some(parent.display().to_string())
}

fn push_resolved(out: &mut Vec<String>, from_path: &str, spec: &str, known: &HashSet<&str>) {
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return;
    }
    let from_dir = Path::new(from_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(""));
    let joined = normalize_rel(&from_dir.join(spec));
    for candidate in with_source_extensions(&joined) {
        push_if_known(out, &candidate, known);
    }
}

fn push_python_module(out: &mut Vec<String>, module: &str, known: &HashSet<&str>) {
    let path = module.replace('.', "/");
    for candidate in [format!("{path}.py"), format!("{path}/__init__.py")] {
        push_if_known(out, &candidate, known);
    }
}

fn resolve_python_relative(from_path: &str, spec: &str) -> Option<String> {
    if !spec.starts_with('.') {
        return None;
    }
    let mut dots = 0usize;
    for ch in spec.chars() {
        if ch == '.' {
            dots += 1;
        } else {
            break;
        }
    }
    let rest = &spec[dots..];
    let mut dir = Path::new(from_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(""));
    for _ in 1..dots {
        dir = dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(""));
    }
    if rest.is_empty() {
        return Some(path_to_rel(&dir.join("__init__.py")));
    }
    Some(path_to_rel(&dir.join(rest.replace('.', "/"))))
}

fn with_source_extensions(base: &str) -> Vec<String> {
    let mut out = Vec::new();
    if Path::new(base).extension().is_some() {
        out.push(base.to_owned());
        return out;
    }
    for ext in [
        ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".rs", ".py", ".go",
    ] {
        out.push(format!("{base}{ext}"));
    }
    out.push(format!("{base}/index.ts"));
    out.push(format!("{base}/index.tsx"));
    out.push(format!("{base}/index.js"));
    out.push(format!("{base}/mod.rs"));
    out.push(format!("{base}/__init__.py"));
    out
}

fn push_if_known(out: &mut Vec<String>, rel: &str, known: &HashSet<&str>) {
    let normalized = rel.replace('\\', "/");
    if known.contains(normalized.as_str()) && !out.contains(&normalized) {
        out.push(normalized);
    }
}

fn path_to_rel(path: &Path) -> String {
    normalize_path_str(&path.display().to_string())
}

fn normalize_rel(path: &Path) -> String {
    path_to_rel(path)
}

fn normalize_path_str(s: &str) -> String {
    let mut out = String::new();
    for part in s.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if let Some(idx) = out.rfind('/') {
                out.truncate(idx);
            } else {
                out.clear();
            }
            continue;
        }
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(part);
    }
    out
}

fn quoted_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let idx = line.find(marker)?;
    first_quoted(&line[idx + marker.len()..])
}

fn first_quoted(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let quote = bytes[i];
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let start = i + 1;
    let end = s[start..].find(quote as char)? + start;
    Some(&s[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::store::IndexStore;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        fs::write(path, content).unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn repo_map_ranks_imported_hub_file() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let index = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(repo.path(), "src/core.rs", "pub fn important() {}\n");
        write(
            repo.path(),
            "src/a.rs",
            "use crate::core::important;\npub fn a() { important(); }\n",
        );
        write(
            repo.path(),
            "src/b.rs",
            "use crate::core::important;\npub fn b() { important(); }\n",
        );
        write(repo.path(), "src/leaf.rs", "pub fn leaf() {}\n");

        let mut store =
            IndexStore::open(repo.path(), index.path()).unwrap_or_else(|e| panic!("{e}"));
        store.build().unwrap_or_else(|e| panic!("{e}"));

        let map = build_repo_map(&store, 2000);
        assert!(map.contains("src/core.rs"), "expected hub in map: {map}");
        let core_pos = map.find("src/core.rs").expect("core");
        let leaf_pos = map.find("src/leaf.rs").expect("leaf");
        assert!(core_pos < leaf_pos, "core should rank above leaf:\n{map}");
    }

    #[test]
    fn repo_map_respects_token_budget() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let index = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        for i in 0..40 {
            write(
                repo.path(),
                &format!("src/f{i}.rs"),
                &format!("pub fn f{i}() {{}}\n"),
            );
        }
        let mut store =
            IndexStore::open(repo.path(), index.path()).unwrap_or_else(|e| panic!("{e}"));
        store.build().unwrap_or_else(|e| panic!("{e}"));

        let map = build_repo_map(&store, 80);
        assert!(
            map.len() <= 80 * CHARS_PER_TOKEN + 80,
            "map len {} exceeded soft budget",
            map.len()
        );
        assert!(map.contains("truncated") || map.lines().count() < 45);
    }

    #[test]
    fn repo_map_cache_hits_on_second_call() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let index = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(repo.path(), "src/core.rs", "pub fn important() {}\n");
        write(
            repo.path(),
            "src/a.rs",
            "use crate::core::important;\npub fn a() { important(); }\n",
        );

        let mut store =
            IndexStore::open(repo.path(), index.path()).unwrap_or_else(|e| panic!("{e}"));
        store.build().unwrap_or_else(|e| panic!("{e}"));

        let (first, count, hit1) = build_repo_map_cached(&store, 2000);
        assert!(!hit1, "first call must miss cache");
        assert!(count >= 2);
        assert!(first.contains("src/core.rs"), "{first}");

        let (second, count2, hit2) = build_repo_map_cached(&store, 2000);
        assert!(hit2, "second call must hit cache");
        assert_eq!(count2, count);
        assert_eq!(second, first);
    }

    #[test]
    fn js_relative_imports_resolve() {
        let known: HashSet<&str> = ["src/util.ts", "src/main.ts"].into_iter().collect();
        let edges = extract_js_imports("src/main.ts", "import { x } from './util';\n", &known);
        assert_eq!(edges, vec!["src/util.ts".to_owned()]);
    }
}
