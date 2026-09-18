//! Integration tests for `logscope::core::pipeline` — ported from
//! `logscope/tests/test_pipeline.py` (4 cases).

use std::fs::{self, File};
use std::io::Write;

use chrono::{DateTime, TimeZone, Utc};
use tempfile::TempDir;
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use logscope::core::pipeline::{run_pipeline, PipelineResult};

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

/// 1. test_pipeline_single_file
#[test]
fn pipeline_single_file() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    let out = tmp.path().join("unified.log");
    let result = run_pipeline(&simple, ts(10, 1, 0), ts(10, 5, 0), &out).unwrap();
    assert_eq!(result, PipelineResult { total_entries: 6, filtered_entries: 3, sources: 1 });
    assert!(out.exists());
    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("2026-08-26T10:01:00Z"));
    assert!(content.contains("2026-08-26T10:05:00Z"));
    assert!(!content.contains("2026-08-26T10:00:00Z"));
}

/// 2. test_pipeline_zip
#[test]
fn pipeline_zip() {
    let tmp = TempDir::new().unwrap();
    write_zip(&tmp, "logs.zip", &[("simple.log", SIMPLE_LOG), ("multiline.log", MULTILINE_LOG)]);
    let zip_path = tmp.path().join("logs.zip");
    let out = tmp.path().join("unified.log");
    let result = run_pipeline(&zip_path, ts(10, 0, 0), ts(10, 0, 59), &out).unwrap();
    assert_eq!(result.sources, 2);
    assert_eq!(result.filtered_entries, 2);
    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("[simple.log]") && content.contains("[multiline.log]"));
}

/// 3. test_pipeline_no_matches
#[test]
fn pipeline_no_matches() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    let out = tmp.path().join("unified.log");
    let result = run_pipeline(&simple, ts(11, 0, 0), ts(12, 0, 0), &out).unwrap();
    assert_eq!(result.filtered_entries, 0);
    assert!(out.exists());
    assert_eq!(fs::read_to_string(&out).unwrap().trim(), "");
}

/// 4. test_pipeline_multiline_export
#[test]
fn pipeline_multiline_export() {
    let tmp = TempDir::new().unwrap();
    let multiline = tmp.path().join("multiline.log");
    fs::write(&multiline, MULTILINE_LOG).unwrap();
    let out = tmp.path().join("unified.log");
    let result = run_pipeline(&multiline, ts(10, 0, 0), ts(10, 0, 59), &out).unwrap();
    assert_eq!(result.filtered_entries, 1);
    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("Traceback") && content.contains("RuntimeError: boom"));
}

#[test]
fn pipeline_heatmap_data() {
    let tmp = TempDir::new().unwrap();
    let simple = write_simple(&tmp);
    let (hm, count) = logscope::core::pipeline::run_heatmap_data(&[simple], ts(10, 0, 0), ts(11, 0, 0), None, 10).unwrap();
    
    assert!(hm.sources.len() >= 1);
    assert_eq!(hm.num_buckets, 10);
    assert!(hm.min_ts.is_some());
    assert!(count >= 1);
    
    // Also assert run_heatmap_data on a nonexistent input returns Ok with an empty heatmap
    let nonexistent = tmp.path().join("does_not_exist.log");
    let (hm2, count2) = logscope::core::pipeline::run_heatmap_data(&[nonexistent], ts(10, 0, 0), ts(11, 0, 0), None, 10).unwrap();
    assert!(hm2.sources.is_empty());
    assert!(hm2.min_ts.is_none());
    assert_eq!(count2, 0);
}
