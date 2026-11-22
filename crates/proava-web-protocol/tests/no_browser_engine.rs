//! proAVA stays pure Rust: the browser engine (wry, tao, WebKitGTK, WebView2)
//! lives only in the helper `apps/proava-web`, a Cargo workspace of its own.
//! This test reads the lock files (no process, no `cargo tree`) and walks the
//! dependency graph of `proava` and `proava-mcp`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Crates of a browser engine – never in proAVA or its MCP server.
const ENGINE: [&str; 6] = [
    "wry",
    "tao",
    "webkit2gtk",
    "javascriptcore-rs",
    "soup3",
    "webview2-com",
];

fn is_engine(name: &str) -> bool {
    ENGINE
        .iter()
        .any(|engine| name == *engine || name.starts_with(&format!("{engine}-")))
}

/// Package name → names of its dependencies, from a Cargo.lock.
fn graph(lock: &str) -> BTreeMap<String, Vec<String>> {
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut name = None;
    let mut in_dependencies = false;
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            name = None;
            in_dependencies = false;
        } else if let Some(value) = line.strip_prefix("name = ") {
            let value = value.trim_matches('"').to_string();
            graph.entry(value.clone()).or_default();
            name = Some(value);
        } else if line.starts_with("dependencies = [") {
            in_dependencies = true;
        } else if in_dependencies && line == "]" {
            in_dependencies = false;
        } else if in_dependencies {
            // "serde", or "serde 1.0.1", or "serde 1.0.1 (registry+…)"
            let dependency = line
                .trim_matches(|c| c == '"' || c == ',')
                .split(' ')
                .next()
                .unwrap_or_default()
                .to_string();
            if let Some(name) = &name {
                graph.entry(name.clone()).or_default().push(dependency);
            }
        }
    }
    graph
}

fn reachable(graph: &BTreeMap<String, Vec<String>>, root: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root.to_string()];
    while let Some(name) = pending.pop() {
        if seen.insert(name.clone()) {
            pending.extend(graph.get(&name).into_iter().flatten().cloned());
        }
    }
    seen
}

#[test]
fn proava_and_its_mcp_server_have_no_browser_engine() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock des Workspace");
    let main_graph = graph(&lock);
    for program in ["proava", "proava-mcp"] {
        let all = reachable(&main_graph, program);
        assert!(
            all.len() > 50,
            "{program}: Abhängigkeiten nicht gefunden ({})",
            all.len()
        );
        let engine: Vec<&String> = all.iter().filter(|name| is_engine(name)).collect();
        assert!(
            engine.is_empty(),
            "{program} hängt an einer Browser-Engine: {engine:?}"
        );
        assert!(
            all.contains("proava-web-protocol"),
            "{program}: Protokoll fehlt"
        );
    }
    // The helper itself does use one (its own lock file), so the check above
    // would notice a mixed-up workspace.
    let helper = std::fs::read_to_string(root.join("apps/proava-web/Cargo.lock"))
        .expect("Cargo.lock des Hilfsprogramms");
    let helper_graph = graph(&helper);
    let helper_all = reachable(&helper_graph, "proava-web");
    assert!(helper_all.contains("wry") && helper_all.contains("tao"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("exclude = [\"apps/proava-web\"]"),
        "Hilfsprogramm gehört nicht zum Workspace"
    );
}
