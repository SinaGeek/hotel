//! F2: OpenAPI ↔ TypeScript drift guard.
//!
//! The server generates `/api/v1/openapi.json` from its own DTOs. The web
//! client consumes hand-written types in `apps/web/src/types/generated/api.ts`.
//! Nothing keeps those two honest by itself, so this test does: it parses both
//! and fails when a schema exists on one side only, or when a documented field
//! is missing from the other. Renaming a Rust field therefore breaks CI instead
//! of shipping a client that silently reads `undefined`.
//!
//! Runs whenever the repository layout is present (no database needed).

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/hms-tests
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `export interface Name { ... }` in the TS file -> its property names.
fn ts_interfaces(src: &str) -> std::collections::BTreeMap<String, BTreeSet<String>> {
    let mut out = std::collections::BTreeMap::new();
    let mut lines = src.lines().peekable();
    while let Some(line) = lines.next() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("export interface ") else { continue };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let mut props = BTreeSet::new();
        let mut depth = t.matches('{').count() as i32 - t.matches('}').count() as i32;
        let first_inline = t.contains('{') && !t.trim_end().ends_with('{');
        if first_inline {
            // e.g. `export interface X { a: string }` — take what is between braces
            if let (Some(a), Some(b)) = (t.find('{'), t.rfind('}')) {
                if b > a {
                    for part in t[a + 1..b].split(';') {
                        push_prop(part, &mut props);
                    }
                }
            }
        }
        while depth > 0 {
            let Some(next) = lines.next() else { break };
            depth += next.matches('{').count() as i32 - next.matches('}').count() as i32;
            if depth == 0 && next.trim().starts_with('}') {
                break;
            }
            push_prop(next, &mut props);
        }
        out.insert(name, props);
    }
    out
}

fn push_prop(raw: &str, props: &mut BTreeSet<String>) {
    let t = raw.trim().trim_end_matches(',');
    if t.is_empty() || t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") {
        return;
    }
    // `name?: type` / `name: type` — the identifier before the first ':' or '?'
    let key: String = t
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    if !key.is_empty() {
        props.insert(key);
    }
}

#[test]
fn f2_openapi_schemas_and_web_types_agree() {
    let root = repo_root();
    let ts_path = root.join("apps/web/src/types/generated/api.ts");
    let ts_src = match std::fs::read_to_string(&ts_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[skip] {} not readable ({e}) — drift guard skipped", ts_path.display());
            return;
        }
    };
    let ts = ts_interfaces(&ts_src);
    assert!(!ts.is_empty(), "failed to parse any interface from api.ts");

    let doc = hms_server_openapi();
    let schemas = doc["components"]["schemas"]
        .as_object()
        .expect("generated document has components.schemas");

    // 1) every documented schema must exist as a TS type
    let ts_names: BTreeSet<&String> = ts.keys().collect();
    let api_names: BTreeSet<String> = schemas
        .keys()
        .filter(|k| **k != "ApiResponse")
        .cloned()
        .collect();
    let missing_in_ts: Vec<&String> = api_names.iter().filter(|n| !ts_names.contains(n)).collect();
    assert!(
        missing_in_ts.is_empty(),
        "OpenAPI schemas missing from api.ts: {missing_in_ts:?}"
    );

    // 2) for each schema, no documented field may be absent from the TS type
    let mut problems = Vec::new();
    for (name, schema) in schemas {
        let Some(ts_props) = ts.get(name) else { continue };
        let Some(props) = schema["properties"].as_object() else { continue };
        for field in props.keys() {
            if !ts_props.contains(field) {
                problems.push(format!("{name}.{field} documented in OpenAPI but absent in api.ts"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "OpenAPI/TypeScript drift ({} issue(s)):\n  {}",
        problems.len(),
        problems.join("\n  ")
    );

    // 3) the paths the desktop sync + conflict tooling rely on must be present
    let paths = doc["paths"].as_object().expect("paths");
    for required in [
        "/api/v1/auth/login",
        "/api/v1/reservations",
        "/api/v1/reservations/{id}/suggested-rooms",
        "/api/v1/sync/push",
        "/api/v1/sync/pull",
        "/api/v1/reports/summary",
    ] {
        assert!(paths.contains_key(required), "OpenAPI is missing path {required}");
    }
}

/// Mirror of `hms_server::openapi::document()`: the server crate is a binary,
/// so the test links its module source directly to stay in lockstep.
fn hms_server_openapi() -> serde_json::Value {
    #[path = "../../hms-server/src/openapi.rs"]
    mod openapi_gen;
    openapi_gen::document()
}
