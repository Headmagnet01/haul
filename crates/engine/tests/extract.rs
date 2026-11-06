//! The real bundled 7-Zip against the committed fixture archives.

use haul_engine::postproc::extract::{ExtractOutcome, Extractor, SevenZip, seven_zip_for_tests};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/archives").join(name)
}

fn sz() -> SevenZip {
    SevenZip { exe: seven_zip_for_tests() }
}

fn run(name: &str, pw: &str) -> (ExtractOutcome, tempfile::TempDir) {
    let out = tempfile::tempdir().unwrap();
    let r = sz().extract(&fixture(name), &out.path().join("x"), pw);
    (r, out)
}

fn hello(out: &tempfile::TempDir) -> String {
    std::fs::read_to_string(out.path().join("x/hello.txt")).unwrap()
}

#[test]
fn extracts_zip_and_7z() {
    for name in ["plain.zip", "plain.7z"] {
        let (r, out) = run(name, "");
        assert_eq!(r, ExtractOutcome::Ok, "{name}");
        assert_eq!(hello(&out), "hello haul\n");
        assert_eq!(std::fs::read_to_string(out.path().join("x/inner/readme.md")).unwrap(), "# inner\n");
    }
}

#[test]
fn extracts_split_7z() {
    let (r, out) = run("split.7z.001", "");
    assert_eq!(r, ExtractOutcome::Ok);
    assert_eq!(hello(&out), "hello haul\n");
    assert_eq!(std::fs::metadata(out.path().join("x/data.bin")).unwrap().len(), 65536);
}

#[test]
fn extracts_multipart_rar() {
    let (r, out) = run("multi.part1.rar", "");
    assert_eq!(r, ExtractOutcome::Ok);
    assert_eq!(std::fs::metadata(out.path().join("x/data.bin")).unwrap().len(), 65536);
}

#[test]
fn extracts_oldstyle_rar() {
    let (r, out) = run("oldstyle.rar", "");
    assert_eq!(r, ExtractOutcome::Ok);
    assert_eq!(hello(&out), "hello haul\n");
}

#[test]
fn wrong_password_is_told_apart() {
    for name in ["secret.7z", "secret.rar"] {
        for pw in ["", "nope"] {
            assert_eq!(run(name, pw).0, ExtractOutcome::WrongPassword, "{name} with {pw:?}");
        }
    }
}

#[test]
fn right_password_extracts() {
    for name in ["secret.7z", "secret.rar"] {
        let (r, out) = run(name, "haul-secret");
        assert_eq!(r, ExtractOutcome::Ok, "{name}");
        assert_eq!(hello(&out), "hello haul\n");
    }
}

#[test]
fn damaged_is_failed_not_password() {
    match run("damaged.7z", "").0 {
        ExtractOutcome::Failed(msg) => assert!(!msg.is_empty()),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn missing_7zip_is_failed() {
    let out = tempfile::tempdir().unwrap();
    let r = SevenZip { exe: out.path().join("nope.exe") }.extract(&fixture("plain.zip"), out.path(), "");
    assert!(matches!(r, ExtractOutcome::Failed(_)), "{r:?}");
}
