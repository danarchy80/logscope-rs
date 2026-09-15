//! Differential dump: print parse_line results canonically for a battery of
//! tricky inputs. Run with `cargo run --example dump 2>/dev/null` and diff
//! against the Python equivalent.
use chrono::SecondsFormat;
use logscope::core::parser::parse_line;

fn main() {
    let inputs = [
        "2026-08-26T10:00:00Z INFO",
        "2026-08-26T10:00:00.123456Z INFO",
        "2026-08-26T10:00:00+00:00 INFO",
        "2026-08-26T10:00:00+0000 INFO",
        "2026-08-26T10:00:00.5+00:00 INFO",
        "2026-08-26 10:00:00 INFO",
        "2026-08-26 10:00:00.123 INFO",
        "2026-08-26 10:00:00 +0200 INFO",
        "2026-08-26 10:00:00 +02:00 INFO",
        "2026-08-26 10:00:00+0200 INFO",
        "26/Aug/2026:10:00:00 +0000 INFO",
        "26/Aug/2026:10:00:00 +00:00 INFO",
        "Aug 26 10:00:00 INFO",
        "This has no timestamp",
        "  File \"app.py\", line 42",
        "2026-08-26 10:00:00.123456 INFO",
        "2026-08-26T10:00:00.123+02:00 INFO",
        "Dec 31 23:59:59 INFO",
    ];
    for s in inputs {
        match parse_line(s) {
            Some(dt) => println!("{s}\t=>\t{}", dt.to_rfc3339_opts(SecondsFormat::Micros, true)),
            None => println!("{s}\t=>\tNone"),
        }
    }
}
