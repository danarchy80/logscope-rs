//! End-to-end tests — ported from `logscope/tests/test_e2e.py` (8 cases).
//! Exercises the full ingest → filter → normalize → export chain through
//! `run_pipeline` over single files, zip, tar.gz, and folder inputs.

use std::fs::{self, File};
use std::io::Write;

use chrono::{DateTime, TimeZone, Utc};
use flate2::write::GzEncoder;
use flate2::Compression as GzCompression;
use tar::{Builder, Header};
use tempfile::TempDir;
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use logscope::core::pipeline::run_pipeline;

const SIMPLE_LOG: &str = "\
2026-08-26 10:00:00 INFO Application started
2026-08-26 10:01:00 INFO Loading config
2026-08-26 10:02:30 WARN Connection timeout
2026-08-26 10:05:00 ERROR Failed to connect
2026-08-26 10:10:00 INFO Retrying
2026-08-26 10:15:00 INFO Connected successfully
";

const MULTILINE_LOG: &str = "\
2026-08-26 10:00:00 ERROR Something went wrong
Traceback (most recent call last):
  File \"app.py\", line 42, in <module>
    main()
  File \"app.py\", line 18, in main
    raise RuntimeError(\"boom\")
RuntimeError: boom
2026-08-26 10:01:00 INFO Recovered
";

const MIXED_FORMATS_LOG: &str = "\
2026-08-26T10:00:00Z INFO Using ISO format
2026-08-26 10:01:00.123 INFO Using space-separated with millis
26/Aug/2026:10:02:00 +0000 INFO Apache-style timestamp
2026-08-26 10:03:00 INFO Back to normal
";

const NO_TIMESTAMPS_LOG: &str = "\
This line has no timestamp
Neither does this one
2026-08-26 10:00:00 This one does
But this continuation line does not
";

fn ts(h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 26, h, mi, s).single().unwrap()
}

fn write_simple(dir: &TempDir) -> std::path::PathBuf {
    let p = dir.path().join("simple.log");
    fs::write(&p, SIMPLE_LOG).unwrap();
    p
}

fn write_zip(dir: &TempDir, name: &str, members: &[(&str, &str)]) {
    let zip_path = dir.path().join(name);
    let file = File::create(&zip_path).unwrap();
    let mut zw = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (member_name, content) in members {
        zw.start_file(*member_name, opts).unwrap();
        zw.write_all(content.as_bytes()).unwrap();
    }
    zw.finish().unwrap();
}

/// Build a `.tar.gz` containing one member (Python `tarfile.open(..., "w:gz")`).
fn write_tar_gz(dir: &TempDir, name: &str, member: &str, content: &str) -> std::path::PathBuf {
    let tar_path = dir.path().join(name);
    let enc = GzEncoder::new(File::create(&tar_path).unwrap(), GzCompression::default());
    let mut builder = Builder::new(enc);
    let mut header = Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, member, content.as_bytes()).unwrap();
    builder.finish().unwrap();
    tar_path
}

/// 1. test_e2e_single_file_to_export
#[test]
fn e2e_single_file_to_export() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    let out = tmp.path().join("unified.log");
    let result = run_pipeline(&simple, ts(10, 0, 0), ts(10, 15, 0), &out).unwrap();
    assert_eq!(result.sources, 1);
    assert_eq!(result.filtered_entries, 6);
    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("2026-08-26T10:00:00Z"));
    assert!(content.contains("2026-08-26T10:15:00Z"));
    assert!(content.contains("[simple.log]"));
}

/// 2. test_e2e_multiline_preserved
#[test]
fn e2e_multiline_preserved() {
    let tmp = TempDir::new().unwrap();
    let multiline = tmp.path().join("multiline.log");
    fs::write(&multiline, MULTILINE_LOG).unwrap();
    let out = tmp.path().join("unified.log");
    run_pipeline(&multiline, ts(10, 0, 0), ts(10, 0, 59), &out).unwrap();
    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("Traceback"));
    assert!(content.contains("RuntimeError: boom"));
    assert!(content.contains("File \"app.py\""));
}

/// 3. test_e2e_zip_with_multiple_files
#[test]
fn e2e_zip_with_multiple_files() {
    let tmp = TempDir::new().unwrap();
    write_zip(
        &tmp,
        "logs.zip",
        &[
            ("simple.log", SIMPLE_LOG),
            ("multiline.log", MULTILINE_LOG),
            ("mixed.log", MIXED_FORMATS_LOG),
        ],
    );
    let zip_path = tmp.path().join("logs.zip");
    let out = tmp.path().join("unified.log");
    let result = run_pipeline(&zip_path, ts(10, 0, 0), ts(10, 5, 0), &out).unwrap();
    assert_eq!(result.sources, 3);
    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("[simple.log]"));
    assert!(content.contains("[multiline.log]"));
    assert!(content.contains("[mixed.log]"));
}

/// 4. test_e2e_tar_with_single_file
#[test]
fn e2e_tar_with_single_file() {
    let tmp = TempDir::new().unwrap();
    let tar_path = write_tar_gz(&tmp, "logs.tar.gz", "simple.log", SIMPLE_LOG);
    let out = tmp.path().join("unified.log");
    let result = run_pipeline(&tar_path, ts(10, 0, 0), ts(10, 15, 0), &out).unwrap();
    assert_eq!(result.sources, 1);
    assert_eq!(result.filtered_entries, 6);
}

/// 5. test_e2e_folder_input
#[test]
fn e2e_folder_input() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("logs");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("simple.log"), SIMPLE_LOG).unwrap();
    fs::write(folder.join("multiline.log"), MULTILINE_LOG).unwrap();
    fs::write(folder.join("mixed_formats.log"), MIXED_FORMATS_LOG).unwrap();
    fs::write(folder.join("no_timestamps.log"), NO_TIMESTAMPS_LOG).unwrap();
    let out = tmp.path().join("unified.log");
    let result = run_pipeline(&folder, ts(10, 0, 0), ts(10, 15, 0), &out).unwrap();
    assert!(result.sources >= 4);
    let content = fs::read_to_string(&out).unwrap();
    assert!(!content.is_empty());
}

/// 6. test_e2e_chronological_order
#[test]
fn e2e_chronological_order() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    let out = tmp.path().join("unified.log");
    run_pipeline(&simple, ts(10, 0, 0), ts(10, 15, 0), &out).unwrap();
    let content = fs::read_to_string(&out).unwrap();
    let timestamps: Vec<&str> = content
        .lines()
        .filter(|l| l.starts_with("2026-"))
        .map(|l| l.split(' ').next().unwrap())
        .collect();
    let mut sorted = timestamps.clone();
    sorted.sort();
    assert_eq!(timestamps, sorted);
}

/// 7. test_e2e_empty_result
#[test]
fn e2e_empty_result() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    let out = tmp.path().join("unified.log");
    let result = run_pipeline(
        &simple,
        Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).single().unwrap(),
        Utc.with_ymd_and_hms(2026, 8, 28, 0, 0, 0).single().unwrap(),
        &out,
    )
    .unwrap();
    assert_eq!(result.filtered_entries, 0);
    assert_eq!(fs::read_to_string(&out).unwrap().trim(), "");
}
