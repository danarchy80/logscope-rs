//! Full pipeline differential: run run_pipeline on the shared samples (single,
//! folder, zip, tar.gz) over a wide time range and dump output to files for
//! byte-comparison against the Python implementation.
use std::path::{Path, PathBuf};
use chrono::{TimeZone, Utc};
use logscope::core::pipeline::run_pipeline;

fn ts(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s).single().unwrap()
}

fn main() {
    let samples = Path::new("/home/danarchy/workspace/logscope/tests/samples");
    let out_dir = PathBuf::from(
        std::env::var("LOGSCOPE_DIFF_DIR")
            .expect("set LOGSCOPE_DIFF_DIR to the shared diff dir (parity_check.sh does this)"),
    );
    std::fs::create_dir_all(&out_dir).unwrap();

    let start = ts(2026, 8, 26, 0, 0, 0);
    let end = ts(2026, 8, 27, 0, 0, 0);

    // single file
    run(&samples.join("simple.log"), start, end, &out_dir.join("simple.out"));

    // folder (all 4 samples)
    run(samples, start, end, &out_dir.join("folder.out"));

    // zip
    let zp = out_dir.join("logs.zip");
    {
        let f = std::fs::File::create(&zp).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        let mut names: Vec<PathBuf> = std::fs::read_dir(samples)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "log").unwrap_or(false))
            .collect();
        names.sort();
        for n in &names {
            w.start_file(n.file_name().unwrap().to_string_lossy(), opts).unwrap();
            std::io::Write::write_all(&mut w, &std::fs::read(n).unwrap()).unwrap();
        }
        w.finish().unwrap();
    }
    run(&zp, start, end, &out_dir.join("zip.out"));

    // tar.gz
    let tp = out_dir.join("logs.tar.gz");
    {
        let f = std::fs::File::create(&tp).unwrap();
        let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
        {
            let mut b = tar::Builder::new(&mut enc);
            let mut names: Vec<PathBuf> = std::fs::read_dir(samples)
                .unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "log").unwrap_or(false))
                .collect();
            names.sort();
            for n in &names {
                let mut f = std::fs::File::open(n).unwrap();
                let nm = n.file_name().unwrap().to_string_lossy().into_owned();
                b.append_file(&nm, &mut f).unwrap();
            }
            b.finish().unwrap();
        }
        enc.finish().unwrap();
    }
    run(&tp, start, end, &out_dir.join("tar.out"));

    println!("done");
}

fn run(input: &Path, start: chrono::DateTime<Utc>, end: chrono::DateTime<Utc>, out: &PathBuf) {
    let r = run_pipeline(input, start, end, out).unwrap();
    println!(
        "{} -> sources={} total={} filtered={}",
        input.file_name().unwrap().to_string_lossy(),
        r.sources,
        r.total_entries,
        r.filtered_entries
    );
}
