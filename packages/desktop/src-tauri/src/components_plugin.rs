//! Desktop Components UI plugin — React detection + TSX/JSX inventory.
//!
//! Not part of the agent engine: Tauri IPC only, consumed by the right-panel
//! Components tab registered through the frontend UI plugin registry.
//!
//! Discovery is heuristic (PascalCase exports + relative import edges), not a
//! React DevTools / Fiber bridge.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use std::sync::LazyLock;

use ignore::WalkBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{DesktopError, DesktopResult};

/// Max files scanned per project (soft cap for large monorepos).
const MAX_FILES: usize = 2_000;
/// Max bytes read per source file when extracting exports / props.
const MAX_FILE_BYTES: u64 = 256 * 1024;
/// Lines of source returned for the detail pane.
const SNIPPET_LINES: usize = 48;

static RE_EXPORT_FN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*export\s+(?:default\s+)?(?:async\s+)?function\s+([A-Z][A-Za-z0-9_]*)\s*\(")
        .expect("export fn regex")
});
static RE_EXPORT_CONST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*export\s+(?:default\s+)?(?:const|let|var)\s+([A-Z][A-Za-z0-9_]*)\s*=")
        .expect("export const regex")
});
static RE_EXPORT_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*export\s+(?:default\s+)?class\s+([A-Z][A-Za-z0-9_]*)\b")
        .expect("export class regex")
});
static RE_EXPORT_DEFAULT_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*export\s+default\s+([A-Z][A-Za-z0-9_]*)\s*;?\s*$")
        .expect("export default name regex")
});
static RE_NAMED_EXPORT_LIST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*export\s*\{([^}]+)\}").expect("named export list regex"));
static RE_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?m)^\s*import\s+(?:type\s+)?(?:(\w+)|(?:\{([^}]+)\})|(?:\*\s+as\s+(\w+)))\s+from\s+['"]([^'"]+)['"]"#,
    )
    .expect("import regex")
});
static RE_PROPS_INTERFACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ms)(?:export\s+)?(?:interface|type)\s+([A-Z][A-Za-z0-9_]*(?:Props|Properties)?)\s*(?:=\s*)?\{([^}]*)\}",
    )
    .expect("props interface regex")
});
static RE_PROP_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*([A-Za-z_][A-Za-z0-9_]*)\s*(\?)?\s*:").expect("prop field")
});

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentsDetectResult {
    pub is_react: bool,
    /// Short reason shown in the empty state (e.g. "react in package.json").
    pub reason: String,
    pub package_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentNode {
    pub id: String,
    pub name: String,
    /// Path relative to project cwd, using `/` separators.
    pub file: String,
    pub export_name: String,
    /// Child component ids (imported and used as PascalCase symbols).
    pub children: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentsListResult {
    pub is_react: bool,
    pub components: Vec<ComponentNode>,
    /// Root ids (not imported by any other discovered component).
    pub roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPropSummary {
    pub name: String,
    pub optional: bool,
    pub type_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentDetail {
    pub id: String,
    pub name: String,
    pub file: String,
    pub export_name: String,
    pub props: Vec<ComponentPropSummary>,
    pub source_snippet: String,
    pub children: Vec<String>,
}

fn message(err: impl Into<String>) -> DesktopError {
    DesktopError::Message(err.into())
}

fn normalize_rel(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_component_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()),
        Some(ref e) if e == "tsx" || e == "jsx"
    )
}

fn read_capped(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > MAX_FILE_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn find_package_json(cwd: &Path) -> Option<PathBuf> {
    let mut dir = cwd.to_path_buf();
    loop {
        let candidate = dir.join("package.json");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn package_json_is_react(raw: &str) -> (bool, String, Option<String>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return (false, "unreadable package.json".into(), None);
    };
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let deps = [
        value.get("dependencies"),
        value.get("devDependencies"),
        value.get("peerDependencies"),
    ];

    let has_dep = |key: &str| -> bool {
        deps.iter().any(|section| {
            section
                .and_then(|s| s.as_object())
                .is_some_and(|obj| obj.contains_key(key))
        })
    };

    if has_dep("react") || has_dep("react-dom") {
        return (true, "react in package.json".into(), name);
    }
    if has_dep("next") || has_dep("@next/swc") || has_dep("eslint-config-next") {
        return (true, "next in package.json".into(), name);
    }
    if has_dep("@vitejs/plugin-react") || has_dep("@vitejs/plugin-react-swc") {
        return (true, "vite react plugin in package.json".into(), name);
    }
    if has_dep("react-scripts") {
        return (true, "create-react-app (react-scripts)".into(), name);
    }
    if has_dep("remix")
        || has_dep("@remix-run/react")
        || has_dep("@remix-run/node")
        || has_dep("@remix-run/dev")
    {
        return (true, "remix in package.json".into(), name);
    }
    // Scripts often name the framework even when deps are hoisted elsewhere.
    if let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) {
        let mentions_next = scripts.values().any(|v| {
            v.as_str()
                .is_some_and(|s| s.split_whitespace().any(|tok| tok == "next"))
        });
        if mentions_next {
            return (true, "next script in package.json".into(), name);
        }
    }
    (false, "no React markers in package.json".into(), name)
}

const NEXT_CONFIG_NAMES: &[&str] = &[
    "next.config.js",
    "next.config.mjs",
    "next.config.cjs",
    "next.config.ts",
    "next.config.mts",
];

fn has_next_config(dir: &Path) -> bool {
    NEXT_CONFIG_NAMES
        .iter()
        .any(|name| dir.join(name).is_file())
}

fn try_package_json(path: &Path) -> Option<(bool, String, Option<String>)> {
    let raw = fs::read_to_string(path).ok()?;
    Some(package_json_is_react(&raw))
}

/// Scan one level of common monorepo / app folders when the root package.json
/// is a workspace shell without React deps (e.g. Next lives in `apps/web`).
fn detect_react_in_children(root: &Path) -> Option<ComponentsDetectResult> {
    const CHILD_DIRS: &[&str] = &["apps", "packages", "web", "frontend", "client", "app", "src"];
    let Ok(entries) = fs::read_dir(root) else {
        return None;
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    for dir in &dirs {
        let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Prefer well-known app folders; also accept any immediate child that
        // ships its own package.json (covers `apps/web`-style layouts when we
        // recurse one extra level below `apps`/`packages`).
        let interesting = CHILD_DIRS.contains(&name) || dir.join("package.json").is_file();
        if !interesting {
            continue;
        }

        if has_next_config(dir) {
            let package_name = try_package_json(&dir.join("package.json")).and_then(|r| r.2);
            return Some(ComponentsDetectResult {
                is_react: true,
                reason: format!("next.config in {}", name),
                package_name,
            });
        }
        if let Some((true, reason, package_name)) = try_package_json(&dir.join("package.json")) {
            return Some(ComponentsDetectResult {
                is_react: true,
                reason: format!("{reason} ({name})"),
                package_name,
            });
        }

        // One more level for `apps/web`, `packages/ui`, etc.
        if matches!(name, "apps" | "packages") {
            let Ok(nested) = fs::read_dir(dir) else {
                continue;
            };
            let mut nested_dirs: Vec<PathBuf> = nested
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            nested_dirs.sort();
            for child in nested_dirs {
                let child_name = child
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("app");
                if has_next_config(&child) {
                    let package_name =
                        try_package_json(&child.join("package.json")).and_then(|r| r.2);
                    return Some(ComponentsDetectResult {
                        is_react: true,
                        reason: format!("next.config in {name}/{child_name}"),
                        package_name,
                    });
                }
                if let Some((true, reason, package_name)) =
                    try_package_json(&child.join("package.json"))
                {
                    return Some(ComponentsDetectResult {
                        is_react: true,
                        reason: format!("{reason} ({name}/{child_name})"),
                        package_name,
                    });
                }
            }
        }
    }
    None
}

/// Detect whether `cwd` looks like a React / Next.js application.
pub fn detect_react(cwd: &Path) -> ComponentsDetectResult {
    if !cwd.is_dir() {
        return ComponentsDetectResult {
            is_react: false,
            reason: "cwd is not a directory".into(),
            package_name: None,
        };
    }

    // next.config.* is decisive even before package.json (covers pnpm workspaces
    // where `next` is only declared in a nested package).
    if has_next_config(cwd) {
        let package_name = find_package_json(cwd)
            .and_then(|p| try_package_json(&p))
            .and_then(|r| r.2);
        return ComponentsDetectResult {
            is_react: true,
            reason: "next.config present".into(),
            package_name,
        };
    }

    let Some(pkg_path) = find_package_json(cwd) else {
        // No package.json up-tree — still try shallow children (opened a
        // parent folder that isn't itself a JS package).
        return detect_react_in_children(cwd).unwrap_or(ComponentsDetectResult {
            is_react: false,
            reason: "no package.json found".into(),
            package_name: None,
        });
    };
    let Ok(raw) = fs::read_to_string(&pkg_path) else {
        return ComponentsDetectResult {
            is_react: false,
            reason: "could not read package.json".into(),
            package_name: None,
        };
    };
    let (is_react, reason, package_name) = package_json_is_react(&raw);
    if is_react {
        return ComponentsDetectResult {
            is_react: true,
            reason,
            package_name,
        };
    }

    // Workspace root without React deps — look in apps/packages/web/…
    let search_root = pkg_path.parent().unwrap_or(cwd);
    if let Some(hit) = detect_react_in_children(search_root) {
        return hit;
    }
    // Also search from the session cwd when it differs (opened a subfolder).
    if search_root != cwd {
        if let Some(hit) = detect_react_in_children(cwd) {
            return hit;
        }
    }

    ComponentsDetectResult {
        is_react: false,
        reason,
        package_name,
    }
}

fn extract_exports(source: &str) -> Vec<String> {
    let mut names: HashSet<String> = HashSet::new();
    for re in [
        &*RE_EXPORT_FN,
        &*RE_EXPORT_CONST,
        &*RE_EXPORT_CLASS,
        &*RE_EXPORT_DEFAULT_NAME,
    ] {
        for cap in re.captures_iter(source) {
            if let Some(m) = cap.get(1) {
                names.insert(m.as_str().to_string());
            }
        }
    }
    for cap in RE_NAMED_EXPORT_LIST.captures_iter(source) {
        let Some(list) = cap.get(1) else { continue };
        for part in list.as_str().split(',') {
            let token = part
                .split(" as ")
                .next()
                .unwrap_or(part)
                .trim()
                .trim_start_matches("type ")
                .trim();
            if token.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                names.insert(token.to_string());
            }
        }
    }
    let mut out: Vec<String> = names.into_iter().collect();
    out.sort();
    out
}

/// Resolve a relative import specifier to a `.tsx`/`.jsx` file.
fn resolve_import(from_file: &Path, spec: &str, _root: &Path) -> Option<PathBuf> {
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return None;
    }
    let base = from_file.parent()?.join(spec);
    let candidates = [
        base.clone(),
        PathBuf::from(format!("{}.tsx", base.display())),
        PathBuf::from(format!("{}.jsx", base.display())),
        base.join("index.tsx"),
        base.join("index.jsx"),
    ];
    candidates
        .into_iter()
        .find(|c| c.is_file() && is_component_source(c))
}

fn extract_component_imports(source: &str, from_file: &Path, root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for cap in RE_IMPORT.captures_iter(source) {
        let spec = cap.get(4).map(|m| m.as_str()).unwrap_or("");
        // Default import or namespace — only keep PascalCase (component-like).
        let default_or_ns = cap
            .get(1)
            .or_else(|| cap.get(3))
            .map(|m| m.as_str())
            .filter(|n| n.chars().next().is_some_and(|c| c.is_ascii_uppercase()));
        let named = cap.get(2).map(|m| {
            m.as_str()
                .split(',')
                .filter_map(|part| {
                    let name = part
                        .split(" as ")
                        .last()
                        .unwrap_or(part)
                        .trim()
                        .trim_start_matches("type ");
                    if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                        Some(name.to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        });
        let has_component_import =
            default_or_ns.is_some() || named.as_ref().is_some_and(|n| !n.is_empty());
        if !has_component_import {
            continue;
        }
        if let Some(resolved) = resolve_import(from_file, spec, root) {
            out.push(resolved);
        }
    }
    out
}

fn extract_props(source: &str, component_name: &str) -> Vec<ComponentPropSummary> {
    let mut best: Option<(usize, Vec<ComponentPropSummary>)> = None;
    for cap in RE_PROPS_INTERFACE.captures_iter(source) {
        let iface = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let body = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let score = if iface == format!("{component_name}Props") {
            3
        } else if iface.ends_with("Props") || iface.ends_with("Properties") {
            2
        } else if iface == "Props" {
            1
        } else {
            0
        };
        if score == 0 {
            continue;
        }
        let mut props = Vec::new();
        for field in RE_PROP_FIELD.captures_iter(body) {
            let name = field.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            if name.is_empty() || name == "children" {
                // Still include children — useful for the LLM.
            }
            if name.is_empty() {
                continue;
            }
            let optional = field.get(2).is_some();
            // Grab a short type hint after the colon until `;` or newline.
            let type_hint = field.get(0).and_then(|whole| {
                let start = whole.end();
                let rest = &body[start.min(body.len())..];
                let end = rest.find([';', '\n']).unwrap_or(rest.len().min(60));
                let hint = rest[..end].trim().trim_end_matches(',').trim();
                if hint.is_empty() {
                    None
                } else {
                    Some(hint.chars().take(80).collect())
                }
            });
            props.push(ComponentPropSummary {
                name,
                optional,
                type_hint,
            });
        }
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, props));
        }
    }
    best.map(|(_, p)| p).unwrap_or_default()
}

fn source_snippet(source: &str) -> String {
    source
        .lines()
        .take(SNIPPET_LINES)
        .collect::<Vec<_>>()
        .join("\n")
}

fn preferred_scan_roots(cwd: &Path) -> Vec<PathBuf> {
    let named = ["src", "app", "components", "pages", "lib", "ui"];
    let mut roots: Vec<PathBuf> = named
        .iter()
        .map(|n| cwd.join(n))
        .filter(|p| p.is_dir())
        .collect();
    if roots.is_empty() {
        roots.push(cwd.to_path_buf());
    }
    roots
}

fn scan_component_files(cwd: &Path) -> Vec<(PathBuf, String, Vec<String>)> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for root in preferred_scan_roots(cwd) {
        let mut builder = WalkBuilder::new(&root);
        builder
            .standard_filters(true)
            .hidden(true)
            .max_depth(Some(12));
        for entry in builder.build().flatten() {
            if files.len() >= MAX_FILES {
                break;
            }
            let path = entry.path();
            if !path.is_file() || !is_component_source(path) {
                continue;
            }
            let abs = path.to_path_buf();
            if !seen.insert(abs.clone()) {
                continue;
            }
            let Some(source) = read_capped(&abs) else {
                continue;
            };
            let exports = extract_exports(&source);
            if exports.is_empty() {
                continue;
            }
            files.push((abs, source, exports));
        }
    }
    files
}

/// Inventory React components under `cwd`.
pub fn list_components(cwd: &Path) -> ComponentsListResult {
    let detect = detect_react(cwd);
    if !detect.is_react {
        return ComponentsListResult {
            is_react: false,
            components: Vec::new(),
            roots: Vec::new(),
        };
    }

    let scanned = scan_component_files(cwd);
    // file → component ids declared there
    let mut file_to_ids: HashMap<PathBuf, Vec<String>> = HashMap::new();
    let mut nodes: HashMap<String, ComponentNode> = HashMap::new();

    for (path, _source, exports) in &scanned {
        let rel = normalize_rel(path, cwd);
        let mut ids = Vec::new();
        for export_name in exports {
            let id = format!("{rel}#{export_name}");
            ids.push(id.clone());
            nodes.insert(
                id.clone(),
                ComponentNode {
                    id: id.clone(),
                    name: export_name.clone(),
                    file: rel.clone(),
                    export_name: export_name.clone(),
                    children: Vec::new(),
                },
            );
        }
        file_to_ids.insert(path.clone(), ids);
    }

    // Import edges: parent file → child files → child component ids
    for (path, source, _) in &scanned {
        let Some(parent_ids) = file_to_ids.get(path) else {
            continue;
        };
        let child_files = extract_component_imports(source, path, cwd);
        let mut child_ids = Vec::new();
        for child_path in child_files {
            if let Some(ids) = file_to_ids.get(&child_path) {
                child_ids.extend(ids.iter().cloned());
            }
        }
        child_ids.sort();
        child_ids.dedup();
        for pid in parent_ids {
            if let Some(node) = nodes.get_mut(pid) {
                node.children = child_ids.clone();
            }
        }
    }

    let all_ids: HashSet<String> = nodes.keys().cloned().collect();
    let mut referenced: HashSet<String> = HashSet::new();
    for node in nodes.values() {
        for c in &node.children {
            referenced.insert(c.clone());
        }
    }
    let mut roots: Vec<String> = all_ids.difference(&referenced).cloned().collect();
    roots.sort();

    let mut components: Vec<ComponentNode> = nodes.into_values().collect();
    components.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));

    ComponentsListResult {
        is_react: true,
        components,
        roots,
    }
}

/// Detail for one component id (`file#ExportName`).
pub fn component_detail(cwd: &Path, id: &str) -> DesktopResult<ComponentDetail> {
    let Some((file_rel, export_name)) = id.split_once('#') else {
        return Err(message("invalid component id"));
    };
    let path = cwd.join(file_rel);
    if !path.is_file() {
        return Err(message(format!("component file not found: {file_rel}")));
    }
    let source = read_capped(&path).ok_or_else(|| message("could not read component file"))?;
    let props = extract_props(&source, export_name);
    let children = extract_component_imports(&source, &path, cwd)
        .into_iter()
        .filter_map(|p| {
            let rel = normalize_rel(&p, cwd);
            let src = read_capped(&p)?;
            let exports = extract_exports(&src);
            Some(
                exports
                    .into_iter()
                    .map(|e| format!("{rel}#{e}"))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect::<Vec<_>>();

    Ok(ComponentDetail {
        id: id.to_string(),
        name: export_name.to_string(),
        file: file_rel.replace('\\', "/"),
        export_name: export_name.to_string(),
        props,
        source_snippet: source_snippet(&source),
        children,
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn components_detect(
    cwd: String,
    fallback_cwd: Option<String>,
) -> DesktopResult<ComponentsDetectResult> {
    let Some(path) = crate::path_resolve::resolve_existing_dir(&cwd, fallback_cwd.as_deref())
    else {
        let shown = cwd.trim();
        return Ok(ComponentsDetectResult {
            is_react: false,
            reason: if shown.is_empty() {
                "cwd is not a directory (empty)".into()
            } else {
                format!("cwd is not a directory ({shown})")
            },
            package_name: None,
        });
    };
    Ok(detect_react(&path))
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn components_list(
    cwd: String,
    fallback_cwd: Option<String>,
) -> DesktopResult<ComponentsListResult> {
    let Some(path) = crate::path_resolve::resolve_existing_dir(&cwd, fallback_cwd.as_deref())
    else {
        let shown = cwd.trim();
        return Err(message(if shown.is_empty() {
            "cwd is not a directory (empty)".into()
        } else {
            format!("cwd is not a directory ({shown})")
        }));
    };
    Ok(list_components(&path))
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn components_detail(
    cwd: String,
    id: String,
    fallback_cwd: Option<String>,
) -> DesktopResult<ComponentDetail> {
    let Some(path) = crate::path_resolve::resolve_existing_dir(&cwd, fallback_cwd.as_deref())
    else {
        let shown = cwd.trim();
        return Err(message(if shown.is_empty() {
            "cwd is not a directory (empty)".into()
        } else {
            format!("cwd is not a directory ({shown})")
        }));
    };
    component_detail(&path, id.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn detects_react_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("package.json"),
            r#"{"name":"demo","dependencies":{"react":"^19.0.0","react-dom":"^19.0.0"}}"#,
        );
        let r = detect_react(dir.path());
        assert!(r.is_react);
        assert!(r.reason.contains("react"));
        assert_eq!(r.package_name.as_deref(), Some("demo"));
    }

    #[test]
    fn rejects_backend_only_package() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("package.json"),
            r#"{"name":"api","dependencies":{"express":"^4.0.0"}}"#,
        );
        let r = detect_react(dir.path());
        assert!(!r.is_react);
    }

    #[test]
    fn detects_next_from_config_file() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("package.json"), r#"{"name":"web"}"#);
        write_file(&dir.path().join("next.config.mjs"), "export default {}\n");
        let r = detect_react(dir.path());
        assert!(r.is_react);
        assert!(r.reason.contains("next.config"));
    }

    #[test]
    fn detects_next_in_monorepo_apps_web() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("package.json"),
            r#"{"name":"mono","private":true,"workspaces":["apps/*"]}"#,
        );
        write_file(
            &dir.path().join("apps/web/package.json"),
            r#"{"name":"web","dependencies":{"next":"15.0.0","react":"19.0.0","react-dom":"19.0.0"}}"#,
        );
        let r = detect_react(dir.path());
        assert!(r.is_react, "reason={}", r.reason);
        assert_eq!(r.package_name.as_deref(), Some("web"));
    }

    #[test]
    fn detects_next_script_without_dep_key() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("package.json"),
            r#"{"name":"web","scripts":{"dev":"next dev","build":"next build"}}"#,
        );
        let r = detect_react(dir.path());
        assert!(r.is_react);
        assert!(r.reason.contains("next script"));
    }

    #[test]
    fn lists_pascal_case_exports_and_import_tree() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("package.json"),
            r#"{"dependencies":{"react":"19"}}"#,
        );
        write_file(
            &dir.path().join("src/components/Button.tsx"),
            r#"
export interface ButtonProps {
  label: string;
  disabled?: boolean;
}
export function Button({ label }: ButtonProps) {
  return <button>{label}</button>;
}
"#,
        );
        write_file(
            &dir.path().join("src/components/Card.tsx"),
            r#"
import { Button } from './Button';
export const Card = () => <div><Button label="Go" /></div>;
"#,
        );

        let list = list_components(dir.path());
        assert!(list.is_react);
        assert_eq!(list.components.len(), 2);
        let card = list
            .components
            .iter()
            .find(|c| c.name == "Card")
            .expect("Card");
        assert!(card.children.iter().any(|c| c.contains("Button")));
        assert!(list.roots.iter().any(|r| r.contains("Card")));
        assert!(!list.roots.iter().any(|r| r.contains("Button")));

        let detail = component_detail(dir.path(), &card.id).unwrap();
        assert_eq!(detail.name, "Card");
        assert!(!detail.source_snippet.is_empty());

        let btn_id = list
            .components
            .iter()
            .find(|c| c.name == "Button")
            .unwrap()
            .id
            .clone();
        let btn = component_detail(dir.path(), &btn_id).unwrap();
        assert!(btn.props.iter().any(|p| p.name == "label"));
        assert!(btn.props.iter().any(|p| p.name == "disabled" && p.optional));
    }

    #[test]
    fn empty_when_not_react() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("package.json"), r#"{"name":"x"}"#);
        write_file(
            &dir.path().join("src/Foo.tsx"),
            "export function Foo() { return null }",
        );
        let list = list_components(dir.path());
        assert!(!list.is_react);
        assert!(list.components.is_empty());
    }
}
