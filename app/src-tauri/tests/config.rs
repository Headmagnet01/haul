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

fn repo_file(rel: &str) -> String {
    let path = format!("{}/../../{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{rel} exists"))
}

#[test]
fn bundle_is_per_user_nsis() {
    let b = &base()["bundle"];
    assert_eq!(b["targets"], serde_json::json!(["nsis"]));
    let nsis = &b["windows"]["nsis"];
    assert_eq!(nsis["installMode"], "currentUser");
    assert_eq!(nsis["installerHooks"], "installer-hooks.nsh");
    assert_eq!(nsis["languages"], serde_json::json!(["English"]));
    // `license` is the SPDX name; `licenseFile` is what the installer's licence page shows.
    assert_eq!(b["license"], "MIT OR Apache-2.0");
    assert_eq!(b["licenseFile"], "../../LICENSE-MIT");
    assert!(repo_file("LICENSE-MIT").contains("MIT License"));
    assert!(repo_file("LICENSE-MIT").contains("The Haul contributors"));
    assert!(repo_file("LICENSE-APACHE").contains("Apache License"));
    assert_eq!(b["resources"]["../../LICENSE-MIT"], "LICENSE-MIT");
    assert_eq!(b["resources"]["../../LICENSE-APACHE"], "LICENSE-APACHE");
    assert_eq!(b["resources"]["resources/7zip/*"], "7zip/");
}

/// Every file `extension/dist/manifest.json` names, and every local file its pages load.
#[test]
fn bundled_extension_has_every_manifest_file() {
    assert_eq!(base()["bundle"]["resources"]["../../extension/dist/"], "extension/");
    let dist = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../extension/dist"));
    let text = std::fs::read_to_string(dist.join("manifest.json")).expect("run npm run build in extension/");
    let m = json(&text);
    let mut named: Vec<String> = Vec::new();
    let mut add = |v: &Value| {
        if let Some(s) = v.as_str() {
            named.push(s.to_string());
        }
    };
    add(&m["background"]["service_worker"]);
    add(&m["action"]["default_popup"]);
    add(&m["options_page"]);
    add(&m["options_ui"]["page"]);
    for key in [&m["icons"], &m["action"]["default_icon"]] {
        for v in key.as_object().into_iter().flat_map(|o| o.values()) {
            add(v);
        }
    }
    for cs in m["content_scripts"].as_array().into_iter().flatten() {
        for v in cs["js"].as_array().into_iter().flatten() {
            add(v);
        }
    }
    assert!(named.len() >= 6, "the manifest names its files: {named:?}");
    // The extension's pages (popup, options, picker) and what they load.
    for page in std::fs::read_dir(&dist).unwrap().flatten() {
        let p = page.path();
        if p.extension().is_some_and(|e| e == "html") {
            let html = std::fs::read_to_string(&p).unwrap();
            for attr in ["src=\"", "href=\""] {
                for part in html.split(attr).skip(1) {
                    let r = part.split('"').next().unwrap();
                    if !r.contains(':') && !r.starts_with('#') {
                        named.push(r.trim_start_matches("./").to_string());
                    }
                }
            }
        }
    }
    let missing: Vec<&String> = named.iter().filter(|f| !dist.join(f).is_file()).collect();
    assert!(missing.is_empty(), "named but missing from extension/dist: {missing:?}");
}

#[test]
fn uninstall_hook_removes_run_value() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/installer-hooks.nsh");
    let nsh = std::fs::read_to_string(path).expect("installer-hooks.nsh exists");
    assert!(nsh.contains("!macro NSIS_HOOK_POSTUNINSTALL"));
    assert!(nsh.contains(r#"DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Haul""#));
    assert!(nsh.contains(
        r#"DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Haul""#
    ));
}

#[test]
fn before_build_builds_extension() {
    assert_eq!(base()["build"]["beforeBuildCommand"], "npm --prefix ../extension run build && npm run build");
}
