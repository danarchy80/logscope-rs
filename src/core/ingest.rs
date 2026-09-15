//! File ingestion — ported from Python `logscope/core/ingest.py`.
//!
//! Behavior contract (ported exactly):
//! - Nonexistent path → empty result (never an error).
//! - Single file: `.zip` → zip extraction; archive suffixes (`.tar`, `.tar.gz`,
//!   `.tgz`, `.tar.bz2`, `.tbz2`, `.gz`, `.bz2`) that ARE a tar → tar
//!   extraction; a `.gz`/`.bz2` that is NOT a tar → decompress and read as
//!   text; anything else → plain text read.
//! - Text is decoded UTF-8 with invalid bytes replaced (`String::from_utf8_lossy`),
//!   then split on `str::lines` (handles `\n` and `\r\n`).
//! - Directory: recursive walk, files sorted within each directory; same
//!   per-file handling; only the BASENAME is kept as the source name (nested
//!   zip/tar members use the member's basename too).
//! - Errors: a corrupt/unreadable TOP-LEVEL file propagates `Err` (matches the
//!   Python pass-2 fix where bad gzip raises). Archive MEMBERS inside a
//!   zip/tar stay best-effort: unreadable members are skipped.
//!
//! Python's multi-suffix detection (`name.endswith(...)` style) is reproduced by
//! matching against the lowercased full filename, because `Path::extension()`
//! only ever yields the LAST suffix (`.gz` for `logs.tar.gz`).

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use bzip2::read::BzDecoder;
use tar::Archive as TarArchive;
use zip::ZipArchive;

use crate::models::LogSource;
use crate::core::parser::parse_file;

/// Errors surfaced by [`ingest`] / [`extract_files`].
///
/// Top-level archive failures wrap the underlying crate error; member-level
/// failures inside an archive are swallowed (best-effort) and never produce
/// these variants.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
}

/// Archive-ish suffixes checked against the lowercased FILENAME (not
/// `Path::extension`, which would collapse `logs.tar.gz` to `.gz`).
const TAR_SUFFIXES: [&str; 7] = [
    ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".gz", ".bz2",
];

/// Extract `(filename, lines)` pairs for every readable log file at `path`.
///
/// Mirrors Python `extract_files`: nonexistent → empty vec; single file or
/// recursive directory walk with per-directory sorted files.
pub fn extract_files(path: &Path) -> Result<Vec<(String, Vec<String>)>, IngestError> {
    if !path.exists() {
        return Ok(vec![]);
    }
    if path.is_file() {
        return extract_single(path);
    }
    if path.is_dir() {
        return extract_dir(path);
    }
    Ok(vec![])
}

/// Ingest a path (file, directory, or archive) into parsed [`LogSource`]s.
pub fn ingest(path: &Path) -> Result<Vec<LogSource>, IngestError> {
    let file_data = extract_files(path)?;
    Ok(file_data
        .iter()
        .map(|(name, lines)| parse_file(name, lines))
        .collect())
}

/// Python `extract_files` single-file branch.
fn extract_single(path: &Path) -> Result<Vec<(String, Vec<String>)>, IngestError> {
    let name = file_name(path);
    let lower = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    if lower.ends_with(".zip") {
        return extract_zip(path);
    }
    if TAR_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
        if is_tar(path) {
            return extract_tar(path);
        }
        return Ok(vec![(name, read_lines(path)?)]);
    }
    Ok(vec![(name, read_lines(path)?)])
}

/// Python `os.walk` branch: recurse, files sorted per directory, basename only.
///
/// Uses a manual recursive `read_dir` (sorted) rather than `walkdir` so the
/// per-directory sort order matches Python exactly; walkdir's own ordering is
/// not guaranteed to interleave dirs/files the same way.
fn extract_dir(root: &Path) -> Result<Vec<(String, Vec<String>)>, IngestError> {
    let mut results = Vec::new();
    walk_dir(root, &mut results)?;
    Ok(results)
}

fn walk_dir(dir: &Path, results: &mut Vec<(String, Vec<String>)>) -> Result<(), IngestError> {
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        entries.push(entry?.path());
    }
    // Sort by file name so ordering matches Python's `sorted(files)` per dir.
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    // Files first (Python os.walk yields the file list before recursing).
    for p in &entries {
        if p.is_file() {
            let lower = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            if lower.ends_with(".zip") {
                results.extend(extract_zip(p)?);
            } else if TAR_SUFFIXES.iter().any(|s| lower.ends_with(s)) && is_tar(p) {
                results.extend(extract_tar(p)?);
            } else {
                results.push((file_name(p), read_lines(p)?));
            }
        }
    }
    for p in &entries {
        if p.is_dir() {
            walk_dir(p, results)?;
        }
    }
    Ok(())
}

/// Read a plain/gz/bz2 text file into lines. Top-level errors propagate.
fn read_lines(path: &Path) -> Result<Vec<String>, IngestError> {
    let lower = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    let content = if lower.ends_with(".gz") {
        let mut buf = Vec::new();
        GzDecoder::new(File::open(path)?).read_to_end(&mut buf)?;
        buf
    } else if lower.ends_with(".bz2") {
        let mut buf = Vec::new();
        BzDecoder::new(File::open(path)?).read_to_end(&mut buf)?;
        buf
    } else {
        std::fs::read(path)?
    };
    Ok(String::from_utf8_lossy(&content).lines().map(String::from).collect())
}

/// Open `path` as a tar stream, applying the decompression wrapper implied by
/// the lowercased suffix (Python's `tarfile.open(path, "r:*")` auto-detects
/// gzip/bzip2 wrappers; the Rust `tar` crate does not, so we pick the decoder
/// from the suffix).
fn open_tar_reader(path: &Path) -> std::io::Result<Box<dyn Read>> {
    let lower = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    let f = File::open(path)?;
    if lower.ends_with(".gz") || lower.ends_with(".tgz") {
        Ok(Box::new(GzDecoder::new(f)))
    } else if lower.ends_with(".bz2") || lower.ends_with(".tbz2") {
        Ok(Box::new(BzDecoder::new(f)))
    } else {
        Ok(Box::new(f))
    }
}

/// Python `_is_tar`: try opening as an auto-detect tar; any failure → false.
///
/// `tar::Archive::entries()` is lazy — it only returns an iterator without
/// touching the stream — so we must pull the FIRST entry to actually validate
/// the tar magic (and any gzip/bzip2 wrapper). A corrupt member after the
/// first is still handled best-effort by `extract_tar`, matching Python's
/// try/except-per-member semantics closely enough for this port.
fn is_tar(path: &Path) -> bool {
    let reader = match open_tar_reader(path) {
        Ok(r) => r,
        Err(_) => return false,
    };
    match TarArchive::new(reader).entries() {
        Ok(mut entries) => match entries.next() {
            Some(Ok(_)) => true,
            Some(Err(_)) => false,
            None => false, // empty archive: Python's tarfile also rejects it
        },
        Err(_) => false,
    }
}

/// Python `_extract_zip`: members sorted by name, directory entries skipped,
/// member read failures skipped (best-effort). Basename only.
fn extract_zip(path: &Path) -> Result<Vec<(String, Vec<String>)>, IngestError> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut names: Vec<String> = archive.file_names().map(|n| n.to_string()).collect();
    names.sort();
    let mut results = Vec::new();
    for name in names {
        if name.ends_with('/') {
            continue;
        }
        // Best-effort per member, matching the Python try/except continue.
        let mut data = Vec::new();
        match archive.by_name(&name) {
            Ok(mut zf) => {
                if zf.read_to_end(&mut data).is_err() {
                    continue;
                }
            }
            Err(_) => continue,
        }
        results.push((basename(&name), String::from_utf8_lossy(&data).lines().map(String::from).collect()));
    }
    Ok(results)
}

/// Python `_extract_tar`: members sorted by name, regular files only,
/// member read failures skipped (best-effort). Basename only.
///
/// Single pass: read each regular member's bytes while iterating, then sort
/// the collected `(name, data)` pairs by member name — same observable order
/// as Python's sort-then-read, without re-opening the archive per member.
fn extract_tar(path: &Path) -> Result<Vec<(String, Vec<String>)>, IngestError> {
    let reader = open_tar_reader(path)?;
    let mut archive = TarArchive::new(reader);
    let mut collected: Vec<(String, Vec<u8>)> = Vec::new();
    let mut entries = archive.entries()?;
    while let Some(entry) = entries.next() {
        let entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name = entry.path()?.to_string_lossy().into_owned();
        let size = entry.header().size().unwrap_or(0);
        // Best-effort per member, matching the Python try/except continue.
        let mut data = Vec::new();
        if entry.take(size).read_to_end(&mut data).is_err() {
            continue;
        }
        collected.push((name, data));
    }
    collected.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(collected
        .into_iter()
        .map(|(name, data)| {
            (
                basename(&name),
                String::from_utf8_lossy(&data).lines().map(String::from).collect(),
            )
        })
        .collect())
}

/// Basename of a path or archive-member name (Python `Path(name).name`).
fn basename(name: &str) -> String {
    Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}
