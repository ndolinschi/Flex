//! Desktop Components UI plugin — React / Vue / Angular detection + inventory.
//!
//! Not part of the agent engine: Tauri IPC only, consumed by the right-panel
//! Components tab registered through the frontend UI plugin registry.
//!
//! Discovery is heuristic (exports / SFC names / `@Component` classes + relative
//! import edges), not a DevTools / Fiber / compiler bridge.

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
static RE_VUE_DEFINE_OPTIONS_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"defineOptions\s*\(\s*\{[^}]*\bname\s*:\s*['"]([^'"]+)['"]"#)
        .expect("vue defineOptions name")
});
static RE_VUE_COMPONENT_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:defineComponent|component)\s*\(\s*\{[^}]*\bname\s*:\s*['"]([^'"]+)['"]"#)
        .expect("vue component name")
});
static RE_ANGULAR_COMPONENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?s)@Component\s*\([^)]*\)\s*(?:export\s+)?(?:default\s+)?class\s+([A-Z][A-Za-z0-9_]*)",
    )
    .expect("angular @Component class")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Framework {
    React,
    Vue,
    Angular,
}

impl Framework {
    fn id(self) -> &'static str {
        match self {
            Self::React => "react",
            Self::Vue => "vue",
            Self::Angular => "angular",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentsDetectResult {
    /// True when React is among `frameworks` (compat with older UI).
    pub is_react: bool,
    /// Detected UI frameworks: `"react"`, `"vue"`, `"angular"`.
    #[serde(default)]
    pub frameworks: Vec<String>,
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
    #[serde(default)]
    pub frameworks: Vec<String>,
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

fn make_detect(
    frameworks: &[Framework],
    reason: impl Into<String>,
    package_name: Option<String>,
) -> ComponentsDetectResult {
    let ids: Vec<String> = frameworks.iter().map(|f| f.id().to_string()).collect();
    ComponentsDetectResult {
        is_react: frameworks.contains(&Framework::React),
        frameworks: ids,
        reason: reason.into(),
        package_name,
    }
}

fn ext_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

fn is_react_source(path: &Path) -> bool {
    matches!(ext_lower(path).as_deref(), Some("tsx" | "jsx"))
}

fn is_vue_source(path: &Path) -> bool {
    matches!(ext_lower(path).as_deref(), Some("vue"))
}

fn is_angular_source(path: &Path) -> bool {
    let Some(ext) = ext_lower(path) else {
        return false;
    };
    if ext != "ts" {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    !name.ends_with(".d.ts") && !name.ends_with(".spec.ts") && !name.ends_with(".test.ts")
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

fn package_json_frameworks(raw: &str) -> (Vec<Framework>, String, Option<String>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return (Vec::new(), "unreadable package.json".into(), None);
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

    let mut frameworks = Vec::new();
    let mut reasons = Vec::new();

    if has_dep("react")
        || has_dep("react-dom")
        || has_dep("next")
        || has_dep("@next/swc")
        || has_dep("eslint-config-next")
        || has_dep("@vitejs/plugin-react")
        || has_dep("@vitejs/plugin-react-swc")
        || has_dep("react-scripts")
        || has_dep("remix")
        || has_dep("@remix-run/react")
        || has_dep("@remix-run/node")
        || has_dep("@remix-run/dev")
    {
        frameworks.push(Framework::React);
        reasons.push("react in package.json");
    } else if let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) {
        let mentions_next = scripts.values().any(|v| {
            v.as_str()
                .is_some_and(|s| s.split_whitespace().any(|tok| tok == "next"))
        });
        if mentions_next {
            frameworks.push(Framework::React);
            reasons.push("next script in package.json");
        }
    }

    if has_dep("vue")
        || has_dep("nuxt")
        || has_dep("@nuxt/kit")
        || has_dep("@nuxt/schema")
        || has_dep("@vitejs/plugin-vue")
        || has_dep("vue-router")
    {
        frameworks.push(Framework::Vue);
        reasons.push("vue in package.json");
    } else if let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) {
        let mentions_nuxt = scripts.values().any(|v| {
            v.as_str()
                .is_some_and(|s| s.split_whitespace().any(|tok| tok == "nuxt"))
        });
        if mentions_nuxt {
            frameworks.push(Framework::Vue);
            reasons.push("nuxt script in package.json");
        }
    }

    if has_dep("@angular/core")
        || has_dep("@angular/cli")
        || has_dep("@angular/compiler")
        || has_dep("@angular/common")
    {
        frameworks.push(Framework::Angular);
        reasons.push("angular in package.json");
    }

    frameworks.sort_by_key(|f| f.id());
    frameworks.dedup();
    let reason = if reasons.is_empty() {
        "no React/Vue/Angular markers in package.json".into()
    } else {
        reasons.join("; ")
    };
    (frameworks, reason, name)
}

const NEXT_CONFIG_NAMES: &[&str] = &[
    "next.config.js",
    "next.config.mjs",
    "next.config.cjs",
    "next.config.ts",
    "next.config.mts",
];

const NUXT_CONFIG_NAMES: &[&str] = &[
    "nuxt.config.js",
    "nuxt.config.mjs",
    "nuxt.config.cjs",
    "nuxt.config.ts",
    "nuxt.config.mts",
];

fn has_named_config(dir: &Path, names: &[&str]) -> bool {
    names.iter().any(|name| dir.join(name).is_file())
}

fn try_package_json(path: &Path) -> Option<(Vec<Framework>, String, Option<String>)> {
    let raw = fs::read_to_string(path).ok()?;
    Some(package_json_frameworks(&raw))
}

fn frameworks_from_dir_markers(dir: &Path) -> Vec<(Framework, String)> {
    let mut out = Vec::new();
    if has_named_config(dir, NEXT_CONFIG_NAMES) {
        out.push((
            Framework::React,
            format!(
                "next.config in {}",
                dir.file_name().and_then(|n| n.to_str()).unwrap_or(".")
            ),
        ));
    }
    if has_named_config(dir, NUXT_CONFIG_NAMES) {
        out.push((
            Framework::Vue,
            format!(
                "nuxt.config in {}",
                dir.file_name().and_then(|n| n.to_str()).unwrap_or(".")
            ),
        ));
    }
    if dir.join("angular.json").is_file() {
        out.push((
            Framework::Angular,
            format!(
                "angular.json in {}",
                dir.file_name().and_then(|n| n.to_str()).unwrap_or(".")
            ),
        ));
    }
    out
}

/// Scan one level of common monorepo / app folders when the root package.json
/// is a workspace shell without UI deps.
fn detect_in_children(root: &Path) -> Option<ComponentsDetectResult> {
    const CHILD_DIRS: &[&str] = &[
        "apps", "packages", "web", "frontend", "client", "app", "src",
    ];
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
        let interesting = CHILD_DIRS.contains(&name) || dir.join("package.json").is_file();
        if !interesting {
            continue;
        }

        let markers = frameworks_from_dir_markers(dir);
        if !markers.is_empty() {
            let frameworks: Vec<Framework> = markers.iter().map(|(f, _)| *f).collect();
            let reason = markers
                .iter()
                .map(|(_, r)| r.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            let package_name = try_package_json(&dir.join("package.json")).and_then(|r| r.2);
            return Some(make_detect(&frameworks, reason, package_name));
        }
        if let Some((fw, reason, package_name)) = try_package_json(&dir.join("package.json")) {
            if !fw.is_empty() {
                return Some(make_detect(&fw, format!("{reason} ({name})"), package_name));
            }
        }

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
                let child_name = child.file_name().and_then(|n| n.to_str()).unwrap_or("app");
                let markers = frameworks_from_dir_markers(&child);
                if !markers.is_empty() {
                    let frameworks: Vec<Framework> = markers.iter().map(|(f, _)| *f).collect();
                    let reason = markers
                        .iter()
                        .map(|(_, r)| r.to_string())
                        .collect::<Vec<_>>()
                        .join("; ");
                    let package_name =
                        try_package_json(&child.join("package.json")).and_then(|r| r.2);
                    return Some(make_detect(
                        &frameworks,
                        format!("{reason} ({name}/{child_name})"),
                        package_name,
                    ));
                }
                if let Some((fw, reason, package_name)) =
                    try_package_json(&child.join("package.json"))
                {
                    if !fw.is_empty() {
                        return Some(make_detect(
                            &fw,
                            format!("{reason} ({name}/{child_name})"),
                            package_name,
                        ));
                    }
                }
            }
        }
    }
    None
}

/// Detect whether `cwd` looks like a React / Vue / Angular application.
pub fn detect_ui_frameworks(cwd: &Path) -> ComponentsDetectResult {
    if !cwd.is_dir() {
        return make_detect(&[], "cwd is not a directory", None);
    }

    let root_markers = frameworks_from_dir_markers(cwd);
    if !root_markers.is_empty() {
        let frameworks: Vec<Framework> = root_markers.iter().map(|(f, _)| *f).collect();
        let reason = root_markers
            .iter()
            .map(|(_, r)| {
                // Prefer short reasons at root ("next.config present").
                if r.contains("next.config") {
                    "next.config present"
                } else if r.contains("nuxt.config") {
                    "nuxt.config present"
                } else if r.contains("angular.json") {
                    "angular.json present"
                } else {
                    r.as_str()
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        let package_name = find_package_json(cwd)
            .and_then(|p| try_package_json(&p))
            .and_then(|r| r.2);
        return make_detect(&frameworks, reason, package_name);
    }

    let Some(pkg_path) = find_package_json(cwd) else {
        return detect_in_children(cwd)
            .unwrap_or_else(|| make_detect(&[], "no package.json found", None));
    };
    let Ok(raw) = fs::read_to_string(&pkg_path) else {
        return make_detect(&[], "could not read package.json", None);
    };
    let (frameworks, reason, package_name) = package_json_frameworks(&raw);
    if !frameworks.is_empty() {
        return make_detect(&frameworks, reason, package_name);
    }

    let search_root = pkg_path.parent().unwrap_or(cwd);
    if let Some(hit) = detect_in_children(search_root) {
        return hit;
    }
    if search_root != cwd {
        if let Some(hit) = detect_in_children(cwd) {
            return hit;
        }
    }

    make_detect(&[], reason, package_name)
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

fn to_pascal_case(stem: &str) -> String {
    let mut out = String::new();
    let mut cap = true;
    for ch in stem.chars() {
        if ch == '-' || ch == '_' || ch == '.' {
            cap = true;
            continue;
        }
        if cap {
            out.extend(ch.to_uppercase());
            cap = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn extract_vue_exports(path: &Path, source: &str) -> Vec<String> {
    if let Some(cap) = RE_VUE_DEFINE_OPTIONS_NAME.captures(source) {
        if let Some(m) = cap.get(1) {
            return vec![m.as_str().to_string()];
        }
    }
    if let Some(cap) = RE_VUE_COMPONENT_NAME.captures(source) {
        if let Some(m) = cap.get(1) {
            return vec![m.as_str().to_string()];
        }
    }
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Component");
    let name = to_pascal_case(stem);
    if name.is_empty() {
        Vec::new()
    } else {
        vec![name]
    }
}

fn extract_angular_exports(source: &str) -> Vec<String> {
    let mut names: HashSet<String> = HashSet::new();
    for cap in RE_ANGULAR_COMPONENT.captures_iter(source) {
        if let Some(m) = cap.get(1) {
            names.insert(m.as_str().to_string());
        }
    }
    let mut out: Vec<String> = names.into_iter().collect();
    out.sort();
    out
}

/// Resolve a relative import specifier to a component source file.
fn resolve_import(from_file: &Path, spec: &str, _root: &Path) -> Option<PathBuf> {
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return None;
    }
    let base = from_file.parent()?.join(spec);
    let candidates = [
        base.clone(),
        PathBuf::from(format!("{}.tsx", base.display())),
        PathBuf::from(format!("{}.jsx", base.display())),
        PathBuf::from(format!("{}.vue", base.display())),
        PathBuf::from(format!("{}.ts", base.display())),
        base.join("index.tsx"),
        base.join("index.jsx"),
        base.join("index.vue"),
        base.join("index.ts"),
    ];
    candidates
        .into_iter()
        .find(|c| c.is_file() && (is_react_source(c) || is_vue_source(c) || is_angular_source(c)))
}

fn extract_component_imports(source: &str, from_file: &Path, root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for cap in RE_IMPORT.captures_iter(source) {
        let spec = cap.get(4).map(|m| m.as_str()).unwrap_or("");
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
        // Vue often default-imports SFCs with any case alias — still resolve
        // relative `.vue` specs even without a PascalCase binding.
        let is_vue_spec = spec.ends_with(".vue");
        if !has_component_import && !is_vue_spec {
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
            if name.is_empty() {
                continue;
            }
            let optional = field.get(2).is_some();
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
    let named = [
        "src",
        "app",
        "apps",
        "components",
        "pages",
        "lib",
        "ui",
        "views",
    ];
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

fn wants_path(path: &Path, frameworks: &[Framework]) -> bool {
    let react = frameworks.contains(&Framework::React);
    let vue = frameworks.contains(&Framework::Vue);
    let angular = frameworks.contains(&Framework::Angular);
    (react && is_react_source(path))
        || (vue && is_vue_source(path))
        || (angular && is_angular_source(path))
}

fn exports_for_file(path: &Path, source: &str, frameworks: &[Framework]) -> Vec<String> {
    if is_vue_source(path) && frameworks.contains(&Framework::Vue) {
        return extract_vue_exports(path, source);
    }
    if is_angular_source(path) && frameworks.contains(&Framework::Angular) {
        return extract_angular_exports(source);
    }
    if is_react_source(path) && frameworks.contains(&Framework::React) {
        return extract_exports(source);
    }
    Vec::new()
}

fn scan_component_files(
    cwd: &Path,
    frameworks: &[Framework],
) -> Vec<(PathBuf, String, Vec<String>)> {
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
            if !path.is_file() || !wants_path(path, frameworks) {
                continue;
            }
            let abs = path.to_path_buf();
            if !seen.insert(abs.clone()) {
                continue;
            }
            let Some(source) = read_capped(&abs) else {
                continue;
            };
            let exports = exports_for_file(&abs, &source, frameworks);
            if exports.is_empty() {
                continue;
            }
            files.push((abs, source, exports));
        }
    }
    files
}

fn frameworks_from_detect(detect: &ComponentsDetectResult) -> Vec<Framework> {
    detect
        .frameworks
        .iter()
        .filter_map(|id| match id.as_str() {
            "react" => Some(Framework::React),
            "vue" => Some(Framework::Vue),
            "angular" => Some(Framework::Angular),
            _ => None,
        })
        .collect()
}

/// Inventory UI components under `cwd` for detected frameworks.
pub fn list_components(cwd: &Path) -> ComponentsListResult {
    let detect = detect_ui_frameworks(cwd);
    let frameworks = frameworks_from_detect(&detect);
    if frameworks.is_empty() {
        return ComponentsListResult {
            is_react: false,
            frameworks: Vec::new(),
            components: Vec::new(),
            roots: Vec::new(),
        };
    }

    let scanned = scan_component_files(cwd, &frameworks);
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
        is_react: detect.is_react,
        frameworks: detect.frameworks,
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
    let props = if is_react_source(&path) {
        extract_props(&source, export_name)
    } else {
        Vec::new()
    };
    let children = extract_component_imports(&source, &path, cwd)
        .into_iter()
        .filter_map(|p| {
            let rel = normalize_rel(&p, cwd);
            let src = read_capped(&p)?;
            let exports = if is_vue_source(&p) {
                extract_vue_exports(&p, &src)
            } else if is_angular_source(&p) {
                extract_angular_exports(&src)
            } else {
                extract_exports(&src)
            };
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

fn cwd_not_a_directory_msg(cwd: &str) -> String {
    let shown = cwd.trim();
    if shown.is_empty() {
        "cwd is not a directory (empty)".into()
    } else {
        format!("cwd is not a directory ({shown})")
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn components_detect(
    cwd: String,
    fallback_cwd: Option<String>,
) -> DesktopResult<ComponentsDetectResult> {
    let Some(path) = crate::path_resolve::resolve_existing_dir(&cwd, fallback_cwd.as_deref())
    else {
        return Ok(make_detect(&[], cwd_not_a_directory_msg(&cwd), None));
    };
    Ok(detect_ui_frameworks(&path))
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn components_list(
    cwd: String,
    fallback_cwd: Option<String>,
) -> DesktopResult<ComponentsListResult> {
    let Some(path) = crate::path_resolve::resolve_existing_dir(&cwd, fallback_cwd.as_deref())
    else {
        return Err(message(cwd_not_a_directory_msg(&cwd)));
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
        return Err(message(cwd_not_a_directory_msg(&cwd)));
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
        let r = detect_ui_frameworks(dir.path());
        assert!(r.is_react);
        assert!(r.frameworks.iter().any(|f| f == "react"));
        assert!(r.reason.contains("react"));
        assert_eq!(r.package_name.as_deref(), Some("demo"));
    }

    #[test]
    fn detects_vue_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("package.json"),
            r#"{"name":"demo","dependencies":{"vue":"^3.5.0"}}"#,
        );
        let r = detect_ui_frameworks(dir.path());
        assert!(!r.is_react);
        assert_eq!(r.frameworks, vec!["vue".to_string()]);
    }

    #[test]
    fn detects_angular_from_angular_json() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("package.json"), r#"{"name":"ng-app"}"#);
        write_file(&dir.path().join("angular.json"), r#"{"version":1}"#);
        let r = detect_ui_frameworks(dir.path());
        assert!(r.frameworks.iter().any(|f| f == "angular"));
        assert!(r.reason.contains("angular.json"));
    }

    #[test]
    fn rejects_backend_only_package() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("package.json"),
            r#"{"name":"api","dependencies":{"express":"^4.0.0"}}"#,
        );
        let r = detect_ui_frameworks(dir.path());
        assert!(r.frameworks.is_empty());
        assert!(!r.is_react);
    }

    #[test]
    fn detects_next_from_config_file() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("package.json"), r#"{"name":"web"}"#);
        write_file(&dir.path().join("next.config.mjs"), "export default {}\n");
        let r = detect_ui_frameworks(dir.path());
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
        let r = detect_ui_frameworks(dir.path());
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
        let r = detect_ui_frameworks(dir.path());
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
    fn lists_vue_sfc_by_filename() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("package.json"),
            r#"{"dependencies":{"vue":"3"}}"#,
        );
        write_file(
            &dir.path().join("src/components/HelloWorld.vue"),
            r#"
<script setup>
const msg = 'hi'
</script>
<template><p>{{ msg }}</p></template>
"#,
        );
        let list = list_components(dir.path());
        assert_eq!(list.frameworks, vec!["vue".to_string()]);
        assert!(list.components.iter().any(|c| c.name == "HelloWorld"));
    }

    #[test]
    fn lists_angular_component_class() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("package.json"),
            r#"{"dependencies":{"@angular/core":"19"}}"#,
        );
        write_file(
            &dir.path().join("src/app/hero.component.ts"),
            r#"
import { Component } from '@angular/core';
@Component({ selector: 'app-hero', template: '<p>hero</p>' })
export class HeroComponent {}
"#,
        );
        let list = list_components(dir.path());
        assert!(list.frameworks.iter().any(|f| f == "angular"));
        assert!(list.components.iter().any(|c| c.name == "HeroComponent"));
    }

    #[test]
    fn empty_when_no_ui_framework() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("package.json"), r#"{"name":"x"}"#);
        write_file(
            &dir.path().join("src/Foo.tsx"),
            "export function Foo() { return null }",
        );
        let list = list_components(dir.path());
        assert!(!list.is_react);
        assert!(list.frameworks.is_empty());
        assert!(list.components.is_empty());
    }
}
