//! Integration tests for `logscope::core::ingest` — ported from
//! `logscope/tests/test_ingest.py` (10 cases). Sample bytes are inlined
//! verbatim from the Python repo's tests/samples/.

use std::fs;
use std::io::{Cursor, Write};

use flate2::write::GzEncoder;

use bzip2::write::BzEncoder;
use bzip2::Compression as BzCompression;
use tar::{Builder, Header};
use tempfile::TempDir;
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use logscope::core::ingest::{extract_files, ingest};

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

fn write_simple(dir: &TempDir) -> std::path::PathBuf {
    let p = dir.path().join("simple.log");
    fs::write(&p, SIMPLE_LOG).unwrap();
    p
}

/// 1. test_ingest_single_file
#[test]
fn ingest_single_file() {
    let tmp = TempDir::new().unwrap();
    let f = write_simple(&tmp);
    let sources = ingest(&f).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].name, "simple.log");
    assert_eq!(sources[0].entries.len(), 6);
}

/// 2. test_ingest_folder — write the 4 sample files into a temp dir.
#[test]
fn ingest_folder() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("logs");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("simple.log"), SIMPLE_LOG).unwrap();
    fs::write(folder.join("multiline.log"), MULTILINE_LOG).unwrap();
    fs::write(folder.join("mixed_formats.log"), MIXED_FORMATS_LOG).unwrap();
    fs::write(folder.join("no_timestamps.log"), NO_TIMESTAMPS_LOG).unwrap();
    let sources = ingest(&folder).unwrap();
    assert!(sources.len() >= 4, "expected >=4 sources, got {}", sources.len());
}

/// 3. test_ingest_zip — zip with 2 files -> 2 sources.
#[test]
fn ingest_zip() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("logs.zip");
    let file = fs::File::create(&zip_path).unwrap();
    let mut zw = ZipWriter::new(file);
    let opts: SimpleFileOptions =
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zw.start_file("simple.log", opts).unwrap();
    zw.write_all(SIMPLE_LOG.as_bytes()).unwrap();
    zw.start_file("multiline.log", opts).unwrap();
    zw.write_all(MULTILINE_LOG.as_bytes()).unwrap();
    zw.finish().unwrap();

    let sources = ingest(&zip_path).unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].name, "multiline.log");
    assert_eq!(sources[1].name, "simple.log");
}

/// 4. test_ingest_tar — tar.gz with 1 file -> 1 source, name passthrough.
#[test]
fn ingest_tar_gz() {
    let tmp = TempDir::new().unwrap();
    let tar_path = tmp.path().join("logs.tar.gz");
    // Build the plain tar first, then gzip it (ensures both streams finalize).
    let mut tar_buf = Vec::new();
    {
        let mut tb = Builder::new(&mut tar_buf);
        let mut header = Header::new_gnu();
        header.set_size(SIMPLE_LOG.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tb.append_data(&mut header, "simple.log", Cursor::new(SIMPLE_LOG)).unwrap();
        tb.finish().unwrap();
    }
    let mut enc = GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&tar_buf).unwrap();
    fs::write(&tar_path, enc.finish().unwrap()).unwrap();

    let sources = ingest(&tar_path).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].name, "simple.log");
}

/// 5. test_ingest_nonexistent — empty, not Err.
#[test]
fn ingest_nonexistent() {
    let tmp = TempDir::new().unwrap();
    let sources = ingest(&tmp.path().join("does_not_exist.log")).unwrap();
    assert!(sources.is_empty());
    assert!(extract_files(&tmp.path().join("does_not_exist.log")).unwrap().is_empty());
}

/// 6. test_ingest_empty_file — 1 source, 0 entries.
#[test]
fn ingest_empty_file() {
    let tmp = TempDir::new().unwrap();
    let f = tmp.path().join("empty.log");
    fs::write(&f, "").unwrap();
    let sources = ingest(&f).unwrap();
    assert_eq!(sources.len(), 1);
    assert!(sources[0].entries.is_empty());
}

/// 7. test_ingest_plain_gzip — plain .gz (not tar) -> decompressed text.
#[test]
fn ingest_plain_gzip() {
    let tmp = TempDir::new().unwrap();
    let gz = tmp.path().join("simple.log.gz");
    let mut enc = GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    );
    enc.write_all(SIMPLE_LOG.as_bytes()).unwrap();
    fs::write(&gz, enc.finish().unwrap()).unwrap();

    let sources = ingest(&gz).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].name, "simple.log.gz");
    assert_eq!(sources[0].entries.len(), 6);
}

/// 8. test_ingest_plain_bzip2
#[test]
fn ingest_plain_bzip2() {
    let tmp = TempDir::new().unwrap();
    let bz = tmp.path().join("simple.log.bz2");
    let mut enc = BzEncoder::new(Vec::new(), BzCompression::default());
    enc.write_all(SIMPLE_LOG.as_bytes()).unwrap();
    fs::write(&bz, enc.finish().unwrap()).unwrap();

    let sources = ingest(&bz).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].entries.len(), 6);
}

/// 9. test_ingest_corrupt_gzip_raises — corrupt .gz must be Err, not empty.
#[test]
fn ingest_corrupt_gzip_raises() {
    let tmp = TempDir::new().unwrap();
    let bad = tmp.path().join("bad.log.gz");
    fs::write(&bad, b"this is not gzip data at all").unwrap();
    assert!(ingest(&bad).is_err());
    assert!(extract_files(&bad).is_err());
}

/// 10. folder recurses into a subdirectory; nested file included by basename.
#[test]
fn ingest_folder_recurses_subdirectory() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("logs");
    let nested = folder.join("sub");
    fs::create_dir_all(&nested).unwrap();
    fs::write(folder.join("top.log"), SIMPLE_LOG).unwrap();
    fs::write(nested.join("nested.log"), MULTILINE_LOG).unwrap();

    let sources = ingest(&folder).unwrap();
    assert_eq!(sources.len(), 2);
    let names: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"top.log"), "names: {:?}", names);
    assert!(names.contains(&"nested.log"), "names: {:?}", names);
}
