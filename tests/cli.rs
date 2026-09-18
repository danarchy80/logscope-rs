//! Integration tests for the `logscope-cli` binary (Phase 5a).

use std::fs;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates as predicate;
use tempfile::TempDir;

const SIMPLE_LOG: &str = "\
2026-08-26 10:00:00 INFO Application started
2026-08-26 10:01:00 INFO Loading config
2026-08-26 10:02:30 WARN Connection timeout
2026-08-26 10:05:00 ERROR Failed to connect
2026-08-26 10:10:00 INFO Retrying
2026-08-26 10:15:00 INFO Connected successfully
";

fn write_simple(dir: &TempDir) -> std::path::PathBuf {
    let p = dir.path().join("simple.log");
    fs::write(&p, SIMPLE_LOG).unwrap();
    p
}

fn cli() -> Command {
    // assert_cmd resolves the built `logscope-cli` binary by crate package name.
    Command::new(assert_cmd::cargo::cargo_bin!("logscope-cli"))
}

/// 1. Narrow range -> exit 0, stdout reports the filtered subset of 6 total.
#[test]
fn cli_narrow_range() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    cli()
        .args([
            "--input",
            simple.to_str().unwrap(),
            "--start",
            "2026-08-26 10:00:00",
            "--end",
            "2026-08-26 10:05:00",
            "--output",
            tmp.path().join("out.log").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Exported"))
        .stdout(predicate::str::contains("of 6 entries"));
}

/// 2. --start after --end -> exit code 2, stderr non-empty.
#[test]
fn cli_start_after_end() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    cli()
        .args([
            "--input",
            simple.to_str().unwrap(),
            "--start",
            "2026-08-26 12:00:00",
            "--end",
            "2026-08-26 09:00:00",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty().not());
}

/// 3. Nonexistent input -> Python parity: empty ingest, still exit 0 with zeros.
#[test]
fn cli_nonexistent_input() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("does-not-exist.log");
    cli()
        .args([
            "--input",
            missing.to_str().unwrap(),
            "--output",
            tmp.path().join("out.log").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Exported 0 of 0 entries from 0 source(s)",
        ));
}

/// 4. No --start/--end -> default full range exports all 6.
#[test]
fn cli_no_range_exports_all() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    cli()
        .args([
            "--input",
            simple.to_str().unwrap(),
            "--output",
            tmp.path().join("out.log").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Exported 6 of 6 entries from 1 source(s)"));
}

/// 5. --output to an explicit path -> file exists and is non-empty.
#[test]
fn cli_output_file_written() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    let out = tmp.path().join("out.log");
    cli()
        .args([
            "--input",
            simple.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(out.exists());
    assert!(!fs::read_to_string(&out).unwrap().is_empty());
}

#[test]
fn cli_search() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    let out = tmp.path().join("out.log");
    cli()
        .args([
            "--input",
            simple.to_str().unwrap(),
            "--search",
            "timeout",
            "--output",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Exported 1 of 6 entries"));
}

#[test]
fn cli_heatmap() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    let out = tmp.path().join("out.log");
    let svg = tmp.path().join("out.svg");
    cli()
        .args([
            "--input",
            simple.to_str().unwrap(),
            "--heatmap",
            svg.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Heatmap written to"));
    
    assert!(svg.exists());
    let content = fs::read_to_string(&svg).unwrap();
    assert!(content.starts_with("<svg"));
}
