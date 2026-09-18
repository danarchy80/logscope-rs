use chrono::{TimeZone, Utc};
use logscope::core::heatmap::{build_heatmap, render_heatmap_svg};
use logscope::models::{Level, LogEntry};

#[test]
fn test_build_heatmap_basic() {
    let t1 = Utc.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2023, 1, 1, 11, 0, 0).unwrap();

    let entries = vec![
        LogEntry {
            timestamp: t1,
            level: Level::Info,
            source: "src_B".to_string(),
            original_line_number: 1,
            raw_lines: vec!["msg".to_string()],
        },
        LogEntry {
            timestamp: t1,
            level: Level::Error,
            source: "src_B".to_string(),
            original_line_number: 2,
            raw_lines: vec!["msg".to_string()],
        },
        LogEntry {
            timestamp: t2,
            level: Level::Warning,
            source: "src_A".to_string(),
            original_line_number: 3,
            raw_lines: vec!["msg".to_string()],
        },
    ];

    let hm = build_heatmap(&entries, 2);
    
    assert_eq!(hm.sources, vec!["src_A", "src_B"]);
    assert_eq!(hm.num_buckets, 2);
    assert_eq!(hm.min_ts, Some(t1));
    assert_eq!(hm.max_ts, Some(t2));
    
    // src_A (idx 0), src_B (idx 1)
    // bucket 0 (t1): src_A empty, src_B Error+Info => Error
    // bucket 1 (t2): src_A Warning, src_B empty
    assert_eq!(hm.cells[0][0], Level::Unknown);
    assert_eq!(hm.cells[0][1], Level::Warning);
    
    assert_eq!(hm.cells[1][0], Level::Error);
    assert_eq!(hm.cells[1][1], Level::Unknown);
}

#[test]
fn test_build_heatmap_equal_timestamps() {
    let t1 = Utc.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap();

    let entries = vec![
        LogEntry {
            timestamp: t1,
            level: Level::Info,
            source: "A".to_string(),
            original_line_number: 1,
            raw_lines: vec![],
        },
        LogEntry {
            timestamp: t1,
            level: Level::Error,
            source: "B".to_string(),
            original_line_number: 2,
            raw_lines: vec![],
        },
    ];

    let hm = build_heatmap(&entries, 5);
    assert_eq!(hm.cells[0][0], Level::Info);
    assert_eq!(hm.cells[1][0], Level::Error);
    // the rest are Unknown
    assert_eq!(hm.cells[0][1], Level::Unknown);
}

#[test]
fn test_build_heatmap_empty() {
    let hm = build_heatmap(&[], 5);
    assert!(hm.sources.is_empty());
    assert_eq!(hm.min_ts, None);
    assert_eq!(hm.max_ts, None);
    assert!(hm.cells.is_empty());
}

#[test]
fn test_render_heatmap_svg_non_empty() {
    let t1 = Utc.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap();
    let entries = vec![
        LogEntry {
            timestamp: t1,
            level: Level::Error,
            source: "src_XYZ".to_string(),
            original_line_number: 1,
            raw_lines: vec![],
        },
    ];
    let hm = build_heatmap(&entries, 1);
    let svg = render_heatmap_svg(&hm);

    assert!(svg.contains("<svg"));
    assert!(svg.contains("src_XYZ"));
    assert!(svg.contains("#dc2626")); // Error color
    assert!(svg.contains("Critical")); // Legend
}

#[test]
fn test_render_heatmap_svg_empty() {
    let hm = build_heatmap(&[], 2);
    let svg = render_heatmap_svg(&hm);
    assert!(svg.contains("no events"));
}

#[test]
fn test_render_heatmap_determinism() {
    let t1 = Utc.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2023, 1, 1, 11, 0, 0).unwrap();
    let entries = vec![
        LogEntry {
            timestamp: t1,
            level: Level::Warning,
            source: "B".to_string(),
            original_line_number: 1,
            raw_lines: vec![],
        },
        LogEntry {
            timestamp: t2,
            level: Level::Debug,
            source: "A".to_string(),
            original_line_number: 2,
            raw_lines: vec![],
        },
    ];
    let hm = build_heatmap(&entries, 5);
    
    let svg1 = render_heatmap_svg(&hm);
    let svg2 = render_heatmap_svg(&hm);
    assert_eq!(svg1, svg2);
}

#[test]
fn test_level_color() {
    assert_eq!(logscope::core::heatmap::level_color(Level::Error), "#dc2626");
    assert_eq!(logscope::core::heatmap::level_color(Level::Unknown), "#1f2937");
    assert_eq!(logscope::core::heatmap::level_color(Level::Critical), "#7f1d1d");
}
