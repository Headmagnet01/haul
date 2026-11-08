//! Checks on the Tauri config files: dev and release builds keep their data apart,
//! and the test-only window permission never ships.

use serde_json::Value;

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("valid JSON")
}

fn base() -> Value {
    json(include_str!("../tauri.conf.json"))
}

fn release() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.release.conf.json");
    json(&std::fs::read_to_string(path).expect("tauri.release.conf.json exists"))
}

fn capability(name: &str) -> Value {
    let path = format!("{}/capabilities/{name}.json", env!("CARGO_MANIFEST_DIR"));
    json(&std::fs::read_to_string(path).unwrap_or_else(|_| panic!("capabilities/{name}.json exists")))
}

fn permissions(cap: &Value) -> Vec<String> {
    cap["permissions"].as_array().unwrap().iter().filter_map(|p| p.as_str().map(String::from)).collect()
}

#[test]
fn dev_and_release_identifiers_differ() {
    assert_eq!(base()["identifier"], "dev.haul.desktop.dev");
    assert_eq!(release()["identifier"], "dev.haul.desktop");
}

#[test]
fn allow_close_is_dev_only() {
    assert!(!permissions(&capability("default")).contains(&"core:window:allow-close".to_string()));
    assert!(permissions(&capability("dev-test")).contains(&"core:window:allow-close".to_string()));
    assert_eq!(base()["app"]["security"]["capabilities"], serde_json::json!(["default", "dev-test"]));
    assert_eq!(release()["app"]["security"]["capabilities"], serde_json::json!(["default"]));
}

#[test]
fn release_script_uses_overlay() {
    let pkg = json(include_str!("../../package.json"));
    assert_eq!(pkg["scripts"]["release"], "tauri build --config src-tauri/tauri.release.conf.json");
}
